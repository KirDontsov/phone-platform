//! PostgreSQL/Actix contract acceptance on synthetic tenant data; no media/provider requests.
use actix_web::{http::header, test, web, App};
use diesel::{prelude::*, sql_types::Uuid as SqlUuid};
use persistence::{connection, create_pool, migrate, save_document};
use phone_api::{auth, AppState};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const PASSWORD: &str = "Recording-availability-synthetic-2026";
const PRIVATE_PATH: &str = "synthetic-private-audio-path-must-not-leak.wav";

fn call(id: Uuid, label: &str, has_recording: bool) -> Value {
    json!({"id":id,"provider_id":id.to_string(),"started_at":"2026-01-02T10:00:00Z","phone":"synthetic-contact","contact_name":label,"employee":"Synthetic employee","direction":"in","provider_result":"bridged","human_connected":true,"duration":30,"stage":"imported","status_id":null,"tag_ids":[],"contact_kind":"unknown","score":null,"has_recording":has_recording,"recording_state":"ready","note":"","manual_lock":false,"coverage_complete":false})
}
fn uri(filter: Option<Value>, limit: usize, cursor: Option<&str>) -> String {
    let mut url = reqwest::Url::parse("http://localhost/api/calls").unwrap();
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("limit", &limit.to_string());
        if let Some(filter) = filter {
            query.append_pair("filter", &filter.to_string());
        }
        if let Some(cursor) = cursor {
            query.append_pair("cursor", cursor);
        }
    }
    url.as_str()
        .strip_prefix("http://localhost")
        .unwrap()
        .to_owned()
}
fn filter(state: &str) -> Value {
    json!({"op":"eq","field":"recording_state","value":state})
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; actual Actix list/detail/filter"]
async fn recording_availability_is_derived_scoped_and_shared_by_filters_totals_and_pagination() {
    let pool = create_pool(
        &std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
    )
    .unwrap();
    migrate(&pool).unwrap();
    let suffix = Uuid::new_v4();
    let email = format!("recording-owner-{suffix}@example.test");
    let foreign_email = format!("recording-foreign-{suffix}@example.test");
    let specs = vec![
        (
            "ready",
            true,
            "ready",
            vec![json!({"state":"ready","ready":true})],
        ),
        (
            "mixed-ready",
            true,
            "ready",
            vec![
                json!({"state":"failed"}),
                json!({"state":"pending","last_error":"provider_rate_limited"}),
                json!({"state":"ready","ready":true}),
            ],
        ),
        (
            "legacy-ready-flag",
            true,
            "ready",
            vec![json!({"state":"pending","ready":true})],
        ),
        (
            "ready-state",
            true,
            "ready",
            vec![json!({"state":"ready","ready":false})],
        ),
        (
            "pending",
            true,
            "pending",
            vec![json!({"state":"pending","ready":false})],
        ),
        (
            "failed-and-pending",
            true,
            "pending",
            vec![json!({"state":"failed"}), json!({"state":"pending"})],
        ),
        (
            "rate-limited",
            true,
            "rate_limited",
            vec![json!({"state":"pending","last_error":"provider_rate_limited"})],
        ),
        (
            "mixed-rate-limited",
            true,
            "rate_limited",
            vec![
                json!({"state":"failed"}),
                json!({"state":"pending"}),
                json!({"state":"pending","last_error":"provider_rate_limited"}),
            ],
        ),
        (
            "all-failed",
            true,
            "failed",
            vec![json!({"state":"failed"}), json!({"state":"failed"})],
        ),
        (
            "failed-stale-rate",
            true,
            "failed",
            vec![json!({"state":"failed","last_error":"provider_rate_limited"})],
        ),
        (
            "pending-ignores-failed-rate",
            true,
            "pending",
            vec![
                json!({"state":"failed","last_error":"provider_rate_limited"}),
                json!({"state":"pending"}),
            ],
        ),
        ("none-with-spoofed-ready", false, "none", vec![]),
        ("unknown-with-spoofed-ready", true, "unknown", vec![]),
        ("foreign-only-recording", true, "unknown", vec![]),
        (
            "ready-with-false-provider-flag",
            false,
            "ready",
            vec![json!({"state":"ready","ready":true})],
        ),
    ];
    let (org, owner, foreign_org, foreign_owner, expected, foreign_only) = {
        let mut c = connection(&pool).unwrap();
        let (org, owner) = auth::bootstrap(
            &mut c,
            &email,
            PASSWORD,
            "Recording owner",
            "Recording synthetic test",
        )
        .unwrap();
        let (foreign_org, foreign_owner) = auth::bootstrap(
            &mut c,
            &foreign_email,
            PASSWORD,
            "Foreign recording owner",
            "Foreign recording synthetic test",
        )
        .unwrap();
        let mut expected = HashMap::new();
        let mut foreign_parent = None;
        for (label, presence, state, records) in specs {
            let id = Uuid::new_v4();
            save_document(
                &mut c,
                org,
                id,
                "call",
                None,
                call(id, label, presence),
                None,
            )
            .unwrap();
            expected.insert(id, (state.to_string(), json!(presence)));
            if label == "foreign-only-recording" {
                foreign_parent = Some(id);
            }
            for mut record in records {
                record["call_id"] = json!(id);
                record["path"] = json!(format!("{org}/{PRIVATE_PATH}"));
                record["download_url"] = json!(format!("https://example.invalid/{PRIVATE_PATH}"));
                save_document(
                    &mut c,
                    org,
                    Uuid::new_v4(),
                    "recording",
                    Some(id),
                    record,
                    None,
                )
                .unwrap();
            }
        }
        for missing in [true, false] {
            let id = Uuid::new_v4();
            let mut legacy = call(id, "Legacy recording flag unavailable", true);
            if missing {
                legacy.as_object_mut().unwrap().remove("has_recording");
            } else {
                legacy["has_recording"] = Value::Null;
            }
            save_document(&mut c, org, id, "call", None, legacy, None).unwrap();
            expected.insert(id, ("unknown".to_owned(), Value::Null));
        }
        // The UUID exists in both tenants, so a missing organization predicate
        // would incorrectly adopt the other company's playable recording.
        let parent = foreign_parent.unwrap();
        save_document(
            &mut c,
            foreign_org,
            parent,
            "call",
            None,
            call(parent, "Foreign duplicate UUID", true),
            None,
        )
        .unwrap();
        save_document(
            &mut c,
            foreign_org,
            Uuid::new_v4(),
            "recording",
            Some(parent),
            json!({"call_id":parent,"state":"ready","ready":true,"path":PRIVATE_PATH}),
            None,
        )
        .unwrap();
        let foreign_only = Uuid::new_v4();
        save_document(
            &mut c,
            foreign_org,
            foreign_only,
            "call",
            None,
            call(foreign_only, "Foreign only", true),
            None,
        )
        .unwrap();
        (
            org,
            owner,
            foreign_org,
            foreign_owner,
            expected,
            foreign_only,
        )
    };
    let state = web::Data::new(AppState {
        pool: pool.clone(),
        key: [41; 32],
        demo: false,
        secure_cookie: false,
        public_url: "http://localhost".into(),
        allowed_origins: vec!["http://localhost".into()],
        media_root: std::env::temp_dir().join(format!("recording-availability-unused-{suffix}")),
        throttle: Arc::new(Mutex::new(HashMap::new())),
        dummy_hash: auth::password_hash("Unusable-recording-dummy").unwrap(),
    });
    let app = test::init_service(App::new().app_data(state).configure(phone_api::configure)).await;
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/login")
            .insert_header((header::ORIGIN, "http://localhost"))
            .set_json(json!({"email":email,"password":PASSWORD}))
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
    let _: Value = test::read_body_json(response).await;
    let request = |path: &str| {
        test::TestRequest::get()
            .uri(path)
            .insert_header((header::COOKIE, cookie.clone()))
            .insert_header(("X-Organization-Id", org.to_string()))
            .to_request()
    };
    let response = test::call_service(&app, request(&uri(None, 100, None))).await;
    assert_eq!(response.status().as_u16(), 200);
    let body: Value = test::read_body_json(response).await;
    assert!(!body.to_string().contains(PRIVATE_PATH));
    assert_eq!(
        body["data"]["total"].as_u64().unwrap() as usize,
        expected.len()
    );
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), expected.len());
    for item in items {
        let id = Uuid::parse_str(item["id"].as_str().unwrap()).unwrap();
        let (state, presence) = &expected[&id];
        assert_eq!(
            &item["recording_state"], state,
            "Wrong availability for {}",
            item["contact_name"]
        );
        assert_eq!(
            item["has_recording"], *presence,
            "Technical provider presence must remain independent from playability"
        );
        let response = test::call_service(&app, request(&format!("/api/calls/{id}"))).await;
        assert_eq!(response.status().as_u16(), 200);
        let detail: Value = test::read_body_json(response).await;
        assert_eq!(
            &detail["data"]["call"]["recording_state"], state,
            "Detail and list must share the same aggregation"
        );
        assert!(
            !detail.to_string().contains(PRIVATE_PATH),
            "Raw recording paths/URLs leaked from detail"
        );
    }
    for state in [
        "ready",
        "pending",
        "rate_limited",
        "failed",
        "none",
        "unknown",
    ] {
        let wanted: HashSet<_> = expected
            .iter()
            .filter(|(_, (value, _))| value == state)
            .map(|(id, _)| *id)
            .collect();
        assert!(!wanted.is_empty());
        let mut seen = HashSet::new();
        let mut cursor = None;
        loop {
            let response = test::call_service(
                &app,
                request(&uri(Some(filter(state)), 1, cursor.as_deref())),
            )
            .await;
            assert_eq!(
                response.status().as_u16(),
                200,
                "recording_state must be an accepted typed filter"
            );
            let page: Value = test::read_body_json(response).await;
            assert_eq!(
                page["data"]["total"].as_u64().unwrap() as usize,
                wanted.len(),
                "Total must count the filtered availability set before pagination"
            );
            let items = page["data"]["items"].as_array().unwrap();
            assert_eq!(items.len(), 1);
            let item = &items[0];
            assert_eq!(item["recording_state"], state);
            let id = Uuid::parse_str(item["id"].as_str().unwrap()).unwrap();
            assert!(wanted.contains(&id));
            assert!(
                seen.insert(id),
                "Cursor repeated a call with a tied timestamp"
            );
            cursor = page["data"]["next_cursor"].as_str().map(str::to_owned);
            if cursor.is_none() {
                break;
            }
            assert!(seen.len() < wanted.len(), "Pagination did not terminate");
        }
        assert_eq!(
            seen, wanted,
            "Pagination lost calls sharing the same timestamp"
        );
    }
    let response = test::call_service(&app, request(&format!("/api/calls/{foreign_only}"))).await;
    assert_eq!(response.status().as_u16(), 404);
    let response = test::call_service(
        &app,
        request(&uri(
            Some(json!({"op":"eq","field":"recording_state","value":true})),
            100,
            None,
        )),
    )
    .await;
    assert_eq!(
        response.status().as_u16(),
        400,
        "Availability filters must retain text type validation"
    );
    drop(app);
    let mut c = connection(&pool).unwrap();
    for id in [org, foreign_org] {
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(id)
            .execute(&mut c)
            .unwrap();
    }
    for id in [owner, foreign_owner] {
        diesel::sql_query("DELETE FROM users WHERE id=$1")
            .bind::<SqlUuid, _>(id)
            .execute(&mut c)
            .unwrap();
    }
}
