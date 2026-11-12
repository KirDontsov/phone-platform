//! Configuration assistant: server-owned authority, explicit private projections.
mod model_auth;
mod runtime;
mod store;
pub mod tools;
use crate::{auth as sessions, json, ok, settings, ApiError, AppState, Document, Uuid, Value};
use actix_web::{web, HttpRequest, HttpResponse};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use diesel::prelude::*;
use serde::Deserialize;
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

static CAPACITY: OnceLock<Arc<Semaphore>> = OnceLock::new();
fn capacity() -> Arc<Semaphore> {
    CAPACITY.get_or_init(|| Arc::new(Semaphore::new(2))).clone()
}
fn request_id(v: &Value) -> Result<Uuid, ApiError> {
    crate::id_field(v, "client_request_id")
}
fn title(v: Option<&str>) -> Result<String, ApiError> {
    let title = v.unwrap_or("Новый чат").trim();
    if title.is_empty() || title.chars().count() > 120 || sensitive_text(title) {
        return Err(ApiError::bad(
            "Проверьте название чата; ключи передавайте защищённой карточкой",
        ));
    }
    Ok(title.to_owned())
}
pub(crate) fn sensitive_text(text: &str) -> bool {
    let lower = text.to_lowercase();
    if lower.contains("-----begin") && lower.contains("private key-----") {
        return true;
    }
    if text
        .split(|c: char| c.is_whitespace() || "\"'{}[],:".contains(c))
        .any(|s| s.starts_with("sk-") && s.len() > 20)
    {
        return true;
    }
    for marker in [
        "app_secret",
        "app secret",
        "api_key",
        "api key",
        "access_token",
        "refresh_token",
        "client_secret",
        "password",
        "пароль",
    ] {
        let mut tail = lower.as_str();
        while let Some(index) = tail.find(marker) {
            tail = &tail[index + marker.len()..];
            let rest =
                tail.trim_start_matches(|c: char| c.is_whitespace() || c == '\"' || c == '\'');
            if let Some(rest) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=')) {
                if rest
                    .trim_matches(|c: char| c.is_whitespace() || "\"'{}[],".contains(c))
                    .chars()
                    .count()
                    > 5
                {
                    return true;
                }
            }
        }
    }
    false
}
#[derive(Deserialize)]
struct Listing {
    limit: Option<usize>,
    cursor: Option<String>,
}
async fn list(
    state: web::Data<AppState>,
    req: HttpRequest,
    q: web::Query<Listing>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, false, 2).await?;
    let limit = q.limit.unwrap_or(30).clamp(1, 50);
    let cursor = q.cursor.clone();
    Ok(ok(state.db(move|c|{
  let scope=sessions::hash(&format!("{}:{}",ctx.organization_id,ctx.user_id));
  let mut items=persistence::list_documents(c,ctx.organization_id,store::CONVERSATION)?.into_iter().filter(|d|d.data["user_id"]==ctx.user_id.to_string()).collect::<Vec<_>>();
  items.sort_by(|a,b|b.updated_at.cmp(&a.updated_at).then(b.id.cmp(&a.id)));
  if let Some(cursor)=cursor{let raw=URL_SAFE_NO_PAD.decode(cursor).map_err(|_|ApiError::bad("Неверный курсор"))?;let value:Value=serde_json::from_slice(&raw).map_err(|_|ApiError::bad("Неверный курсор"))?;if value["scope"]!=scope{return Err(ApiError::bad("Курсор другого пользователя или компании"));}let index=items.iter().position(|d|value["id"]==d.id.to_string()).ok_or_else(||ApiError::new(409,"cursor_changed","Список изменился. Обновите историю"))?;items.drain(..=index);}
  let more=items.len()>limit;items.truncate(limit);let next=if more{items.last().map(|d|URL_SAFE_NO_PAD.encode(json!({"scope":scope,"id":d.id}).to_string()))}else{None};
  Ok(json!({"items":items.iter().map(store::conversation_json).collect::<Vec<_>>(),"next_cursor":next}))
 }).await?))
}
async fn create(
    state: web::Data<AppState>,
    req: HttpRequest,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    store::strict_keys(&body, &["title", "client_request_id"])?;
    let client = request_id(&body)?;
    let name = title(body["title"].as_str())?;
    let fingerprint = sessions::hash(&json!({"title":name}).to_string());
    let id = Uuid::new_v5(
        &ctx.organization_id,
        format!("assistant-conversation:{}:{client}", ctx.user_id).as_bytes(),
    );
    Ok(ok(state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  settings::lock_settings_revision(c,ctx.organization_id)?;
  if let Some(d)=persistence::get_document(c,ctx.organization_id,id)?{if d.kind!=store::CONVERSATION||d.data["user_id"]!=ctx.user_id.to_string()||d.data["request_hash"]!=fingerprint{return Err(ApiError::new(409,"idempotency_conflict","ID запроса уже использован"));}return Ok(store::conversation_json(&d));}
  if persistence::list_documents(c,ctx.organization_id,store::CONVERSATION)?.iter().filter(|d|d.data["user_id"]==ctx.user_id.to_string()).count()>=100{return Err(ApiError::new(429,"assistant_history_limit","Удалите ненужные чаты перед созданием новых"));}
  let d=persistence::save_document(c,ctx.organization_id,id,store::CONVERSATION,None,json!({"user_id":ctx.user_id,"title":name,"client_request_id":client,"request_hash":fingerprint,"active_run_id":null}),None)?;
  settings::audit(c,&ctx,"assistant.conversation_created",Some(id),json!({}))?;Ok(store::conversation_json(&d))
 })).await?))
}
fn detail_in_db(
    c: &mut diesel::PgConnection,
    ctx: &sessions::Context,
    id: Uuid,
) -> Result<Value, ApiError> {
    store::recover(c, ctx, id)?;
    let d = store::document(c, ctx, id, store::CONVERSATION)?;
    let children = persistence::list_children(c, ctx.organization_id, id, None)?;
    let ordered = store::ordered_messages(
        children
            .iter()
            .filter(|d| d.kind == store::MESSAGE && d.data["user_id"] == ctx.user_id.to_string())
            .cloned()
            .collect(),
    );
    Ok(
        json!({"conversation":store::conversation_json(&d),"messages":ordered.iter().take(500).map(store::message_json).collect::<Vec<_>>(),"runs":children.iter().filter(|d|d.kind==store::RUN&&d.data["user_id"]==ctx.user_id.to_string()).map(store::run_json).collect::<Vec<_>>(),"credentials":children.iter().filter(|d|d.kind==store::CREDENTIAL&&d.data["user_id"]==ctx.user_id.to_string()&&store::valid_deadline(&d.data)&&d.data["config_encrypted"].is_string()).map(store::credential_json).collect::<Vec<_>>()}),
    )
}
async fn detail(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, false, 2).await?;
    let id = *id;
    Ok(ok(state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                store::conversation(c, &ctx, id)?;
                detail_in_db(c, &ctx, id)
            })
        })
        .await?))
}
async fn rename(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    store::strict_keys(&body, &["title", "version"])?;
    let id = *id;
    let name = title(Some(crate::str_field(&body, "title")?))?;
    let expected = body["version"]
        .as_i64()
        .ok_or_else(|| ApiError::bad("Нужна версия чата"))?;
    Ok(ok(state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                let mut d = store::conversation(c, &ctx, id)?;
                if d.version != expected {
                    return Err(ApiError::new(
                        409,
                        "conflict",
                        "Чат изменился. Обновите данные",
                    ));
                }
                d.data["title"] = json!(name);
                let data = d.data.clone();
                let d = store::save(c, d, data)?;
                settings::audit(
                    c,
                    &ctx,
                    "assistant.conversation_renamed",
                    Some(id),
                    json!({}),
                )?;
                Ok(store::conversation_json(&d))
            })
        })
        .await?))
}
async fn delete(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    let id = *id;
    let org = ctx.organization_id;
    let runs = state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                store::conversation(c, &ctx, id)?;
                let runs =
                    persistence::list_children(c, ctx.organization_id, id, Some(store::RUN))?;
                for run in &runs {
                    store::finish(
                        c,
                        ctx.organization_id,
                        run.id,
                        "cancelled",
                        Some("assistant_cancelled"),
                        None,
                        None,
                        None,
                    )?;
                }
                persistence::tombstone(c, ctx.organization_id, id, None)?;
                persistence::delete_document(c, ctx.organization_id, id)?;
                settings::audit(
                    c,
                    &ctx,
                    "assistant.conversation_deleted",
                    Some(id),
                    json!({}),
                )?;
                Ok(runs.into_iter().map(|d| d.id).collect::<Vec<_>>())
            })
        })
        .await?;
    for run in runs {
        runtime::abort(org, run);
        let _ = runtime::rpc("/cancel", json!({"organization_id":org,"run_id":run}), 3).await;
    }
    Ok(ok(json!({"ok":true})))
}
async fn credentials(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    store::strict_keys(&body, &["kind", "label", "config"])?;
    if body["kind"] != "telphin" {
        return Err(ApiError::bad("Поддерживаются доступы Телфина"));
    }
    let config = body["config"].clone();
    store::strict_keys(&config, &["host", "app_id", "app_secret", "client_id"])?;
    for key in ["app_id", "app_secret", "client_id"] {
        let v = crate::str_field(&config, key)?;
        if v.len() > 4096 || v.chars().any(char::is_control) {
            return Err(ApiError::bad("Проверьте формат доступов"));
        }
    }
    let client = config["client_id"].as_str().unwrap();
    if client.len() > 40 || !client.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ApiError::bad("Нужен числовой ID клиента АТС"));
    }
    if !["apiproxy.telphin.ru", "altapi.telphin.ru"]
        .contains(&config["host"].as_str().unwrap_or("apiproxy.telphin.ru"))
    {
        return Err(ApiError::bad("Неверный сервер Телфина"));
    }
    let mut label = body["label"]
        .as_str()
        .unwrap_or("Доступ Телфин")
        .trim()
        .chars()
        .take(100)
        .collect::<String>();
    if label.is_empty()
        || sensitive_text(&label)
        || config
            .as_object()
            .unwrap()
            .values()
            .filter_map(Value::as_str)
            .any(|v| v.len() > 5 && label.contains(v))
    {
        label = "Доступ Телфин".into();
    }
    let encrypted = contracts::seal(&config, &state.key).map_err(|_| ApiError::internal())?;
    let conversation = *id;
    Ok(ok(state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  store::conversation(c,&ctx,conversation)?;let count=persistence::list_children(c,ctx.organization_id,conversation,Some(store::CREDENTIAL))?.iter().filter(|d|store::valid_deadline(&d.data)&&d.data["config_encrypted"].is_string()).count();if count>=8{return Err(ApiError::new(429,"assistant_credential_limit","Удалите лишние доступы перед добавлением новых"));}
  let id=Uuid::new_v4();let d=persistence::save_document(c,ctx.organization_id,id,store::CREDENTIAL,Some(conversation),json!({"user_id":ctx.user_id,"kind":"telphin","label":label,"config_encrypted":encrypted,"expires_at":Utc::now()+Duration::hours(24)}),None)?;
  settings::audit(c,&ctx,"assistant.credential_added",Some(id),json!({"kind":"telphin"}))?;Ok(store::credential_json(&d))
 })).await?))
}
async fn delete_credential(
    state: web::Data<AppState>,
    req: HttpRequest,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    let (conversation, id) = path.into_inner();
    state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                store::conversation(c, &ctx, conversation)?;
                let d = store::document(c, &ctx, id, store::CREDENTIAL)?;
                if d.parent_id != Some(conversation) {
                    return Err(ApiError::missing());
                }
                persistence::tombstone(c, ctx.organization_id, id, None)?;
                persistence::delete_document(c, ctx.organization_id, id)?;
                settings::audit(c, &ctx, "assistant.credential_deleted", Some(id), json!({}))?;
                Ok(())
            })
        })
        .await?;
    Ok(ok(json!({"ok":true})))
}
fn run_response(
    c: &mut diesel::PgConnection,
    ctx: &sessions::Context,
    run: &Document,
) -> Result<Value, ApiError> {
    let message = |c: &mut diesel::PgConnection, key: &str| -> Result<Value, ApiError> {
        let id = run.data[key]
            .as_str()
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or_else(ApiError::missing)?;
        Ok(store::message_json(&store::document(
            c,
            ctx,
            id,
            store::MESSAGE,
        )?))
    };
    Ok(
        json!({"run":store::run_json(run),"user_message":message(c,"user_message_id")?,"assistant_message":message(c,"assistant_message_id")?}),
    )
}
async fn message(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
    body: web::Json<Value>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    store::strict_keys(&body, &["text", "client_request_id", "credential_refs"])?;
    let text = crate::str_field(&body, "text")?.trim().to_owned();
    if text.chars().count() > 8000 {
        return Err(ApiError::bad("Сообщение слишком длинное"));
    }
    if sensitive_text(&text) {
        return Err(ApiError::new(400,"assistant_secret_in_message","Доступы передавайте через защищённую карточку «Добавить доступы», а не обычным сообщением"));
    }
    let refs = if let Some(v) = body.get("credential_refs") {
        let list = v
            .as_array()
            .filter(|v| v.len() <= 8)
            .ok_or_else(|| ApiError::bad("Не более восьми доступов"))?;
        list.iter()
            .map(|v| {
                v.as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .ok_or_else(|| ApiError::bad("Неверный ID доступа"))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        vec![]
    };
    let client = request_id(&body)?;
    let conversation = *id;
    let run_id = Uuid::new_v5(&conversation, format!("run:{client}").as_bytes());
    let fingerprint = sessions::hash(&json!({"text":text,"credential_refs":refs}).to_string());
    let context = ctx.clone();
    let prior = state
        .db(move |c| {
            store::document(c, &context, conversation, store::CONVERSATION)?;
            if let Some(run) = persistence::get_document(c, context.organization_id, run_id)? {
                if run.kind != store::RUN
                    || run.data["user_id"] != context.user_id.to_string()
                    || run.data["request_hash"] != fingerprint
                {
                    return Err(ApiError::new(
                        409,
                        "idempotency_conflict",
                        "ID сообщения уже использован с другим содержимым",
                    ));
                }
                Ok(Some(run_response(c, &context, &run)?))
            } else {
                Ok(None)
            }
        })
        .await?;
    if let Some(data) = prior {
        return Ok(HttpResponse::Accepted().json(json!({"data":data})));
    }
    let status = model_auth::status(&state, &ctx).await?;
    if status["authenticated"] != true {
        return Err(ApiError::new(
            409,
            "assistant_auth_required",
            "Подключите Codex, чтобы отправить сообщение",
        ));
    }
    let generation = status["auth_generation"]
        .as_i64()
        .ok_or_else(ApiError::internal)?;
    let permit = capacity().try_acquire_owned().map_err(|_| {
        ApiError::new(
            429,
            "assistant_busy",
            "Помощник занят. Повторите чуть позже",
        )
    })?;
    let token = sessions::random_token();
    let token_hash = sessions::hash(&token);
    let context = ctx.clone();
    let fingerprint = sessions::hash(&json!({"text":text,"credential_refs":refs}).to_string());
    let prepared=state.db(move|c|c.transaction::<_,ApiError,_>(|c|{
  settings::lock_settings_revision(c,context.organization_id)?;
  let control=persistence::get_document(c,context.organization_id,model_auth::control_id(context.organization_id))?.ok_or_else(||ApiError::new(409,"assistant_auth_required","Подключите Codex"))?;
  if control.data["authenticated"]!=true||control.data["confirmed_generation"]!=generation{return Err(ApiError::new(409,"assistant_auth_required","Авторизация Codex изменилась. Подключитесь заново"));}

  store::conversation(c,&context,conversation)?;store::recover(c,&context,conversation)?;let mut convo=store::document(c,&context,conversation,store::CONVERSATION)?;
  store::live_context(c,context.organization_id,context.user_id,&context.token_hash,true)?;
  if let Some(run)=persistence::get_document(c,context.organization_id,run_id)?{if run.data["request_hash"]!=fingerprint{return Err(ApiError::new(409,"idempotency_conflict","ID сообщения уже использован"));}return Ok((false,run_response(c,&context,&run)?,run,Value::Null,Value::Null));}
  if convo.data["active_run_id"].is_string(){return Err(ApiError::new(409,"assistant_run_active","Дождитесь ответа или остановите выполнение"));}
  let children=persistence::list_children(c,context.organization_id,conversation,None)?;let messages=store::ordered_messages(children.iter().filter(|d|d.kind==store::MESSAGE).cloned().collect());if messages.len()>=498{return Err(ApiError::new(429,"assistant_history_limit","Создайте новый чат"));}
  let recent=persistence::list_documents(c,context.organization_id,store::RUN)?.iter().filter(|d|d.created_at>Utc::now()-Duration::minutes(15)).count();if recent>=30{return Err(ApiError::new(429,"assistant_rate_limit","Слишком много запросов за короткое время"));}
  for id in refs{let d=store::document(c,&context,id,store::CREDENTIAL)?;if d.parent_id!=Some(conversation)||!store::valid_deadline(&d.data)||!d.data["config_encrypted"].is_string(){return Err(ApiError::new(400,"credential_unavailable","Доступ истёк, использован или относится к другому чату"));}}
  let mut history=messages.iter().rev().take(39).rev().map(|d|json!({"role":d.data["role"],"text":d.data["text"]})).collect::<Vec<_>>();history.push(json!({"role":"user","text":text}));if serde_json::to_vec(&history).map_err(|_|ApiError::internal())?.len()>128*1024{return Err(ApiError::bad("История слишком большая. Создайте новый чат"));}
  let credentials=children.iter().filter(|d|d.kind==store::CREDENTIAL&&d.data["user_id"]==context.user_id.to_string()&&store::valid_deadline(&d.data)&&d.data["config_encrypted"].is_string()).map(store::credential_json).collect::<Vec<_>>();
  let user_id=Uuid::new_v5(&run_id,b"user-message");let assistant_id=Uuid::new_v5(&run_id,b"assistant-message");
  persistence::save_document(c,context.organization_id,user_id,store::MESSAGE,Some(conversation),json!({"user_id":context.user_id,"role":"user","text":text,"status":"ready","run_id":run_id,"sequence":messages.len()+1}),None)?;
  persistence::save_document(c,context.organization_id,assistant_id,store::MESSAGE,Some(conversation),json!({"user_id":context.user_id,"role":"assistant","text":"","status":"running","run_id":run_id,"sequence":messages.len()+2}),None)?;
  let run=persistence::save_document(c,context.organization_id,run_id,store::RUN,Some(conversation),json!({"user_id":context.user_id,"session_hash":context.token_hash,"auth_generation":generation,"request_hash":fingerprint,"client_request_id":client,"capability_hash":token_hash,"state":"queued","tool_count":0,"assistant_message_id":assistant_id,"user_message_id":user_id,"events":[],"started_at":Utc::now(),"expires_at":Utc::now()+Duration::seconds(180),"finished_at":null,"error_code":null}),None)?;
  convo.data["active_run_id"]=json!(run_id);let data=convo.data.clone();store::save(c,convo,data)?;
  Ok((true,run_response(c,&context,&run)?,run,json!(history),json!(credentials)))
 })).await?;
    if prepared.0 {
        runtime::start(
            state.get_ref().clone(),
            ctx,
            prepared.2,
            token,
            prepared.3,
            prepared.4,
            permit,
        );
    } else {
        drop(permit);
    }
    Ok(HttpResponse::Accepted().json(json!({"data":prepared.1})))
}
async fn run(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, false, 2).await?;
    let id = *id;
    Ok(ok(state.db(move|c|c.transaction::<_,ApiError,_>(|c|{let d=store::document(c,&ctx,id,store::RUN)?;let parent=d.parent_id.ok_or_else(ApiError::missing)?;store::conversation(c,&ctx,parent)?;store::recover(c,&ctx,parent)?;let d=store::document(c,&ctx,id,store::RUN)?;let response=run_response(c,&ctx,&d)?;Ok(json!({"run":response["run"],"messages":[response["user_message"],response["assistant_message"]]}))})).await?))
}
async fn cancel(
    state: web::Data<AppState>,
    req: HttpRequest,
    id: web::Path<Uuid>,
) -> Result<HttpResponse, ApiError> {
    let ctx = sessions::context(&state, &req, true, 2).await?;
    let id = *id;
    let org = ctx.organization_id;
    let result = state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                let d = store::document(c, &ctx, id, store::RUN)?;
                store::conversation(c, &ctx, d.parent_id.ok_or_else(ApiError::missing)?)?;
                store::finish(
                    c,
                    ctx.organization_id,
                    id,
                    "cancelled",
                    Some("assistant_cancelled"),
                    None,
                    None,
                    None,
                )?;
                settings::audit(c, &ctx, "assistant.run_cancelled", Some(id), json!({}))?;
                Ok(store::run_json(&store::document(c, &ctx, id, store::RUN)?))
            })
        })
        .await?;
    runtime::abort(org, id);
    let _ = runtime::rpc("/cancel", json!({"organization_id":org,"run_id":id}), 3).await;
    Ok(ok(json!({"run":result})))
}
/// Bounded maintenance is also exposed for verifier tests, never mints authority.
pub async fn maintenance(state: &AppState) -> Result<(), ApiError> {
    state.db(|c|{
  let stale=diesel::sql_query("SELECT * FROM documents WHERE kind='assistant_credential' OR(kind='assistant_run' AND data->>'state' IN ('queued','running')) OR(kind='assistant_auth_attempt' AND data->>'state' IN ('pending','approved','revoked','failed')) ORDER BY data->>'expires_at' LIMIT 1000").load::<Document>(c)?;
  for d in stale{
   if d.kind==store::RUN&&store::active(&d.data)&&!store::valid_deadline(&d.data){store::finish(c,d.organization_id,d.id,"interrupted",Some("assistant_interrupted"),None,None,None)?;runtime::abort(d.organization_id,d.id);}
   if d.kind==store::CREDENTIAL&&(!store::valid_deadline(&d.data)||d.data["consumed"]==true){c.transaction::<_,ApiError,_>(|c|{settings::lock_settings_revision(c,d.organization_id)?;if let Some(parent)=d.parent_id{store::lock_document(c,d.organization_id,parent)?;}persistence::tombstone(c,d.organization_id,d.id,None)?;persistence::delete_document(c,d.organization_id,d.id)?;Ok(())})?;}
   if d.kind==store::AUTH_ATTEMPT&&!store::valid_deadline(&d.data){let mut data=d.data.clone();data.as_object_mut().unwrap().remove("capability_hash");data["state"]=json!("expired");store::save(c,d,data)?;}
  }
  Ok(())
 }).await
}
pub fn configure(c: &mut web::ServiceConfig) {
    model_auth::configure(c);
    c.route("/api/assistant/conversations", web::get().to(list))
        .route("/api/assistant/conversations", web::post().to(create))
        .route("/api/assistant/conversations/{id}", web::get().to(detail))
        .route("/api/assistant/conversations/{id}", web::patch().to(rename))
        .route(
            "/api/assistant/conversations/{id}",
            web::delete().to(delete),
        )
        .route(
            "/api/assistant/conversations/{id}/credentials",
            web::post().to(credentials),
        )
        .route(
            "/api/assistant/conversations/{id}/credentials/{credential_id}",
            web::delete().to(delete_credential),
        )
        .route(
            "/api/assistant/conversations/{id}/messages",
            web::post().to(message),
        )
        .route("/api/assistant/runs/{id}", web::get().to(run))
        .route("/api/assistant/runs/{id}/cancel", web::post().to(cancel))
        .route("/api/assistant/tools", web::post().to(tools::gateway));
}
