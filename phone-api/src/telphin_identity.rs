//! Resolve the PBX account identifier from the application's authenticated /user/ response.
use crate::{json, ApiError, Value};
use actix_web::{web, HttpRequest};
use std::{future::Future, pin::Pin, sync::Arc, time::Duration};
pub type IdentityFuture = Pin<Box<dyn Future<Output = Result<String, ApiError>> + Send>>;
/// Trusted server bootstrap injection; neither HTTP input nor runtime URLs select a resolver.
pub struct IdentityResolver(pub Arc<dyn Fn(Value) -> IdentityFuture + Send + Sync>);

pub fn needs_resolution(config: &Value) -> Result<bool, ApiError> {
    match config.get("client_id") {
        None => Ok(true),
        Some(Value::String(id)) => Ok(id.trim().is_empty()),
        _ => Err(ApiError::bad("ID клиента АТС должен быть строкой")),
    }
}
pub async fn complete(req: &HttpRequest, mut config: Value) -> Result<Value, ApiError> {
    if !needs_resolution(&config)? {
        return Ok(config);
    }
    if !["apiproxy.telphin.ru", "altapi.telphin.ru"]
        .contains(&config["host"].as_str().unwrap_or("apiproxy.telphin.ru"))
    {
        return Err(ApiError::bad("Неверный API хост Телфина"));
    }
    for field in ["app_id", "app_secret"] {
        let value = crate::str_field(&config, field)?;
        if value.len() > 4096 || value.chars().any(char::is_control) {
            return Err(ApiError::bad("Проверьте App ID и App Secret"));
        }
    }
    config
        .as_object_mut()
        .ok_or_else(|| ApiError::bad("Нужна конфигурация подключения"))?
        .remove("client_id");
    let result = if let Some(resolver) = req.app_data::<web::Data<IdentityResolver>>() {
        tokio::time::timeout(Duration::from_secs(25), (resolver.0)(config.clone())).await
    } else {
        let input = config.clone();
        tokio::time::timeout(Duration::from_secs(25), async move {
            let resolved = phone_worker::providers::resolve_telphin_config(&input)
                .await
                .map_err(|error| match error.code.as_str() {
                    "provider_reauthorization_required" | "provider_permission_denied" => {
                        ApiError::new(
                            400,
                            "telphin_identity_auth_failed",
                            "Телфин не принял App ID и App Secret. Проверьте данные приложения",
                        )
                    }
                    "telphin_client_scope_required" => ApiError::new(
                        403,
                        "telphin_identity_scope_required",
                        "Нужен доступ приложения ко всей АТС, а не только к добавочному номеру",
                    ),
                    _ => ApiError::new(
                        503,
                        "telphin_identity_unavailable",
                        "Не удалось определить ID АТС через Телфин. Повторите попытку",
                    ),
                })?;
            resolved["client_id"]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| {
                    ApiError::new(
                        502,
                        "telphin_identity_invalid",
                        "Телфин не вернул корректный ID АТС",
                    )
                })
        })
        .await
    };
    let id = result.map_err(|_| {
        ApiError::new(
            504,
            "telphin_identity_timeout",
            "Телфин не ответил вовремя. Повторите сохранение",
        )
    })??;
    if id.is_empty()
        || !id.bytes().all(|b| b.is_ascii_digit())
        || id.parse::<u64>().unwrap_or(0) == 0
    {
        return Err(ApiError::new(
            502,
            "telphin_identity_invalid",
            "Телфин не вернул корректный ID АТС",
        ));
    }
    config["client_id"] = json!(id);
    Ok(config)
}
