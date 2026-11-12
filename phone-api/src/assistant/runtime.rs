use super::store;
use crate::{auth as sessions, json, ApiError, AppState, Document, Uuid, Value};
use diesel::Connection;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::{sync::OwnedSemaphorePermit, task::AbortHandle};
static RUNS: OnceLock<Mutex<HashMap<(Uuid, Uuid), AbortHandle>>> = OnceLock::new();
fn handles() -> &'static Mutex<HashMap<(Uuid, Uuid), AbortHandle>> {
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}
pub fn abort(org: Uuid, id: Uuid) {
    if let Ok(mut map) = handles().lock() {
        if let Some(handle) = map.remove(&(org, id)) {
            handle.abort();
        }
    }
}
fn request(path: &str, body: Value, seconds: u64) -> Result<reqwest::RequestBuilder, ApiError> {
    if ![
        "/auth/status",
        "/auth/start",
        "/auth/cancel",
        "/auth/logout",
        "/turn",
        "/cancel",
    ]
    .contains(&path)
    {
        return Err(ApiError::bad("Неизвестный метод помощника"));
    }
    let base = std::env::var("ASSISTANT_RUNNER_URL")
        .unwrap_or_else(|_| "http://assistant-runner:8091".into());
    let url = reqwest::Url::parse(&base).map_err(|_| ApiError::internal())?;
    if !["http", "https"].contains(&url.scheme())
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::internal());
    }
    let token = std::env::var("ASSISTANT_RUNNER_TOKEN")
        .ok()
        .filter(|s| s.len() >= 24)
        .ok_or_else(|| {
            ApiError::new(
                503,
                "assistant_runner_unavailable",
                "Помощник ещё не подключён",
            )
        })?;
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(seconds))
        .build()
        .map_err(|_| ApiError::internal())?;
    Ok(client
        .post(url.join(path).map_err(|_| ApiError::internal())?)
        .bearer_auth(token)
        .json(&body))
}
pub async fn rpc(path: &str, body: Value, seconds: u64) -> Result<Value, ApiError> {
    let mut response = request(path, body, seconds)?.send().await.map_err(|_| {
        ApiError::new(
            503,
            "assistant_runner_unavailable",
            "Помощник временно недоступен",
        )
    })?;
    let status = response.status();
    let mut bytes = vec![];
    while let Some(part) = response.chunk().await.map_err(|_| ApiError::internal())? {
        if bytes.len() + part.len() > 128 * 1024 {
            return Err(ApiError::new(
                502,
                "assistant_protocol_error",
                "Слишком большой ответ помощника",
            ));
        }
        bytes.extend_from_slice(&part);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| {
        ApiError::new(
            502,
            "assistant_protocol_error",
            "Не удалось прочитать ответ помощника",
        )
    })?;
    if !status.is_success() {
        return Err(ApiError::new(
            if status.as_u16() == 409 { 409 } else { 503 },
            "assistant_runner_failed",
            "Помощник не смог выполнить запрос",
        ));
    }
    Ok(value)
}
struct Registration {
    org: Uuid,
    id: Uuid,
}
impl Drop for Registration {
    fn drop(&mut self) {
        if let Ok(mut map) = handles().lock() {
            map.remove(&(self.org, self.id));
        }
    }
}
pub fn start(
    state: AppState,
    ctx: sessions::Context,
    run: Document,
    token: String,
    history: Value,
    credentials: Value,
    permit: OwnedSemaphorePermit,
) {
    let org = run.organization_id;
    let id = run.id;
    let (setup_tx, setup_rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(async move {
        let _ = setup_rx.await;
        let _permit = permit;
        let _registration = Registration { org, id };
        let result = execute(&state, &ctx, &run, token, history, credentials).await;
        if let Err(error) = result {
            let code = error.code;
            let _ = state
                .db(move |c| store::finish(c, org, id, "failed", Some(code), None, None, None))
                .await;
        }
    });
    if let Ok(mut map) = handles().lock() {
        map.insert((org, id), task.abort_handle());
    }
    let _ = setup_tx.send(());
}
async fn partial(
    state: &AppState,
    ctx: &sessions::Context,
    run_id: Uuid,
    text: &str,
    status: Option<String>,
) -> Result<(), ApiError> {
    let context = ctx.clone();
    let text = text.to_owned();
    state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                crate::settings::lock_settings_revision(c, context.organization_id)?;
                let snapshot = store::document(c, &context, run_id, store::RUN)?;
                let parent = snapshot.parent_id.ok_or_else(ApiError::missing)?;
                store::conversation(c, &context, parent)?;
                store::lock_document(c, context.organization_id, run_id)?;
                let mut run = store::document(c, &context, run_id, store::RUN)?;
                if !store::active(&run.data) || !store::valid_deadline(&run.data) {
                    return Err(ApiError::new(
                        409,
                        "assistant_interrupted",
                        "Запрос помощника остановлен",
                    ));
                }
                store::live_context(
                    c,
                    context.organization_id,
                    context.user_id,
                    &context.token_hash,
                    true,
                )?;
                let control = persistence::get_document(
                    c,
                    context.organization_id,
                    super::model_auth::control_id(context.organization_id),
                )?
                .ok_or_else(ApiError::missing)?;
                if control.data["authenticated"] != true
                    || control.data["confirmed_generation"] != run.data["auth_generation"]
                {
                    return Err(ApiError::new(
                        403,
                        "assistant_access_revoked",
                        "Авторизация Codex изменилась",
                    ));
                }
                run.data["state"] = json!("running");
                if let Some(status) = status {
                    store::event(&mut run.data, "status", &status, "running", None);
                }
                let message_id = crate::id_field(&run.data, "assistant_message_id")?;
                let mut message = store::document(c, &context, message_id, store::MESSAGE)?;
                message.data["text"] = json!(text);
                let data = message.data.clone();
                store::save(c, message, data)?;
                let data = run.data.clone();
                store::save(c, run, data)?;
                Ok(())
            })
        })
        .await
}
fn safe_text(text: &str) -> Result<(), ApiError> {
    if text.len() > 128 * 1024 || text.chars().count() > 32000 {
        return Err(ApiError::new(
            502,
            "assistant_output_too_large",
            "Ответ помощника слишком большой",
        ));
    }
    if super::sensitive_text(text) {
        return Err(ApiError::new(
            502,
            "assistant_sensitive_output",
            "Ответ скрыт: обнаружен секретный фрагмент",
        ));
    }
    Ok(())
}
async fn execute(
    state: &AppState,
    ctx: &sessions::Context,
    run: &Document,
    token: String,
    history: Value,
    credentials: Value,
) -> Result<(), ApiError> {
    let payload = json!({"organization_id":ctx.organization_id,"auth_generation":run.data["auth_generation"],"run_id":run.id,"mcp_token":token,"history":history,"credential_refs":credentials});
    partial(state, ctx, run.id, "", Some("Подключение помощника".into())).await?;
    let response = request("/turn", payload, 185)?.send().await.map_err(|_| {
        ApiError::new(
            503,
            "assistant_runner_unavailable",
            "Помощник временно недоступен",
        )
    })?;
    if !response.status().is_success() {
        return Err(ApiError::new(
            503,
            "assistant_runner_failed",
            "Не удалось запустить запрос помощника",
        ));
    }
    if !response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("application/x-ndjson"))
    {
        return Err(ApiError::new(
            502,
            "assistant_protocol_error",
            "Неверный формат потока помощника",
        ));
    }
    let org = ctx.organization_id;
    let id = run.id;
    let mut response = response;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    let mut buffer = vec![];
    let mut total = 0usize;
    let mut text = String::new();
    let mut flushed = Instant::now();
    let mut terminal = false;
    let mut dirty = false;
    let mut final_model = None;
    let mut final_usage = None;
    loop {
        let chunk = tokio::select! {
            result=tokio::time::timeout_at(deadline,response.chunk())=>result.map_err(|_|ApiError::new(504,"assistant_timeout","Время запроса истекло"))?.map_err(|_|ApiError::new(502,"assistant_stream_interrupted","Поток ответа прерван"))?,
            _=tokio::time::sleep_until(tokio::time::Instant::from_std(flushed+Duration::from_millis(250))),if dirty=>{
                partial(state,ctx,id,&text,None).await?;dirty=false;flushed=Instant::now();continue;
            }
        };
        let Some(chunk) = chunk else { break };
        total += chunk.len();
        if total > 1024 * 1024 || buffer.len() + chunk.len() > 256 * 1024 {
            return Err(ApiError::new(
                502,
                "assistant_protocol_error",
                "Слишком большой поток помощника",
            ));
        }
        buffer.extend_from_slice(&chunk);
        while let Some(end) = buffer.iter().position(|b| *b == b'\n') {
            let line = buffer.drain(..=end).collect::<Vec<_>>();
            if line.iter().all(|c| c.is_ascii_whitespace()) {
                continue;
            }
            if terminal {
                return Err(ApiError::new(
                    502,
                    "assistant_protocol_error",
                    "Лишние события после завершения",
                ));
            }
            let event: Value = serde_json::from_slice(&line).map_err(|_| {
                ApiError::new(
                    502,
                    "assistant_protocol_error",
                    "Повреждённое событие помощника",
                )
            })?;
            match event["type"].as_str() {
                Some("status") => {
                    let title = event["message"]
                        .as_str()
                        .filter(|s| s.chars().count() <= 160)
                        .ok_or_else(|| {
                            ApiError::new(
                                502,
                                "assistant_protocol_error",
                                "Неверный статус помощника",
                            )
                        })?;
                    safe_text(title)?;
                    partial(state, ctx, id, &text, Some(title.to_owned())).await?;
                }
                Some("delta") => {
                    text.push_str(event["text"].as_str().ok_or_else(|| {
                        ApiError::new(502, "assistant_protocol_error", "Неверный фрагмент ответа")
                    })?);
                    safe_text(&text)?;
                    dirty = true;
                    if flushed.elapsed() > Duration::from_millis(250) {
                        partial(state, ctx, id, &text, None).await?;
                        dirty = false;
                        flushed = Instant::now();
                    }
                }
                Some("completed") => {
                    let final_text = event["text"]
                        .as_str()
                        .filter(|v| !v.trim().is_empty())
                        .ok_or_else(|| {
                            ApiError::new(502, "assistant_empty_response", "Пустой ответ помощника")
                        })?;
                    safe_text(final_text)?;
                    text = final_text.to_owned();
                    if let Some(model) = event["model"]
                        .as_str()
                        .filter(|v| v.len() <= 200 && !super::sensitive_text(v))
                    {
                        final_model = Some(json!(model));
                    }
                    if let Some(usage) = event["usage"].as_object() {
                        let mut safe = json!({});
                        for key in [
                            "input_tokens",
                            "output_tokens",
                            "cached_input_tokens",
                            "cost",
                        ] {
                            if let Some(value) = usage.get(key).filter(|v| v.is_number()) {
                                safe[key] = value.clone();
                            }
                        }
                        final_usage = Some(safe);
                    }
                    terminal = true;
                }
                Some("error") => {
                    let code = event["code"].as_str().unwrap_or("");
                    return Err(ApiError::new(
                        502,
                        match code {
                            "assistant_auth_required" => "assistant_auth_required",
                            "assistant_run_cancelled" => "assistant_cancelled",
                            "assistant_response_too_large" => "assistant_output_too_large",
                            _ => "assistant_runtime_failed",
                        },
                        "Помощник не завершил запрос. Уже применённые действия видны в истории",
                    ));
                }
                _ => {
                    return Err(ApiError::new(
                        502,
                        "assistant_protocol_error",
                        "Неизвестное событие помощника",
                    ))
                }
            }
        }
    }
    if !terminal || buffer.iter().any(|b| !b.is_ascii_whitespace()) {
        return Err(ApiError::new(
            502,
            "assistant_stream_interrupted",
            "Ответ помощника не завершён",
        ));
    }
    let context = ctx.clone();
    state
        .db(move |c| {
            c.transaction::<_, ApiError, _>(|c| {
                // Completion is not proof of still-valid authority; recheck original session and generation.
                crate::settings::lock_settings_revision(c, context.organization_id)?;
                let snapshot = store::document(c, &context, id, store::RUN)?;
                store::conversation(
                    c,
                    &context,
                    snapshot.parent_id.ok_or_else(ApiError::missing)?,
                )?;
                store::live_context(c, org, context.user_id, &context.token_hash, true)?;
                let control =
                    persistence::get_document(c, org, super::model_auth::control_id(org))?
                        .ok_or_else(ApiError::missing)?;
                if control.data["authenticated"] != true
                    || control.data["confirmed_generation"] != snapshot.data["auth_generation"]
                {
                    return Err(ApiError::new(
                        403,
                        "assistant_access_revoked",
                        "Доступ помощника изменился",
                    ));
                }
                store::finish(
                    c,
                    org,
                    id,
                    "completed",
                    None,
                    Some(&text),
                    final_model,
                    final_usage,
                )
            })
        })
        .await?;
    Ok(())
}
