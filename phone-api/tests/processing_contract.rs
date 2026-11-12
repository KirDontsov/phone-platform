//! Actual HTTP/PG processing contract on synthetic tenant data; no provider calls.
use actix_web::{http::header, test, web, App};
use chrono::Utc;
use diesel::{prelude::*, sql_types::Uuid as SqlUuid};
use phone_api::{auth, AppState};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[actix_web::test]
#[ignore = "requires isolated TEST_DATABASE_URL"]
async fn processing_preflight_projection_queue_and_relative_callbacks_are_tenant_scoped() {
    let pool = persistence::create_pool(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    persistence::migrate(&pool).unwrap();
    let key = [71; 32];
    let suffix = Uuid::new_v4();
    let email = format!("processing-{suffix}@example.test");
    let password = "Synthetic-processing-2026";
    let (org, foreign, call, missing, callback, closed, transcript, job) = {
        let mut c = persistence::connection(&pool).unwrap();
        let (org, _) =
            auth::bootstrap(&mut c, &email, password, "Synthetic", "Processing").unwrap();
        let (foreign, _) = auth::bootstrap(
            &mut c,
            &format!("foreign-{suffix}@example.test"),
            password,
            "Synthetic",
            "Foreign",
        )
        .unwrap();
        let call = Uuid::new_v4();
        let missing = Uuid::new_v4();
        let callback = Uuid::new_v4();
        let closed = Uuid::new_v4();
        for (id, has_recording, minutes, direction, result, phone) in [
            (call, true, 30, "in", "bridged", "synthetic-a"),
            (missing, false, 40, "in", "bridged", "synthetic-b"),
            (callback, false, 180, "in", "not answered", "+70000000001"),
            (closed, false, 240, "in", "not answered", "+70000000002"),
        ] {
            persistence::save_document(&mut c,org,id,"call",None,json!({"started_at":Utc::now()-chrono::Duration::minutes(minutes),"phone":phone,"direction":direction,"provider_result":result,"duration":30,"has_recording":has_recording,"stage":"ready","score":99,"coverage_complete":true,"analysis_state":"analyzed","transcript_state":"ready"}),None).unwrap();
        }
        persistence::save_document(&mut c,org,Uuid::new_v4(),"call",None,json!({"started_at":Utc::now()-chrono::Duration::minutes(1),"phone":"+70000000002","direction":"out","provider_result":"bridged","duration":30,"has_recording":false,"coverage_complete":true}),None).unwrap();
        persistence::save_document(
            &mut c,
            org,
            Uuid::new_v4(),
            "recording",
            Some(call),
            json!({"ready":true,"path":"private-path-must-not-leak"}),
            None,
        )
        .unwrap();
        let transcript = Uuid::new_v4();
        persistence::save_document(
            &mut c,
            org,
            transcript,
            "transcript",
            Some(call),
            json!({"state":"ready","segments":[{"id":"s1","text":"synthetic"}]}),
            None,
        )
        .unwrap();
        persistence::save_document(
            &mut c,
            org,
            Uuid::new_v4(),
            "analysis",
            Some(call),
            json!({"state":"ready","score":88,"transcript_id":Uuid::new_v4()}),
            None,
        )
        .unwrap();
        persistence::save_document(
            &mut c,
            org,
            Uuid::new_v4(),
            "rubric",
            None,
            json!({"active":true,"criteria":[{"id":"a","name":"Greeting","weight":1}]}),
            None,
        )
        .unwrap();
        for kind in ["asr", "analysis"] {
            persistence::save_document(&mut c,org,Uuid::new_v4(),"connection",None,json!({"kind":kind,"state":"configured","config_encrypted":contracts::seal(&json!({"provider":"demo","model":"demo"}),&key).unwrap()}),None).unwrap();
        }
        // Same UUID in another tenant must never contribute analysis/transcripts/jobs/queue.
        persistence::save_document(
            &mut c,
            foreign,
            missing,
            "call",
            None,
            json!({"has_recording":true}),
            None,
        )
        .unwrap();
        persistence::save_document(
            &mut c,
            foreign,
            Uuid::new_v4(),
            "transcript",
            Some(missing),
            json!({"state":"ready"}),
            None,
        )
        .unwrap();
        for n in 0..4 {
            persistence::enqueue(
                &mut c,
                foreign,
                "recording",
                Some(missing),
                &format!("foreign-{n}"),
                json!({"secret":"private-payload-must-not-leak"}),
            )
            .unwrap();
        }
        // Latest succeeded wrapper must not hide the failed child. More than200 newer jobs.
        let job = persistence::enqueue(
            &mut c,
            org,
            "transcribe",
            Some(call),
            "old-child",
            json!({"secret":"private-payload-must-not-leak"}),
        )
        .unwrap();
        diesel::sql_query("UPDATE jobs SET state='failed',error_code='demo_provider_not_allowed',created_at=now()-interval '1 day' WHERE id=$1").bind::<SqlUuid,_>(job.id).execute(&mut c).unwrap();
        let wrapper =
            persistence::enqueue(&mut c, org, "reprocess", Some(call), "wrapper", json!({}))
                .unwrap();
        diesel::sql_query("UPDATE jobs SET state='succeeded' WHERE id=$1")
            .bind::<SqlUuid, _>(wrapper.id)
            .execute(&mut c)
            .unwrap();
        for n in 0..205 {
            let j = persistence::enqueue(
                &mut c,
                org,
                "retention",
                None,
                &format!("noise-{n}"),
                json!({}),
            )
            .unwrap();
            diesel::sql_query("UPDATE jobs SET state='succeeded' WHERE id=$1")
                .bind::<SqlUuid, _>(j.id)
                .execute(&mut c)
                .unwrap();
        }
        (
            org, foreign, call, missing, callback, closed, transcript, job.id,
        )
    };
    let state = web::Data::new(AppState {
        pool: pool.clone(),
        key,
        demo: true,
        secure_cookie: false,
        public_url: "http://localhost".into(),
        allowed_origins: vec!["http://localhost".into()],
        media_root: std::env::temp_dir().join(format!("unused-processing-{suffix}")),
        throttle: Arc::new(Mutex::new(HashMap::new())),
        dummy_hash: auth::password_hash("unusable-synthetic").unwrap(),
    });
    let app = test::init_service(App::new().app_data(state).configure(phone_api::configure)).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/login")
            .insert_header((header::ORIGIN, "http://localhost"))
            .set_json(json!({"email":email,"password":password}))
            .to_request(),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
    let cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let login: Value = test::read_body_json(response).await;
    let csrf = login["data"]["csrf_token"].as_str().unwrap();
    macro_rules! get {
        ($path:expr) => {{
            let r = test::call_service(
                &app,
                test::TestRequest::get()
                    .uri(&$path)
                    .insert_header((header::COOKIE, cookie.clone()))
                    .to_request(),
            )
            .await;
            assert_eq!(r.status().as_u16(), 200);
            test::read_body_json::<Value, _>(r).await["data"].clone()
        }};
    }
    macro_rules! post {($id:expr,$stage:expr)=>{test::TestRequest::post().uri(&format!("/api/calls/{}/reprocess",$id)).insert_header((header::COOKIE,cookie.clone())).insert_header((header::ORIGIN,"http://localhost")).insert_header(("X-CSRF-Token",csrf)).set_json(json!({"stage":$stage})).to_request()};}
    let detail = get!(format!("/api/calls/{call}"));
    assert_eq!(detail["call"]["transcript_state"], "ready");
    assert_eq!(
        detail["call"]["analysis_state"], "not_analyzed",
        "old transcript analysis is history only"
    );
    assert_eq!(detail["processing"]["state"], "failed");
    assert_eq!(detail["processing"]["job_id"], job.to_string());
    assert_eq!(
        detail["processing"]["error_code"],
        "demo_provider_not_allowed"
    );
    assert_eq!(detail["processing"]["providers"]["asr"]["ready"], false);
    assert_eq!(
        detail["processing"]["providers"]["asr"]["reason"],
        "demo_provider_not_allowed"
    );
    assert_eq!(detail["processing"]["queue"]["ready_total"], 0);
    assert!(!detail.to_string().contains("private-path-must-not-leak"));
    assert!(!detail.to_string().contains("private-payload-must-not-leak"));
    assert!(!detail.to_string().contains("config_encrypted"));
    assert!(!detail["processing"].to_string().contains("lease_owner"));
    let missing_detail = get!(format!("/api/calls/{missing}"));
    assert_eq!(missing_detail["call"]["transcript_state"], "missing");
    assert_eq!(missing_detail["call"]["analysis_state"], "not_analyzed");
    let r = test::call_service(&app, post!(call, "transcribe")).await;
    assert_eq!(r.status().as_u16(), 422);
    let error: Value = test::read_body_json(r).await;
    assert_eq!(error["error"]["code"], "asr_not_configured");
    {
        let mut c = persistence::connection(&pool).unwrap();
        assert!(!persistence::list_jobs(&mut c, org, 1000)
            .unwrap()
            .iter()
            .any(|j| j.kind == "reprocess" && j.state == "queued"));
        let mut provider = persistence::list_documents(&mut c, org, "connection")
            .unwrap()
            .into_iter()
            .find(|d| d.data["kind"] == "asr")
            .unwrap();
        provider.data["config_encrypted"]=json!(contracts::seal(&json!({"provider":"openai","model":"synthetic-asr","base_url":"https://api.openai.com/v1","api_key":"synthetic-never-sent"}),&key).unwrap());
        persistence::save_document(
            &mut c,
            org,
            provider.id,
            "connection",
            None,
            provider.data,
            Some(provider.version),
        )
        .unwrap();
    }
    // Two concurrent HTTP submissions share one durable job under the call row lock.
    let (a, b) = tokio::join!(
        test::call_service(&app, post!(call, "transcribe")),
        test::call_service(&app, post!(call, "transcribe"))
    );
    let statuses = [a.status().as_u16(), b.status().as_u16()];
    assert!(statuses.contains(&200) && statuses.contains(&202));
    let a: Value = test::read_body_json(a).await;
    let b: Value = test::read_body_json(b).await;
    assert_eq!(a["data"]["job_id"], b["data"]["job_id"]);
    let active = a["data"]["job_id"]
        .as_str()
        .unwrap()
        .parse::<Uuid>()
        .unwrap();
    let detail = get!(format!("/api/calls/{call}"));
    assert_eq!(detail["processing"]["state"], "queued");
    assert_eq!(detail["processing"]["queue"]["ready_total"], 1);
    assert_eq!(detail["processing"]["queue"]["ready_ahead"], 0);
    {
        let mut c = persistence::connection(&pool).unwrap();
        diesel::sql_query("UPDATE jobs SET state='retry',next_attempt_at=now()+interval '2 hours',error_code='provider_rate_limited' WHERE id=$1").bind::<SqlUuid,_>(active).execute(&mut c).unwrap();
        persistence::save_document(&mut c,org,Uuid::new_v4(),"analysis",Some(call),json!({"state":"partial","score":null,"abstention_reason":"unknown_roles","transcript_id":transcript}),None).unwrap();
    }
    let detail = get!(format!("/api/calls/{call}"));
    assert_eq!(detail["call"]["analysis_state"], "partial");
    assert_eq!(detail["processing"]["state"], "retry");
    assert_eq!(detail["processing"]["queue"]["scheduled_total"], 1);
    assert!(detail["processing"]["queue"]["ready_ahead"].is_null());
    {
        let mut c = persistence::connection(&pool).unwrap();
        let child = persistence::enqueue(
            &mut c,
            org,
            "transcribe",
            Some(call),
            "active-child",
            json!({"secret":"private-payload-must-not-leak"}),
        )
        .unwrap();
        diesel::sql_query("UPDATE jobs SET state='succeeded' WHERE id=$1")
            .bind::<SqlUuid, _>(active)
            .execute(&mut c)
            .unwrap();
        // Newer completed wrappers do not hide the active child; arbitrary legacy errors are sanitized.
        let wrapper = persistence::enqueue(
            &mut c,
            org,
            "reprocess",
            Some(call),
            "new-wrapper",
            json!({}),
        )
        .unwrap();
        diesel::sql_query("UPDATE jobs SET state='succeeded' WHERE id=$1")
            .bind::<SqlUuid, _>(wrapper.id)
            .execute(&mut c)
            .unwrap();
        diesel::sql_query("UPDATE jobs SET state='retry',error_code='private-untrusted-error-text',next_attempt_at=now()+interval '1 hour' WHERE id=$1").bind::<SqlUuid,_>(child.id).execute(&mut c).unwrap();
        let mut provider = persistence::list_documents(&mut c, org, "connection")
            .unwrap()
            .into_iter()
            .find(|d| d.data["kind"] == "asr")
            .unwrap();
        provider.data["state"] = json!("disconnected");
        persistence::save_document(
            &mut c,
            org,
            provider.id,
            "connection",
            None,
            provider.data,
            Some(provider.version),
        )
        .unwrap();
    }
    let detail = get!(format!("/api/calls/{call}"));
    assert_eq!(detail["processing"]["step"], "transcribe");
    assert_eq!(detail["processing"]["state"], "retry");
    assert_eq!(detail["processing"]["error_code"], "processing_failed");
    assert!(!detail.to_string().contains("private-untrusted-error-text"));
    let r = test::call_service(&app, post!(call, "transcribe")).await;
    assert_eq!(
        r.status().as_u16(),
        202,
        "existing work reused even after provider disconnect"
    );
    let r = test::call_service(&app, post!(missing, "transcribe")).await;
    assert_eq!(r.status().as_u16(), 422);
    let r = test::call_service(&app, post!(missing, "analyze")).await;
    assert_eq!(r.status().as_u16(), 422);
    let error: Value = test::read_body_json(r).await;
    assert_eq!(error["error"]["code"], "transcript_required");
    {
        let mut c = persistence::connection(&pool).unwrap();
        diesel::sql_query("UPDATE jobs SET state='running',lease_until=now()+interval '5 minutes' WHERE organization_id=$1 AND kind='transcribe' AND state='retry'").bind::<SqlUuid,_>(org).execute(&mut c).unwrap();
    }
    let detail = get!(format!("/api/calls/{call}"));
    assert_eq!(detail["processing"]["state"], "running");
    assert_eq!(detail["processing"]["queue"]["running_total"], 1);
    assert!(detail["processing"]["error_code"].is_null());
    {
        let mut c = persistence::connection(&pool).unwrap();
        diesel::sql_query("UPDATE jobs SET state='succeeded' WHERE organization_id=$1 AND kind='transcribe' AND state='running'").bind::<SqlUuid,_>(org).execute(&mut c).unwrap();
        persistence::save_document(
            &mut c,
            org,
            Uuid::new_v4(),
            "transcript",
            Some(missing),
            json!({"state":"ready","segments":[{"id":"s1","text":"synthetic"}]}),
            None,
        )
        .unwrap();
    }
    let detail = get!(format!("/api/calls/{call}"));
    assert_eq!(detail["processing"]["state"], "completed");
    assert_eq!(detail["call"]["analysis_state"], "partial");
    {
        let mut c = persistence::connection(&pool).unwrap();
        let failed = persistence::enqueue(
            &mut c,
            org,
            "reprocess",
            Some(call),
            "new-failed-wrapper",
            json!({}),
        )
        .unwrap();
        diesel::sql_query(
            "UPDATE jobs SET state='failed',error_code='configuration_missing' WHERE id=$1",
        )
        .bind::<SqlUuid, _>(failed.id)
        .execute(&mut c)
        .unwrap();
    }
    let detail = get!(format!("/api/calls/{call}"));
    assert_eq!(detail["processing"]["step"], "reprocess");
    assert_eq!(detail["processing"]["state"], "failed");
    assert_eq!(detail["processing"]["error_code"], "configuration_missing");
    assert_eq!(detail["call"]["analysis_state"], "partial");
    let r = test::call_service(&app, post!(missing, "analyze")).await;
    assert_eq!(r.status().as_u16(), 422);
    let error: Value = test::read_body_json(r).await;
    assert_eq!(error["error"]["code"], "analysis_not_configured");
    {
        let mut c = persistence::connection(&pool).unwrap();
        let mut provider = persistence::list_documents(&mut c, org, "connection")
            .unwrap()
            .into_iter()
            .find(|d| d.data["kind"] == "analysis")
            .unwrap();
        provider.data["config_encrypted"]=json!(contracts::seal(&json!({"provider":"openai","model":"synthetic-analysis","base_url":"https://api.openai.com/v1","api_key":"synthetic-never-sent"}),&key).unwrap());
        persistence::save_document(
            &mut c,
            org,
            provider.id,
            "connection",
            None,
            provider.data,
            Some(provider.version),
        )
        .unwrap();
        diesel::sql_query("DELETE FROM documents WHERE organization_id=$1 AND kind='rubric'")
            .bind::<SqlUuid, _>(org)
            .execute(&mut c)
            .unwrap();
    }
    let r = test::call_service(&app, post!(missing, "analyze")).await;
    assert_eq!(r.status().as_u16(), 422);
    let error: Value = test::read_body_json(r).await;
    assert_eq!(error["error"]["code"], "active_rubric_required");
    let ast = json!({"op":"and","rules":[{"field":"callback_state","op":"in","value":["open","overdue"]},{"field":"callback_age_minutes","op":"gte","value":120},{"field":"callback_attempts","op":"eq","value":0}]});
    let mut u = reqwest::Url::parse("http://localhost/api/calls").unwrap();
    u.query_pairs_mut().append_pair("filter", &ast.to_string());
    let list = get!(format!("{}?{}", u.path(), u.query().unwrap()));
    assert_eq!(list["total"], 1);
    assert_eq!(list["items"][0]["id"], callback.to_string());
    assert!(list["items"][0]["callback_age_minutes"].as_f64().unwrap() >= 180.);
    let callbacks = get!(format!("/api/callbacks?{}", u.query().unwrap()));
    assert_eq!(callbacks["items"].as_array().unwrap().len(), 1);
    assert_eq!(callbacks["items"][0]["call_id"], callback.to_string());
    assert_eq!(callbacks["items"][0]["callback_attempts"], 0);
    let dash = get!(format!("/api/dashboard?{}", u.query().unwrap()));
    assert_eq!(dash["totals"]["calls"], 1);
    assert_eq!(dash["totals"]["overdue"], 1);
    let cdetail = get!(format!("/api/calls/{closed}"));
    assert_eq!(cdetail["call"]["callback_state"], "resolved");
    // History outside the date filter still closes the original missed episode.
    let mut dated = reqwest::Url::parse("http://localhost/api/calls").unwrap();
    dated
        .query_pairs_mut()
        .append_pair(
            "to",
            &(Utc::now() - chrono::Duration::minutes(60)).to_rfc3339(),
        )
        .append_pair("filter", &ast.to_string());
    let dated = get!(format!("{}?{}", dated.path(), dated.query().unwrap()));
    assert_eq!(dated["total"], 1);
    assert_eq!(dated["items"][0]["id"], callback.to_string());
    let mut c = persistence::connection(&pool).unwrap();
    for tenant in [org, foreign] {
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(tenant)
            .execute(&mut c)
            .unwrap();
    }
}
