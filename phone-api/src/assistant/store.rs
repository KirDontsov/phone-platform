use crate::{auth as sessions, json, settings, ApiError, Document, Uuid, Value};
use chrono::{DateTime, Utc};
use diesel::{
    prelude::*,
    sql_types::{Jsonb, Text, Uuid as SqlUuid},
};

pub const CONVERSATION: &str = "assistant_conversation";
pub const RUN: &str = "assistant_run";
pub const MESSAGE: &str = "assistant_message";
pub const CREDENTIAL: &str = "assistant_credential";
pub const RECEIPT: &str = "assistant_tool_receipt";
pub const AUTH_ATTEMPT: &str = "assistant_auth_attempt";
pub const AUTH_CONTROL: &str = "assistant_auth_control";

pub fn document(
    c: &mut diesel::PgConnection,
    ctx: &sessions::Context,
    id: Uuid,
    kind: &str,
) -> Result<Document, ApiError> {
    let d = settings::document(c, ctx.organization_id, id, kind)?;
    if d.data["user_id"].as_str() != Some(ctx.user_id.to_string().as_str()) {
        return Err(ApiError::missing());
    }
    Ok(d)
}
pub fn lock_document(c: &mut diesel::PgConnection, org: Uuid, id: Uuid) -> Result<(), ApiError> {
    diesel::sql_query("SELECT id FROM documents WHERE organization_id=$1 AND id=$2 FOR UPDATE")
        .bind::<SqlUuid, _>(org)
        .bind::<SqlUuid, _>(id)
        .execute(c)?;
    Ok(())
}
pub fn conversation(
    c: &mut diesel::PgConnection,
    ctx: &sessions::Context,
    id: Uuid,
) -> Result<Document, ApiError> {
    settings::lock_settings_revision(c, ctx.organization_id)?;
    lock_document(c, ctx.organization_id, id)?;
    document(c, ctx, id, CONVERSATION)
}
pub fn parse_time(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}
pub fn active(v: &Value) -> bool {
    matches!(v["state"].as_str(), Some("queued" | "running"))
}
pub fn valid_deadline(v: &Value) -> bool {
    parse_time(&v["expires_at"]).is_some_and(|d| d > Utc::now())
}
pub fn save(
    c: &mut diesel::PgConnection,
    mut d: Document,
    data: Value,
) -> Result<Document, ApiError> {
    d.data = data;
    Ok(persistence::save_document(
        c,
        d.organization_id,
        d.id,
        &d.kind,
        d.parent_id,
        d.data,
        Some(d.version),
    )?)
}
pub fn live_context(
    c: &mut diesel::PgConnection,
    org: Uuid,
    user: Uuid,
    session_hash: &str,
    lock: bool,
) -> Result<sessions::Context, ApiError> {
    let sql="SELECT s.user_id,s.organization_id,m.role,s.csrf_token,u.email,u.name,s.token_hash FROM sessions s JOIN memberships m ON m.user_id=s.user_id AND m.organization_id=s.organization_id JOIN users u ON u.id=s.user_id WHERE s.token_hash=$1 AND s.organization_id=$2 AND s.user_id=$3 AND s.expires_at>now() AND m.role='owner'";
    let sql = if lock {
        format!("{sql} FOR SHARE OF s,m")
    } else {
        sql.to_owned()
    };
    diesel::sql_query(sql)
        .bind::<Text, _>(session_hash)
        .bind::<SqlUuid, _>(org)
        .bind::<SqlUuid, _>(user)
        .get_result::<sessions::Context>(c)
        .optional()?
        .ok_or_else(|| {
            ApiError::new(
                403,
                "assistant_access_revoked",
                "Доступ помощника отозван. Войдите заново",
            )
        })
}
pub fn capability(
    c: &mut diesel::PgConnection,
    hash: &str,
    kind: &str,
) -> Result<Document, ApiError> {
    let rows = diesel::sql_query("SELECT * FROM documents WHERE kind=$1 AND data @> $2 LIMIT 2")
        .bind::<Text, _>(kind)
        .bind::<Jsonb, _>(json!({"capability_hash":hash}))
        .load::<Document>(c)?;
    if rows.len() != 1 {
        return Err(ApiError::new(
            403,
            "assistant_capability_invalid",
            "Доступ помощника истёк или отозван",
        ));
    }
    Ok(rows.into_iter().next().unwrap())
}
pub fn authorized_run(
    c: &mut diesel::PgConnection,
    token_hash: &str,
) -> Result<(Document, sessions::Context), ApiError> {
    let candidate = capability(c, token_hash, RUN)?;
    settings::lock_settings_revision(c, candidate.organization_id)?;
    let conversation_id = candidate.parent_id.ok_or_else(ApiError::missing)?;
    lock_document(c, candidate.organization_id, conversation_id)?;
    lock_document(c, candidate.organization_id, candidate.id)?;
    let run = persistence::get_document(c, candidate.organization_id, candidate.id)?
        .ok_or_else(ApiError::missing)?;
    if run.kind != RUN
        || run.data["capability_hash"] != token_hash
        || !active(&run.data)
        || !valid_deadline(&run.data)
    {
        return Err(ApiError::new(
            403,
            "assistant_capability_invalid",
            "Запрос помощника завершён или остановлен",
        ));
    }
    let control = persistence::get_document(
        c,
        run.organization_id,
        super::model_auth::control_id(run.organization_id),
    )?
    .ok_or_else(ApiError::missing)?;
    if control.data["authenticated"] != true
        || control.data["confirmed_generation"] != run.data["auth_generation"]
    {
        return Err(ApiError::new(
            403,
            "assistant_access_revoked",
            "Авторизация помощника изменилась",
        ));
    }
    let user = run.data["user_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok())
        .ok_or_else(ApiError::missing)?;
    let session = run.data["session_hash"]
        .as_str()
        .ok_or_else(ApiError::missing)?;
    let ctx = live_context(c, run.organization_id, user, session, true)?;
    document(c, &ctx, conversation_id, CONVERSATION)?;
    Ok((run, ctx))
}
pub fn bearer(req: &actix_web::HttpRequest) -> Result<String, ApiError> {
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            ApiError::new(
                403,
                "assistant_capability_invalid",
                "Требуется доступ помощника",
            )
        })?;
    Ok(sessions::hash(token))
}
pub fn ordered_messages(mut documents: Vec<Document>) -> Vec<Document> {
    documents.sort_by(|a, b| {
        a.data["sequence"]
            .as_u64()
            .unwrap_or(0)
            .cmp(&b.data["sequence"].as_u64().unwrap_or(0))
            .then(a.created_at.cmp(&b.created_at))
            .then_with(|| {
                let rank = |d: &Document| if d.data["role"] == "user" { 0 } else { 1 };
                rank(a).cmp(&rank(b))
            })
            .then(a.id.cmp(&b.id))
    });
    documents
}
pub fn message_json(d: &Document) -> Value {
    json!({"id":d.id,"role":d.data["role"],"text":d.data["text"],"status":d.data["status"],"run_id":d.data["run_id"],"created_at":d.created_at,"sequence":d.data["sequence"]})
}
pub fn conversation_json(d: &Document) -> Value {
    json!({"id":d.id,"title":d.data["title"],"version":d.version,"active_run_id":d.data["active_run_id"],"created_at":d.created_at,"updated_at":d.updated_at})
}
pub fn credential_json(d: &Document) -> Value {
    json!({"id":d.id,"kind":"telphin","label":d.data["label"],"created_at":d.created_at,"expires_at":d.data["expires_at"]})
}
pub fn run_json(d: &Document) -> Value {
    json!({"id":d.id,"conversation_id":d.parent_id,"state":d.data["state"],"assistant_message_id":d.data["assistant_message_id"],"events":d.data["events"].as_array().cloned().unwrap_or_default(),"error_code":d.data["error_code"],"model":d.data["model"],"usage":d.data["usage"],"started_at":d.data["started_at"],"finished_at":d.data["finished_at"]})
}
pub fn event(data: &mut Value, type_: &str, title: &str, status: &str, tool: Option<&str>) {
    let mut events = data["events"].as_array().cloned().unwrap_or_default();
    let seq = events.last().and_then(|v| v["seq"].as_u64()).unwrap_or(0) + 1;
    let mut v = json!({"seq":seq,"type":type_,"title":title,"status":status});
    if let Some(tool) = tool {
        v["tool_name"] = json!(tool);
    }
    events.push(v);
    if events.len() > 100 {
        events.drain(..events.len() - 100);
    }
    data["events"] = json!(events);
}
pub fn finish(
    c: &mut diesel::PgConnection,
    org: Uuid,
    run_id: Uuid,
    state: &str,
    error: Option<&str>,
    text: Option<&str>,
    model: Option<Value>,
    usage: Option<Value>,
) -> Result<(), ApiError> {
    c.transaction::<_, ApiError, _>(|c| {
        settings::lock_settings_revision(c, org)?;
        let Some(snapshot) = persistence::get_document(c, org, run_id)? else {
            return Ok(());
        };
        let Some(parent) = snapshot.parent_id else {
            return Ok(());
        };
        lock_document(c, org, parent)?;
        lock_document(c, org, run_id)?;
        let Some(mut run) = persistence::get_document(c, org, run_id)? else {
            return Ok(());
        };
        if !active(&run.data) {
            return Ok(());
        }
        run.data["state"] = json!(state);
        run.data["error_code"] = json!(error);
        run.data["finished_at"] = json!(Utc::now());
        run.data.as_object_mut().unwrap().remove("capability_hash");
        if let Some(model) = model {
            run.data["model"] = model;
        }
        if let Some(usage) = usage {
            run.data["usage"] = usage;
        }
        if let Some(events) = run.data["events"].as_array_mut() {
            for e in events {
                if e["status"] == "running" {
                    if state == "completed" && e["type"] == "status" {
                        e["status"] = json!("completed");
                    } else {
                        e["status"] = json!("uncertain");
                        e["summary"] = json!(
                            "Действие не подтверждено; уже выполненные изменения сохраняются"
                        );
                    }
                }
            }
        }
        event(
            &mut run.data,
            "status",
            if state == "completed" {
                "Ответ готов"
            } else if state == "cancelled" {
                "Выполнение остановлено"
            } else {
                "Запрос не завершён"
            },
            if state == "completed" {
                "completed"
            } else {
                "failed"
            },
            None,
        );
        if let Some(id) = run.data["assistant_message_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
        {
            if let Some(mut message) = persistence::get_document(c, org, id)? {
                if let Some(text) = text {
                    message.data["text"] = json!(text);
                }
                message.data["status"] = json!(if state == "completed" {
                    "completed"
                } else if state == "cancelled" {
                    "cancelled"
                } else {
                    "failed"
                });
                let data = message.data.clone();
                save(c, message, data)?;
            }
        }
        if let Some(mut convo) = persistence::get_document(c, org, parent)? {
            if convo.data["active_run_id"] == run_id.to_string() {
                convo.data["active_run_id"] = Value::Null;
                let data = convo.data.clone();
                save(c, convo, data)?;
            }
        }
        let data = run.data.clone();
        save(c, run, data)?;
        Ok(())
    })
}
pub fn recover(
    c: &mut diesel::PgConnection,
    ctx: &sessions::Context,
    conversation_id: Uuid,
) -> Result<(), ApiError> {
    for run in persistence::list_children(c, ctx.organization_id, conversation_id, Some(RUN))? {
        if active(&run.data)
            && (!valid_deadline(&run.data)
                || live_context(
                    c,
                    ctx.organization_id,
                    ctx.user_id,
                    run.data["session_hash"].as_str().unwrap_or(""),
                    false,
                )
                .is_err())
        {
            finish(
                c,
                ctx.organization_id,
                run.id,
                "interrupted",
                Some("assistant_interrupted"),
                None,
                None,
                None,
            )?;
        }
    }
    Ok(())
}
pub fn strict_keys(v: &Value, allowed: &[&str]) -> Result<(), ApiError> {
    let obj = v
        .as_object()
        .ok_or_else(|| ApiError::bad("Нужен JSON объект"))?;
    if obj.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err(ApiError::bad("Неподдерживаемое поле запроса"));
    }
    Ok(())
}
