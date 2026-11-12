pub mod assistant;
pub mod telphin_identity;
pub mod auth;
pub mod calls;
pub mod operations;
pub mod processing;
pub mod seed;
pub mod settings;

use actix_web::{http::StatusCode, web, HttpResponse, ResponseError};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use persistence::{Document, Pool};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub pool: Pool,
    pub key: [u8; 32],
    pub demo: bool,
    pub secure_cookie: bool,
    pub public_url: String,
    pub allowed_origins: Vec<String>,
    pub media_root: PathBuf,
    pub throttle: Arc<Mutex<HashMap<String, (u32, DateTime<Utc>)>>>,
    pub dummy_hash: String,
}

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}
impl ApiError {
    pub fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
    pub fn bad(message: impl Into<String>) -> Self {
        Self::new(400, "invalid_request", message)
    }
    pub fn forbidden() -> Self {
        Self::new(403, "forbidden", "Недостаточно прав")
    }
    pub fn missing() -> Self {
        Self::new(404, "not_found", "Объект не найден")
    }
    pub fn internal() -> Self {
        Self::new(503, "service_unavailable", "Сервис временно недоступен")
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)
    }
}
impl std::error::Error for ApiError {}
impl ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
    fn error_response(&self) -> HttpResponse {
        HttpResponse::build(self.status_code())
            .insert_header(("Cache-Control", "no-store"))
            .json(json!({"error":{"code":self.code,"message":self.message}}))
    }
}
impl From<persistence::DbError> for ApiError {
    fn from(e: persistence::DbError) -> Self {
        match e {
            persistence::DbError::Conflict => Self::new(
                409,
                "conflict",
                "Данные изменились. Обновите страницу и повторите действие",
            ),
            persistence::DbError::Validation(_) => {
                Self::bad("Неверные данные или ссылка на объект")
            }
            _ => Self::internal(),
        }
    }
}
impl From<diesel::result::Error> for ApiError {
    fn from(_: diesel::result::Error) -> Self {
        Self::internal()
    }
}
impl AppState {
    pub async fn db<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut diesel::PgConnection) -> Result<T, ApiError> + Send + 'static,
    {
        let pool = self.pool.clone();
        web::block(move || {
            let mut conn = persistence::connection(&pool).map_err(ApiError::from)?;
            f(&mut conn)
        })
        .await
        .map_err(|_| ApiError::internal())?
    }
    pub fn from_env(pool: Pool) -> Result<Self, ApiError> {
        let encoded = std::env::var("ENCRYPTION_KEY")
            .map_err(|_| ApiError::bad("ENCRYPTION_KEY обязателен"))?;
        let raw = STANDARD
            .decode(encoded)
            .map_err(|_| ApiError::bad("ENCRYPTION_KEY должен быть base64"))?;
        let key: [u8; 32] = raw
            .try_into()
            .map_err(|_| ApiError::bad("ENCRYPTION_KEY должен содержать 32 байта"))?;
        let public_url =
            std::env::var("PUBLIC_URL").unwrap_or_else(|_| "http://localhost:18081".into());
        Ok(Self {
            pool,
            key,
            demo: std::env::var("DEMO_MODE").as_deref() == Ok("true"),
            secure_cookie: std::env::var("COOKIE_SECURE").as_deref() != Ok("false"),
            allowed_origins: std::env::var("ALLOWED_ORIGINS")
                .unwrap_or_else(|_| public_url.clone())
                .split(',')
                .map(|v| v.trim().to_string())
                .collect(),
            public_url,
            media_root: std::env::var("MEDIA_ROOT")
                .unwrap_or_else(|_| "data/media".into())
                .into(),
            throttle: Arc::new(Mutex::new(HashMap::new())),
            dummy_hash: auth::password_hash("unusable-random-dummy-password")?,
        })
    }
}
pub fn ok(data: Value) -> HttpResponse {
    HttpResponse::Ok()
        .insert_header(("Cache-Control", "no-store"))
        .json(json!({"data":data}))
}
pub fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, ApiError> {
    v.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ApiError::bad(format!("Поле {key} обязательно")))
}
pub fn id_field(v: &Value, key: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(str_field(v, key)?).map_err(|_| ApiError::bad("Неверный идентификатор"))
}
pub fn to_json(d: Document) -> Value {
    let semantic = if d.kind == "transcript" {
        d.data
            .get("transcript_version")
            .or_else(|| d.data.get("version"))
            .cloned()
    } else {
        None
    };
    let mut v = d.data;
    if !v.is_object() {
        v = json!({});
    }
    v["id"] = json!(d.id);
    v["version"] = semantic.unwrap_or(json!(d.version));
    v["row_version"] = json!(d.version);
    if v.get("created_at").is_none() {
        v["created_at"] = json!(d.created_at);
    }
    v
}
pub fn configure(cfg: &mut web::ServiceConfig) {
    auth::configure(cfg);
    assistant::configure(cfg);
    calls::configure(cfg);
    settings::configure(cfg);
    operations::configure(cfg);
}
