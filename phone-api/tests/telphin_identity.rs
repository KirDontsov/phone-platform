//! Real HTTP/PostgreSQL acceptance with a trusted bootstrap resolver; no live Telphin.
use actix_web::{dev::ServerHandle, web, App, HttpServer};
use diesel::{
    prelude::*,
    sql_types::{Jsonb, Uuid as SqlUuid},
};
use persistence::{connection, create_pool, migrate, Document, Pool};
use phone_api::{auth, telphin_identity::IdentityResolver, AppState};
use reqwest::{Client, Method};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::Semaphore;
use uuid::Uuid;

const PASSWORD: &str = "Identity-synthetic-password-2026";
const SECRET: &str = "synthetic-identity-app-secret-never-public";
const KEY: [u8; 32] = [31; 32];

struct ResolverState {
    requests: Mutex<Vec<Value>>,
    identity: Mutex<String>,
    hold: Mutex<bool>,
    arrived: Semaphore,
    released: Semaphore,
}
impl ResolverState {
    fn new() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            identity: Mutex::new("42042".into()),
            hold: Mutex::new(false),
            arrived: Semaphore::new(0),
            released: Semaphore::new(0),
        }
    }
    fn count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
    async fn entered(&self) {
        tokio::time::timeout(Duration::from_secs(5), self.arrived.acquire())
            .await
            .expect("resolver did not start")
            .unwrap()
            .forget();
    }
    fn release(&self) {
        self.released.add_permits(1);
    }
}
#[derive(Clone)]
struct Session {
    cookie: String,
    csrf: String,
    org: Uuid,
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
}
struct Harness {
    url: String,
    client: Client,
    pool: Pool,
    org: Uuid,
    foreign_org: Uuid,
    users: Vec<Uuid>,
    email: String,
    foreign_email: String,
    resolver: Arc<ResolverState>,
    server: ServerHandle,
}
impl Harness {
    async fn new() -> Self {
        let pool = create_pool(
            &std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
        )
        .unwrap();
        migrate(&pool).unwrap();
        let suffix = Uuid::new_v4();
        let email = format!("identity-owner-{suffix}@example.test");
        let foreign_email = format!("identity-foreign-{suffix}@example.test");
        let (org, owner, foreign_org, foreign_owner) = {
            let mut c = connection(&pool).unwrap();
            let (org, owner) = auth::bootstrap(
                &mut c,
                &email,
                PASSWORD,
                "Identity owner",
                "Identity synthetic test",
            )
            .unwrap();
            let (foreign_org, foreign_owner) = auth::bootstrap(
                &mut c,
                &foreign_email,
                PASSWORD,
                "Foreign owner",
                "Identity foreign test",
            )
            .unwrap();
            (org, owner, foreign_org, foreign_owner)
        };
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let state = web::Data::new(AppState {
            pool: pool.clone(),
            key: KEY,
            demo: false,
            secure_cookie: false,
            public_url: url.clone(),
            allowed_origins: vec![url.clone()],
            media_root: std::env::temp_dir().join(format!("identity-unused-media-{suffix}")),
            throttle: Arc::new(Mutex::new(HashMap::new())),
            dummy_hash: auth::password_hash("Unusable-identity-dummy").unwrap(),
        });
        let resolver = Arc::new(ResolverState::new());
        let fixture = resolver.clone();
        let injected = web::Data::new(IdentityResolver(Arc::new(move |config| {
            let fixture = fixture.clone();
            Box::pin(async move {
                fixture.requests.lock().unwrap().push(config);
                fixture.arrived.add_permits(1);
                let hold = *fixture.hold.lock().unwrap();
                if hold {
                    fixture.released.acquire().await.unwrap().forget();
                }
                let result = fixture.identity.lock().unwrap().clone();
                Ok(result)
            })
        })));
        let server = HttpServer::new(move || {
            App::new()
                .app_data(state.clone())
                .app_data(injected.clone())
                .configure(phone_api::configure)
        })
        .workers(4)
        .listen(listener)
        .unwrap()
        .run();
        let handle = server.handle();
        actix_web::rt::spawn(server);
        Self {
            url,
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .unwrap(),
            pool,
            org,
            foreign_org,
            users: vec![owner, foreign_owner],
            email,
            foreign_email,
            resolver,
            server: handle,
        }
    }
    async fn login(&self, email: &str) -> Session {
        let response = self
            .client
            .post(format!("{}/api/auth/login", self.url))
            .header("Origin", &self.url)
            .json(&json!({"email":email,"password":PASSWORD}))
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
        let data: Value = response.json().await.unwrap();
        Session {
            cookie,
            csrf: data["data"]["csrf_token"].as_str().unwrap().into(),
            org: Uuid::parse_str(data["data"]["organization"]["id"].as_str().unwrap()).unwrap(),
        }
    }
    async fn request(&self, session: &Session, method: Method, path: &str, body: Value) -> Reply {
        send(&self.client, &self.url, session, method, path, body).await
    }
    async fn conversation(&self, session: &Session) -> Uuid {
        let result = self
            .request(
                session,
                Method::POST,
                "/api/assistant/conversations",
                json!({"title":"Identity synthetic test","client_request_id":Uuid::new_v4()}),
            )
            .await
            .ok();
        Uuid::parse_str(result["id"].as_str().unwrap()).unwrap()
    }
    fn documents(&self, kind: &str) -> Vec<Document> {
        persistence::list_documents(&mut connection(&self.pool).unwrap(), self.org, kind).unwrap()
    }
    fn decrypted(&self, doc: &Document) -> Value {
        contracts::unseal(doc.data["config_encrypted"].as_str().unwrap(), &KEY).unwrap()
    }
    fn assert_safe(&self, value: &Value) {
        assert!(
            !value.to_string().contains(SECRET),
            "Secret leaked into public response"
        );
    }
    fn assert_documents_safe(&self) {
        #[derive(QueryableByName)]
        struct Row {
            #[diesel(sql_type=Jsonb)]
            data: Value,
        }
        let rows = diesel::sql_query("SELECT data FROM documents WHERE organization_id=$1")
            .bind::<SqlUuid, _>(self.org)
            .load::<Row>(&mut connection(&self.pool).unwrap())
            .unwrap();
        for row in rows {
            self.assert_safe(&row.data);
        }
    }
    fn spawn_request(
        &self,
        session: &Session,
        method: Method,
        path: String,
        body: Value,
    ) -> tokio::task::JoinHandle<Reply> {
        let client = self.client.clone();
        let url = self.url.clone();
        let session = session.clone();
        tokio::spawn(async move { send(&client, &url, &session, method, &path, body).await })
    }
    async fn cleanup(self) {
        self.server.stop(true).await;
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
    }
}
async fn send(
    client: &Client,
    url: &str,
    session: &Session,
    method: Method,
    path: &str,
    body: Value,
) -> Reply {
    let response = client
        .request(method, format!("{url}{path}"))
        .header("Origin", url)
        .header("Cookie", &session.cookie)
        .header("X-CSRF-Token", &session.csrf)
        .header("X-Organization-Id", session.org.to_string())
        .json(&body)
        .send()
        .await
        .expect("local identity acceptance HTTP");
    Reply {
        status: response.status().as_u16(),
        body: response.json().await.expect("JSON response"),
    }
}
fn access() -> Value {
    json!({"app_id":"synthetic-identity-application","app_secret":SECRET})
}

#[actix_web::test]
#[ignore = "isolated PostgreSQL TEST_DATABASE_URL required; no live providers"]
async fn identity_optional_client_id_is_resolved_encrypted_and_explicit_id_preserved() {
    let h = Harness::new().await;
    let owner = h.login(&h.email).await;
    let conversation = h.conversation(&owner).await;
    let path = format!("/api/assistant/conversations/{conversation}/credentials");
    for client in [None, Some("   ")] {
        let mut config = access();
        if let Some(client) = client {
            config["client_id"] = json!(client);
        }
        let result = h
            .request(
                &owner,
                Method::POST,
                &path,
                json!({"kind":"telphin","config":config}),
            )
            .await
            .ok();
        h.assert_safe(&result);
        let id = Uuid::parse_str(result["id"].as_str().unwrap()).unwrap();
        let document = h
            .documents("assistant_credential")
            .into_iter()
            .find(|doc| doc.id == id)
            .unwrap();
        assert_eq!(h.decrypted(&document)["client_id"], "42042");
        assert_eq!(h.decrypted(&document)["app_secret"], SECRET);
    }
    assert_eq!(h.resolver.count(), 2);
    let mut explicit = access();
    explicit["client_id"] = json!("7531");
    h.request(
        &owner,
        Method::POST,
        &path,
        json!({"kind":"telphin","config":explicit}),
    )
    .await
    .ok();
    assert_eq!(
        h.resolver.count(),
        2,
        "An explicit Client ID must not cause external identity lookup"
    );
    let explicit_doc = h
        .documents("assistant_credential")
        .into_iter()
        .find(|doc| h.decrypted(doc)["client_id"] == "7531")
        .expect("Explicit ID was changed");
    assert_eq!(h.decrypted(&explicit_doc)["client_id"], "7531");
    let result = h
        .request(
            &owner,
            Method::PUT,
            "/api/connections/telphin",
            json!({"config":access()}),
        )
        .await
        .ok();
    h.assert_safe(&result);
    assert_eq!(
        h.decrypted(&h.documents("connection")[0])["client_id"],
        "42042"
    );
    assert_eq!(h.resolver.count(), 3);
    *h.resolver.identity.lock().unwrap() = "86086".into();
    let result = h.request(&owner, Method::PUT, "/api/connections/telphin", json!({"config":{"app_id":"synthetic-identity-application","app_secret":"<configured>","client_id":""}})).await.ok();
    h.assert_safe(&result);
    let stored = h.decrypted(&h.documents("connection")[0]);
    assert_eq!(stored["client_id"], "86086");
    assert_eq!(stored["app_secret"], SECRET);
    assert_eq!(
        h.resolver.requests.lock().unwrap().last().unwrap()["app_secret"],
        SECRET,
        "Masked placeholder must resolve using stored secret"
    );
    let mut explicit = access();
    explicit["client_id"] = json!("9123");
    h.request(
        &owner,
        Method::PUT,
        "/api/connections/telphin",
        json!({"config":explicit}),
    )
    .await
    .ok();
    assert_eq!(h.resolver.count(), 4);
    assert_eq!(
        h.decrypted(&h.documents("connection")[0])["client_id"],
        "9123"
    );
    h.assert_documents_safe();
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "isolated PostgreSQL TEST_DATABASE_URL required; scope and malformed identity"]
async fn identity_foreign_conversation_is_rejected_before_lookup_and_invalid_result_not_saved() {
    let h = Harness::new().await;
    let owner = h.login(&h.email).await;
    let mut foreign = h.login(&h.foreign_email).await;
    let conversation = h.conversation(&owner).await;
    let path = format!("/api/assistant/conversations/{conversation}/credentials");
    let response = h
        .request(
            &foreign,
            Method::POST,
            &path,
            json!({"kind":"telphin","config":access()}),
        )
        .await;
    assert_eq!(response.status, 404);
    assert_eq!(h.resolver.count(), 0);
    // Company ownership alone does not authorize another owner's private chat.
    {
        let mut c = connection(&h.pool).unwrap();
        diesel::sql_query(
            "INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,'owner')",
        )
        .bind::<SqlUuid, _>(h.org)
        .bind::<SqlUuid, _>(h.users[1])
        .execute(&mut c)
        .unwrap();
        diesel::sql_query("UPDATE sessions SET organization_id=$1 WHERE user_id=$2")
            .bind::<SqlUuid, _>(h.org)
            .bind::<SqlUuid, _>(h.users[1])
            .execute(&mut c)
            .unwrap();
    }
    foreign.org = h.org;
    let response = h
        .request(
            &foreign,
            Method::POST,
            &path,
            json!({"kind":"telphin","config":access()}),
        )
        .await;
    assert_eq!(response.status, 404);
    assert_eq!(
        h.resolver.count(),
        0,
        "Another owner of the same company must be denied before an external lookup"
    );
    for invalid in ["", "0", "not-an-id", "18446744073709551616"] {
        *h.resolver.identity.lock().unwrap() = invalid.into();
        for (method, path, body) in [
            (
                Method::POST,
                path.clone(),
                json!({"kind":"telphin","config":access()}),
            ),
            (
                Method::PUT,
                "/api/connections/telphin".into(),
                json!({"config":access()}),
            ),
        ] {
            let response = h.request(&owner, method, &path, body).await;
            assert_eq!(response.status, 502);
            assert_eq!(response.body["error"]["code"], "telphin_identity_invalid");
            h.assert_safe(&response.body);
            assert!(h.documents("assistant_credential").is_empty());
            assert!(h.documents("connection").is_empty());
        }
    }
    let count = h.resolver.count();
    let mut evil = access();
    evil["host"] = json!("evil.example.test");
    assert_eq!(
        h.request(
            &owner,
            Method::POST,
            &path,
            json!({"kind":"telphin","config":evil})
        )
        .await
        .status,
        400
    );
    assert_eq!(
        h.resolver.count(),
        count,
        "Unapproved host must never reach the resolver"
    );
    h.assert_documents_safe();
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "isolated PostgreSQL TEST_DATABASE_URL required; real HTTP logout during resolver await"]
async fn identity_logout_during_lookup_denies_both_credential_and_connection_commit() {
    let h = Harness::new().await;
    for credentials in [true, false] {
        let owner = h.login(&h.email).await;
        let (method, path, body) = if credentials {
            let conversation = h.conversation(&owner).await;
            (
                Method::POST,
                format!("/api/assistant/conversations/{conversation}/credentials"),
                json!({"kind":"telphin","config":access()}),
            )
        } else {
            (
                Method::PUT,
                "/api/connections/telphin".into(),
                json!({"config":access()}),
            )
        };
        *h.resolver.hold.lock().unwrap() = true;
        let pending = h.spawn_request(&owner, method, path, body);
        h.resolver.entered().await;
        h.request(&owner, Method::POST, "/api/auth/logout", json!({}))
            .await
            .ok();
        h.resolver.release();
        let response = pending.await.unwrap();
        assert!(
            (400..500).contains(&response.status),
            "A logged-out session committed a resolved identity: {}",
            response.body
        );
        h.assert_safe(&response.body);
        assert!(h.documents("assistant_credential").is_empty());
        assert!(h.documents("connection").is_empty());
    }
    h.assert_documents_safe();
    h.cleanup().await;
}

#[actix_web::test]
#[ignore = "isolated PostgreSQL TEST_DATABASE_URL required; stale settings CAS after external await"]
async fn identity_concurrent_manual_connection_update_wins_over_late_resolver() {
    let h = Harness::new().await;
    let owner = h.login(&h.email).await;
    let mut initial = access();
    initial["client_id"] = json!("1001");
    h.request(
        &owner,
        Method::PUT,
        "/api/connections/telphin",
        json!({"config":initial}),
    )
    .await
    .ok();
    *h.resolver.hold.lock().unwrap() = true;
    let pending = h.spawn_request(
        &owner,
        Method::PUT,
        "/api/connections/telphin".into(),
        json!({"config":{"app_secret":"<configured>","client_id":""}}),
    );
    h.resolver.entered().await;
    let mut manual = access();
    manual["client_id"] = json!("2002");
    manual["app_id"] = json!("manual-new-application");
    h.request(
        &owner,
        Method::PUT,
        "/api/connections/telphin",
        json!({"config":manual}),
    )
    .await
    .ok();
    h.resolver.release();
    let stale = pending.await.unwrap();
    assert_eq!(stale.status, 409);
    assert_eq!(stale.body["error"]["code"], "connection_changed");
    let stored = h.decrypted(&h.documents("connection")[0]);
    assert_eq!(stored["client_id"], "2002");
    assert_eq!(stored["app_id"], "manual-new-application");
    assert_eq!(stored["app_secret"], SECRET);
    h.assert_documents_safe();
    h.cleanup().await;
}
