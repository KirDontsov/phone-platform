//! Synthetic tenant HTTP/PG checks; never calls a provider or uses customer media.
use actix_web::{http::header, test, web, App};
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
async fn asr_provider_keys_are_not_reused_and_connector_models_urls_are_validated() {
    let pool = persistence::create_pool(&std::env::var("TEST_DATABASE_URL").unwrap()).unwrap();
    persistence::migrate(&pool).unwrap();
    let key = [73; 32];
    let suffix = Uuid::new_v4();
    let email = format!("asr-{suffix}@example.test");
    let password = "Synthetic-ASR-only-2026";
    let (org, foreign, call) = {
        let mut c = persistence::connection(&pool).unwrap();
        let (org, _) = auth::bootstrap(&mut c, &email, password, "Owner", "ASR").unwrap();
        let (foreign, _) = auth::bootstrap(
            &mut c,
            &format!("other-{suffix}@example.test"),
            password,
            "Other",
            "Other",
        )
        .unwrap();
        let call = Uuid::new_v4();
        persistence::save_document(&mut c,org,call,"call",None,json!({"started_at":chrono::Utc::now(),"direction":"in","provider_result":"bridged","duration":3,"stage":"imported","has_recording":true,"tag_ids":[]}),None).unwrap();
        persistence::save_document(
            &mut c,
            org,
            Uuid::new_v4(),
            "recording",
            Some(call),
            json!({"ready":true,"state":"ready","path":"synthetic-private-path"}),
            None,
        )
        .unwrap();
        let id = Uuid::new_v5(&org, b"connection:asr");
        persistence::save_document(&mut c,org,id,"connection",None,json!({"kind":"asr","state":"configured","config_encrypted":contracts::seal(&json!({"provider":"openai","base_url":"https://api.openai.com/v1","model":"synthetic-asr","api_key":"synthetic-old-provider-key"}),&key).unwrap()}),None).unwrap();
        let id = Uuid::new_v5(&foreign, b"connection:asr");
        persistence::save_document(&mut c,foreign,id,"connection",None,json!({"kind":"asr","state":"configured","config_encrypted":contracts::seal(&json!({"provider":"opencode","base_url":"http://model-runner:4096","model":"moondream/parakeet-ultra","api_key":"synthetic-foreign-key"}),&key).unwrap()}),None).unwrap();
        (org, foreign, call)
    };
    let state = AppState {
        pool: pool.clone(),
        key,
        demo: false,
        secure_cookie: false,
        public_url: "http://localhost".into(),
        allowed_origins: vec!["http://localhost".into()],
        media_root: std::env::temp_dir(),
        throttle: Arc::new(Mutex::new(HashMap::new())),
        dummy_hash: auth::password_hash("Synthetic-unused-only").unwrap(),
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
    macro_rules! put {($config:expr)=>{test::call_service(&app,test::TestRequest::put().uri("/api/connections/asr").insert_header((header::COOKIE,cookie.clone())).insert_header((header::ORIGIN,"http://localhost")).insert_header(("X-CSRF-Token",csrf)).set_json(json!({"config":$config})).to_request()).await};}
    let id = Uuid::new_v5(&org, b"connection:asr");
    let config = json!({"provider":"opencode","base_url":"http://model-runner:4096","model":"moondream/parakeet-ultra","api_key":""});
    let response = put!(config);
    assert_eq!(
        response.status().as_u16(),
        400,
        "provider switch must never inherit a key from the previous provider"
    );
    let mut config = config;
    config["api_key"] = json!("synthetic-tenant-openrouter-key");
    let response = put!(config.clone());
    assert_eq!(response.status().as_u16(), 200);
    let public: Value = test::read_body_json(response).await;
    assert_eq!(public["data"]["config"]["api_key"], "<configured>");
    assert!(!public
        .to_string()
        .contains("synthetic-tenant-openrouter-key"));
    assert!(!public.to_string().contains("config_encrypted"));
    config["api_key"] = json!("");
    config["model"] = json!("nvidia/nemotron-3.5-transcribe");
    let response = put!(config.clone());
    assert_eq!(
        response.status().as_u16(),
        200,
        "same provider keeps its existing key"
    );
    let mut c = persistence::connection(&pool).unwrap();
    let doc = persistence::get_document(&mut c, org, id).unwrap().unwrap();
    let saved = phone_api::settings::config_value(&doc, &key).unwrap();
    assert_eq!(saved["api_key"], "synthetic-tenant-openrouter-key");
    assert_eq!(saved["model"], "nvidia/nemotron-3.5-transcribe");
    let foreign_doc =
        persistence::get_document(&mut c, foreign, Uuid::new_v5(&foreign, b"connection:asr"))
            .unwrap()
            .unwrap();
    assert_eq!(
        phone_api::settings::config_value(&foreign_doc, &key).unwrap()["api_key"],
        "synthetic-foreign-key"
    );
    let version = doc.version;
    config["model"] = json!("not-an-approved/model");
    let response = put!(config.clone());
    assert_eq!(response.status().as_u16(), 422);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "unsupported_asr_model");
    config["model"] = json!("moondream/parakeet-ultra");
    config["base_url"] = json!("https://openrouter.ai/api/v1");
    config["api_key"] = json!("synthetic-new-key");
    let response = put!(config.clone());
    assert_eq!(response.status().as_u16(), 422);
    let error: Value = test::read_body_json(response).await;
    assert_eq!(error["error"]["code"], "asr_runner_url_mismatch");
    config["provider"] = json!("http");
    config["api_key"] = json!("");
    let response = put!(config);
    assert_eq!(response.status().as_u16(), 400);
    assert_eq!(
        persistence::get_document(&mut c, org, id)
            .unwrap()
            .unwrap()
            .version,
        version,
        "invalid writes are rolled back"
    );
    let response = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/calls/{call}"))
            .insert_header((header::COOKIE, cookie.clone()))
            .to_request(),
    )
    .await;
    assert_eq!(response.status().as_u16(), 200);
    let detail: Value = test::read_body_json(response).await;
    assert_eq!(
        detail["data"]["processing"]["providers"]["asr"]["ready"],
        true
    );
    assert!(!detail
        .to_string()
        .contains("synthetic-tenant-openrouter-key"));
    let response = test::call_service(
        &app,
        test::TestRequest::post()
            .uri(&format!("/api/calls/{call}/reprocess"))
            .insert_header((header::COOKIE, cookie))
            .insert_header((header::ORIGIN, "http://localhost"))
            .insert_header(("X-CSRF-Token", csrf))
            .set_json(json!({"stage":"transcribe"}))
            .to_request(),
    )
    .await;
    assert_eq!(
        response.status().as_u16(),
        200,
        "valid connector configuration passes preflight without requiring analysis configuration"
    );
    for org in [org, foreign] {
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(org)
            .execute(&mut c)
            .unwrap();
    }
}
