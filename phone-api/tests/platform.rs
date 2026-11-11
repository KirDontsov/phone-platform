//! HTTP proof against an isolated DB. Never use an existing production database.
//! TEST_DATABASE_URL=... cargo test -p phone-api --test platform -- --ignored --nocapture
use actix_web::{
    http::{header, StatusCode},
    test, web, App,
};
use chrono::{Duration, Utc};
use persistence::{
    connection, create_pool,
    diesel::{
        self,
        prelude::*,
        sql_types::{Text, Timestamptz, Uuid as SqlUuid},
    },
    migrate, save_document,
};
use phone_api::{auth, AppState};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const PASSWORD: &str = "Synthetic-test-password-123";
fn call(id: Uuid, time: chrono::DateTime<Utc>, phone: &str, result: &str) -> Value {
    json!({"id":id,"provider_id":id.to_string(),"started_at":time.to_rfc3339(),"phone":phone,"contact_name":"Синтетический клиент","employee":"Тест","direction":"in","provider_result":result,"human_connected":result=="bridged","duration":60,"stage":"imported","status_id":null,"tag_ids":[],"contact_kind":"unknown","score":null,"has_recording":false,"note":"","manual_lock":false,"coverage_complete":true})
}
fn session_cookie(response: &actix_web::dev::ServiceResponse) -> String {
    response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn platform_auth_tenants_csrf_roles_annotations_filters_and_secrets() {
    let pool = create_pool(
        &std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
    )
    .unwrap();
    migrate(&pool).unwrap();
    let suffix = Uuid::new_v4();
    let email_a = format!("owner-a-{suffix}@example.test");
    let email_b = format!("owner-b-{suffix}@example.test");
    let email_r = format!("reviewer-{suffix}@example.test");
    let (org_a, user_a, org_b, reviewer, call_a, call_b, tag_a, tag_b, reset_token) = {
        let mut conn = connection(&pool).unwrap();
        let (org_a, user_a) = auth::bootstrap(
            &mut conn,
            &email_a,
            PASSWORD,
            "Owner A",
            &format!("Test A {suffix}"),
        )
        .unwrap();
        let (org_b, _user_b) = auth::bootstrap(
            &mut conn,
            &email_b,
            PASSWORD,
            "Owner B",
            &format!("Test B {suffix}"),
        )
        .unwrap();
        // Same person has a different, restricted role in another organization.
        diesel::sql_query(
            "INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,'reviewer')",
        )
        .bind::<SqlUuid, _>(org_b)
        .bind::<SqlUuid, _>(user_a)
        .execute(&mut conn)
        .unwrap();
        let reviewer = Uuid::new_v4();
        diesel::sql_query(
            "INSERT INTO users(id,email,name,password_hash) VALUES($1,$2,'Reviewer',$3)",
        )
        .bind::<SqlUuid, _>(reviewer)
        .bind::<Text, _>(&email_r)
        .bind::<Text, _>(auth::password_hash(PASSWORD).unwrap())
        .execute(&mut conn)
        .unwrap();
        diesel::sql_query(
            "INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,'reviewer')",
        )
        .bind::<SqlUuid, _>(org_a)
        .bind::<SqlUuid, _>(reviewer)
        .execute(&mut conn)
        .unwrap();
        let call_a = Uuid::new_v4();
        let call_b = Uuid::new_v4();
        let tag_a = Uuid::new_v4();
        let tag_b = Uuid::new_v4();
        let time = Utc::now() - Duration::days(3);
        save_document(
            &mut conn,
            org_a,
            call_a,
            "call",
            None,
            call(call_a, time, "+79990000101", "not answered"),
            None,
        )
        .unwrap();
        save_document(
            &mut conn,
            org_b,
            call_b,
            "call",
            None,
            call(call_b, time, "+79990000202", "bridged"),
            None,
        )
        .unwrap();
        for (org, id) in [(org_a, tag_a), (org_b, tag_b)] {
            save_document(
                &mut conn,
                org,
                id,
                "tag",
                None,
                json!({"name":"Test tag","color":"#0d9488","archived":false}),
                None,
            )
            .unwrap();
        }
        let reset_token = auth::random_token();
        diesel::sql_query(
            "INSERT INTO token_grants(token_hash,kind,user_id,expires_at) VALUES($1,'reset',$2,$3)",
        )
        .bind::<Text, _>(auth::hash(&reset_token))
        .bind::<SqlUuid, _>(user_a)
        .bind::<Timestamptz, _>(Utc::now() + Duration::minutes(20))
        .execute(&mut conn)
        .unwrap();
        (
            org_a,
            user_a,
            org_b,
            reviewer,
            call_a,
            call_b,
            tag_a,
            tag_b,
            reset_token,
        )
    };
    let state = AppState {
        pool: pool.clone(),
        key: [11; 32],
        demo: true,
        secure_cookie: false,
        public_url: "http://localhost".into(),
        allowed_origins: vec!["http://localhost".into()],
        media_root: std::env::temp_dir().join(format!("phone-api-test-{suffix}")),
        throttle: Arc::new(Mutex::new(HashMap::new())),
        dummy_hash: auth::password_hash("synthetic-unusable-dummy").unwrap(),
    };
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(phone_api::configure),
    )
    .await;
    macro_rules! request {
        ($method:ident,$uri:expr,$body:expr,$cookie:expr,$csrf:expr) => {{
            let mut r = test::TestRequest::$method()
                .uri($uri)
                .insert_header((header::ORIGIN, "http://localhost"))
                .set_json($body);
            if let Some(c) = $cookie {
                r = r.insert_header((header::COOKIE, c));
            }
            if let Some(csrf) = $csrf {
                r = r.insert_header(("X-CSRF-Token", csrf));
            }
            test::call_service(&app, r.to_request()).await
        }};
    }
    let login = request!(
        post,
        "/api/auth/login",
        json!({"email":email_a,"password":PASSWORD}),
        None::<&str>,
        None::<&str>
    );
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = session_cookie(&login);
    let me: Value = test::read_body_json(login).await;
    let csrf = me["data"]["csrf_token"].as_str().unwrap().to_owned();
    assert_eq!(me["data"]["organization"]["id"], org_a.to_string());
    assert_eq!(me["data"]["organizations"].as_array().unwrap().len(), 2);
    let me_response = request!(
        get,
        "/api/auth/me",
        json!({}),
        Some(cookie.as_str()),
        None::<&str>
    );
    assert_eq!(me_response.status(), StatusCode::OK);
    let missing_csrf = request!(
        patch,
        &format!("/api/calls/{call_a}"),
        json!({"note":"no csrf"}),
        Some(cookie.as_str()),
        None::<&str>
    );
    assert_eq!(missing_csrf.status(), StatusCode::FORBIDDEN);
    let evil_origin = test::call_service(
        &app,
        test::TestRequest::patch()
            .uri(&format!("/api/calls/{call_a}"))
            .insert_header((header::ORIGIN, "https://attacker.example"))
            .insert_header((header::COOKIE, cookie.clone()))
            .insert_header(("X-CSRF-Token", csrf.clone()))
            .set_json(json!({"note":"wrong origin"}))
            .to_request(),
    )
    .await;
    assert_eq!(evil_origin.status(), StatusCode::FORBIDDEN);
    let cross = request!(
        get,
        &format!("/api/calls/{call_b}"),
        json!({}),
        Some(cookie.as_str()),
        None::<&str>
    );
    assert_eq!(cross.status(), StatusCode::NOT_FOUND);
    let own = request!(
        get,
        "/api/calls",
        json!({}),
        Some(cookie.as_str()),
        None::<&str>
    );
    let own: Value = test::read_body_json(own).await;
    assert_eq!(own["data"]["total"], 1);
    assert_eq!(own["data"]["items"][0]["id"], call_a.to_string());
    let wrong_tag = request!(
        patch,
        &format!("/api/calls/{call_a}"),
        json!({"tag_ids":[tag_b]}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(wrong_tag.status(), StatusCode::NOT_FOUND);
    let annotation = request!(
        patch,
        &format!("/api/calls/{call_a}"),
        json!({"tag_ids":[tag_a],"note":"manual correction","version":1}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(annotation.status(), StatusCode::OK);
    let annotation: Value = test::read_body_json(annotation).await;
    assert_eq!(annotation["data"]["manual_lock"], true);
    assert_eq!(annotation["data"]["version"], 2);
    let stale = request!(
        patch,
        &format!("/api/calls/{call_a}"),
        json!({"note":"stale overwrite","version":1}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    // Archived taxonomy remains valid on an existing call, but cannot be newly assigned.
    let historical_status = Uuid::new_v4();
    {
        let mut c = connection(&pool).unwrap();
        save_document(
            &mut c,
            org_a,
            historical_status,
            "status",
            None,
            json!({"name":"Historical","archived":false,"color":"#0d9488"}),
            None,
        )
        .unwrap();
    }
    let status_assigned = request!(
        patch,
        &format!("/api/calls/{call_a}"),
        json!({"status_id":historical_status}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(status_assigned.status(), StatusCode::OK);
    let status_archived = request!(
        patch,
        &format!("/api/taxonomy/{historical_status}"),
        json!({"archived":true}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(status_archived.status(), StatusCode::OK);
    let archived = request!(
        patch,
        &format!("/api/taxonomy/{tag_a}"),
        json!({"archived":true}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(archived.status(), StatusCode::OK);
    let retained = request!(
        patch,
        &format!("/api/calls/{call_a}"),
        json!({"tag_ids":[tag_a],"status_id":historical_status,"note":"Keep historical archived taxonomy"}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(retained.status(), StatusCode::OK);
    let fresh = Uuid::new_v4();
    let archived_new = Uuid::new_v4();
    {
        let mut c = connection(&pool).unwrap();
        save_document(
            &mut c,
            org_a,
            fresh,
            "call",
            None,
            call(
                fresh,
                Utc::now() - Duration::hours(1),
                "+79990000109",
                "bridged",
            ),
            None,
        )
        .unwrap();
        save_document(
            &mut c,
            org_a,
            archived_new,
            "tag",
            None,
            json!({"name":"Old tag","archived":true,"color":"#0d9488"}),
            None,
        )
        .unwrap();
    }
    let forbidden_new = request!(
        patch,
        &format!("/api/calls/{fresh}"),
        json!({"tag_ids":[archived_new]}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(forbidden_new.status(), StatusCode::BAD_REQUEST);
    let forbidden_status = request!(
        patch,
        &format!("/api/calls/{fresh}"),
        json!({"status_id":historical_status}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(forbidden_status.status(), StatusCode::BAD_REQUEST);
    let note_only = request!(
        patch,
        &format!("/api/calls/{fresh}"),
        json!({"note":"Note must not lock stale unknown classification"}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(note_only.status(), StatusCode::OK);
    let note_only: Value = test::read_body_json(note_only).await;
    assert_eq!(note_only["data"]["contact_kind"], "primary");
    assert_eq!(note_only["data"]["manual_lock"], true);
    {
        let mut c = connection(&pool).unwrap();
        persistence::delete_document(&mut c, org_a, fresh).unwrap();
    }
    let bad_filter = request!(
        post,
        "/api/filters",
        json!({"name":"Invalid","ast":{"field":"arbitrary_sql","op":"eq","value":"x"}}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(bad_filter.status(), StatusCode::BAD_REQUEST);
    let configured = request!(
        put,
        "/api/connections/asr",
        json!({"config":{"provider":"http","base_url":"https://asr.example.test","api_key":"synthetic-private-api-key"}}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(configured.status(), StatusCode::OK);
    let configured: Value = test::read_body_json(configured).await;
    assert_eq!(configured["data"]["config"]["api_key"], "<configured>");
    assert!(!configured.to_string().contains("synthetic-private-api-key"));
    assert!(configured["data"].get("config_encrypted").is_none());
    let configs = request!(
        get,
        "/api/connections",
        json!({}),
        Some(cookie.as_str()),
        None::<&str>
    );
    let configs: Value = test::read_body_json(configs).await;
    assert!(!configs.to_string().contains("synthetic-private-api-key"));
    assert!(!configs.to_string().contains("config_encrypted"));
    let invalid_policy = request!(
        patch,
        "/api/organization",
        json!({"working_hours":{"enabled":true,"start":"09:00","end":"18:00","days":[8]}}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(
        invalid_policy.status(),
        StatusCode::BAD_REQUEST,
        "Invalid work calendar must be rejected, not silently persisted"
    );
    let retention = request!(
        patch,
        "/api/organization",
        json!({"retention_days":365}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(retention.status(), StatusCode::OK);
    let cleanup = request!(
        post,
        "/api/operations/purge",
        json!({}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(cleanup.status(), StatusCode::OK);
    let cleanup: Value = test::read_body_json(cleanup).await;
    {
        let mut c = connection(&pool).unwrap();
        let jobs = persistence::list_jobs(&mut c, org_a, 100).unwrap();
        let job = jobs
            .iter()
            .find(|j| cleanup["data"]["job_id"] == j.id.to_string())
            .unwrap();
        assert_eq!(job.kind, "retention");
        assert_eq!(job.payload["retention_days"], 365);
    }
    {
        let mut c = connection(&pool).unwrap();
        diesel::sql_query(
            "UPDATE memberships SET extension='123' WHERE organization_id=$1 AND user_id=$2",
        )
        .bind::<SqlUuid, _>(org_a)
        .bind::<SqlUuid, _>(reviewer)
        .execute(&mut c)
        .unwrap();
    }
    let role_only = request!(
        patch,
        &format!("/api/members/{reviewer}"),
        json!({"role":"manager"}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(role_only.status(), StatusCode::OK);
    let members = request!(
        get,
        "/api/members",
        json!({}),
        Some(cookie.as_str()),
        None::<&str>
    );
    let members: Value = test::read_body_json(members).await;
    let member = members["data"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == reviewer.to_string())
        .unwrap();
    assert_eq!(
        member["extension"], "123",
        "Role-only updates must preserve extension mapping"
    );
    // Access changes are audited atomically and only inside the active tenant.
    let audit_before = {
        let mut c = connection(&pool).unwrap();
        let audits = persistence::list_documents(&mut c, org_a, "audit").unwrap();
        let changed = audits
            .iter()
            .find(|a| {
                a.data["action"] == "membership.updated"
                    && a.data["subject_id"] == reviewer.to_string()
            })
            .unwrap();
        assert_eq!(changed.organization_id, org_a);
        assert_eq!(changed.data["user_id"], user_a.to_string());
        assert_eq!(changed.data["details"]["previous_role"], "reviewer");
        assert_eq!(changed.data["details"]["requested_role"], "manager");
        assert_eq!(changed.data["details"]["extension_changed"], false);
        audits.len()
    };
    let missing_member = request!(
        patch,
        &format!("/api/members/{}", Uuid::new_v4()),
        json!({"role":"owner"}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(missing_member.status(), StatusCode::NOT_FOUND);
    {
        let mut c = connection(&pool).unwrap();
        assert_eq!(
            persistence::list_documents(&mut c, org_a, "audit")
                .unwrap()
                .len(),
            audit_before
        );
    }
    let invitation = Uuid::new_v4();
    let foreign_invitation = Uuid::new_v4();
    {
        let mut c = connection(&pool).unwrap();
        for (id, org) in [(invitation, org_a), (foreign_invitation, org_b)] {
            diesel::sql_query("INSERT INTO token_grants(id,token_hash,kind,organization_id,role,expires_at) VALUES($1,$2,'invite',$3,'reviewer',$4)").bind::<SqlUuid,_>(id).bind::<Text,_>(auth::hash(&id.to_string())).bind::<SqlUuid,_>(org).bind::<Timestamptz,_>(Utc::now()+Duration::hours(1)).execute(&mut c).unwrap();
        }
    }
    let foreign_revoke = request!(
        delete,
        &format!("/api/members/invitations/{foreign_invitation}"),
        json!({}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(foreign_revoke.status(), StatusCode::NOT_FOUND);
    {
        let mut c = connection(&pool).unwrap();
        assert_eq!(
            persistence::list_documents(&mut c, org_a, "audit")
                .unwrap()
                .len(),
            audit_before
        );
        assert!(persistence::list_documents(&mut c, org_b, "audit")
            .unwrap()
            .is_empty());
    }
    let revoke = request!(
        delete,
        &format!("/api/members/invitations/{invitation}"),
        json!({}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(revoke.status(), StatusCode::OK);
    let removal = request!(
        delete,
        &format!("/api/members/{reviewer}"),
        json!({}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(removal.status(), StatusCode::OK);
    {
        let mut c = connection(&pool).unwrap();
        let audits = persistence::list_documents(&mut c, org_a, "audit").unwrap();
        for (action, subject) in [
            ("invitation.revoked", invitation),
            ("membership.removed", reviewer),
        ] {
            let event = audits
                .iter()
                .find(|a| a.data["action"] == action && a.data["subject_id"] == subject.to_string())
                .unwrap();
            assert_eq!(event.organization_id, org_a);
            assert_eq!(event.data["user_id"], user_a.to_string());
            assert!(!event.data.to_string().contains("token_hash"));
            assert!(!event.data.to_string().contains(&email_r));
        }
    }
    let telphin = request!(
        put,
        "/api/connections/telphin",
        json!({"config":{"host":"apiproxy.telphin.ru","client_id":"42","app_id":"synthetic-app","app_secret":"synthetic-app-secret"}}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(telphin.status(), StatusCode::OK);
    let sync_first = request!(
        post,
        "/api/connections/telphin/sync",
        json!({}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(sync_first.status(), StatusCode::OK);
    let sync_first: Value = test::read_body_json(sync_first).await;
    let first_job = Uuid::parse_str(sync_first["data"]["job_id"].as_str().unwrap()).unwrap();
    {
        let mut c = connection(&pool).unwrap();
        diesel::sql_query("UPDATE jobs SET state='succeeded' WHERE organization_id=$1 AND id=$2")
            .bind::<SqlUuid, _>(org_a)
            .bind::<SqlUuid, _>(first_job)
            .execute(&mut c)
            .unwrap();
    }
    let sync_second = request!(
        post,
        "/api/connections/telphin/sync",
        json!({}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(sync_second.status(), StatusCode::OK);
    let sync_second: Value = test::read_body_json(sync_second).await;
    assert_ne!(
        sync_second["data"]["job_id"], sync_first["data"]["job_id"],
        "Fresh manual sync must not return an old succeeded job"
    );
    let stranger = Uuid::new_v4();
    let switch_bad = request!(
        post,
        "/api/auth/organization",
        json!({"organization_id":stranger}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(switch_bad.status(), StatusCode::FORBIDDEN);
    let switched = request!(
        post,
        "/api/auth/organization",
        json!({"organization_id":org_b}),
        Some(cookie.as_str()),
        Some(csrf.as_str())
    );
    assert_eq!(switched.status(), StatusCode::OK);
    let cookie_b = session_cookie(&switched);
    let switched: Value = test::read_body_json(switched).await;
    let csrf_b = switched["data"]["csrf_token"].as_str().unwrap().to_owned();
    assert_ne!(csrf, csrf_b);
    assert_eq!(switched["data"]["role"], "reviewer");
    let old_session = request!(
        get,
        "/api/auth/me",
        json!({}),
        Some(cookie.as_str()),
        None::<&str>
    );
    assert_eq!(old_session.status(), StatusCode::UNAUTHORIZED);
    let write_as_reviewer = request!(
        patch,
        &format!("/api/calls/{call_b}"),
        json!({"note":"forbidden"}),
        Some(cookie_b.as_str()),
        Some(csrf_b.as_str())
    );
    assert_eq!(write_as_reviewer.status(), StatusCode::FORBIDDEN);
    let forbidden_access_change = request!(
        patch,
        &format!("/api/members/{user_a}"),
        json!({"role":"owner"}),
        Some(cookie_b.as_str()),
        Some(csrf_b.as_str())
    );
    assert_eq!(forbidden_access_change.status(), StatusCode::FORBIDDEN);
    {
        let mut c = connection(&pool).unwrap();
        assert!(persistence::list_documents(&mut c, org_b, "audit")
            .unwrap()
            .is_empty());
    }
    let foreign_after_switch = request!(
        get,
        &format!("/api/calls/{call_a}"),
        json!({}),
        Some(cookie_b.as_str()),
        None::<&str>
    );
    assert_eq!(foreign_after_switch.status(), StatusCode::NOT_FOUND);
    let new_password = "Synthetic-new-password-456";
    let reset = request!(
        post,
        "/api/auth/reset/confirm",
        json!({"token":reset_token,"password":new_password}),
        None::<&str>,
        None::<&str>
    );
    assert_eq!(reset.status(), StatusCode::OK);
    let reset_replay = request!(
        post,
        "/api/auth/reset/confirm",
        json!({"token":reset_token,"password":new_password}),
        None::<&str>,
        None::<&str>
    );
    assert_eq!(reset_replay.status(), StatusCode::BAD_REQUEST);
    let revoked = request!(
        get,
        "/api/auth/me",
        json!({}),
        Some(cookie_b.as_str()),
        None::<&str>
    );
    assert_eq!(revoked.status(), StatusCode::UNAUTHORIZED);
    let login_new = request!(
        post,
        "/api/auth/login",
        json!({"email":email_a,"password":new_password}),
        None::<&str>,
        None::<&str>
    );
    assert_eq!(login_new.status(), StatusCode::OK);
    let cookie_new = session_cookie(&login_new);
    let login_new: Value = test::read_body_json(login_new).await;
    let csrf_new = login_new["data"]["csrf_token"].as_str().unwrap().to_owned();
    let logout = request!(
        post,
        "/api/auth/logout",
        json!({}),
        Some(cookie_new.as_str()),
        Some(csrf_new.as_str())
    );
    assert_eq!(logout.status(), StatusCode::OK);
    let logout_reuse = request!(
        get,
        "/api/auth/me",
        json!({}),
        Some(cookie_new.as_str()),
        None::<&str>
    );
    assert_eq!(logout_reuse.status(), StatusCode::UNAUTHORIZED);
    let mut conn = connection(&pool).unwrap();
    for org in [org_a, org_b] {
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(org)
            .execute(&mut conn)
            .unwrap();
    }
    for user in [user_a, reviewer] {
        diesel::sql_query("DELETE FROM users WHERE id=$1")
            .bind::<SqlUuid, _>(user)
            .execute(&mut conn)
            .unwrap();
    }
    diesel::sql_query("DELETE FROM users WHERE email=$1")
        .bind::<Text, _>(&email_b)
        .execute(&mut conn)
        .unwrap();
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn dashboard_filters_do_not_erase_resolving_calls_and_reopen_callbacks() {
    let pool = create_pool(
        &std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
    )
    .unwrap();
    migrate(&pool).unwrap();
    let suffix = Uuid::new_v4();
    let email = format!("callback-{suffix}@example.test");
    let (org, user, missed, resolved, day) = {
        let mut c = connection(&pool).unwrap();
        let (org, user) =
            auth::bootstrap(&mut c, &email, PASSWORD, "Owner", "Callback test").unwrap();
        let missed = Uuid::new_v4();
        let resolved = Uuid::new_v4();
        let time = (Utc::now() - Duration::days(4))
            .date_naive()
            .and_hms_opt(12, 0, 0)
            .unwrap()
            .and_utc();
        save_document(
            &mut c,
            org,
            missed,
            "call",
            None,
            call(missed, time, "+79990000303", "not answered"),
            None,
        )
        .unwrap();
        let mut response = call(
            resolved,
            time + Duration::days(1),
            "+79990000303",
            "bridged",
        );
        response["direction"] = json!("out");
        save_document(&mut c, org, resolved, "call", None, response, None).unwrap();
        (
            org,
            user,
            missed,
            resolved,
            time.format("%Y-%m-%d").to_string(),
        )
    };
    let state = AppState {
        pool: pool.clone(),
        key: [12; 32],
        demo: true,
        secure_cookie: false,
        public_url: "http://localhost".into(),
        allowed_origins: vec!["http://localhost".into()],
        media_root: std::env::temp_dir(),
        throttle: Arc::new(Mutex::new(HashMap::new())),
        dummy_hash: auth::password_hash("synthetic-dummy").unwrap(),
    };
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(state))
            .configure(phone_api::configure),
    )
    .await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(json!({"email":email,"password":PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = session_cookie(&response);
    let list = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/calls?from={day}&to={day}"))
            .insert_header((header::COOKIE, cookie.as_str()))
            .to_request(),
    )
    .await;
    let list: Value = test::read_body_json(list).await;
    assert_eq!(list["data"]["total"], 1);
    assert_eq!(list["data"]["items"][0]["id"], missed.to_string());
    assert_eq!(list["data"]["items"][0]["callback_state"], "resolved");
    let dashboard = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/dashboard?from={day}&to={day}"))
            .insert_header((header::COOKIE, cookie.as_str()))
            .to_request(),
    )
    .await;
    let dashboard: Value = test::read_body_json(dashboard).await;
    assert_eq!(dashboard["data"]["totals"]["calls"], 1);
    assert_eq!(
        dashboard["data"]["totals"]["unhandled"], 0,
        "Filtered dashboard must preserve the successful callback outside the selected day"
    );
    assert_eq!(dashboard["data"]["totals"]["overdue"], 0);
    let detail = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/calls/{missed}"))
            .insert_header((header::COOKIE, cookie.as_str()))
            .to_request(),
    )
    .await;
    let detail: Value = test::read_body_json(detail).await;
    assert_eq!(
        detail["data"]["call"]["contact_kind"], list["data"]["items"][0]["contact_kind"],
        "Call detail and list must use the same history-derived classification"
    );
    let mut c = connection(&pool).unwrap();
    let _ = (resolved,);
    diesel::sql_query("DELETE FROM organizations WHERE id=$1")
        .bind::<SqlUuid, _>(org)
        .execute(&mut c)
        .unwrap();
    diesel::sql_query("DELETE FROM users WHERE id=$1")
        .bind::<SqlUuid, _>(user)
        .execute(&mut c)
        .unwrap();
}
