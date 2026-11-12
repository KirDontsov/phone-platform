use super::{runtime, store};
use crate::{auth as sessions, json, ok, settings, ApiError, AppState, Uuid, Value};
use actix_web::{web, HttpRequest, HttpResponse};
use chrono::{Duration, Utc};
use diesel::prelude::*;

pub(super) fn control_id(org: Uuid) -> Uuid {
    Uuid::new_v5(&org, b"assistant-auth-control")
}
fn next_generation(previous: i64, remote: i64) -> i64 {
    Utc::now()
        .timestamp_micros()
        .max(previous.saturating_add(1))
        .max(remote.saturating_add(1))
        .min(9_007_199_254_740_991)
}
fn latest_generation(value: &Value) -> i64 {
    ["generation", "pending_generation", "last_generation"]
        .iter()
        .filter_map(|k| value[*k].as_i64())
        .max()
        .unwrap_or(0)
}
fn stage_control(previous: Option<&Value>, user: Uuid, attempt: Uuid, generation: i64) -> Value {
    let mut data = previous.cloned().unwrap_or_else(
        || json!({"user_id":user,"generation":0,"authenticated":false,"confirmed_generation":null}),
    );
    data["pending_attempt"] = json!(attempt);
    data["pending_generation"] = json!(generation);
    data
}
fn commit_control(data: &mut Value, user: Uuid, generation: i64) {
    data["user_id"] = json!(user);
    data["generation"] = json!(generation);
    data["authenticated"] = json!(true);
    data["confirmed_generation"] = json!(generation);
    data["pending_attempt"] = Value::Null;
    data["pending_generation"] = Value::Null;
}
fn unavailable(code: &str) -> Value {
    json!({"available":std::env::var("ASSISTANT_RUNNER_TOKEN").is_ok_and(|v|!v.is_empty()),"runner_ready":false,"authenticated":false,"auth_mode":null,"codex_version":null,"model":null,"error_code":code,"pending_login":null})
}
pub async fn status(state: &AppState, ctx: &sessions::Context) -> Result<Value, ApiError> {
    let org = ctx.organization_id;
    let data = state
        .db(move |c| Ok(persistence::get_document(c, org, control_id(org))?))
        .await?;
    let raw = match runtime::rpc("/auth/status", json!({"organization_id":org}), 5).await {
        Ok(v) => v,
        Err(_) => return Ok(unavailable("assistant_runner_unavailable")),
    };
    let confirmed = data.as_ref().is_some_and(|d| {
        d.data["authenticated"] == true
            && d.data["confirmed_generation"].as_i64() == raw["generation"].as_i64()
            && raw["generation"].is_i64()
    });
    let authenticated = confirmed && raw["authenticated"] == true;
    let mut pending = Value::Null;
    if let Some(control) = data.as_ref() {
        if let Some(id) = control.data["pending_attempt"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            let context = ctx.clone();
            let remote_generation = raw
                .get("pending_generation")
                .filter(|v| v.is_i64())
                .unwrap_or(&raw["generation"])
                .clone();
            let allowed = state
                .db(move |c| {
                    let d = store::document(c, &context, id, store::AUTH_ATTEMPT).ok();
                    Ok(d.is_some_and(|d| {
                        d.data["session_hash"] == context.token_hash
                            && matches!(d.data["state"].as_str(), Some("pending" | "prepared"))
                            && store::valid_deadline(&d.data)
                            && d.data["generation"] == remote_generation
                    }))
                })
                .await?;
            if allowed {
                pending = raw_pending(&raw["pending_login"]);
            }
        }
    }
    let mode = raw["auth_mode"]
        .as_str()
        .filter(|s| ["apiKey", "chatgpt"].contains(s));
    let error = if raw["error_code"].is_string() {
        raw["error_code"].clone()
    } else if !authenticated && pending.is_null() {
        json!("assistant_auth_required")
    } else {
        Value::Null
    };
    Ok(
        json!({"available":true,"runner_ready":true,"authenticated":authenticated,"auth_mode":if authenticated{json!(mode)}else{Value::Null},"codex_version":raw["codex_version"].as_str().filter(|s|s.len()<50),"model":raw["model"].as_str().filter(|s|s.len()<200),"error_code":error,"pending_login":pending,"auth_generation":if authenticated{raw["generation"].clone()}else{Value::Null}}),
    )
}
fn raw_pending(v: &Value) -> Value {
    let Some(url) = v["verification_url"]
        .as_str()
        .and_then(|s| reqwest::Url::parse(s).ok())
    else {
        return Value::Null;
    };
    if url.scheme() != "https"
        || url.host_str() != Some("auth.openai.com")
        || url.path() != "/codex/device"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Value::Null;
    }
    let Some(code) = v["user_code"].as_str().filter(|s| {
        s.len() <= 64
            && s.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'-')
    }) else {
        return Value::Null;
    };
    let Some(id) = v["login_id"].as_str().filter(|s| s.len() <= 128) else {
        return Value::Null;
    };
    if !store::parse_time(&v["expires_at"]).is_some_and(|d| d > Utc::now()) {
        return Value::Null;
    }
    json!({"verification_url":url.as_str(),"user_code":code,"login_id":id,"expires_at":v["expires_at"]})
}
async fn public_status(
    state: web::Data<AppState>,
    req: HttpRequest,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, false, 2).await?;
    let mut data = status(&state, &ctx).await?;
    data.as_object_mut().unwrap().remove("auth_generation");
    Ok(ok(data))
}
fn revoke_runs(c: &mut diesel::PgConnection, org: Uuid) -> Result<Vec<Uuid>, ApiError> {
    let runs = persistence::list_documents(c, org, store::RUN)?
        .into_iter()
        .filter(|d| store::active(&d.data))
        .collect::<Vec<_>>();
    for run in &runs {
        store::finish(
            c,
            org,
            run.id,
            "cancelled",
            Some("assistant_auth_changed"),
            None,
            None,
            None,
        )?;
    }
    Ok(runs.into_iter().map(|r| r.id).collect())
}
async fn stop_runs(org: Uuid, runs: Vec<Uuid>) {
    for run in runs {
        runtime::abort(org, run);
        let _ = runtime::rpc("/cancel", json!({"organization_id":org,"run_id":run}), 2).await;
    }
}
async fn start(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    store::strict_keys(&body, &["type", "api_key"])?;
    let type_ = crate::str_field(&body, "type")?.to_owned();
    if !["apiKey", "chatgptDeviceCode"].contains(&type_.as_str()) {
        return Err(ApiError::bad("Выберите способ подключения Codex"));
    }
    let key = if type_ == "apiKey" {
        let key = crate::str_field(&body, "api_key")?.trim().to_owned();
        if key.len() < 10 || key.len() > 8192 || key.chars().any(char::is_control) {
            return Err(ApiError::bad("Проверьте ключ доступа к Codex"));
        }
        Some(key)
    } else {
        if body.get("api_key").is_some() {
            return Err(ApiError::bad("Ключ не нужен для входа по коду"));
        }
        None
    };
    let org = ctx.organization_id;
    let remote = runtime::rpc("/auth/status", json!({"organization_id":org}), 5).await?;
    let remote_gen = latest_generation(&remote);
    let token = sessions::random_token();
    let hash = sessions::hash(&token);
    let context = ctx.clone();
    let requested = type_.clone();
    let(attempt,generation)=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  settings::lock_settings_revision(c,context.organization_id)?;store::live_context(c,context.organization_id,context.user_id,&context.token_hash,true)?;
  let id=control_id(context.organization_id);let old=persistence::get_document(c,context.organization_id,id)?;let generation=next_generation(old.as_ref().map(|d|latest_generation(&d.data)).unwrap_or(0),remote_gen);
  if let Some(previous)=old.as_ref().and_then(|d|d.data["pending_attempt"].as_str()).and_then(|s|Uuid::parse_str(s).ok()){if let Some(mut previous)=persistence::get_document(c,context.organization_id,previous)?{previous.data["state"]=json!("revoked");previous.data.as_object_mut().unwrap().remove("capability_hash");let data=previous.data.clone();store::save(c,previous,data)?;}}
  let attempt=Uuid::new_v4();persistence::save_document(c,context.organization_id,id,store::AUTH_CONTROL,None,stage_control(old.as_ref().map(|d|&d.data),context.user_id,attempt,generation),old.as_ref().map(|d|d.version))?;
  persistence::save_document(c,context.organization_id,attempt,store::AUTH_ATTEMPT,Some(id),json!({"user_id":context.user_id,"session_hash":context.token_hash,"generation":generation,"capability_hash":hash,"state":"pending","type":requested,"expires_at":Utc::now()+Duration::minutes(10)}),None)?;
  settings::audit(c,&context,"assistant.auth_started",Some(attempt),json!({"type":requested}))?;Ok((attempt,generation))
 })).await?;
    let mut payload = json!({"organization_id":org,"auth_attempt_id":attempt,"generation":generation,"authorization_token":token,"type":type_});
    if let Some(key) = key {
        payload["api_key"] = json!(key);
    }
    let started = runtime::rpc("/auth/start", payload, 35).await;
    if started.is_err() {
        state
            .db(move |c| {
                if let Some(mut d) = persistence::get_document(c, org, attempt)? {
                    if d.data["state"] == "committed" {
                        return Ok(());
                    }
                    d.data["state"] = json!("failed");
                    d.data.as_object_mut().unwrap().remove("capability_hash");
                    let data = d.data.clone();
                    store::save(c, d, data)?;
                }
                if let Some(mut control) = persistence::get_document(c, org, control_id(org))? {
                    if control.data["pending_generation"] == generation {
                        control.data["pending_attempt"] = Value::Null;
                        control.data["pending_generation"] = Value::Null;
                        let data = control.data.clone();
                        store::save(c, control, data)?;
                    }
                }
                Ok(())
            })
            .await?;
        return Err(ApiError::new(
            503,
            "assistant_login_failed",
            "Не удалось подключить Codex. Повторите вход",
        ));
    }
    if let Some(login_id) = started.ok().and_then(|v| {
        v["pending_login"]["login_id"]
            .as_str()
            .filter(|id| id.len() <= 128)
            .map(str::to_owned)
    }) {
        state
            .db(move |c| {
                c.transaction::<_, ApiError, _>(|c| {
                    settings::lock_settings_revision(c, org)?;
                    if let Some(mut d) = persistence::get_document(c, org, attempt)? {
                        if d.data["generation"] == generation
                            && matches!(d.data["state"].as_str(), Some("pending" | "prepared"))
                        {
                            d.data["login_id"] = json!(login_id);
                            let data = d.data.clone();
                            store::save(c, d, data)?;
                        }
                    }
                    Ok(())
                })
            })
            .await?;
    }
    let ctx = sessions::context(&state, &req, false, 2).await?;
    let mut data = status(&state, &ctx).await?;
    data.as_object_mut().unwrap().remove("auth_generation");
    Ok(ok(data))
}
async fn revoke(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    let cancel = req.path().ends_with("/cancel");
    store::strict_keys(&body, if cancel { &["login_id"] } else { &[] })?;
    let login = if cancel {
        Some(crate::str_field(&body, "login_id")?.to_owned())
    } else {
        None
    };
    if login.as_ref().is_some_and(|s| s.len() > 128) {
        return Err(ApiError::bad("Неверный ID входа"));
    }
    let org = ctx.organization_id;
    let remote = runtime::rpc("/auth/status", json!({"organization_id":org}), 3)
        .await
        .ok();
    if let (Some(login), Some(remote)) = (&login, &remote) {
        if remote["pending_login"]["login_id"] != *login {
            return Err(ApiError::new(
                409,
                "login_changed",
                "Попытка входа изменилась",
            ));
        }
    }
    let remote_gen = remote.as_ref().map(latest_generation).unwrap_or(0);
    let context = ctx.clone();
    let expected_login = login.clone();
    let(generation,runs)=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  settings::lock_settings_revision(c,context.organization_id)?;store::live_context(c,context.organization_id,context.user_id,&context.token_hash,true)?;
  let id=control_id(context.organization_id);let old=persistence::get_document(c,context.organization_id,id)?;
  if let Some(login)=expected_login.as_ref(){
    let pending=old.as_ref().and_then(|d|d.data["pending_attempt"].as_str()).and_then(|id|Uuid::parse_str(id).ok());
    let pending=match pending{Some(id)=>persistence::get_document(c,context.organization_id,id)?,None=>None};
    if !pending.is_some_and(|d|d.data["login_id"]==*login && matches!(d.data["state"].as_str(),Some("pending"|"prepared")) && d.data["session_hash"]==context.token_hash && store::valid_deadline(&d.data)) {return Err(ApiError::new(409,"login_changed","Попытка входа изменилась"));}
  }
  let generation=next_generation(old.as_ref().map(|d|latest_generation(&d.data)).unwrap_or(0),remote_gen);
  if let Some(attempt)=old.as_ref().and_then(|d|d.data["pending_attempt"].as_str()).and_then(|s|Uuid::parse_str(s).ok()){if let Some(mut d)=persistence::get_document(c,context.organization_id,attempt)?{d.data["state"]=json!("revoked");d.data.as_object_mut().unwrap().remove("capability_hash");let data=d.data.clone();store::save(c,d,data)?;}}
  persistence::save_document(c,context.organization_id,id,store::AUTH_CONTROL,None,json!({"user_id":context.user_id,"generation":generation,"authenticated":false,"confirmed_generation":null,"pending_attempt":null,"pending_generation":null}),old.map(|d|d.version))?;
  let runs=revoke_runs(c,context.organization_id)?;settings::audit(c,&context,"assistant.auth_revoked",Some(id),json!({}))?;Ok((generation,runs))
 })).await?;
    stop_runs(org, runs).await;
    let mut payload = json!({"organization_id":org,"generation":generation});
    if let Some(login) = login {
        payload["login_id"] = json!(login);
    }
    let _ = runtime::rpc(
        if cancel {
            "/auth/cancel"
        } else {
            "/auth/logout"
        },
        payload,
        10,
    )
    .await;
    let mut data = status(&state, &ctx).await?;
    data.as_object_mut().unwrap().remove("auth_generation");
    Ok(ok(data))
}
async fn complete(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let hash = store::bearer(&req)?;
    store::strict_keys(&body, &["auth_attempt_id", "generation", "phase"])?;
    let id = crate::id_field(&body, "auth_attempt_id")?;
    let generation = body["generation"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("Неверная версия авторизации"))?;
    let phase = match body.get("phase") {
        None => "commit",
        Some(Value::String(value)) => value.as_str(),
        Some(_) => return Err(ApiError::bad("Неверная стадия подключения")),
    }
    .to_owned();
    if !["prepare", "commit", "status"].contains(&phase.as_str()) {
        return Err(ApiError::bad("Неверная стадия подключения"));
    }
    let (result,org,runs) = state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
        let candidate=store::capability(c,&hash,store::AUTH_ATTEMPT)?;
        if candidate.id!=id{return Err(ApiError::forbidden());}
        let org=candidate.organization_id;
        settings::lock_settings_revision(c,org)?;
        store::lock_document(c,org,control_id(org))?;store::lock_document(c,org,id)?;
        let attempt=persistence::get_document(c,org,id)?.ok_or_else(ApiError::missing)?;
        let mut control=persistence::get_document(c,org,control_id(org))?.ok_or_else(ApiError::missing)?;
        let committed=attempt.data["state"]=="committed" && control.data["generation"]==generation && control.data["confirmed_generation"]==generation && control.data["authenticated"]==true;
        let pending=control.data["pending_generation"]==generation && control.data["pending_attempt"]==id.to_string();
        if !matches!(attempt.data["state"].as_str(),Some("pending"|"prepared"|"approved"|"committed")) || attempt.data["capability_hash"]!=hash || attempt.data["generation"]!=generation || (!pending && !committed) || !store::valid_deadline(&attempt.data) {
            return Err(ApiError::new(403,"assistant_authorization_revoked","Попытка входа отозвана"));
        }
        let user=crate::id_field(&attempt.data,"user_id")?;
        let context=store::live_context(c,org,user,attempt.data["session_hash"].as_str().ok_or_else(ApiError::missing)?,true)?;
        if phase=="status" {return Ok((json!({"approved":true,"generation":generation,"committed":committed,"active_generation":control.data["generation"]}),org,vec![]));}
        if committed {return Ok((json!({"approved":true,"generation":generation,"committed":true}),org,vec![]));}
        let mut data=attempt.data.clone();
        if phase=="prepare" {
            data["state"]=json!("prepared");store::save(c,attempt,data)?;
            return Ok((json!({"approved":true,"generation":generation,"committed":false}),org,vec![]));
        }
        data["state"]=json!("committed");store::save(c,attempt,data)?;
        commit_control(&mut control.data,user,generation);
        let data=control.data.clone();store::save(c,control,data)?;
        let runs=revoke_runs(c,org)?;
        settings::audit(c,&context,"assistant.auth_connected",Some(id),json!({}))?;
        Ok((json!({"approved":true,"generation":generation,"committed":true}),org,runs))
    })).await?;
    stop_runs(org, runs).await;
    Ok(ok(result))
}
pub fn configure(c: &mut web::ServiceConfig) {
    c.route("/api/assistant/status", web::get().to(public_status))
        .route("/api/assistant/auth/start", web::post().to(start))
        .route("/api/assistant/auth/cancel", web::post().to(revoke))
        .route("/api/assistant/auth/logout", web::post().to(revoke))
        .route("/api/assistant/auth/complete", web::post().to(complete));
}

#[cfg(test)]
mod preservation_tests {
    use super::*;
    #[test]
    fn staging_preserves_active_authority_and_only_commit_replaces_it() {
        let owner = Uuid::new_v4();
        let requester = Uuid::new_v4();
        let attempt = Uuid::new_v4();
        let previous =
            json!({"user_id":owner,"generation":7,"authenticated":true,"confirmed_generation":7});
        let mut staged = stage_control(Some(&previous), requester, attempt, 11);
        assert_eq!(staged["generation"], 7);
        assert_eq!(staged["confirmed_generation"], 7);
        assert_eq!(staged["authenticated"], true);
        assert_eq!(staged["user_id"], owner.to_string());
        assert_eq!(staged["pending_generation"], 11);
        assert_eq!(latest_generation(&staged), 11);
        commit_control(&mut staged, requester, 11);
        assert_eq!(staged["generation"], 11);
        assert_eq!(staged["confirmed_generation"], 11);
        assert_eq!(staged["pending_generation"], Value::Null);
        assert_eq!(staged["pending_attempt"], Value::Null);
        let fresh = stage_control(None, owner, attempt, 13);
        assert_eq!(fresh["authenticated"], false);
        assert_eq!(fresh["confirmed_generation"], Value::Null);
        assert_eq!(fresh["generation"], 0);
    }
}
