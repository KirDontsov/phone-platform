//! Contract acceptance uses isolated PostgreSQL and real local HTTP, never live providers.
//! TEST_DATABASE_URL=... cargo test -p phone-api --test assistant -- --ignored --test-threads=1
use actix_web::{
    body::{BodySize, MessageBody},
    dev::ServerHandle,
    web, App, HttpRequest, HttpResponse, HttpServer,
};
use chrono::{Duration, Utc};
use diesel::{
    prelude::*,
    sql_types::{Integer, Jsonb, Text, Uuid as SqlUuid},
};
use persistence::{connection, create_pool, migrate, Pool};
use phone_api::{auth, AppState};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io,
    net::TcpListener,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration as StdDuration,
};
use tokio::sync::mpsc;
use uuid::Uuid;

static SERIAL: Mutex<()> = Mutex::new(());
const PASSWORD: &str = "Synthetic-assistant-password-2026";
const RUNNER_TOKEN: &str = "synthetic-private-runner-token-not-a-live-credential";
const KEY_SENTINEL: &str = "synthetic-api-key-sentinel-never-in-model-history";
const TELPHIN_SENTINEL: &str = "synthetic-telphin-secret-sentinel-never-in-history";

struct ControlledBody(mpsc::UnboundedReceiver<web::Bytes>);
impl MessageBody for ControlledBody {
    type Error = io::Error;
    fn size(&self) -> BodySize {
        BodySize::Stream
    }
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<web::Bytes, io::Error>>> {
        self.0.poll_recv(cx).map(|v| v.map(Ok))
    }
}
#[derive(Clone)]
struct StagedAuth {
    request: Value,
    login_id: String,
    authenticated: bool,
}
#[derive(Default)]
struct FixtureState {
    api: String,
    automatic: bool,
    current_auth: HashMap<String, StagedAuth>,
    attempts: Vec<Value>,
    turns: HashMap<Uuid, Value>,
    streams: HashMap<Uuid, mpsc::UnboundedSender<web::Bytes>>,
    requests: Vec<(String, Value)>,
}
#[derive(Clone, Default)]
struct RunnerFixture(Arc<Mutex<FixtureState>>);
impl RunnerFixture {
    async fn complete(&self, attempt: &Value) -> (u16, Value) {
        let api = self.0.lock().unwrap().api.clone();
        let response=Client::new().post(format!("{api}/api/assistant/auth/complete"))
            .bearer_auth(attempt["authorization_token"].as_str().expect("private auth token"))
            .json(&json!({"auth_attempt_id":attempt["auth_attempt_id"],"generation":attempt["generation"]}))
            .timeout(StdDuration::from_secs(10)).send().await.expect("local auth completion HTTP");
        let status = response.status().as_u16();
        let body = response.json().await.unwrap_or(Value::Null);
        if status == 200 {
            assert_eq!(
                body["data"]["approved"], true,
                "Auth callback requires explicit API approval"
            );
            assert_eq!(body["data"]["generation"], attempt["generation"]);
            let org = attempt["organization_id"].as_str().unwrap();
            let mut state = self.0.lock().unwrap();
            if let Some(current) = state.current_auth.get_mut(org) {
                if current.request["auth_attempt_id"] == attempt["auth_attempt_id"] {
                    current.authenticated = true;
                }
            }
        }
        (status, body)
    }
    fn status(&self, org: &str) -> Value {
        let state = self.0.lock().unwrap();
        let current = state.current_auth.get(org);
        let authenticated = current.is_some_and(|a| a.authenticated);
        let pending=current.filter(|a|!a.authenticated&&a.request["type"]=="chatgptDeviceCode").map(|a|json!({"login_id":a.login_id,"verification_url":"https://auth.openai.com/codex/device","user_code":"TEST-0000","expires_at":Utc::now()+Duration::minutes(10)}));
        json!({"authenticated":authenticated,"auth_mode":if authenticated{Some("apiKey")}else{None},"codex_version":"fixture-no-model-executed","model":"test-wire-fixture","pending_login":pending,"generation":current.map(|a|a.request["generation"].clone()),"error_code":null})
    }
    async fn grant(&self, run: Uuid) -> Value {
        let deadline = tokio::time::Instant::now() + StdDuration::from_secs(10);
        loop {
            if let Some(value) = self.0.lock().unwrap().turns.get(&run).cloned() {
                return value;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "API did not dispatch the durable run to local fixture"
            );
            tokio::time::sleep(StdDuration::from_millis(10)).await;
        }
    }
    fn emit(&self, run: Uuid, event: Value) {
        let sender = self
            .0
            .lock()
            .unwrap()
            .streams
            .get(&run)
            .cloned()
            .expect("active fixture stream");
        sender.send(web::Bytes::from(format!("{event}\n"))).unwrap();
    }
    fn close(&self, run: Uuid) {
        self.0.lock().unwrap().streams.remove(&run);
    }
    fn finish(&self, run: Uuid, event: Value) {
        if let Some(sender) = self.0.lock().unwrap().streams.remove(&run) {
            let _ = sender.send(web::Bytes::from(format!("{}\n", event)));
        }
    }
    fn last_attempt(&self) -> Value {
        self.0
            .lock()
            .unwrap()
            .attempts
            .last()
            .expect("auth attempt captured")
            .clone()
    }
}
async fn automatic_fixture_turn(fixture: RunnerFixture, run: Uuid, request: Value) {
    if request["history"].as_array().is_some_and(|history| {
        history.iter().any(|m| {
            m["text"]
                .as_str()
                .is_some_and(|text| text.contains("[hold]"))
        })
    }) {
        return;
    }
    let api = fixture.0.lock().unwrap().api.clone();
    let token = request["mcp_token"].as_str().unwrap();
    let client = Client::new();
    let result=async {
        let snapshot=client.post(format!("{api}/api/assistant/tools")).bearer_auth(token).json(&json!({"method":"call","call_id":"fixture-get-workspace","name":"get_workspace_configuration","arguments":{}})).send().await.map_err(|_|())?;
        if !snapshot.status().is_success(){return Err(());}let snapshot:Value=snapshot.json().await.map_err(|_|())?;
        let revision=snapshot["data"]["result"]["revision"].as_i64().ok_or(())?;
        let saved=client.post(format!("{api}/api/assistant/tools")).bearer_auth(token).json(&json!({"method":"call","call_id":"fixture-create-tag","name":"upsert_taxonomy","arguments":{"expected_revision":revision,"kind":"tag","name":"Тестовый тег помощника","color":"#0d9488"}})).send().await.map_err(|_|())?;
        if !saved.status().is_success(){return Err(());}Ok::<(),()>(())
    }.await;
    fixture.finish(run,if result.is_ok(){json!({"type":"completed","text":"Тестовый стенд применил тег через настоящий API. Это протокольная fixture без вызова модели.","model":"test-wire-fixture","usage":{"input_tokens":0,"output_tokens":0}})}else{json!({"type":"error","code":"fixture_tool_failed"})});
}
async fn fixture_handler(
    req: HttpRequest,
    fixture: web::Data<RunnerFixture>,
    body: web::Json<Value>,
) -> HttpResponse {
    if req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        != Some(&format!("Bearer {RUNNER_TOKEN}"))
    {
        return HttpResponse::Unauthorized().finish();
    }
    let path = req.path().to_owned();
    let value = body.into_inner();
    fixture
        .0
        .lock()
        .unwrap()
        .requests
        .push((path.clone(), value.clone()));
    let org = value["organization_id"].as_str().unwrap_or("").to_string();
    match path.as_str() {
        "/auth/status" => HttpResponse::Ok().json(fixture.status(&org)),
        "/auth/start" => {
            let staged = StagedAuth {
                request: value.clone(),
                login_id: Uuid::new_v4().to_string(),
                authenticated: false,
            };
            {
                let mut state = fixture.0.lock().unwrap();
                state.attempts.push(value.clone());
                state.current_auth.insert(org.clone(), staged);
            }
            if value["type"] == "apiKey" {
                let (status, _) = fixture.complete(&value).await;
                if status != 200 {
                    return HttpResponse::Conflict()
                        .json(json!({"error_code":"fixture_auth_promotion_denied"}));
                }
            }
            HttpResponse::Ok().json(fixture.status(&org))
        }
        "/auth/cancel" | "/auth/logout" => {
            fixture.0.lock().unwrap().current_auth.remove(&org);
            HttpResponse::Ok().json(fixture.status(&org))
        }
        "/turn" => {
            let run = Uuid::parse_str(value["run_id"].as_str().unwrap()).unwrap();
            let (sender, receiver) = mpsc::unbounded_channel();
            let _ = sender.send(web::Bytes::from_static(
                b"{\"type\":\"status\",\"message\":\"Local protocol fixture; no inference\"}\n",
            ));
            {
                let mut state = fixture.0.lock().unwrap();
                state.turns.insert(run, value.clone());
                state.streams.insert(run, sender);
            }
            if fixture.0.lock().unwrap().automatic {
                actix_web::rt::spawn(automatic_fixture_turn(
                    fixture.get_ref().clone(),
                    run,
                    value,
                ));
            }
            HttpResponse::Ok()
                .content_type("application/x-ndjson")
                .message_body(ControlledBody(receiver))
                .unwrap()
                .map_into_boxed_body()
        }
        "/cancel" => {
            if let Some(id) = value["run_id"]
                .as_str()
                .and_then(|id| Uuid::parse_str(id).ok())
            {
                fixture.finish(id, json!({"type":"error","code":"fixture_cancelled"}));
            }
            HttpResponse::Ok().json(json!({"cancelled":true}))
        }
        _ => HttpResponse::NotFound().finish(),
    }
}
#[derive(Clone)]
struct Session {
    cookie: String,
    csrf: String,
    org: Uuid,
    user: Uuid,
}
struct Reply {
    status: u16,
    body: Value,
}
impl Reply {
    fn ok(self) -> Value {
        assert!(
            (200..300).contains(&self.status),
            "HTTP {}: {}",
            self.status,
            self.body
        );
        self.body["data"].clone()
    }
    fn denied(&self) {
        assert!(
            (400..500).contains(&self.status),
            "Must deny with 4xx, got {}: {}",
            self.status,
            self.body
        );
    }
}
struct Harness {
    api: String,
    password: String,
    client: Client,
    pool: Pool,
    state: AppState,
    fixture: RunnerFixture,
    api_server: ServerHandle,
    runner_server: ServerHandle,
    org: Uuid,
    foreign_org: Uuid,
    users: Vec<Uuid>,
    owner_email: String,
    peer_email: String,
    foreign_email: String,
    reviewer_email: String,
}
impl Harness {
    async fn new() -> Self {
        let pool = create_pool(
            &std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
        )
        .unwrap();
        migrate(&pool).unwrap();
        let suffix = Uuid::new_v4();
        let browser = std::env::var("ASSISTANT_BROWSER_FIXTURE").as_deref() == Ok("1");
        let password = if browser {
            "PhoneAssistant!2026"
        } else {
            PASSWORD
        }
        .to_string();
        let owner_email = if browser {
            "assistant-owner@example.test".to_string()
        } else {
            format!("assistant-owner-{suffix}@example.test")
        };
        let peer_email = format!("assistant-peer-{suffix}@example.test");
        let foreign_email = format!("assistant-foreign-{suffix}@example.test");
        let reviewer_email = if browser {
            "assistant-reviewer@example.test".to_string()
        } else {
            format!("assistant-reviewer-{suffix}@example.test")
        };
        let (org, owner, foreign_org, foreign, peer, reviewer) = {
            let mut conn = connection(&pool).unwrap();
            let (org, owner) = auth::bootstrap(
                &mut conn,
                &owner_email,
                &password,
                "Owner",
                "Assistant synthetic test",
            )
            .unwrap();
            let (foreign_org, foreign) = auth::bootstrap(
                &mut conn,
                &foreign_email,
                &password,
                "Foreign owner",
                "Assistant foreign synthetic test",
            )
            .unwrap();
            let peer = Uuid::new_v4();
            let reviewer = Uuid::new_v4();
            for (id, email, role) in [
                (peer, &peer_email, "owner"),
                (reviewer, &reviewer_email, "reviewer"),
            ] {
                diesel::sql_query("INSERT INTO users(id,email,name,password_hash) VALUES($1,$2,'Synthetic assistant test',$3)").bind::<SqlUuid,_>(id).bind::<Text,_>(email).bind::<Text,_>(auth::password_hash(&password).unwrap()).execute(&mut conn).unwrap();
                diesel::sql_query(
                    "INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,$3)",
                )
                .bind::<SqlUuid, _>(org)
                .bind::<SqlUuid, _>(id)
                .bind::<Text, _>(role)
                .execute(&mut conn)
                .unwrap();
            }
            diesel::sql_query(
                "INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,$3)",
            )
            .bind::<SqlUuid, _>(foreign_org)
            .bind::<SqlUuid, _>(owner)
            .bind::<Text, _>(if browser { "owner" } else { "reviewer" })
            .execute(&mut conn)
            .unwrap();
            (org, owner, foreign_org, foreign, peer, reviewer)
        };
        let listener = TcpListener::bind(("127.0.0.1", if browser { 18083 } else { 0 })).unwrap();
        let api = format!("http://{}", listener.local_addr().unwrap());
        let fixture = RunnerFixture::default();
        fixture.0.lock().unwrap().api = api.clone();
        fixture.0.lock().unwrap().automatic = browser;
        let data = web::Data::new(fixture.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(data.clone())
                .route(
                    "/health",
                    web::get().to(|| async {
                        HttpResponse::Ok()
                            .json(json!({"status":"ok","synthetic":true,"model_requests":0}))
                    }),
                )
                .default_service(web::post().to(fixture_handler))
        })
        .workers(1)
        .bind(("127.0.0.1", if browser { 18084 } else { 0 }))
        .unwrap();
        let runner_url = format!("http://{}", server.addrs()[0]);
        let server = server.run();
        let runner_server = server.handle();
        actix_web::rt::spawn(server);
        std::env::set_var("ASSISTANT_RUNNER_URL", runner_url);
        std::env::set_var("ASSISTANT_RUNNER_TOKEN", RUNNER_TOKEN);
        let state = AppState {
            pool: pool.clone(),
            key: [17; 32],
            demo: false,
            secure_cookie: false,
            public_url: api.clone(),
            allowed_origins: if browser {
                vec![api.clone(), "http://127.0.0.1:18085".into()]
            } else {
                vec![api.clone()]
            },
            media_root: std::env::temp_dir().join(format!("assistant-media-{suffix}")),
            throttle: Arc::new(Mutex::new(HashMap::new())),
            dummy_hash: auth::password_hash("Unusable-assistant-dummy").unwrap(),
        };
        let data = web::Data::new(state.clone());
        let server = HttpServer::new(move || {
            App::new()
                .app_data(data.clone())
                .configure(phone_api::configure)
        })
        .workers(4)
        .listen(listener)
        .unwrap()
        .run();
        let api_server = server.handle();
        actix_web::rt::spawn(server);
        Self {
            api,
            password,
            client: Client::builder()
                .timeout(StdDuration::from_secs(15))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            pool,
            state,
            fixture,
            api_server,
            runner_server,
            org,
            foreign_org,
            users: vec![owner, foreign, peer, reviewer],
            owner_email,
            peer_email,
            foreign_email,
            reviewer_email,
        }
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Value,
        session: Option<&Session>,
    ) -> Reply {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.api))
            .header("Origin", &self.api)
            .json(&body);
        if let Some(session) = session {
            request = request
                .header("Cookie", &session.cookie)
                .header("X-CSRF-Token", &session.csrf)
                .header("X-Organization-Id", session.org.to_string());
        }
        let response = request.send().await.expect("local API HTTP");
        let status = response.status().as_u16();
        let bytes = response.bytes().await.unwrap();
        let body = serde_json::from_slice(&bytes)
            .unwrap_or_else(|_| json!({"non_json_response":String::from_utf8_lossy(&bytes)}));
        Reply { status, body }
    }
    async fn login(&self, email: &str) -> Session {
        let response = self
            .client
            .post(format!("{}/api/auth/login", self.api))
            .header("Origin", &self.api)
            .json(&json!({"email":email,"password":self.password}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        let cookie = response.headers()["set-cookie"]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string();
        let body: Value = response.json().await.unwrap();
        let data = &body["data"];
        Session {
            cookie,
            csrf: data["csrf_token"].as_str().unwrap().into(),
            org: Uuid::parse_str(data["organization"]["id"].as_str().unwrap()).unwrap(),
            user: Uuid::parse_str(data["user"]["id"].as_str().unwrap()).unwrap(),
        }
    }
    async fn authorize(&self, session: &Session) {
        let value = self
            .request(
                Method::POST,
                "/api/assistant/auth/start",
                json!({"type":"apiKey","api_key":KEY_SENTINEL}),
                Some(session),
            )
            .await
            .ok();
        assert_eq!(value["authenticated"], true);
    }
    async fn conversation(&self, session: &Session) -> Uuid {
        let value = self
            .request(
                Method::POST,
                "/api/assistant/conversations",
                json!({"title":"Synthetic contract test","client_request_id":Uuid::new_v4()}),
                Some(session),
            )
            .await
            .ok();
        Uuid::parse_str(value["id"].as_str().expect("Conversation.id")).unwrap()
    }
    async fn run(&self, session: &Session, conversation: Uuid, refs: Vec<Uuid>) -> (Uuid, String) {
        let reply=self.request(Method::POST,&format!("/api/assistant/conversations/{conversation}/messages"),json!({"text":"Настрой синтетический тестовый тег","client_request_id":Uuid::new_v4(),"credential_refs":refs}),Some(session)).await;
        assert_eq!(reply.status, 202, "{}", reply.body);
        let run = Uuid::parse_str(reply.body["data"]["run"]["id"].as_str().unwrap()).unwrap();
        let private = self.fixture.grant(run).await;
        let deadline = tokio::time::Instant::now() + StdDuration::from_secs(8);
        loop {
            let current = self
                .request(
                    Method::GET,
                    &format!("/api/assistant/runs/{run}"),
                    json!({}),
                    Some(session),
                )
                .await
                .ok();
            if current["run"]["events"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["title"] == "Local protocol fixture; no inference")
            {
                break;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "Initial runner status was not consumed before concurrency barrier"
            );
            tokio::time::sleep(StdDuration::from_millis(10)).await;
        }
        (
            run,
            private["mcp_token"]
                .as_str()
                .expect("Private turn MCP capability")
                .into(),
        )
    }
    async fn tool(&self, token: &str, call_id: &str, name: &str, arguments: Value) -> Reply {
        let response = self
            .client
            .post(format!("{}/api/assistant/tools", self.api))
            .bearer_auth(token)
            .json(&json!({"method":"call","call_id":call_id,"name":name,"arguments":arguments}))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let body = response.json().await.unwrap_or(Value::Null);
        Reply { status, body }
    }
    async fn snapshot(&self, token: &str) -> Value {
        self.tool(
            token,
            &Uuid::new_v4().to_string(),
            "get_workspace_configuration",
            json!({}),
        )
        .await
        .ok()["result"]
            .clone()
    }
    async fn terminal(&self, session: &Session, run: Uuid) -> Value {
        let deadline = tokio::time::Instant::now() + StdDuration::from_secs(10);
        loop {
            let value = self
                .request(
                    Method::GET,
                    &format!("/api/assistant/runs/{run}"),
                    json!({}),
                    Some(session),
                )
                .await
                .ok();
            if !matches!(value["run"]["state"].as_str(), Some("queued" | "running")) {
                return value;
            }
            assert!(
                tokio::time::Instant::now() < deadline,
                "Run did not reach terminal state"
            );
            tokio::time::sleep(StdDuration::from_millis(10)).await;
        }
    }
    fn documents(&self, org: Uuid) -> Vec<Value> {
        #[derive(QueryableByName)]
        struct Row {
            #[diesel(sql_type=Jsonb)]
            data: Value,
        }
        let mut c = connection(&self.pool).unwrap();
        diesel::sql_query("SELECT data FROM documents WHERE organization_id=$1")
            .bind::<SqlUuid, _>(org)
            .load::<Row>(&mut c)
            .unwrap()
            .into_iter()
            .map(|r| r.data)
            .collect()
    }
    fn tag_count(&self, name: &str) -> usize {
        let mut c = connection(&self.pool).unwrap();
        persistence::list_documents(&mut c, self.org, "tag")
            .unwrap()
            .iter()
            .filter(|d| d.data["name"] == name)
            .count()
    }
    async fn cleanup(self) {
        self.fixture.0.lock().unwrap().streams.clear();
        self.api_server.stop(true).await;
        self.runner_server.stop(true).await;
        let mut c = connection(&self.pool).unwrap();
        for org in [self.org, self.foreign_org] {
            diesel::sql_query("DELETE FROM organizations WHERE id=$1")
                .bind::<SqlUuid, _>(org)
                .execute(&mut c)
                .unwrap();
        }
        for user in self.users {
            diesel::sql_query("DELETE FROM users WHERE id=$1")
                .bind::<SqlUuid, _>(user)
                .execute(&mut c)
                .unwrap();
        }
        std::env::remove_var("ASSISTANT_RUNNER_URL");
        std::env::remove_var("ASSISTANT_RUNNER_TOKEN");
    }
}
fn assert_no_secrets(value: &Value) {
    let text = value.to_string();
    for secret in [KEY_SENTINEL, TELPHIN_SENTINEL] {
        assert!(
            !text.contains(secret),
            "Secret reflected into a public/model projection"
        );
    }
}
fn revision(snapshot: &Value) -> i64 {
    snapshot["revision"]
        .as_i64()
        .expect("Workspace/result must expose integer revision")
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; no live providers"]
async fn assistant_scope_secret_privacy_and_atomic_configuration_receipts() {
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    let peer = h.login(&h.peer_email).await;
    let foreign = h.login(&h.foreign_email).await;
    let reviewer = h.login(&h.reviewer_email).await;
    h.request(Method::GET, "/api/assistant/conversations", json!({}), None)
        .await
        .denied();
    h.request(
        Method::GET,
        "/api/assistant/conversations",
        json!({}),
        Some(&reviewer),
    )
    .await
    .denied();
    let initial = h
        .request(
            Method::GET,
            "/api/assistant/status",
            json!({}),
            Some(&owner),
        )
        .await
        .ok();
    assert_eq!(initial["authenticated"], false);
    let csrf = h
        .client
        .post(format!("{}/api/assistant/conversations", h.api))
        .header("Origin", &h.api)
        .header("Cookie", &owner.cookie)
        .json(&json!({"client_request_id":Uuid::new_v4()}))
        .send()
        .await
        .unwrap();
    assert_eq!(csrf.status().as_u16(), 403);
    let origin = h
        .client
        .post(format!("{}/api/assistant/conversations", h.api))
        .header("Origin", "https://evil.example")
        .header("Cookie", &owner.cookie)
        .header("X-CSRF-Token", &owner.csrf)
        .json(&json!({"client_request_id":Uuid::new_v4()}))
        .send()
        .await
        .unwrap();
    assert_eq!(origin.status().as_u16(), 403);
    let unauthenticated_conversation = h.conversation(&owner).await;
    let missing = h
        .request(
            Method::POST,
            &format!("/api/assistant/conversations/{unauthenticated_conversation}/messages"),
            json!({"text":"Model auth is deliberately absent","client_request_id":Uuid::new_v4()}),
            Some(&owner),
        )
        .await;
    assert_eq!(missing.status, 409);
    assert_eq!(missing.body["error"]["code"], "assistant_auth_required");
    let empty = h
        .request(
            Method::GET,
            &format!("/api/assistant/conversations/{unauthenticated_conversation}"),
            json!({}),
            Some(&owner),
        )
        .await
        .ok();
    assert!(empty["messages"].as_array().unwrap().is_empty());
    assert!(empty["runs"].as_array().unwrap().is_empty());
    h.authorize(&owner).await;
    let create_key = Uuid::new_v4();
    let create = json!({"title":"Receipt test","client_request_id":create_key});
    let first = h
        .request(
            Method::POST,
            "/api/assistant/conversations",
            create.clone(),
            Some(&owner),
        )
        .await
        .ok();
    let conversation = Uuid::parse_str(first["id"].as_str().unwrap()).unwrap();
    let repeated = h
        .request(
            Method::POST,
            "/api/assistant/conversations",
            create,
            Some(&owner),
        )
        .await
        .ok();
    assert_eq!(repeated["id"], first["id"]);
    assert_eq!(
        h.request(
            Method::POST,
            "/api/assistant/conversations",
            json!({"title":"Changed request","client_request_id":create_key}),
            Some(&owner)
        )
        .await
        .status,
        409
    );
    let detail_path = format!("/api/assistant/conversations/{conversation}");
    for other in [&peer, &foreign, &reviewer] {
        h.request(Method::GET, &detail_path, json!({}), Some(other))
            .await
            .denied();
    }
    let credentials=h.request(Method::POST,&format!("{detail_path}/credentials"),json!({"kind":"telphin","label":"Synthetic ATS access","config":{"host":"apiproxy.telphin.ru","app_id":"synthetic-app-id","app_secret":TELPHIN_SENTINEL,"client_id":"42"}}),Some(&owner)).await.ok();
    assert_no_secrets(&credentials);
    assert!(credentials.get("config").is_none());
    let credential = Uuid::parse_str(credentials["id"].as_str().unwrap()).unwrap();
    for other in [&peer, &foreign] {
        h.request(
            Method::DELETE,
            &format!("{detail_path}/credentials/{credential}"),
            json!({}),
            Some(other),
        )
        .await
        .denied();
    }
    let refused=h.request(Method::POST,&format!("{detail_path}/messages"),json!({"text":format!("app_secret={TELPHIN_SENTINEL}"),"client_request_id":Uuid::new_v4()}),Some(&owner)).await;
    refused.denied();
    assert_no_secrets(&refused.body);
    let peer_conversation = h.conversation(&peer).await;
    h.request(Method::POST,&format!("/api/assistant/conversations/{peer_conversation}/messages"),json!({"text":"Use a foreign owner's credentials","client_request_id":Uuid::new_v4(),"credential_refs":[credential]}),Some(&peer)).await.denied();
    let another = h.conversation(&owner).await;
    h.request(Method::POST,&format!("/api/assistant/conversations/{another}/messages"),json!({"text":"Use another conversation's credentials","client_request_id":Uuid::new_v4(),"credential_refs":[credential]}),Some(&owner)).await.denied();
    let (run, token) = h.run(&owner, conversation, vec![credential]).await;
    let turn = h.fixture.grant(run).await;
    let raw_session = owner.cookie.split_once('=').unwrap().1;
    assert!(
        !turn.to_string().contains(raw_session),
        "Browser session must never be forwarded to the runner"
    );
    assert!(
        !json!(h.documents(h.org)).to_string().contains(&token),
        "Opaque capability must be stored hash-only"
    );
    assert_no_secrets(&turn["history"]);
    assert_no_secrets(&turn["credential_refs"]);
    let list = h
        .client
        .post(format!("{}/api/assistant/tools", h.api))
        .bearer_auth(&token)
        .json(&json!({"method":"list"}))
        .send()
        .await
        .unwrap();
    assert_eq!(list.status().as_u16(), 200);
    let tools: Value = list.json().await.unwrap();
    let names = tools["data"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        std::collections::BTreeSet::from([
            "get_workspace_configuration",
            "connect_telphin",
            "test_telphin_connection",
            "sync_telphin",
            "upsert_taxonomy",
            "upsert_filter",
            "save_rubric",
            "set_callback_rules"
        ])
    );
    h.tool(
        &token,
        "unknown-tool",
        "execute_sql",
        json!({"sql":"SELECT 1"}),
    )
    .await
    .denied();
    h.tool(
        &token,
        "extra-identity",
        "get_workspace_configuration",
        json!({"organization_id":h.foreign_org}),
    )
    .await
    .denied();
    let before = h.snapshot(&token).await;
    assert_no_secrets(&before);
    let connect_args = json!({"credential_ref":credential,"expected_revision":revision(&before)});
    let connected = h
        .tool(
            &token,
            "connect-once",
            "connect_telphin",
            connect_args.clone(),
        )
        .await
        .ok()["result"]
        .clone();
    assert!(revision(&connected) > revision(&before));
    assert_no_secrets(&connected);
    let replay = h
        .tool(&token, "connect-once", "connect_telphin", connect_args)
        .await
        .ok()["result"]
        .clone();
    assert_eq!(
        replay, connected,
        "Committed receipt replay must precede new CAS checks"
    );
    assert_eq!(
        h.tool(
            &token,
            "connect-once",
            "connect_telphin",
            json!({"credential_ref":credential,"expected_revision":revision(&connected)})
        )
        .await
        .status,
        409
    );
    if let Some(doc) =
        persistence::get_document(&mut connection(&h.pool).unwrap(), h.org, credential).unwrap()
    {
        assert!(
            doc.data.get("config_encrypted").is_none() && doc.data.get("ciphertext").is_none(),
            "Consumed temporary credential must lose its ciphertext"
        );
    }
    let tag_name = format!("Assistant tag {run}");
    let tag_args = json!({"expected_revision":revision(&connected),"kind":"tag","name":tag_name,"color":"#0d9488"});
    let created = h
        .tool(
            &token,
            "create-tag-once",
            "upsert_taxonomy",
            tag_args.clone(),
        )
        .await
        .ok()["result"]
        .clone();
    let replay = h
        .tool(&token, "create-tag-once", "upsert_taxonomy", tag_args)
        .await
        .ok()["result"]
        .clone();
    assert_eq!(created, replay);
    assert_eq!(h.tag_count(&tag_name), 1);
    let tag = {
        let mut c = connection(&h.pool).unwrap();
        persistence::list_documents(&mut c, h.org, "tag")
            .unwrap()
            .into_iter()
            .find(|d| d.data["name"] == tag_name)
            .unwrap()
    };
    h.request(
        Method::PATCH,
        &format!("/api/taxonomy/{}", tag.id),
        json!({"name":"Manual owner change"}),
        Some(&peer),
    )
    .await
    .ok();
    let stale_args = json!({"expected_revision":revision(&created),"kind":"tag","name":"Stale AI overwrite","id":tag.id,"version":tag.version});
    assert_eq!(
        h.tool(&token, "stale-config", "upsert_taxonomy", stale_args)
            .await
            .status,
        409
    );
    let after_manual = h.snapshot(&token).await;
    assert!(
        revision(&after_manual) > revision(&created),
        "HTTP settings mutations must bump the same revision used by MCP"
    );
    assert_eq!(h.tool(&token,"missing-row-version","upsert_taxonomy",json!({"expected_revision":revision(&after_manual),"kind":"tag","name":"Blind overwrite","id":tag.id})).await.status,409);
    let policy_revision = revision(&after_manual);
    h.tool(&token,"invalid-filter","upsert_filter",json!({"expected_revision":policy_revision,"name":"Bad filter","ast":{"field":"arbitrary_sql","op":"eq","value":"x"}})).await.denied();
    h.tool(&token,"invalid-calendar","set_callback_rules",json!({"expected_revision":policy_revision,"working_hours":{"enabled":true,"start":"09:00","end":"18:00","days":[8]}})).await.denied();
    assert_eq!(
        revision(&h.snapshot(&token).await),
        policy_revision,
        "Rejected mutations must not commit revision or configuration"
    );
    let valid_filter=h.tool(&token,"create-filter","upsert_filter",json!({"expected_revision":policy_revision,"name":"Overdue calls","ast":{"op":"and","rules":[{"field":"callback_state","op":"eq","value":"overdue"},{"field":"direction","op":"eq","value":"in"}]}})).await.ok()["result"].clone();
    assert!(revision(&valid_filter) > policy_revision);
    let valid_policy=h.tool(&token,"calendar-valid","set_callback_rules",json!({"expected_revision":revision(&valid_filter),"timezone":"Europe/Moscow","callback_minutes":120,"callback_min_duration":10})).await.ok()["result"].clone();
    let rubric=h.tool(&token,"rubric-valid","save_rubric",json!({"expected_revision":revision(&valid_policy),"name":"Greeting evidence","criteria":[{"id":"greeting","name":"Приветствие","weight":1,"required":true,"instruction":"Нужна точная цитата"}],"activate":true})).await.ok()["result"].clone();
    h.tool(&token,"sync-boundary-guard","sync_telphin",json!({"expected_revision":revision(&rubric),"all_available":true,"history_start_verified":true})).await.denied();
    // Metadata import is only enqueued; no worker/live provider is attached to this test DB.
    h.tool(&token,"sync-request","sync_telphin",json!({"expected_revision":revision(&rubric),"from":"2026-01-01","to":"2026-01-02","all_available":false})).await.ok();
    let large_docs = [Uuid::new_v4(), Uuid::new_v4()];
    {
        let mut c = connection(&h.pool).unwrap();
        for (index, id) in large_docs.into_iter().enumerate() {
            let criteria=(0..30).map(|n|json!({"id":format!("criterion-{n}"),"name":"Criterion","weight":1,"required":false,"instruction":"x".repeat(4000)})).collect::<Vec<_>>();
            persistence::save_document(&mut c,h.org,id,"rubric",None,json!({"name":format!("Large legitimate rubric {index}"),"rubric_version":100+index,"criteria":criteria,"active":false}),None).unwrap();
        }
    }
    let bounded = h
        .tool(
            &token,
            "bounded-workspace",
            "get_workspace_configuration",
            json!({}),
        )
        .await;
    assert_eq!(bounded.status,413,"Oversized configuration must be an explicit bounded error, not a partial successful snapshot");
    assert!(bounded.body.to_string().len() < 2048);
    {
        let mut c = connection(&h.pool).unwrap();
        for id in large_docs {
            persistence::delete_document(&mut c, h.org, id).unwrap();
        }
    }
    // A compact configuration can exceed the item limit without reaching the byte
    // limit. Never present a silently truncated list as a complete workspace.
    let count_docs = (0..101).map(|_| Uuid::new_v4()).collect::<Vec<_>>();
    {
        let mut c = connection(&h.pool).unwrap();
        for (index, id) in count_docs.iter().enumerate() {
            persistence::save_document(
                &mut c,
                h.org,
                *id,
                "status",
                None,
                json!({"name":format!("Compact status {index}"),"archived":false}),
                None,
            )
            .unwrap();
        }
    }
    let counted = h
        .tool(
            &token,
            "bounded-workspace-count",
            "get_workspace_configuration",
            json!({}),
        )
        .await;
    assert_eq!(counted.status, 413, "Too many compact settings must fail explicitly instead of silently losing the remaining items");
    assert!(counted.body.to_string().len() < 2048);
    assert_eq!(counted.body["error"]["code"], "assistant_context_too_large");
    let replayed_count = h
        .tool(
            &token,
            "bounded-workspace-count",
            "get_workspace_configuration",
            json!({}),
        )
        .await;
    assert_eq!(replayed_count.status, counted.status);
    assert_eq!(replayed_count.body, counted.body, "Failed receipt replay must retain its safe error code");
    {
        let mut c = connection(&h.pool).unwrap();
        for id in count_docs {
            persistence::delete_document(&mut c, h.org, id).unwrap();
        }
    }
    let private = h.fixture.grant(run).await;
    assert_no_secrets(&private["history"]);
    assert_no_secrets(&private["credential_refs"]);
    h.fixture.finish(run,json!({"type":"completed","text":"Синтетическая конфигурация сохранена.","model":"fixture-only","usage":{"input_tokens":0,"output_tokens":0}}));
    let terminal = h.terminal(&owner, run).await;
    assert_eq!(terminal["run"]["state"], "completed");
    assert!(
        terminal["run"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["type"] == "status")
            .all(|event| event["status"] == "completed"),
        "Completed turn must not label its prior progress statuses interrupted/uncertain"
    );
    assert_eq!(terminal["run"]["model"], "fixture-only");
    assert_eq!(terminal["run"]["usage"]["input_tokens"], 0);
    assert_eq!(terminal["run"]["usage"]["output_tokens"], 0);
    assert_no_secrets(&terminal);
    let detail = h
        .request(Method::GET, &detail_path, json!({}), Some(&owner))
        .await
        .ok();
    assert_no_secrets(&detail);
    let roles = detail["messages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(roles,vec!["user","assistant"],"Conversation history must preserve logical order even when both messages share DB timestamp");
    assert!(
        terminal["run"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["type"] == "tool" && e["status"] == "completed"),
        "Applied changes require durable tool results"
    );
    for document in h.documents(h.org) {
        assert_no_secrets(&document);
    }
    h.request(Method::DELETE, &detail_path, json!({}), Some(&owner))
        .await
        .ok();
    h.request(Method::GET, &detail_path, json!({}), Some(&owner))
        .await
        .denied();
    h.cleanup().await;
}

struct DbBarrier(Option<persistence::Connection>);
impl DbBarrier {
    fn hold_org(pool: &Pool, org: Uuid) -> Self {
        use diesel::connection::SimpleConnection;
        let mut c = connection(pool).unwrap();
        c.batch_execute("BEGIN").unwrap();
        diesel::sql_query("SELECT id FROM organizations WHERE id=$1 FOR NO KEY UPDATE")
            .bind::<SqlUuid, _>(org)
            .execute(&mut c)
            .unwrap();
        Self(Some(c))
    }
    fn hold_document(pool: &Pool, org: Uuid, id: Uuid) -> Self {
        use diesel::connection::SimpleConnection;
        let mut c = connection(pool).unwrap();
        c.batch_execute("BEGIN").unwrap();
        diesel::sql_query("SELECT id FROM documents WHERE organization_id=$1 AND id=$2 FOR UPDATE")
            .bind::<SqlUuid, _>(org)
            .bind::<SqlUuid, _>(id)
            .execute(&mut c)
            .unwrap();
        Self(Some(c))
    }
    fn release(mut self) {
        use diesel::connection::SimpleConnection;
        self.0.take().unwrap().batch_execute("COMMIT").unwrap();
    }
}
impl Drop for DbBarrier {
    fn drop(&mut self) {
        use diesel::connection::SimpleConnection;
        if let Some(c) = &mut self.0 {
            let _ = c.batch_execute("ROLLBACK");
        }
    }
}
async fn waiting_locks(pool: &Pool, minimum: i32) {
    #[derive(QueryableByName)]
    struct Count {
        #[diesel(sql_type=Integer)]
        count: i32,
    }
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(8);
    loop {
        let mut c = connection(pool).unwrap();
        let count=diesel::sql_query("SELECT count(*)::integer AS count FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND pid<>pg_backend_pid()").get_result::<Count>(&mut c).unwrap().count;
        if count >= minimum {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Expected real PostgreSQL lock wait barrier was not reached"
        );
        tokio::time::sleep(StdDuration::from_millis(10)).await;
    }
}
fn spawn_tool(h: &Harness, token: &str, call: &str, args: Value) -> tokio::task::JoinHandle<Reply> {
    let client = h.client.clone();
    let url = format!("{}/api/assistant/tools", h.api);
    let token = token.to_owned();
    let call = call.to_owned();
    tokio::spawn(async move {
        let response = client
            .post(url)
            .bearer_auth(token)
            .json(
                &json!({"method":"call","call_id":call,"name":"upsert_taxonomy","arguments":args}),
            )
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        Reply {
            status,
            body: response.json().await.unwrap_or(Value::Null),
        }
    })
}
fn spawn_cancel(h: &Harness, session: &Session, run: Uuid) -> tokio::task::JoinHandle<Reply> {
    let client = h.client.clone();
    let api = h.api.clone();
    let session = session.clone();
    tokio::spawn(async move {
        let response = client
            .post(format!("{api}/api/assistant/runs/{run}/cancel"))
            .header("Origin", &api)
            .header("Cookie", session.cookie)
            .header("X-CSRF-Token", session.csrf)
            .header("X-Organization-Id", session.org.to_string())
            .json(&json!({}))
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        Reply {
            status,
            body: response.json().await.unwrap_or(Value::Null),
        }
    })
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; real lock races, no live providers"]
async fn assistant_write_races_recheck_cancel_deleted_session_and_membership() {
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    h.authorize(&owner).await;
    // Queue cancel first under the common org barrier, then the tool. Cancel's
    // confirmed revocation must win even if the tool looked up authority before waiting.
    let conversation = h.conversation(&owner).await;
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    let snapshot = h.snapshot(&token).await;
    let blocked = DbBarrier::hold_org(&h.pool, h.org);
    let name = "Cancel must win against stale pre-lock authority";
    let cancellation = spawn_cancel(&h, &owner, run);
    waiting_locks(&h.pool, 1).await;
    let tool = spawn_tool(
        &h,
        &token,
        "cancel-race",
        json!({"expected_revision":revision(&snapshot),"kind":"tag","name":name}),
    );
    waiting_locks(&h.pool, 2).await;
    blocked.release();
    let cancelled = cancellation.await.unwrap().ok();
    assert_eq!(cancelled["run"]["state"], "cancelled");
    tool.await.unwrap().denied();
    assert_eq!(h.tag_count(name), 0);
    h.tool(
        &token,
        "cancel-race",
        "upsert_taxonomy",
        json!({"expected_revision":revision(&snapshot),"kind":"tag","name":name}),
    )
    .await
    .denied();
    // The opposite serialization is allowed: committed mutation remains visible after cancellation.
    let conversation = h.conversation(&owner).await;
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    let snapshot = h.snapshot(&token).await;
    let blocked = DbBarrier::hold_document(&h.pool, h.org, run);
    let name = "Committed action must survive cancellation";
    let tool = spawn_tool(
        &h,
        &token,
        "commit-before-cancel",
        json!({"expected_revision":revision(&snapshot),"kind":"tag","name":name}),
    );
    waiting_locks(&h.pool, 1).await;
    let cancellation = spawn_cancel(&h, &owner, run);
    waiting_locks(&h.pool, 2).await;
    blocked.release();
    tool.await.unwrap().ok();
    let result = cancellation.await.unwrap().ok();
    assert_eq!(result["run"]["state"], "cancelled");
    assert_eq!(h.tag_count(name), 1);
    let persisted = h
        .request(
            Method::GET,
            &format!("/api/assistant/runs/{run}"),
            json!({}),
            Some(&owner),
        )
        .await
        .ok();
    assert!(persisted["run"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["tool_name"] == "upsert_taxonomy" && e["status"] == "completed"));
    // Cancel owns conversation while tool owns the org lock: audit's implicit FK lock
    // must not create org->conversation / conversation->org inversion.
    let conversation = h.conversation(&owner).await;
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    let snapshot = h.snapshot(&token).await;
    let blocked = DbBarrier::hold_document(&h.pool, h.org, run);
    let name = "Cancel audit FK cannot deadlock tool authority";
    let cancellation = spawn_cancel(&h, &owner, run);
    waiting_locks(&h.pool, 1).await;
    let tool = spawn_tool(
        &h,
        &token,
        "cancel-fk-order",
        json!({"expected_revision":revision(&snapshot),"kind":"tag","name":name}),
    );
    waiting_locks(&h.pool, 2).await;
    blocked.release();
    let cancelled = cancellation.await.unwrap().ok();
    assert_eq!(cancelled["run"]["state"], "cancelled");
    tool.await.unwrap().denied();
    assert_eq!(h.tag_count(name), 0);
    // Original browser logout must be rechecked after the blocked tool takes its locks.
    let conversation = h.conversation(&owner).await;
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    let snapshot = h.snapshot(&token).await;
    let blocked = DbBarrier::hold_org(&h.pool, h.org);
    let name = "Logged-out session cannot configure";
    let tool = spawn_tool(
        &h,
        &token,
        "logout-race",
        json!({"expected_revision":revision(&snapshot),"kind":"tag","name":name}),
    );
    waiting_locks(&h.pool, 1).await;
    h.request(Method::POST, "/api/auth/logout", json!({}), Some(&owner))
        .await
        .ok();
    blocked.release();
    tool.await.unwrap().denied();
    assert_eq!(h.tag_count(name), 0);
    h.fixture.finish(
        run,
        json!({"type":"error","code":"fixture_authority_revoked"}),
    );
    // Membership role changes on another DB connection cannot be missed by a cached Context.
    let owner = h.login(&h.owner_email).await;
    let conversation = h.conversation(&owner).await;
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    let snapshot = h.snapshot(&token).await;
    let blocked = DbBarrier::hold_org(&h.pool, h.org);
    let name = "Downgraded owner cannot configure";
    let tool = spawn_tool(
        &h,
        &token,
        "membership-race",
        json!({"expected_revision":revision(&snapshot),"kind":"tag","name":name}),
    );
    waiting_locks(&h.pool, 1).await;
    {
        let mut c = connection(&h.pool).unwrap();
        diesel::sql_query(
            "UPDATE memberships SET role='manager' WHERE organization_id=$1 AND user_id=$2",
        )
        .bind::<SqlUuid, _>(h.org)
        .bind::<SqlUuid, _>(owner.user)
        .execute(&mut c)
        .unwrap();
    }
    blocked.release();
    tool.await.unwrap().denied();
    assert_eq!(h.tag_count(name), 0);
    h.fixture.finish(
        run,
        json!({"type":"error","code":"fixture_authority_revoked"}),
    );
    {
        let mut c = connection(&h.pool).unwrap();
        diesel::sql_query(
            "UPDATE memberships SET role='owner' WHERE organization_id=$1 AND user_id=$2",
        )
        .bind::<SqlUuid, _>(h.org)
        .bind::<SqlUuid, _>(owner.user)
        .execute(&mut c)
        .unwrap();
    }
    // A company switch deletes the original session; retaining its capability cannot follow it.
    let owner = h.login(&h.owner_email).await;
    let conversation = h.conversation(&owner).await;
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    let snapshot = h.snapshot(&token).await;
    let blocked = DbBarrier::hold_org(&h.pool, h.org);
    let name = "Company switch cannot reuse old authority";
    let tool = spawn_tool(
        &h,
        &token,
        "switch-race",
        json!({"expected_revision":revision(&snapshot),"kind":"tag","name":name}),
    );
    waiting_locks(&h.pool, 1).await;
    h.request(
        Method::POST,
        "/api/auth/organization",
        json!({"organization_id":h.foreign_org}),
        Some(&owner),
    )
    .await
    .ok();
    blocked.release();
    tool.await.unwrap().denied();
    assert_eq!(h.tag_count(name), 0);
    h.fixture.finish(
        run,
        json!({"type":"error","code":"fixture_authority_revoked"}),
    );
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; delayed auth protocol only"]
async fn assistant_auth_generation_never_promotes_a_stale_or_revoked_owner_attempt() {
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    let peer = h.login(&h.peer_email).await;
    h.authorize(&owner).await;
    let conversation = h.conversation(&owner).await;
    let (active_run, active_token) = h.run(&owner, conversation, vec![]).await;
    h.request(
        Method::POST,
        "/api/assistant/auth/logout",
        json!({}),
        Some(&owner),
    )
    .await
    .ok();
    let revoked_run = h
        .request(
            Method::GET,
            &format!("/api/assistant/runs/{active_run}"),
            json!({}),
            Some(&owner),
        )
        .await
        .ok();
    assert_eq!(revoked_run["run"]["state"], "cancelled");
    h.tool(
        &active_token,
        "model-auth-revoked",
        "get_workspace_configuration",
        json!({}),
    )
    .await
    .denied();
    let start = h
        .request(
            Method::POST,
            "/api/assistant/auth/start",
            json!({"type":"chatgptDeviceCode"}),
            Some(&owner),
        )
        .await
        .ok();
    assert!(start["pending_login"].is_object());
    let old = h.fixture.last_attempt();
    let peer_status = h
        .request(Method::GET, "/api/assistant/status", json!({}), Some(&peer))
        .await
        .ok();
    assert!(
        peer_status["pending_login"].is_null(),
        "Other owner must not receive a device login code"
    );
    h.request(Method::POST, "/api/auth/logout", json!({}), Some(&owner))
        .await
        .ok();
    let (status, body) = h.fixture.complete(&old).await;
    assert!(
        (400..500).contains(&status),
        "Revoked browser session completed auth: {body}"
    );
    let state = h
        .request(Method::GET, "/api/assistant/status", json!({}), Some(&peer))
        .await
        .ok();
    assert_eq!(state["authenticated"], false);
    let owner = h.login(&h.owner_email).await;
    h.request(
        Method::POST,
        "/api/assistant/auth/start",
        json!({"type":"chatgptDeviceCode"}),
        Some(&owner),
    )
    .await
    .ok();
    let first = h.fixture.last_attempt();
    h.request(
        Method::POST,
        "/api/assistant/auth/start",
        json!({"type":"chatgptDeviceCode"}),
        Some(&owner),
    )
    .await
    .ok();
    let second = h.fixture.last_attempt();
    assert_ne!(first["auth_attempt_id"], second["auth_attempt_id"]);
    assert!(second["generation"].as_i64().unwrap() > first["generation"].as_i64().unwrap());
    let (status, _) = h.fixture.complete(&first).await;
    assert!(
        (400..500).contains(&status),
        "Old completion must not overwrite a new auth generation"
    );
    h.request(
        Method::POST,
        "/api/assistant/auth/logout",
        json!({}),
        Some(&owner),
    )
    .await
    .ok();
    let (status, _) = h.fixture.complete(&second).await;
    assert!(
        (400..500).contains(&status),
        "Late completion restored logged-out model auth"
    );
    h.request(
        Method::POST,
        "/api/assistant/auth/start",
        json!({"type":"chatgptDeviceCode"}),
        Some(&owner),
    )
    .await
    .ok();
    let revoked = h.fixture.last_attempt();
    {
        let mut c = connection(&h.pool).unwrap();
        diesel::sql_query(
            "UPDATE memberships SET role='manager' WHERE organization_id=$1 AND user_id=$2",
        )
        .bind::<SqlUuid, _>(h.org)
        .bind::<SqlUuid, _>(owner.user)
        .execute(&mut c)
        .unwrap();
    }
    let (status, _) = h.fixture.complete(&revoked).await;
    assert!(
        (400..500).contains(&status),
        "A downgraded owner promoted staged credentials"
    );
    let state = h
        .request(Method::GET, "/api/assistant/status", json!({}), Some(&peer))
        .await
        .ok();
    assert_eq!(state["authenticated"], false);
    assert_no_secrets(&state);
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; controlled wire, no model"]
async fn assistant_durable_partials_submission_replay_and_malformed_stream_fail_closed() {
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    h.authorize(&owner).await;
    let conversation = h.conversation(&owner).await;
    let path = format!("/api/assistant/conversations/{conversation}/messages");
    let key = Uuid::new_v4();
    let body = json!({"text":"Сохрани фактический частичный ответ","client_request_id":key});
    let submitted = h
        .request(Method::POST, &path, body.clone(), Some(&owner))
        .await;
    assert_eq!(submitted.status, 202);
    let run = Uuid::parse_str(submitted.body["data"]["run"]["id"].as_str().unwrap()).unwrap();
    h.fixture.grant(run).await;
    let replay = h.request(Method::POST, &path, body, Some(&owner)).await;
    assert_eq!(replay.status, 202);
    assert_eq!(replay.body["data"]["run"]["id"], run.to_string());
    assert_eq!(
        h.request(
            Method::POST,
            &path,
            json!({"text":"Другое тело того же ключа","client_request_id":key}),
            Some(&owner)
        )
        .await
        .status,
        409
    );
    assert_eq!(
        h.request(
            Method::POST,
            &path,
            json!({"text":"Второй одновременный turn","client_request_id":Uuid::new_v4()}),
            Some(&owner)
        )
        .await
        .status,
        409
    );
    h.fixture.emit(
        run,
        json!({"type":"delta","text":"Сохранённая часть синтетического ответа"}),
    );
    let deadline = tokio::time::Instant::now() + StdDuration::from_secs(8);
    loop {
        let snapshot = h
            .request(
                Method::GET,
                &format!("/api/assistant/runs/{run}"),
                json!({}),
                Some(&owner),
            )
            .await
            .ok();
        if snapshot["messages"].as_array().unwrap().iter().any(|m| {
            m["role"] == "assistant"
                && m["text"]
                    .as_str()
                    .is_some_and(|s| s.contains("Сохранённая часть"))
        }) {
            assert!(matches!(
                snapshot["run"]["state"].as_str(),
                Some("running" | "queued")
            ));
            break;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "Partial assistant text was not persisted while stream was active"
        );
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    h.fixture.close(run);
    let interrupted = h.terminal(&owner, run).await;
    assert!(matches!(
        interrupted["run"]["state"].as_str(),
        Some("failed" | "interrupted")
    ));
    assert!(
        interrupted["run"]["error_code"].is_string(),
        "EOF without terminal event must expose an error"
    );
    for (label, event) in [
        (
            "unknown",
            json!({"type":"unknown_future_success","text":"Must not succeed"}),
        ),
        (
            "wrong-shape",
            json!({"type":"delta","text":{"not":"a string"}}),
        ),
        (
            "oversized",
            json!({"type":"delta","text":"x".repeat(2*1024*1024)}),
        ),
    ] {
        let conversation = h.conversation(&owner).await;
        let (run, _) = h.run(&owner, conversation, vec![]).await;
        h.fixture.finish(run, event);
        let failed = h.terminal(&owner, run).await;
        assert!(
            matches!(
                failed["run"]["state"].as_str(),
                Some("failed" | "interrupted")
            ),
            "{label} stream became success: {failed}"
        );
        assert!(failed["run"]["error_code"].is_string());
        assert_eq!(failed["run"]["error_code"], "assistant_protocol_error", "{label} must retain protocol failure instead of becoming a success or a generic startup error");
        assert!(failed["run"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|event| event["type"] == "status")
            .all(|event| event["status"] == "failed"));
        assert_no_secrets(&failed);
    }
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; startup protocol diagnostics, no model"]
async fn assistant_runner_startup_status_deduplication_and_safe_error_codes() {
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    h.authorize(&owner).await;
    for (wire_code, public_code) in [
        ("unexpected_codex_event", "assistant_protocol_incompatible"),
        ("forbidden_codex_item", "assistant_protocol_incompatible"),
        ("unsupported_runtime_request", "assistant_protocol_incompatible"),
        ("unsafe_codex_config", "assistant_protocol_incompatible"),
        ("unsafe_codex_environment", "assistant_protocol_incompatible"),
        ("codex_request_failed", "assistant_codex_request_failed"),
        ("codex_turn_failed", "assistant_codex_turn_failed"),
        ("codex_request_timeout", "assistant_codex_timeout"),
        ("untrusted_future_error", "assistant_runtime_failed"),
    ] {
        let conversation = h.conversation(&owner).await;
        let (run, _) = h.run(&owner, conversation, vec![]).await;
        for _ in 0..3 {
            h.fixture.emit(run, json!({"type":"status","message":"Подключаем Codex"}));
        }
        h.fixture.emit(run, json!({"type":"status","message":"Проверяем конфигурацию"}));
        // Repeated titles separated by a different status are genuine progress,
        // unlike an exact consecutive duplicate notification.
        h.fixture.emit(run, json!({"type":"status","message":"Подключаем Codex"}));
        h.fixture.emit(run, json!({"type":"error","code":wire_code,"stage":"config/read","message":KEY_SENTINEL,"diagnostics":TELPHIN_SENTINEL}));
        h.fixture.finish(run, json!({"type":"error","code":"untrusted_secondary_cleanup_error"}));
        let failed = h.terminal(&owner, run).await;
        assert_eq!(failed["run"]["state"], "failed", "{wire_code}: {failed}");
        assert_eq!(failed["run"]["error_code"], public_code, "Original safe error code must survive secondary cleanup/EOF: {wire_code}");
        let events = failed["run"]["events"].as_array().unwrap();
        assert_eq!(events.iter().filter(|event| event["type"] == "status" && event["title"] == "Подключаем Codex").count(), 2, "Exact consecutive connecting statuses should collapse, while later genuine progress remains");
        assert_eq!(events.iter().filter(|event| event["type"] == "status" && event["title"] == "Проверяем конфигурацию").count(), 1);
        assert!(events.iter().filter(|event| event["type"] == "status").all(|event| event["status"] == "failed"), "Failed startup statuses must be failed, not uncertain: {events:?}");
        assert!(events.iter().all(|event| event["type"] != "tool"), "Startup failure must not invent a tool action");
        assert!(failed["messages"].as_array().unwrap().iter().filter(|message| message["role"] == "assistant").all(|message| message["status"] == "failed"));
        assert_no_secrets(&failed);
        let detail = h.request(Method::GET, &format!("/api/assistant/conversations/{conversation}"), json!({}), Some(&owner)).await.ok();
        assert_no_secrets(&detail);
    }
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; actual physical expiry cleanup"]
async fn assistant_maintenance_erases_expired_credentials_and_revokes_abandoned_authority() {
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    h.authorize(&owner).await;
    let conversation = h.conversation(&owner).await;
    let credential=h.request(Method::POST,&format!("/api/assistant/conversations/{conversation}/credentials"),json!({"kind":"telphin","config":{"app_id":"synthetic","app_secret":TELPHIN_SENTINEL,"client_id":"42"}}),Some(&owner)).await.ok();
    let id = Uuid::parse_str(credential["id"].as_str().unwrap()).unwrap();
    {
        let mut c = connection(&h.pool).unwrap();
        let mut d = persistence::get_document(&mut c, h.org, id)
            .unwrap()
            .unwrap();
        d.data["expires_at"] = json!(Utc::now() - Duration::hours(1));
        persistence::save_document(
            &mut c,
            h.org,
            id,
            &d.kind,
            d.parent_id,
            d.data,
            Some(d.version),
        )
        .unwrap();
    }
    phone_api::assistant::maintenance(&h.state).await.unwrap();
    {
        let mut c = connection(&h.pool).unwrap();
        assert!(
            persistence::get_document(&mut c, h.org, id)
                .unwrap()
                .is_none(),
            "Expiry must physically remove credential ciphertext, not only deny lookups"
        );
        assert!(persistence::is_tombstoned(&mut c, h.org, id, None).unwrap());
    }
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    {
        let mut c = connection(&h.pool).unwrap();
        let mut d = persistence::get_document(&mut c, h.org, run)
            .unwrap()
            .unwrap();
        d.data["expires_at"] = json!(Utc::now() - Duration::minutes(1));
        persistence::save_document(
            &mut c,
            h.org,
            run,
            &d.kind,
            d.parent_id,
            d.data,
            Some(d.version),
        )
        .unwrap();
    }
    phone_api::assistant::maintenance(&h.state).await.unwrap();
    let state = h
        .request(
            Method::GET,
            &format!("/api/assistant/runs/{run}"),
            json!({}),
            Some(&owner),
        )
        .await
        .ok();
    assert_eq!(state["run"]["state"], "interrupted");
    h.tool(
        &token,
        "expired-token",
        "get_workspace_configuration",
        json!({}),
    )
    .await
    .denied();
    h.request(
        Method::POST,
        "/api/assistant/auth/start",
        json!({"type":"chatgptDeviceCode"}),
        Some(&owner),
    )
    .await
    .ok();
    let attempt = h.fixture.last_attempt();
    let id = Uuid::parse_str(attempt["auth_attempt_id"].as_str().unwrap()).unwrap();
    {
        let mut c = connection(&h.pool).unwrap();
        let mut d = persistence::get_document(&mut c, h.org, id)
            .unwrap()
            .unwrap();
        d.data["expires_at"] = json!(Utc::now() - Duration::minutes(1));
        persistence::save_document(
            &mut c,
            h.org,
            id,
            &d.kind,
            d.parent_id,
            d.data,
            Some(d.version),
        )
        .unwrap();
    }
    phone_api::assistant::maintenance(&h.state).await.unwrap();
    let (status, _) = h.fixture.complete(&attempt).await;
    assert!((400..500).contains(&status));
    {
        let mut c = connection(&h.pool).unwrap();
        if let Some(d) = persistence::get_document(&mut c, h.org, id).unwrap() {
            assert!(!d.data["capability_hash"].is_string());
            assert_ne!(d.data["state"], "pending");
        }
    }
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL; failed schema receipts and quota"]
async fn assistant_nested_schema_failures_are_durable_idempotent_and_consume_tool_budget() {
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    h.authorize(&owner).await;
    let conversation = h.conversation(&owner).await;
    let (run, token) = h.run(&owner, conversation, vec![]).await;
    let snapshot = h.snapshot(&token).await;
    let revision = revision(&snapshot);
    let mut first = Value::Null;
    for n in 0..23 {
        let (name, args) = match n % 3 {
            0 => ("get_workspace_configuration", json!({"unexpected":true})),
            1 => (
                "save_rubric",
                json!({"expected_revision":revision,"name":"Malformed nested criterion","criteria":[{"id":"x","name":"X","weight":1,"required":true,"instruction":"Synthetic","rogue":true}]}),
            ),
            _ => (
                "set_callback_rules",
                json!({"expected_revision":revision,"working_hours":{"enabled":false,"days":"not-an-array","rogue":true}}),
            ),
        };
        let reply = h
            .tool(&token, &format!("schema-failure-{n}"), name, args)
            .await;
        assert_eq!(
            reply.status, 400,
            "Nested/extra schema input must fail without applying settings"
        );
        if n == 0 {
            first = reply.body;
        }
    }
    let limited = h
        .tool(
            &token,
            "after-quota",
            "get_workspace_configuration",
            json!({}),
        )
        .await;
    assert_eq!(limited.status, 429);
    let repeated = h
        .tool(
            &token,
            "schema-failure-0",
            "get_workspace_configuration",
            json!({"unexpected":true}),
        )
        .await;
    assert_eq!(repeated.status, 400);
    assert_eq!(
        repeated.body, first,
        "Failed receipt replay must survive exhausted budget"
    );
    assert_eq!(
        h.tool(
            &token,
            "schema-failure-0",
            "get_workspace_configuration",
            json!({"unexpected":false})
        )
        .await
        .status,
        409,
        "Changed args must conflict even if both requests are invalid"
    );
    let state = h
        .request(
            Method::GET,
            &format!("/api/assistant/runs/{run}"),
            json!({}),
            Some(&owner),
        )
        .await
        .ok();
    assert_eq!(
        state["run"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e["type"] == "tool" && e["status"] == "failed")
            .count(),
        23
    );
    {
        let mut c = connection(&h.pool).unwrap();
        let receipts =
            persistence::list_documents(&mut c, h.org, "assistant_tool_receipt").unwrap();
        assert_eq!(receipts.len(), 24);
        assert_eq!(
            receipts
                .iter()
                .filter(|d| d.data["state"] == "failed")
                .count(),
            23
        );
        assert!(persistence::list_documents(&mut c, h.org, "rubric")
            .unwrap()
            .is_empty());
    }
    h.request(
        Method::POST,
        &format!("/api/assistant/runs/{run}/cancel"),
        json!({}),
        Some(&owner),
    )
    .await
    .ok();
    h.cleanup().await;
}

/// Support process, not an additional acceptance assertion. Never compiled into production API.
#[actix_web::test]
#[ignore = "long-running browser fixture; requires ASSISTANT_BROWSER_FIXTURE=1 and isolated browser DB"]
async fn support_assistant_browser_fixture() {
    if std::env::var("ASSISTANT_BROWSER_FIXTURE").as_deref() != Ok("1") {
        eprintln!("SUPPORT ONLY: browser fixture not started");
        return;
    }
    let _serial = SERIAL.lock().unwrap();
    let h = Harness::new().await;
    let owner = h.login(&h.owner_email).await;
    h.authorize(&owner).await;
    eprintln!("ASSISTANT_BROWSER_FIXTURE_READY api=127.0.0.1:18083 runner=127.0.0.1:18084 model=test-wire-fixture (no inference)");
    tokio::signal::ctrl_c().await.unwrap();
    h.cleanup().await;
}
