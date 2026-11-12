//! Real SMTP + isolated PG: email proof must precede any public user/tenant/session.
use actix_web::{http::header, test, web, App};
use base64::{engine::general_purpose::STANDARD, Engine};
use diesel::{prelude::*, sql_types::Jsonb};
use phone_api::{auth, AppState};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};
use uuid::Uuid;
const PASSWORD: &str = "Synthetic-registration-2026!";
static ENV: Mutex<()> = Mutex::new(());
#[derive(QueryableByName)]
struct Row {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}
fn state() -> AppState {
    let pool = persistence::Pool::builder()
        .max_size(4)
        .min_idle(Some(1))
        .connection_timeout(Duration::from_secs(30))
        .build(
            diesel::r2d2::ConnectionManager::<diesel::PgConnection>::new(
                std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
            ),
        )
        .unwrap();
    persistence::migrate(&pool).unwrap();
    AppState {
        pool,
        key: [81; 32],
        demo: false,
        secure_cookie: true,
        public_url: "https://phone.example.test".into(),
        allowed_origins: vec!["https://phone.example.test".into()],
        media_root: std::env::temp_dir().join("registration-fixture"),
        throttle: Arc::new(Mutex::new(HashMap::new())),
        dummy_hash: auth::password_hash("synthetic-unusable-password").unwrap(),
    }
}
fn body(email: &str, company: &str) -> Value {
    json!({"email":email,"password":PASSWORD,"name":"Новый владелец","organization_name":company})
}
fn query(s: &AppState, sql: &str) -> Value {
    let mut c = persistence::connection(&s.pool).unwrap();
    diesel::sql_query(sql)
        .get_result::<Row>(&mut c)
        .unwrap()
        .data
}
fn counts(s: &AppState) -> Value {
    query(s,"SELECT jsonb_build_object('users',(SELECT count(*) FROM users),'organizations',(SELECT count(*) FROM organizations),'memberships',(SELECT count(*) FROM memberships),'sessions',(SELECT count(*) FROM sessions)) AS data")
}
fn execute(s: &AppState, sql: &str) {
    let mut c = persistence::connection(&s.pool).unwrap();
    diesel::sql_query(sql).execute(&mut c).unwrap();
}
fn cookie(r: &actix_web::dev::ServiceResponse, name: &str) -> String {
    r.response()
        .cookies()
        .find(|c| c.name() == name)
        .map(|c| format!("{}={}", name, c.value()))
        .unwrap()
}
struct Smtp {
    port: u16,
    messages: mpsc::Receiver<String>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    env: Vec<(&'static str, Option<String>)>,
}
impl Smtp {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let (tx, messages) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let tx = tx.clone();
                        std::thread::spawn(move || smtp(stream, tx));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        let env = [
            "SMTP_HOST",
            "SMTP_PORT",
            "SMTP_TLS",
            "MAIL_FROM",
            "SMTP_USER",
            "SMTP_PASSWORD",
        ]
        .into_iter()
        .map(|k| (k, std::env::var(k).ok()))
        .collect();
        std::env::set_var("SMTP_HOST", "127.0.0.1");
        std::env::set_var("SMTP_PORT", port.to_string());
        std::env::set_var("SMTP_TLS", "false");
        std::env::set_var("MAIL_FROM", "Phone Test <noreply@example.test>");
        std::env::remove_var("SMTP_USER");
        std::env::remove_var("SMTP_PASSWORD");
        Self {
            port,
            messages,
            stop,
            thread: Some(thread),
            env,
        }
    }
    fn code(&self) -> String {
        let message = self
            .messages
            .recv_timeout(Duration::from_secs(5))
            .expect("real SMTP message required");
        let (headers, payload) = message
            .split_once("\r\n\r\n")
            .expect("SMTP message headers/body");
        let message = if headers
            .to_ascii_lowercase()
            .contains("content-transfer-encoding: base64")
        {
            String::from_utf8(
                STANDARD
                    .decode(payload.split_whitespace().collect::<String>())
                    .unwrap(),
            )
            .unwrap()
        } else {
            payload.to_string()
        };
        assert!(message.contains("/verify-email"));
        message
            .lines()
            .map(str::trim)
            .find(|line| line.len() == 6 && line.bytes().all(|v| v.is_ascii_digit()))
            .expect("numeric code must be present in actual mail")
            .to_owned()
    }
}
impl Drop for Smtp {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            t.join().unwrap();
        }
        for (k, v) in &self.env {
            if let Some(v) = v {
                std::env::set_var(k, v)
            } else {
                std::env::remove_var(k)
            }
        }
    }
}
fn smtp(mut stream: TcpStream, tx: mpsc::Sender<String>) {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    if stream.write_all(b"220 fixture ESMTP\r\n").is_err() {
        return;
    }
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let upper = line.to_ascii_uppercase();
        let response = if upper.starts_with("EHLO") || upper.starts_with("HELO") {
            b"250-fixture\r\n250 8BITMIME\r\n".as_slice()
        } else if upper.starts_with("DATA") {
            if stream.write_all(b"354 End with dot\r\n").is_err() {
                return;
            }
            let mut message = String::new();
            loop {
                let mut part = String::new();
                if reader.read_line(&mut part).unwrap_or(0) == 0 {
                    return;
                }
                if part == ".\r\n" {
                    break;
                }
                message.push_str(&part);
            }
            let _ = tx.send(message);
            b"250 2.0.0 queued\r\n".as_slice()
        } else if upper.starts_with("QUIT") {
            let _ = stream.write_all(b"221 bye\r\n");
            break;
        } else {
            b"250 OK\r\n".as_slice()
        };
        if stream.write_all(response).is_err() {
            break;
        }
    }
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn pending_then_real_mail_proof_creates_only_own_empty_workspace() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let mail = Smtp::new();
    let s = state();
    let suffix = Uuid::new_v4();
    let email = format!("signup-{suffix}@example.test");
    let company = format!("Одинаковое название {suffix}");
    let foreign_call = Uuid::new_v4();
    let foreign;
    {
        let mut c = persistence::connection(&s.pool).unwrap();
        (foreign, _) = auth::bootstrap(
            &mut c,
            &format!("foreign-{suffix}@example.test"),
            PASSWORD,
            "Other",
            &company,
        )
        .unwrap();
        persistence::save_document(
            &mut c,
            foreign,
            foreign_call,
            "call",
            None,
            json!({"phone":"synthetic-only","tag_ids":[]}),
            None,
        )
        .unwrap();
        persistence::save_document(
            &mut c,
            foreign,
            Uuid::new_v4(),
            "connection",
            None,
            json!({"kind":"telphin","state":"configured","config":{}}),
            None,
        )
        .unwrap();
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(s.clone()))
            .configure(phone_api::configure),
    )
    .await;
    let before = counts(&s);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(body(&format!("  {}  ", email.to_uppercase()), &company))
            .to_request(),
    )
    .await;
    assert_eq!(
        r.status().as_u16(),
        202,
        "registration requires email proof, never auto-login"
    );
    assert!(!r.response().cookies().any(|c| c.name() == "phone_session"));
    let nonce = cookie(&r, "phone_registration");
    let pending: Value = test::read_body_json(r).await;
    assert_eq!(pending["data"]["verification_required"], true);
    assert_eq!(pending["data"]["email"], email);
    assert_eq!(counts(&s), before);
    let code = mail.code();
    assert!(!pending.to_string().contains(&code));
    assert!(!pending.to_string().contains(PASSWORD));
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(json!({"email":email,"password":PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 401);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email,"code":code}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 200);
    let session = cookie(&r, "phone_session");
    let set = r
        .response()
        .cookies()
        .find(|c| c.name() == "phone_session")
        .unwrap();
    assert!(set.secure().unwrap() && set.http_only().unwrap());
    let me: Value = test::read_body_json(r).await;
    let me = &me["data"];
    assert_eq!(me["role"], "owner");
    assert_eq!(me["organizations"].as_array().unwrap().len(), 1);
    assert_ne!(me["organization"]["id"], foreign.to_string());
    for path in ["/api/calls", "/api/connections"] {
        let r = test::call_service(
            &app,
            test::TestRequest::get()
                .uri(path)
                .insert_header((header::COOKIE, session.as_str()))
                .to_request(),
        )
        .await;
        assert_eq!(r.status().as_u16(), 200);
        let v: Value = test::read_body_json(r).await;
        assert_eq!(v["data"]["items"].as_array().unwrap().len(), 0);
    }
    let r = test::call_service(
        &app,
        test::TestRequest::get()
            .uri(&format!("/api/calls/{foreign_call}"))
            .insert_header((header::COOKIE, session.as_str()))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 404);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email,"code":code}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 400, "mail proof is single use");
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/login")
            .set_json(json!({"email":email,"password":PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 200);
    let after = counts(&s);
    let mut duplicate = body(&email, "Must not appear");
    duplicate["password"] = json!("New-password-never-applied");
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(duplicate)
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 409);
    assert_eq!(counts(&s), after);
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn code_expiry_attempts_resend_rotation_and_missing_browser_proof() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let mail = Smtp::new();
    let s = state();
    let email = format!("code-{}@example.test", Uuid::new_v4());
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(s.clone()))
            .configure(phone_api::configure),
    )
    .await;
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(body(&email, "Code company"))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 202);
    let nonce = cookie(&r, "phone_registration");
    let code = mail.code();
    let before = counts(&s);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .set_json(json!({"email":email,"code":code}))
            .to_request(),
    )
    .await;
    assert_eq!(
        r.status().as_u16(),
        409,
        "unsolicited email click must not activate a password chosen by another browser"
    );
    for _ in 0..5 {
        let r = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/verify-email")
                .insert_header((header::COOKIE, nonce.as_str()))
                .set_json(json!({"email":email,"code":"000000"}))
                .to_request(),
        )
        .await;
        assert_eq!(r.status().as_u16(), 400);
    }
    assert_eq!(query(&s,&format!("SELECT to_jsonb(attempts) AS data FROM registration_challenges WHERE email='{email}'")),5,"bad attempts persist despite error response");
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email,"code":code}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 400);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/resend-verification")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 429);
    execute(&s,&format!("UPDATE registration_challenges SET updated_at=now()-interval '61 seconds' WHERE email='{email}'"));
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/resend-verification")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 200);
    let next = mail.code();
    assert_ne!(code, next, "resend always invalidates old code");
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email,"code":code}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 400);
    execute(&s,&format!("UPDATE registration_challenges SET expires_at=now()-interval '1 second' WHERE email='{email}'"));
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email,"code":next}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 400);
    assert_eq!(counts(&s), before);
    execute(&s,&format!("UPDATE registration_challenges SET expires_at=now()+interval '15 minutes' WHERE email='{email}'"));
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .set_json(json!({"email":email,"code":next,"password":"Wrong-password-2026"}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 400);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .set_json(json!({"email":email,"code":next,"password":PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(
        r.status().as_u16(),
        200,
        "another browser can prove both email and original password"
    );
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn cross_browser_password_attempts_are_persistently_bounded() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let mail = Smtp::new();
    let s = state();
    let email = format!("password-proof-{}@example.test", Uuid::new_v4());
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(s.clone()))
            .configure(phone_api::configure),
    )
    .await;
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(body(&email, "Proof company"))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 202);
    let code = mail.code();
    let before = counts(&s);
    for _ in 0..5 {
        let r = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/verify-email")
                .set_json(json!({"email":email,"code":code,"password":"Wrong-password-2026"}))
                .to_request(),
        )
        .await;
        assert_eq!(r.status().as_u16(), 400);
    }
    assert_eq!(query(&s,&format!("SELECT to_jsonb(attempts) AS data FROM registration_challenges WHERE email='{email}'")),5);
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .set_json(json!({"email":email,"code":code,"password":PASSWORD}))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 400);
    assert_eq!(counts(&s), before);
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn smtp_failure_is_honest_and_retry_creates_no_identity_until_confirmed() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let mail = Smtp::new();
    let s = state();
    let email = format!("mail-fail-{}@example.test", Uuid::new_v4());
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(s.clone()))
            .configure(phone_api::configure),
    )
    .await;
    let before = counts(&s);
    std::env::set_var("SMTP_PORT", "9");
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(body(&email, "Failure"))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 503);
    assert_eq!(counts(&s), before);
    assert_eq!(query(&s,&format!("SELECT to_jsonb(send_state) AS data FROM registration_challenges WHERE email='{email}'")),"failed");
    std::env::set_var("SMTP_PORT", mail.port.to_string());
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(body(&email, "Retry"))
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 202);
    let _ = mail.code();
    assert_eq!(counts(&s), before);
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn concurrent_registration_confirmation_and_database_rollback_are_atomic() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let mail = Smtp::new();
    let s = state();
    let suffix = Uuid::new_v4().simple().to_string();
    let email = format!("race-{suffix}@example.test");
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(s.clone()))
            .configure(phone_api::configure),
    )
    .await;
    let signup = || {
        test::TestRequest::post()
            .uri("/api/auth/register")
            .set_json(body(&email, "Atomic"))
            .to_request()
    };
    let (a, b) = tokio::join!(
        test::call_service(&app, signup()),
        test::call_service(&app, signup())
    );
    let mut statuses = vec![a.status().as_u16(), b.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, vec![202, 429]);
    let nonce = cookie(
        if a.status().as_u16() == 202 { &a } else { &b },
        "phone_registration",
    );
    let code = mail.code();
    let before = counts(&s);
    let function = format!("signup_fail_{suffix}");
    execute(&s,&format!("CREATE FUNCTION {function}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF EXISTS(SELECT 1 FROM users WHERE id=NEW.user_id AND email='race-{suffix}@example.test') THEN RAISE EXCEPTION 'synthetic signup failure'; END IF; RETURN NEW; END $$"));
    execute(&s,&format!("CREATE TRIGGER {function} BEFORE INSERT ON memberships FOR EACH ROW EXECUTE FUNCTION {function}()"));
    let verify = || {
        test::TestRequest::post()
            .uri("/api/auth/verify-email")
            .insert_header((header::COOKIE, nonce.as_str()))
            .set_json(json!({"email":email,"code":code}))
            .to_request()
    };
    let r = test::call_service(&app, verify()).await;
    execute(&s, &format!("DROP TRIGGER {function} ON memberships"));
    execute(&s, &format!("DROP FUNCTION {function}()"));
    assert_eq!(
        r.status().as_u16(),
        503,
        "forced DB failure follows existing internal service_unavailable contract"
    );
    assert_eq!(counts(&s), before);
    let (a, b) = tokio::join!(
        test::call_service(&app, verify()),
        test::call_service(&app, verify())
    );
    let mut statuses = vec![a.status().as_u16(), b.status().as_u16()];
    statuses.sort();
    assert_eq!(statuses, vec![200, 400]);
    let after = counts(&s);
    for key in ["users", "organizations", "memberships", "sessions"] {
        assert_eq!(
            after[key].as_i64().unwrap(),
            before[key].as_i64().unwrap() + 1,
            "no partial or duplicate {key}"
        );
    }
}

#[actix_web::test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
async fn validation_origin_privilege_injection_and_bounded_throttle() {
    let _guard = ENV.lock().unwrap_or_else(|e| e.into_inner());
    let mail = Smtp::new();
    let s = state();
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(s.clone()))
            .configure(phone_api::configure),
    )
    .await;
    let before = counts(&s);
    let email = format!("invalid-{}@example.test", Uuid::new_v4());
    let base = body(&email, "Valid");
    for (key, value) in [
        ("name", json!(" ")),
        ("name", json!("x".repeat(121))),
        ("organization_name", json!("")),
        ("organization_name", json!("x".repeat(161))),
        ("email", json!("invalid@@example.test")),
        ("email", json!("Name <foo@example.test>")),
        ("password", json!("short")),
        ("password", json!("x".repeat(513))),
        ("role", json!("owner")),
        ("organization_id", json!(Uuid::new_v4())),
    ] {
        let mut b = base.clone();
        b[key] = value;
        let r = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/register")
                .set_json(b)
                .to_request(),
        )
        .await;
        assert_eq!(r.status().as_u16(), 400, "invalid {key}");
    }
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .insert_header((header::ORIGIN, "https://attacker.example.test"))
            .set_json(base.clone())
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 403);
    assert_eq!(counts(&s), before);
    for index in 0..10 {
        let r = test::call_service(
            &app,
            test::TestRequest::post()
                .uri("/api/auth/register")
                .peer_addr("203.0.113.80:1234".parse().unwrap())
                .insert_header(("Forwarded", format!("for=198.51.100.{index}")))
                .insert_header(("X-Real-IP", format!("198.51.100.{index}")))
                .insert_header((
                    "X-Forwarded-For",
                    format!("198.51.100.{index}, 198.51.100.100"),
                ))
                .set_json(base.clone())
                .to_request(),
        )
        .await;
        assert_eq!(r.status().as_u16(), if index == 0 { 202 } else { 429 });
    }
    let _ = mail.code();
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .peer_addr("203.0.113.80:5678".parse().unwrap())
            .set_json(base.clone())
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 429);
    assert_eq!(counts(&s), before);
    {
        let mut t = s.throttle.lock().unwrap();
        t.clear();
        for n in 0..10000 {
            t.insert(format!("fixture-{n}"), (1, chrono::Utc::now()));
        }
    }
    let r = test::call_service(
        &app,
        test::TestRequest::post()
            .uri("/api/auth/register")
            .peer_addr("203.0.113.81:1234".parse().unwrap())
            .set_json(base)
            .to_request(),
    )
    .await;
    assert_eq!(r.status().as_u16(), 429);
    assert_eq!(s.throttle.lock().unwrap().len(), 10000);
}
