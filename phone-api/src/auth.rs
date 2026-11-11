use crate::*;
use actix_web::{
    cookie::{time::Duration, Cookie, SameSite},
    HttpRequest,
};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHasher, PasswordVerifier,
};
use diesel::{
    prelude::*,
    sql_types::{BigInt, Bool, Jsonb, Nullable, Text, Timestamptz, Uuid as SqlUuid},
};
use rand::RngCore;
use sha2::{Digest, Sha256};

pub const COOKIE: &str = "phone_session";
pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}
pub fn hash(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub fn password_hash(password: &str) -> Result<String, ApiError> {
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|h| h.to_string())
        .map_err(|_| ApiError::internal())
}
fn verify_password(password: &str, encoded: &str) -> bool {
    argon2::PasswordHash::new(encoded).ok().is_some_and(|h| {
        Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok()
    })
}
fn password_valid(v: &str) -> Result<(), ApiError> {
    if v.len() < 10 || v.len() > 512 {
        Err(ApiError::bad("Пароль должен содержать от 10 до 512 байт"))
    } else {
        Ok(())
    }
}
fn email_valid(v: &str) -> Result<String, ApiError> {
    let email = v.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') || email.chars().any(char::is_whitespace) {
        Err(ApiError::bad("Укажите корректную почту"))
    } else {
        Ok(email)
    }
}

#[derive(QueryableByName, Clone)]
struct UserRow {
    #[diesel(sql_type=SqlUuid)]
    id: Uuid,
    #[diesel(sql_type=Text)]
    email: String,
    #[diesel(sql_type=Text)]
    name: String,
    #[diesel(sql_type=Text)]
    password_hash: String,
}
#[derive(QueryableByName, Clone)]
pub struct Context {
    #[diesel(sql_type=SqlUuid)]
    pub user_id: Uuid,
    #[diesel(sql_type=SqlUuid)]
    pub organization_id: Uuid,
    #[diesel(sql_type=Text)]
    pub role: String,
    #[diesel(sql_type=Text)]
    pub csrf_token: String,
    #[diesel(sql_type=Text)]
    pub email: String,
    #[diesel(sql_type=Text)]
    pub name: String,
    #[diesel(sql_type=Text)]
    pub token_hash: String,
}
#[derive(QueryableByName)]
struct OrgRow {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}
#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type=BigInt)]
    count: i64,
}
#[derive(QueryableByName)]
struct Grant {
    #[diesel(sql_type=SqlUuid)]
    id: Uuid,
    #[diesel(sql_type=Nullable<SqlUuid>)]
    organization_id: Option<Uuid>,
    #[diesel(sql_type=Nullable<SqlUuid>)]
    user_id: Option<Uuid>,
    #[diesel(sql_type=Nullable<Text>)]
    email: Option<String>,
    #[diesel(sql_type=Nullable<Text>)]
    role: Option<String>,
    #[diesel(sql_type=Jsonb)]
    _payload: Value,
}

pub fn origin(state: &AppState, req: &HttpRequest) -> Result<(), ApiError> {
    if let Some(origin) = req.headers().get("Origin") {
        let origin = origin.to_str().map_err(|_| ApiError::forbidden())?;
        if !state.allowed_origins.iter().any(|v| v == origin) {
            return Err(ApiError::forbidden());
        }
    } else if req
        .headers()
        .get("Sec-Fetch-Site")
        .and_then(|v| v.to_str().ok())
        == Some("cross-site")
    {
        return Err(ApiError::forbidden());
    }
    Ok(())
}
pub async fn context(
    state: &AppState,
    req: &HttpRequest,
    write: bool,
    min_role: u8,
) -> Result<Context, ApiError> {
    if write {
        origin(state, req)?;
    }
    let raw = req
        .cookie(COOKIE)
        .map(|c| c.value().to_string())
        .filter(|s| s.len() == 64)
        .ok_or_else(|| ApiError::new(401, "unauthenticated", "Войдите в систему"))?;
    let token_hash = hash(&raw);
    let ctx=state.db(move|conn|{
  diesel::sql_query("SELECT s.user_id,s.organization_id,m.role,s.csrf_token,u.email,u.name,s.token_hash FROM sessions s JOIN memberships m ON m.user_id=s.user_id AND m.organization_id=s.organization_id JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.expires_at>now()")
    .bind::<Text,_>(token_hash).get_result::<Context>(conn).optional()?.ok_or_else(||ApiError::new(401,"unauthenticated","Сессия истекла"))
 }).await?;
    if let Some(org) = req.headers().get("X-Organization-Id") {
        if org.to_str().ok() != Some(ctx.organization_id.to_string().as_str()) {
            return Err(ApiError::new(
                409,
                "organization_changed",
                "Компания изменилась. Обновите данные",
            ));
        }
    }
    if write {
        let csrf = req
            .headers()
            .get("X-CSRF-Token")
            .and_then(|s| s.to_str().ok())
            .unwrap_or("");
        if hash(csrf) != hash(&ctx.csrf_token) {
            return Err(ApiError::new(
                403,
                "invalid_csrf",
                "Обновите страницу и повторите действие",
            ));
        }
    }
    let role = match ctx.role.as_str() {
        "owner" => 2,
        "manager" => 1,
        _ => 0,
    };
    if role < min_role {
        return Err(ApiError::forbidden());
    }
    Ok(ctx)
}
fn membership_orgs(conn: &mut PgConnection, user_id: Uuid) -> Result<Vec<Value>, ApiError> {
    let rows=diesel::sql_query("SELECT jsonb_build_object('id',o.id,'name',o.name,'role',m.role) AS data FROM organizations o JOIN memberships m ON m.organization_id=o.id WHERE m.user_id=$1 ORDER BY o.created_at,o.id")
 .bind::<SqlUuid,_>(user_id).load::<OrgRow>(conn)?;
    Ok(rows.into_iter().map(|r| r.data).collect())
}
pub fn me_json(conn: &mut PgConnection, ctx: &Context, demo: bool) -> Result<Value, ApiError> {
    let org=diesel::sql_query("SELECT jsonb_build_object('id',id,'name',name,'timezone',timezone,'callback_minutes',callback_minutes,'retention_days',retention_days)||data AS data FROM organizations WHERE id=$1")
 .bind::<SqlUuid,_>(ctx.organization_id).get_result::<OrgRow>(conn)?.data;
    Ok(
        json!({"user":{"id":ctx.user_id,"email":ctx.email,"name":ctx.name},"organization":org,"organizations":membership_orgs(conn,ctx.user_id)?,"role":ctx.role,"csrf_token":ctx.csrf_token,"demo_mode":demo}),
    )
}
fn create_session(
    conn: &mut PgConnection,
    user: &UserRow,
    org: Uuid,
    demo: bool,
) -> Result<(String, Value), ApiError> {
    let membership=diesel::sql_query("SELECT jsonb_build_object('role',role) AS data FROM memberships WHERE organization_id=$1 AND user_id=$2")
 .bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(user.id).get_result::<OrgRow>(conn).optional()?.ok_or_else(ApiError::forbidden)?;
    let token = random_token();
    let token_hash = hash(&token);
    let csrf = random_token();
    diesel::sql_query("INSERT INTO sessions(token_hash,user_id,organization_id,csrf_token,expires_at) VALUES($1,$2,$3,$4,$5)")
 .bind::<Text,_>(&token_hash).bind::<SqlUuid,_>(user.id).bind::<SqlUuid,_>(org).bind::<Text,_>(&csrf).bind::<Timestamptz,_>(Utc::now()+chrono::Duration::hours(24)).execute(conn)?;
    let ctx = Context {
        user_id: user.id,
        organization_id: org,
        role: membership.data["role"]
            .as_str()
            .unwrap_or("reviewer")
            .into(),
        csrf_token: csrf,
        email: user.email.clone(),
        name: user.name.clone(),
        token_hash,
    };
    Ok((token, me_json(conn, &ctx, demo)?))
}
fn session_response(state: &AppState, token: String, value: Value) -> HttpResponse {
    HttpResponse::Ok()
        .cookie(
            Cookie::build(COOKIE, token)
                .http_only(true)
                .secure(state.secure_cookie)
                .same_site(SameSite::Lax)
                .path("/")
                .max_age(Duration::hours(24))
                .finish(),
        )
        .insert_header(("Cache-Control", "no-store"))
        .json(json!({"data":value}))
}
async fn login(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    origin(&state, &req)?;
    let email = email_valid(str_field(&body, "email")?)?;
    let password = str_field(&body, "password")?.to_string();
    if password.len() > 512 {
        return Err(ApiError::bad("Слишком длинный пароль"));
    }
    let ip = req
        .peer_addr()
        .map(|v| v.ip().to_string())
        .unwrap_or_default();
    let key = format!("login:{ip}:{}", hash(&email));
    {
        let mut t = state.throttle.lock().map_err(|_| ApiError::internal())?;
        t.retain(|_, (_, at)| *at > Utc::now() - chrono::Duration::minutes(15));
        let e = t.entry(key.clone()).or_insert((0, Utc::now()));
        if e.0 >= 10 {
            return Err(ApiError::new(
                429,
                "login_throttled",
                "Слишком много попыток. Повторите позже",
            ));
        }
        e.0 += 1;
    }
    let demo = state.demo;
    let dummy = state.dummy_hash.clone();
    let (token, value) = state
        .db(move |conn| {
            let user =
                diesel::sql_query("SELECT id,email,name,password_hash FROM users WHERE email=$1")
                    .bind::<Text, _>(&email)
                    .get_result::<UserRow>(conn)
                    .optional()?;
            let encoded = user
                .as_ref()
                .map(|u| u.password_hash.as_str())
                .unwrap_or(&dummy);
            if !verify_password(&password, encoded) {
                return Err(ApiError::new(
                    401,
                    "invalid_credentials",
                    "Неверная почта или пароль",
                ));
            }
            let user = user.ok_or_else(|| {
                ApiError::new(401, "invalid_credentials", "Неверная почта или пароль")
            })?;
            let orgs = membership_orgs(conn, user.id)?;
            let org = orgs
                .first()
                .and_then(|o| o["id"].as_str())
                .and_then(|v| Uuid::parse_str(v).ok())
                .ok_or_else(ApiError::forbidden)?;
            create_session(conn, &user, org, demo)
        })
        .await?;
    state
        .throttle
        .lock()
        .map_err(|_| ApiError::internal())?
        .remove(&key);
    Ok(session_response(&state, token, value))
}
async fn me(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, false, 0).await?;
    let demo = state.demo;
    Ok(ok(state.db(move |c| me_json(c, &ctx, demo)).await?))
}
async fn logout(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, true, 0).await?;
    state
        .db(move |c| {
            diesel::sql_query("DELETE FROM sessions WHERE token_hash=$1")
                .bind::<Text, _>(ctx.token_hash)
                .execute(c)?;
            Ok(())
        })
        .await?;
    Ok(HttpResponse::Ok()
        .cookie(
            Cookie::build(COOKIE, "")
                .path("/")
                .http_only(true)
                .secure(state.secure_cookie)
                .same_site(SameSite::Lax)
                .max_age(Duration::seconds(0))
                .finish(),
        )
        .json(json!({"data":{"ok":true}})))
}
async fn switch_org(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, true, 0).await?;
    let org = id_field(&body, "organization_id")?;
    let demo = state.demo;
    let (token, value) = state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                let user =
                    diesel::sql_query("SELECT id,email,name,password_hash FROM users WHERE id=$1")
                        .bind::<SqlUuid, _>(ctx.user_id)
                        .get_result::<UserRow>(c)?;
                let result = create_session(c, &user, org, demo)?;
                diesel::sql_query("DELETE FROM sessions WHERE token_hash=$1")
                    .bind::<Text, _>(ctx.token_hash)
                    .execute(c)?;
                Ok(result)
            })
        })
        .await?;
    Ok(session_response(&state, token, value))
}

pub async fn send_mail(to: &str, subject: &str, body: &str) -> Result<(), ApiError> {
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
    let from = std::env::var("MAIL_FROM")
        .unwrap_or_else(|_| "Phone Platform <noreply@phone.local>".into());
    let message = Message::builder()
        .from(from.parse().map_err(|_| ApiError::internal())?)
        .to(to
            .parse()
            .map_err(|_| ApiError::bad("Некорректная почта"))?)
        .subject(subject)
        .body(body.to_string())
        .map_err(|_| ApiError::internal())?;
    let host = std::env::var("SMTP_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = std::env::var("SMTP_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(1025);
    let builder = if std::env::var("SMTP_TLS").as_deref() == Ok("true") {
        AsyncSmtpTransport::<Tokio1Executor>::relay(&host).map_err(|_| ApiError::internal())?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
    };
    let mut builder = builder.port(port);
    if let (Ok(user), Ok(password)) = (std::env::var("SMTP_USER"), std::env::var("SMTP_PASSWORD")) {
        builder = builder.credentials(lettre::transport::smtp::authentication::Credentials::new(
            user, password,
        ));
    }
    builder
        .build()
        .send(message)
        .await
        .map_err(|_| ApiError::new(503, "mail_unavailable", "Почтовый сервис недоступен"))?;
    Ok(())
}
async fn reset_request(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    origin(&state, &req)?;
    let email = email_valid(str_field(&body, "email")?)?;
    let bucket = format!("reset:{}", hash(&email));
    {
        let mut t = state.throttle.lock().map_err(|_| ApiError::internal())?;
        let e = t.entry(bucket).or_insert((0, Utc::now()));
        if e.1 < Utc::now() - chrono::Duration::minutes(15) {
            *e = (0, Utc::now());
        }
        if e.0 >= 3 {
            return Ok(ok(json!({"ok":true})));
        }
        e.0 += 1;
    }
    let token = random_token();
    let digest = hash(&token);
    let target = email.clone();
    let exists=state.db(move|c|{
  let user=diesel::sql_query("SELECT id,email,name,password_hash FROM users WHERE email=$1").bind::<Text,_>(&email).get_result::<UserRow>(c).optional()?;
  if let Some(user)=user{diesel::sql_query("UPDATE token_grants SET consumed_at=now() WHERE kind='reset' AND user_id=$1 AND consumed_at IS NULL").bind::<SqlUuid,_>(user.id).execute(c)?;
   diesel::sql_query("INSERT INTO token_grants(token_hash,kind,user_id,email,expires_at) VALUES($1,'reset',$2,$3,$4)").bind::<Text,_>(digest).bind::<SqlUuid,_>(user.id).bind::<Text,_>(&email).bind::<Timestamptz,_>(Utc::now()+chrono::Duration::minutes(30)).execute(c)?;Ok(true)}else{Ok(false)}
 }).await?;
    if exists {
        let url = format!("{}/reset?token={}", state.public_url, token);
        let _ = send_mail(
            &target,
            "Восстановление доступа Phone Platform",
            &format!("Для смены пароля откройте {url}\nСсылка действует 30 минут."),
        )
        .await;
    }
    Ok(ok(json!({"ok":true})))
}
async fn reset_confirm(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    origin(&state, &req)?;
    let token = str_field(&body, "token")?.to_string();
    let password = str_field(&body, "password")?;
    password_valid(password)?;
    let encoded = password_hash(password)?;
    let digest = hash(&token);
    state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  let grant=diesel::sql_query("SELECT id,organization_id,user_id,email,role,payload AS _payload FROM token_grants WHERE token_hash=$1 AND kind='reset' AND consumed_at IS NULL AND expires_at>now() FOR UPDATE").bind::<Text,_>(digest).get_result::<Grant>(c).optional()?.ok_or_else(||ApiError::bad("Ссылка недействительна или истекла"))?;
  let user_id=grant.user_id.ok_or_else(ApiError::missing)?;
  diesel::sql_query("UPDATE users SET password_hash=$1 WHERE id=$2").bind::<Text,_>(encoded).bind::<SqlUuid,_>(user_id).execute(c)?;
  diesel::sql_query("UPDATE token_grants SET consumed_at=now() WHERE id=$1").bind::<SqlUuid,_>(grant.id).execute(c)?;
  diesel::sql_query("DELETE FROM sessions WHERE user_id=$1").bind::<SqlUuid,_>(user_id).execute(c)?;Ok(())
 })).await?;
    Ok(ok(json!({"ok":true})))
}
async fn accept_invite(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    origin(&state, &req)?;
    let digest = hash(str_field(&body, "token")?);
    let password = str_field(&body, "password")?.to_string();
    password_valid(&password)?;
    let name = body["name"]
        .as_str()
        .unwrap_or("Сотрудник")
        .trim()
        .chars()
        .take(120)
        .collect::<String>();
    let encoded = password_hash(&password)?;
    let demo = state.demo;
    let (token,value)=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  let grant=diesel::sql_query("SELECT id,organization_id,user_id,email,role,payload AS _payload FROM token_grants WHERE token_hash=$1 AND kind='invite' AND consumed_at IS NULL AND expires_at>now() FOR UPDATE").bind::<Text,_>(digest).get_result::<Grant>(c).optional()?.ok_or_else(||ApiError::bad("Приглашение отозвано, использовано или истекло"))?;
  let org=grant.organization_id.ok_or_else(ApiError::missing)?;let email=grant.email.ok_or_else(ApiError::missing)?;
  let old=diesel::sql_query("SELECT id,email,name,password_hash FROM users WHERE email=$1").bind::<Text,_>(&email).get_result::<UserRow>(c).optional()?;
  let user=if let Some(user)=old{if !verify_password(&password,&user.password_hash){return Err(ApiError::new(401,"existing_account_password","Для существующего аккаунта укажите текущий пароль"));}user}else{
   diesel::sql_query("INSERT INTO users(email,name,password_hash) VALUES($1,$2,$3) RETURNING id,email,name,password_hash").bind::<Text,_>(email).bind::<Text,_>(name).bind::<Text,_>(encoded).get_result::<UserRow>(c)?};
  let role=grant.role.unwrap_or_else(||"reviewer".into());
  let membership_created=diesel::sql_query("INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,$3) ON CONFLICT DO NOTHING").bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(user.id).bind::<Text,_>(&role).execute(c)?>0;
  persistence::save_document(c,org,Uuid::new_v4(),"audit",None,json!({"action":if membership_created{"membership.joined"}else{"invitation.accepted"},"user_id":user.id,"user_name":user.name,"subject_id":user.id,"details":{"invited_role":role,"membership_created":membership_created,"invitation_id":grant.id},"created_at":Utc::now()}),None)?;
  diesel::sql_query("UPDATE token_grants SET consumed_at=now() WHERE id=$1").bind::<SqlUuid,_>(grant.id).execute(c)?;
  create_session(c,&user,org,demo)
 })).await?;
    Ok(session_response(&state, token, value))
}

pub fn bootstrap(
    conn: &mut PgConnection,
    email: &str,
    password: &str,
    name: &str,
    organization: &str,
) -> Result<(Uuid, Uuid), ApiError> {
    let email = email_valid(email)?;
    password_valid(password)?;
    let encoded = password_hash(password)?;
    conn.transaction::<_,ApiError,_>(|c|{
 let user=diesel::sql_query("INSERT INTO users(email,name,password_hash) VALUES($1,$2,$3) ON CONFLICT(email) DO UPDATE SET email=EXCLUDED.email RETURNING id,email,name,password_hash").bind::<Text,_>(email).bind::<Text,_>(name).bind::<Text,_>(encoded).get_result::<UserRow>(c)?;
 let orgs=membership_orgs(c,user.id)?;
 if let Some(org)=orgs.iter().find(|v|v["name"].as_str()==Some(organization)){return Ok((Uuid::parse_str(org["id"].as_str().unwrap()).map_err(|_|ApiError::internal())?,user.id));}
 let org=Uuid::new_v4();diesel::sql_query("INSERT INTO organizations(id,name) VALUES($1,$2)").bind::<SqlUuid,_>(org).bind::<Text,_>(organization).execute(c)?;
 diesel::sql_query("INSERT INTO memberships(organization_id,user_id,role) VALUES($1,$2,'owner')").bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(user.id).execute(c)?;Ok((org,user.id))
 })
}

async fn members(state: web::Data<AppState>, req: HttpRequest) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, false, 2).await?;
    Ok(ok(state.db(move|c|{
 let items=diesel::sql_query("SELECT jsonb_build_object('id',u.id,'email',u.email,'name',u.name,'role',m.role,'extension',m.extension) AS data FROM memberships m JOIN users u ON u.id=m.user_id WHERE m.organization_id=$1 ORDER BY u.name").bind::<SqlUuid,_>(ctx.organization_id).load::<OrgRow>(c)?.into_iter().map(|r|r.data).collect::<Vec<_>>();
 let invitations=diesel::sql_query("SELECT jsonb_build_object('id',id,'email',email,'role',role,'expires_at',expires_at,'revoked',consumed_at IS NOT NULL) AS data FROM token_grants WHERE organization_id=$1 AND kind='invite' ORDER BY created_at DESC LIMIT 100").bind::<SqlUuid,_>(ctx.organization_id).load::<OrgRow>(c)?.into_iter().map(|r|r.data).collect::<Vec<_>>();
 Ok(json!({"items":items,"invitations":invitations}))
 }).await?))
}
async fn invite(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, true, 2).await?;
    let email = email_valid(str_field(&body, "email")?)?;
    let role = str_field(&body, "role")?.to_string();
    if !["owner", "manager", "reviewer"].contains(&role.as_str()) {
        return Err(ApiError::bad("Неверная роль"));
    }
    let token = random_token();
    let digest = hash(&token);
    let target = email.clone();
    let id = Uuid::new_v4();
    let expires = Utc::now() + chrono::Duration::days(3);
    let result=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  diesel::sql_query("UPDATE token_grants SET consumed_at=now() WHERE organization_id=$1 AND email=$2 AND kind='invite' AND consumed_at IS NULL").bind::<SqlUuid,_>(ctx.organization_id).bind::<Text,_>(&email).execute(c)?;
  diesel::sql_query("INSERT INTO token_grants(id,token_hash,kind,organization_id,email,role,expires_at) VALUES($1,$2,'invite',$3,$4,$5,$6)").bind::<SqlUuid,_>(id).bind::<Text,_>(digest).bind::<SqlUuid,_>(ctx.organization_id).bind::<Text,_>(&email).bind::<Text,_>(&role).bind::<Timestamptz,_>(expires).execute(c)?;
  crate::settings::audit(c,&ctx,"invitation.created",Some(id),json!({"role":role}))?;
  Ok(json!({"id":id,"email":email,"role":role,"expires_at":expires,"revoked":false}))
 })).await?;
    let url = format!("{}/invite/accept?token={}", state.public_url, token);
    send_mail(
        &target,
        "Приглашение в Phone Platform",
        &format!(
            "Вас приглашают присоединиться к компании. Откройте {url}\nСсылка действует 3 дня."
        ),
    )
    .await?;
    Ok(ok(result))
}
async fn update_member(
    state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<Uuid>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, true, 2).await?;
    let id = *path;
    let role = body
        .get("role")
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or_else(|| ApiError::bad("Неверная роль"))
        })
        .transpose()?;
    let has_extension = body.get("extension").is_some();
    let ext = if let Some(value) = body.get("extension") {
        if value.is_null() {
            None
        } else {
            Some(
                value
                    .as_str()
                    .filter(|v| v.len() <= 40)
                    .ok_or_else(|| ApiError::bad("Неверный добавочный"))?
                    .to_owned(),
            )
        }
    } else {
        None
    };
    if let Some(r) = &role {
        if !["owner", "manager", "reviewer"].contains(&r.as_str()) {
            return Err(ApiError::bad("Неверная роль"));
        }
    }
    state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  diesel::sql_query("SELECT jsonb_build_object('id',id) AS data FROM organizations WHERE id=$1 FOR UPDATE").bind::<SqlUuid,_>(ctx.organization_id).get_result::<OrgRow>(c)?;
  let target=diesel::sql_query("SELECT jsonb_build_object('role',role) AS data FROM memberships WHERE organization_id=$1 AND user_id=$2").bind::<SqlUuid,_>(ctx.organization_id).bind::<SqlUuid,_>(id).get_result::<OrgRow>(c).optional()?.ok_or_else(ApiError::missing)?;
  if target.data["role"]=="owner" && role.as_deref().is_some_and(|v|v!="owner"){
   let count=diesel::sql_query("SELECT count(*) FROM memberships WHERE organization_id=$1 AND role='owner'").bind::<SqlUuid,_>(ctx.organization_id).get_result::<Count>(c)?.count;
   if count<2{return Err(ApiError::bad("Нельзя изменить роль последнего владельца"));}
  }
  let access_change=json!({"previous_role":target.data["role"],"requested_role":role.clone(),"extension_changed":has_extension});
  diesel::sql_query("UPDATE memberships SET role=COALESCE($1,role),extension=CASE WHEN $2 THEN $3 ELSE extension END WHERE organization_id=$4 AND user_id=$5").bind::<Nullable<Text>,_>(role).bind::<Bool,_>(has_extension).bind::<Nullable<Text>,_>(ext).bind::<SqlUuid,_>(ctx.organization_id).bind::<SqlUuid,_>(id).execute(c)?;
  crate::settings::audit(c,&ctx,"membership.updated",Some(id),access_change)?;
  Ok(())
 })).await?;
    Ok(ok(json!({"ok":true})))
}
async fn remove_member(
    state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, true, 2).await?;
    let id = *path;
    state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  diesel::sql_query("SELECT jsonb_build_object('id',id) AS data FROM organizations WHERE id=$1 FOR UPDATE").bind::<SqlUuid,_>(ctx.organization_id).get_result::<OrgRow>(c)?;
  let current=diesel::sql_query("SELECT jsonb_build_object('role',role) AS data FROM memberships WHERE organization_id=$1 AND user_id=$2").bind::<SqlUuid,_>(ctx.organization_id).bind::<SqlUuid,_>(id).get_result::<OrgRow>(c).optional()?.ok_or_else(ApiError::missing)?;
  if current.data["role"]=="owner"{let count=diesel::sql_query("SELECT count(*) FROM memberships WHERE organization_id=$1 AND role='owner'").bind::<SqlUuid,_>(ctx.organization_id).get_result::<Count>(c)?.count;if count<2{return Err(ApiError::bad("Нельзя удалить последнего владельца"));}}
  diesel::sql_query("DELETE FROM memberships WHERE organization_id=$1 AND user_id=$2").bind::<SqlUuid,_>(ctx.organization_id).bind::<SqlUuid,_>(id).execute(c)?;crate::settings::audit(c,&ctx,"membership.removed",Some(id),json!({"previous_role":current.data["role"]}))?;Ok(())
 })).await?;
    Ok(ok(json!({"ok":true})))
}
async fn revoke_invite(
    state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = context(&state, &req, true, 2).await?;
    let id = *path;
    state.db(move|c|c.transaction::<_,ApiError,_>(|c|{let count=diesel::sql_query("UPDATE token_grants SET consumed_at=now() WHERE id=$1 AND organization_id=$2 AND kind='invite' AND consumed_at IS NULL").bind::<SqlUuid,_>(id).bind::<SqlUuid,_>(ctx.organization_id).execute(c)?;if count==0{return Err(ApiError::missing());}crate::settings::audit(c,&ctx,"invitation.revoked",Some(id),json!({}))?;Ok(())})).await?;
    Ok(ok(json!({"ok":true})))
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.route("/api/auth/login", web::post().to(login))
        .route("/api/auth/me", web::get().to(me))
        .route("/api/auth/logout", web::post().to(logout))
        .route("/api/auth/organization", web::post().to(switch_org))
        .route("/api/auth/reset/request", web::post().to(reset_request))
        .route("/api/auth/reset/confirm", web::post().to(reset_confirm))
        .route(
            "/api/auth/invitations/accept",
            web::post().to(accept_invite),
        )
        .route("/api/members", web::get().to(members))
        .route("/api/members/invite", web::post().to(invite))
        .route(
            "/api/members/invitations/{id}",
            web::delete().to(revoke_invite),
        )
        .route("/api/members/{id}", web::patch().to(update_member))
        .route("/api/members/{id}", web::delete().to(remove_member));
}
