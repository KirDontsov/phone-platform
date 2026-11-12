//! Email ownership is required before a public registration becomes a user/tenant.
use super::*;
use diesel::sql_types::Integer;
use rand::Rng;
use serde::Deserialize;
use std::net::IpAddr;
const BROWSER: &str = "phone_registration";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Registration {
    name: String,
    email: String,
    password: String,
    organization_name: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Verification {
    email: String,
    code: String,
    password: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Resend {
    email: String,
}
#[derive(QueryableByName)]
struct Challenge {
    #[diesel(sql_type=SqlUuid)]
    id: Uuid,
    #[diesel(sql_type=Text)]
    email: String,
    #[diesel(sql_type=Text)]
    name: String,
    #[diesel(sql_type=Text)]
    organization_name: String,
    #[diesel(sql_type=Text)]
    password_hash: String,
    #[diesel(sql_type=Text)]
    code_hash: String,
    #[diesel(sql_type=Text)]
    browser_hash: String,
    #[diesel(sql_type=Timestamptz)]
    expires_at: chrono::DateTime<Utc>,
    #[diesel(sql_type=Integer)]
    attempts: i32,
    #[diesel(sql_type=Text)]
    send_state: String,
    #[diesel(sql_type=Timestamptz)]
    updated_at: chrono::DateTime<Utc>,
}
struct Delivery {
    id: Uuid,
    email: String,
    code: String,
}
fn label(value: &str, limit: usize, message: &'static str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > limit || value.chars().any(char::is_control) {
        return Err(ApiError::bad(message));
    }
    Ok(value.to_owned())
}
fn email(value: &str) -> Result<String, ApiError> {
    let value = email_valid(value)?;
    value
        .parse::<lettre::Address>()
        .map_err(|_| ApiError::bad("Укажите корректную почту"))?;
    Ok(value)
}
fn attempt(state: &AppState, req: &HttpRequest, prefix: &str, max: u32) -> Result<(), ApiError> {
    // The fixed production ingress overwrites XFF with one actual client address;
    // the API is private/loopback. Forwarded/X-Real-IP/multiple XFF are not trusted.
    let address = req
        .headers()
        .get("X-Forwarded-For")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<IpAddr>().ok())
        .or_else(|| req.peer_addr().map(|v| v.ip()));
    let key = format!(
        "signup:{prefix}:{}",
        hash(
            &address
                .map(|v| v.to_string())
                .unwrap_or_else(|| "unknown".into())
        )
    );
    let mut t = state.throttle.lock().map_err(|_| ApiError::internal())?;
    t.retain(|_, (_, at)| *at > Utc::now() - chrono::Duration::minutes(15));
    if !t.contains_key(&key) && t.len() >= 10_000 {
        return Err(throttled());
    }
    let entry = t.entry(key).or_insert((0, Utc::now()));
    if entry.0 >= max {
        return Err(throttled());
    }
    entry.0 += 1;
    Ok(())
}
fn throttled() -> ApiError {
    ApiError::new(
        429,
        "registration_throttled",
        "Слишком много попыток. Повторите позже",
    )
}
fn cooldown() -> ApiError {
    ApiError::new(
        429,
        "verification_resend_throttled",
        "Новый код можно запросить через минуту",
    )
}
fn browser(req: &HttpRequest) -> Option<String> {
    req.cookie(BROWSER)
        .map(|v| v.value().to_string())
        .filter(|v| v.len() == 64 && v.bytes().all(|c| c.is_ascii_hexdigit()))
}
fn browser_cookie(state: &AppState, token: String, clear: bool) -> Cookie<'static> {
    Cookie::build(BROWSER, token)
        .path("/api/auth")
        .http_only(true)
        .secure(state.secure_cookie)
        .same_site(SameSite::Lax)
        .max_age(Duration::seconds(if clear { 0 } else { 1800 }))
        .finish()
}
fn lock(c: &mut PgConnection, email: &str) -> Result<(), ApiError> {
    diesel::sql_query("SELECT pg_advisory_xact_lock(hashtextextended($1,730016))")
        .bind::<Text, _>(email)
        .execute(c)?;
    Ok(())
}
fn load(c: &mut PgConnection, email: &str) -> Result<Option<Challenge>, ApiError> {
    Ok(diesel::sql_query("SELECT id,email,name,organization_name,password_hash,code_hash,browser_hash,expires_at,attempts,send_state,updated_at FROM registration_challenges WHERE email=$1 FOR UPDATE").bind::<Text,_>(email).get_result::<Challenge>(c).optional()?)
}
fn user_exists(c: &mut PgConnection, email: &str) -> Result<bool, ApiError> {
    Ok(
        diesel::sql_query("SELECT id,email,name,password_hash FROM users WHERE email=$1")
            .bind::<Text, _>(email)
            .get_result::<UserRow>(c)
            .optional()?
            .is_some(),
    )
}
pub(super) fn cleanup(c: &mut PgConnection) -> Result<(), ApiError> {
    diesel::sql_query("DELETE FROM registration_challenges WHERE expires_at<now()-interval '1 day' AND email IN (SELECT email FROM registration_challenges WHERE expires_at<now()-interval '1 day' ORDER BY expires_at LIMIT 100 FOR UPDATE SKIP LOCKED)").execute(c)?;
    Ok(())
}
fn fresh_code(old: Option<&Challenge>) -> String {
    loop {
        let code = rand::rngs::OsRng.gen_range(100_000..1_000_000).to_string();
        if old.is_none_or(|v| !verify_password(&code, &v.code_hash)) {
            return code;
        }
    }
}
fn recent(value: &Challenge) -> bool {
    value.send_state != "failed" && value.updated_at > Utc::now() - chrono::Duration::seconds(60)
}
fn reserve(c: &mut PgConnection, body: Registration, nonce: &str) -> Result<Delivery, ApiError> {
    c.transaction::<_,ApiError,_>(|c|{
 lock(c,&body.email)?;
 if user_exists(c,&body.email)?{return Err(ApiError::new(409,"email_registered","Этот адрес уже зарегистрирован. Войдите в аккаунт"));}
 let old=load(c,&body.email)?;if old.as_ref().is_some_and(recent){return Err(cooldown());}
 let code=fresh_code(old.as_ref());let code_hash=password_hash(&code)?;let password_hash=password_hash(&body.password)?;let id=Uuid::new_v4();
 diesel::sql_query("INSERT INTO registration_challenges(email,id,name,organization_name,password_hash,code_hash,browser_hash,expires_at,send_state) VALUES($1,$2,$3,$4,$5,$6,$7,now()+interval '15 minutes','sending') ON CONFLICT(email) DO UPDATE SET id=EXCLUDED.id,name=EXCLUDED.name,organization_name=EXCLUDED.organization_name,password_hash=EXCLUDED.password_hash,code_hash=EXCLUDED.code_hash,browser_hash=EXCLUDED.browser_hash,expires_at=EXCLUDED.expires_at,attempts=0,send_state='sending',sent_at=NULL,updated_at=now()")
 .bind::<Text,_>(&body.email).bind::<SqlUuid,_>(id).bind::<Text,_>(&body.name).bind::<Text,_>(&body.organization_name).bind::<Text,_>(password_hash).bind::<Text,_>(code_hash).bind::<Text,_>(hash(nonce)).execute(c)?;
 Ok(Delivery{id,email:body.email,code})
 })
}
async fn deliver(state: &AppState, delivery: Delivery) -> Result<(), ApiError> {
    let mut link = reqwest::Url::parse(&format!(
        "{}/verify-email",
        state.public_url.trim_end_matches('/')
    ))
    .map_err(|_| ApiError::internal())?;
    link.query_pairs_mut()
        .append_pair("email", &delivery.email)
        .append_pair("code", &delivery.code);
    let text=format!("Подтверждение почты в сервисе Сигнал\n\nКод подтверждения:\n{}\n\nПодтвердить почту: {}\n\nКод и ссылка действуют 15 минут. Если ссылка открылась в другом браузере, укажите пароль, выбранный при регистрации. Если вы не регистрировались, проигнорируйте это письмо. Никому не сообщайте код.",delivery.code,link);
    let sent = send_mail(&delivery.email, "Сигнал — подтверждение почты", &text).await;
    let id = delivery.id;
    let success = sent.is_ok();
    let count=state.db(move|c|Ok(diesel::sql_query("UPDATE registration_challenges SET send_state=$1,sent_at=CASE WHEN $2 THEN now() ELSE NULL END WHERE id=$3 AND send_state='sending'").bind::<Text,_>(if success{"sent"}else{"failed"}).bind::<Bool,_>(success).bind::<SqlUuid,_>(id).execute(c)?)).await?;
    sent?;
    if count != 1 {
        return Err(ApiError::new(
            409,
            "registration_changed",
            "Запрос регистрации изменился. Запросите новый код",
        ));
    }
    Ok(())
}

pub(super) async fn register(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Registration>,
) -> Result<HttpResponse, ApiError> {
    origin(&state, &req)?;
    let mut body = body.into_inner();
    body.email = email(&body.email)?;
    body.name = label(&body.name, 120, "Укажите имя длиной до 120 символов")?;
    body.organization_name = label(
        &body.organization_name,
        160,
        "Укажите название компании длиной до 160 символов",
    )?;
    password_valid(&body.password)?;
    attempt(&state, &req, "register", 10)?;
    let nonce = random_token();
    let saved = nonce.clone();
    let recipient = body.email.clone();
    let delivery = state.db(move |c| reserve(c, body, &saved)).await?;
    deliver(&state, delivery).await?;
    Ok(HttpResponse::Accepted().insert_header(("Cache-Control","no-store")).cookie(browser_cookie(&state,nonce,false)).json(json!({"data":{"verification_required":true,"email":recipient,"retry_after":60,"expires_in":900}})))
}

enum Outcome {
    Invalid,
    PasswordRequired,
    Conflict,
    Verified(String, Value),
}
pub(super) async fn verify(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Verification>,
) -> Result<HttpResponse, ApiError> {
    origin(&state, &req)?;
    let body = body.into_inner();
    let email = email(&body.email)?;
    if body.code.len() != 6 || !body.code.bytes().all(|v| v.is_ascii_digit()) {
        return Err(ApiError::bad("Введите шестизначный код из письма"));
    }
    if let Some(password) = body.password.as_ref() {
        password_valid(password)?;
    }
    attempt(&state, &req, "verify", 30)?;
    let nonce = browser(&req).map(|v| hash(&v));
    let demo = state.demo;
    let outcome=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
 lock(c,&email)?;let Some(challenge)=load(c,&email)? else{return Ok(Outcome::Invalid)};
 if challenge.send_state!="sent"||challenge.expires_at<=Utc::now()||challenge.attempts>=5{return Ok(Outcome::Invalid);}
 let bound=nonce.as_ref().is_some_and(|v|v==&challenge.browser_hash);
 if !bound&&body.password.is_none(){return Ok(Outcome::PasswordRequired);}
 let password_ok=bound||body.password.as_ref().is_some_and(|v|verify_password(v,&challenge.password_hash));
 let code_ok=verify_password(&body.code,&challenge.code_hash);
 if !password_ok||!code_ok{
 // A holder of either valid factor gets only five attempts at the other.
 // Anonymous requests with neither factor cannot burn someone else's OTP.
 if password_ok||code_ok{diesel::sql_query("UPDATE registration_challenges SET attempts=attempts+1 WHERE id=$1").bind::<SqlUuid,_>(challenge.id).execute(c)?;}
 return Ok(Outcome::Invalid);
 }
 let user=diesel::sql_query("INSERT INTO users(email,name,password_hash) VALUES($1,$2,$3) ON CONFLICT(email) DO NOTHING RETURNING id,email,name,password_hash").bind::<Text,_>(&challenge.email).bind::<Text,_>(&challenge.name).bind::<Text,_>(&challenge.password_hash).get_result::<UserRow>(c).optional()?;
 let Some(user)=user else{diesel::sql_query("DELETE FROM registration_challenges WHERE id=$1").bind::<SqlUuid,_>(challenge.id).execute(c)?;return Ok(Outcome::Conflict);};
 let organization=Uuid::new_v4();diesel::sql_query("INSERT INTO organizations(id,name) VALUES($1,$2)").bind::<SqlUuid,_>(organization).bind::<Text,_>(&challenge.organization_name).execute(c)?;
 diesel::sql_query("INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,'owner')").bind::<SqlUuid,_>(organization).bind::<SqlUuid,_>(user.id).execute(c)?;
 persistence::save_document(c,organization,Uuid::new_v4(),"audit",None,json!({"action":"organization.registered","user_id":user.id,"user_name":user.name,"details":{"email_verified":true},"created_at":Utc::now()}),None)?;
 let(token,me)=create_session(c,&user,organization,demo)?;diesel::sql_query("DELETE FROM registration_challenges WHERE id=$1").bind::<SqlUuid,_>(challenge.id).execute(c)?;Ok(Outcome::Verified(token,me))
 })).await?;
    match outcome {
        Outcome::Invalid => Err(ApiError::new(
            400,
            "verification_invalid",
            "Код неверный, уже использован или истёк. Запросите новый код",
        )),
        Outcome::PasswordRequired => Err(ApiError::new(
            409,
            "verification_password_required",
            "Ссылка открыта в другом браузере. Введите пароль, указанный при регистрации",
        )),
        Outcome::Conflict => Err(ApiError::new(
            409,
            "email_registered",
            "Этот адрес уже зарегистрирован. Войдите в аккаунт",
        )),
        Outcome::Verified(token, me) => {
            let mut response = session_response(&state, token, me);
            response
                .add_cookie(&browser_cookie(&state, String::new(), true))
                .map_err(|_| ApiError::internal())?;
            Ok(response)
        }
    }
}

fn resend_response() -> HttpResponse {
    ok(
        json!({"message":"Если в этом браузере активна регистрация для указанной почты, новый код отправлен","retry_after":60}),
    )
}
pub(super) async fn resend(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Resend>,
) -> Result<HttpResponse, ApiError> {
    origin(&state, &req)?;
    let email = email(&body.email)?;
    attempt(&state, &req, "resend", 20)?;
    let Some(nonce) = browser(&req) else {
        return Ok(resend_response());
    };
    let nonce_hash = hash(&nonce);
    let delivery=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
 lock(c,&email)?;let Some(challenge)=load(c,&email)?else{return Ok(None)};
 if challenge.browser_hash!=nonce_hash||user_exists(c,&email)?{return Ok(None);}
 if recent(&challenge){return Err(cooldown());}
 let code=fresh_code(Some(&challenge));let id=Uuid::new_v4();
 diesel::sql_query("UPDATE registration_challenges SET id=$1,code_hash=$2,expires_at=now()+interval '15 minutes',attempts=0,send_state='sending',sent_at=NULL,updated_at=now() WHERE email=$3").bind::<SqlUuid,_>(id).bind::<Text,_>(password_hash(&code)?).bind::<Text,_>(&email).execute(c)?;
 Ok(Some(Delivery{id,email,code}))
 })).await?;
    if let Some(delivery) = delivery {
        deliver(&state, delivery).await?;
        let mut response = resend_response();
        response
            .add_cookie(&browser_cookie(&state, nonce, false))
            .map_err(|_| ApiError::internal())?;
        Ok(response)
    } else {
        Ok(resend_response())
    }
}
