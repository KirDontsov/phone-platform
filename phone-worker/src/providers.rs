use chrono::{DateTime, NaiveDateTime, Utc};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use reqwest::{Client, Method, Response};
use serde_json::{json, Value};
use std::time::Duration;
use url::Url;

pub const MAX_JSON: usize = 8 * 1024 * 1024;
pub const MAX_AUDIO: usize = 96 * 1024 * 1024;

#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}")]
pub struct ProviderError {
    pub code: String,
    pub retryable: bool,
    pub retry_after_seconds: Option<u64>,
}
impl ProviderError {
    pub fn permanent(code: &str) -> Self {
        Self {
            code: code.into(),
            retryable: false,
            retry_after_seconds: None,
        }
    }
    pub fn retry(code: &str, seconds: u64) -> Self {
        Self {
            code: code.into(),
            retryable: true,
            retry_after_seconds: Some(seconds.clamp(1, 86_400)),
        }
    }
}
pub type Result<T> = std::result::Result<T, ProviderError>;
pub fn required<'a>(config: &'a Value, key: &str) -> Result<&'a str> {
    config
        .get(key)
        .and_then(Value::as_str)
        .filter(|v| !v.trim().is_empty() && !v.chars().any(char::is_control))
        .ok_or_else(|| ProviderError::permanent("configuration_missing"))
}
pub fn http_client() -> Result<Client> {
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(180))
        .connect_timeout(Duration::from_secs(15))
        .user_agent("phone-platform/0.1")
        .build()
        .map_err(|_| ProviderError::permanent("http_configuration_error"))
}
pub fn endpoint(raw: &str, purpose: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| ProviderError::permanent("invalid_provider_url"))?;
    let host = url
        .host_str()
        .ok_or_else(|| ProviderError::permanent("invalid_provider_url"))?;
    let custom: Vec<String> = std::env::var("PROVIDER_ALLOWED_HOSTS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let telphin = ["apiproxy.telphin.ru", "altapi.telphin.ru"].contains(&host);
    let known = [
        "api.openai.com",
        "openrouter.ai",
        "api.groq.com",
        "oauth2.googleapis.com",
        "accounts.google.com",
        "sheets.googleapis.com",
    ]
    .contains(&host);
    let amo = (host.ends_with(".amocrm.ru") || host.ends_with(".kommo.com"))
        && host.split('.').count() >= 3;
    let local_model = purpose == "model" && host == "model-runner";
    let explicitly_allowed = custom.iter().any(|h| h == host);
    let host_allowed = match purpose {
        "telphin" => telphin,
        "amocrm" => amo,
        "google" => known && host.ends_with("googleapis.com"),
        _ => known || local_model || explicitly_allowed,
    };
    let scheme_allowed = url.scheme() == "https"
        || (purpose == "model" && (local_model || explicitly_allowed) && url.scheme() == "http");
    if !host_allowed
        || !scheme_allowed
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ProviderError::permanent("provider_url_not_allowed"));
    }
    Ok(url)
}
pub async fn checked(response: std::result::Result<Response, reqwest::Error>) -> Result<Response> {
    let response = response.map_err(|_| ProviderError::retry("provider_network_error", 30))?;
    let status = response.status();
    if status.as_u16() == 429 {
        let delay = response
            .headers()
            .get("x-ratelimit-reset")
            .or_else(|| response.headers().get("retry-after"))
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(60);
        return Err(ProviderError::retry("provider_rate_limited", delay));
    }
    if status.is_redirection() {
        return Err(ProviderError::permanent("provider_redirect_blocked"));
    }
    if status.is_server_error() {
        return Err(ProviderError::retry("provider_unavailable", 60));
    }
    if status.as_u16() == 401 {
        return Err(ProviderError::permanent(
            "provider_reauthorization_required",
        ));
    }
    if status.as_u16() == 403 {
        return Err(ProviderError::permanent("provider_permission_denied"));
    }
    if status.as_u16() == 404 {
        return Err(ProviderError::permanent("provider_resource_missing"));
    }
    if !status.is_success() {
        return Err(ProviderError::permanent("provider_request_rejected"));
    }
    Ok(response)
}
pub async fn read_limited(mut response: Response, max: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|length| length > max as u64)
    {
        return Err(ProviderError::permanent("provider_payload_too_large"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ProviderError::retry("provider_body_interrupted", 30))?
    {
        if bytes.len().saturating_add(chunk.len()) > max {
            return Err(ProviderError::permanent("provider_payload_too_large"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
pub async fn read_json(response: Response) -> Result<Value> {
    serde_json::from_slice(&read_limited(response, MAX_JSON).await?)
        .map_err(|_| ProviderError::permanent("invalid_provider_json"))
}
pub fn parse_utc(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(value) = DateTime::parse_from_rfc3339(value) {
        return Ok(value.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
        .map(|t| t.and_utc())
        .map_err(|_| ProviderError::permanent("invalid_provider_timestamp"))
}
pub fn encode(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

pub struct Telphin {
    client: Client,
    config: Value,
    host: Url,
    token: String,
    expires_at: DateTime<Utc>,
}
impl Telphin {
    pub async fn connect(config: &Value) -> Result<Self> {
        let host = required(config, "host").unwrap_or("apiproxy.telphin.ru");
        if !["apiproxy.telphin.ru", "altapi.telphin.ru"].contains(&host) {
            return Err(ProviderError::permanent("provider_url_not_allowed"));
        }
        let host = endpoint(&format!("https://{host}"), "telphin")?;
        Self::connect_with(config, http_client()?, host).await
    }
    // The production entry point above always supplies an allowlisted HTTPS host.
    // Tests exercise the same grant/scope logic with a private loopback fixture.
    async fn connect_with(config: &Value, client: Client, host: Url) -> Result<Self> {
        required(config, "app_id")?;
        required(config, "app_secret")?;
        let expected_client_id = match config.get("client_id") {
            None => None,
            Some(Value::String(value)) if value.trim().is_empty() => None,
            Some(value) => Some(canonical_client_id(Some(value))?),
        };
        let mut telphin = Self {
            client,
            config: config.clone(),
            host,
            token: String::new(),
            expires_at: Utc::now(),
        };
        telphin.grant().await?;
        let user = telphin.get("user/", &[]).await?;
        if user.get("extension_id").is_some_and(|v| !v.is_null())
            || user.get("extension_group_id").is_some_and(|v| !v.is_null())
        {
            return Err(ProviderError::permanent("telphin_client_scope_required"));
        }
        let client_id = canonical_client_id(user.get("client_id"))?;
        if expected_client_id.is_some_and(|expected| expected != client_id) {
            return Err(ProviderError::permanent("telphin_client_mismatch"));
        }
        telphin.config["client_id"] = json!(client_id);
        telphin.config["host"] = json!(required(config, "host").unwrap_or("apiproxy.telphin.ru"));
        Ok(telphin)
    }
    async fn grant(&mut self) -> Result<()> {
        let response = checked(
            self.client
                .post(
                    self.host
                        .join("/oauth/token")
                        .map_err(|_| ProviderError::permanent("invalid_provider_url"))?,
                )
                .header("Accept-Encoding", "gzip")
                .form(&[
                    ("grant_type", "client_credentials"),
                    ("client_id", required(&self.config, "app_id")?),
                    ("client_secret", required(&self.config, "app_secret")?),
                ])
                .send()
                .await,
        )
        .await?;
        let data = read_json(response).await?;
        let ttl = data["expires_in"]
            .as_i64()
            .filter(|v| *v > 0 && *v <= 86_400 * 30)
            .ok_or_else(|| ProviderError::permanent("invalid_oauth_response"))?;
        if data["token_type"].as_str().unwrap_or("").to_lowercase() != "bearer" {
            return Err(ProviderError::permanent("invalid_oauth_response"));
        }
        self.token = required(&data, "access_token")?.to_string();
        self.expires_at = Utc::now() + chrono::Duration::seconds((ttl * 9 / 10).max(1));
        Ok(())
    }
    pub async fn get(&mut self, path: &str, query: &[(&str, String)]) -> Result<Value> {
        for retry in 0..2 {
            if Utc::now() >= self.expires_at {
                self.grant().await?;
            }
            let response = self
                .client
                .get(
                    self.host
                        .join(&format!("/api/ver1.0/{path}"))
                        .map_err(|_| ProviderError::permanent("invalid_provider_url"))?,
                )
                .bearer_auth(&self.token)
                .header("Accept-Encoding", "gzip")
                .query(query)
                .send()
                .await;
            if retry == 0 && response.as_ref().is_ok_and(|r| r.status().as_u16() == 401) {
                self.grant().await?;
                continue;
            }
            return read_json(checked(response).await?).await;
        }
        Err(ProviderError::permanent(
            "provider_reauthorization_required",
        ))
    }
    pub async fn history(
        &mut self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
        page: u32,
        per_page: u32,
    ) -> Result<Value> {
        let client_id = numeric_id(self.config.get("client_id"))?;
        let data = self
            .get(
                &format!("client/{client_id}/call_history/"),
                &[
                    (
                        "start_datetime",
                        start.format("%Y-%m-%d %H:%M:%S").to_string(),
                    ),
                    ("end_datetime", end.format("%Y-%m-%d %H:%M:%S").to_string()),
                    ("page", page.to_string()),
                    ("per_page", per_page.to_string()),
                    ("order", "asc".into()),
                ],
            )
            .await?;
        validate_history(&data, &client_id, start, end, page)?;
        Ok(data)
    }
    pub async fn recording(&mut self, id: &str) -> Result<(Vec<u8>, String)> {
        let client_id = numeric_id(self.config.get("client_id"))?;
        if id.len() > 128 || !id.chars().all(|v| v.is_ascii_alphanumeric() || v == '-') {
            return Err(ProviderError::permanent("invalid_recording_id"));
        }
        let url = self
            .host
            .join(&format!("/api/ver1.0/client/{client_id}/record/{id}"))
            .map_err(|_| ProviderError::permanent("invalid_provider_url"))?;
        if Utc::now() >= self.expires_at {
            self.grant().await?;
        }
        let mut response = None;
        for attempt in 0..2 {
            let current = self
                .client
                .get(url.clone())
                .bearer_auth(&self.token)
                .header("Accept-Encoding", "gzip")
                .send()
                .await;
            if attempt == 0 && current.as_ref().is_ok_and(|r| r.status().as_u16() == 401) {
                self.grant().await?;
                continue;
            }
            response = Some(checked(current).await?);
            break;
        }
        let response = response
            .ok_or_else(|| ProviderError::permanent("provider_reauthorization_required"))?;
        let mime = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("application/octet-stream")
            .split(';')
            .next()
            .unwrap_or("application/octet-stream")
            .to_string();
        if ![
            "audio/wav",
            "audio/x-wav",
            "audio/mpeg",
            "audio/mp3",
            "audio/ogg",
            "audio/mp4",
            "audio/webm",
            "application/octet-stream",
        ]
        .contains(&mime.as_str())
        {
            return Err(ProviderError::permanent("invalid_recording_mime"));
        }
        let bytes = read_limited(response, MAX_AUDIO).await?;
        if bytes.is_empty() {
            return Err(ProviderError::retry("recording_not_ready", 120));
        }
        Ok((bytes, mime))
    }
}
/// Validate API credentials against their authenticated user and return the canonical config.
/// App ID is the OAuth client_id; the PBX client ID is a distinct user resource field.
pub async fn resolve_telphin_config(config: &Value) -> Result<Value> {
    Ok(Telphin::connect(config).await?.config)
}
fn canonical_client_id(value: Option<&Value>) -> Result<String> {
    numeric_id(value)?
        .parse::<u64>()
        .map(|id| id.to_string())
        .map_err(|_| ProviderError::permanent("invalid_client_id"))
}
pub fn numeric_id(value: Option<&Value>) -> Result<String> {
    let raw = value
        .map(|v| match v {
            Value::String(v) => v.clone(),
            v => v.to_string(),
        })
        .unwrap_or_default();
    if raw.is_empty()
        || !raw.bytes().all(|b| b.is_ascii_digit())
        || raw.parse::<u64>().unwrap_or(0) == 0
    {
        return Err(ProviderError::permanent("invalid_client_id"));
    }
    Ok(raw)
}
pub fn validate_history(
    data: &Value,
    client_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    page: u32,
) -> Result<()> {
    let size = data["per_page"]
        .as_u64()
        .filter(|v| *v > 0)
        .ok_or_else(|| ProviderError::permanent("invalid_history_page"))?;
    let calls = data["call_history"]
        .as_array()
        .ok_or_else(|| ProviderError::permanent("invalid_history_page"))?;
    if data["page"].as_u64() != Some(page as u64)
        || data["order"] != "asc"
        || calls.len() > size as usize
    {
        return Err(ProviderError::permanent("invalid_history_page"));
    }
    for call in calls {
        required(call, "call_uuid")?;
        if numeric_id(call.get("client_owner_id"))? != client_id {
            return Err(ProviderError::permanent("call_owner_mismatch"));
        }
        let occurred = parse_utc(required(call, "init_time_gmt")?)?;
        if occurred < start || occurred > end {
            return Err(ProviderError::permanent("call_outside_requested_window"));
        }
        let legs = call["cdr"]
            .as_array()
            .ok_or_else(|| ProviderError::permanent("invalid_history_cdr"))?;
        for item in std::iter::once(call).chain(legs.iter()) {
            if !item.is_object() || numeric_id(item.get("client_owner_id"))? != client_id {
                return Err(ProviderError::permanent("call_owner_mismatch"));
            }
            for field in ["result", "flow", "record_uuid"] {
                if item
                    .get(field)
                    .is_some_and(|value| !value.is_null() && !value.is_string())
                {
                    return Err(ProviderError::permanent("invalid_history_field"));
                }
            }
        }
    }
    Ok(())
}

fn amo_base(config: &Value) -> Result<Url> {
    let domain = required(config, "account_domain")?;
    endpoint(&format!("https://{domain}"), "amocrm")
}
pub fn oauth_authorize_url(provider: &str, config: &Value, state: &str) -> Result<String> {
    if state.len() < 24 || state.len() > 512 {
        return Err(ProviderError::permanent("invalid_oauth_state"));
    }
    let mut url = match provider {
        "amocrm" => Url::parse("https://www.amocrm.ru/oauth").unwrap(),
        "sheets" => Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap(),
        _ => return Err(ProviderError::permanent("unsupported_integration")),
    };
    let redirect = Url::parse(required(config, "redirect_uri")?)
        .map_err(|_| ProviderError::permanent("invalid_redirect_uri"))?;
    if redirect.scheme() != "https"
        && !(redirect.scheme() == "http"
            && matches!(redirect.host_str(), Some("localhost" | "127.0.0.1")))
    {
        return Err(ProviderError::permanent("invalid_redirect_uri"));
    }
    url.query_pairs_mut()
        .append_pair("client_id", required(config, "client_id")?)
        .append_pair("redirect_uri", redirect.as_str())
        .append_pair("state", state)
        .append_pair("response_type", "code");
    if provider == "sheets" {
        url.query_pairs_mut()
            .append_pair("scope", "https://www.googleapis.com/auth/spreadsheets")
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent");
    }
    Ok(url.to_string())
}
pub async fn oauth_exchange(provider: &str, config: &Value, code: &str) -> Result<Value> {
    if code.is_empty() || code.len() > 4096 {
        return Err(ProviderError::permanent("invalid_oauth_code"));
    }
    oauth_token(provider, config, "authorization_code", code).await
}
pub async fn oauth_refresh(provider: &str, config: &Value) -> Result<Value> {
    oauth_token(
        provider,
        config,
        "refresh_token",
        required(config, "refresh_token")?,
    )
    .await
}
async fn oauth_token(
    provider: &str,
    config: &Value,
    grant: &str,
    credential: &str,
) -> Result<Value> {
    let fields = [
        ("client_id", required(config, "client_id")?),
        ("client_secret", required(config, "client_secret")?),
        ("grant_type", grant),
        (
            if grant == "authorization_code" {
                "code"
            } else {
                "refresh_token"
            },
            credential,
        ),
        ("redirect_uri", required(config, "redirect_uri")?),
    ];
    let client = http_client()?;
    let response = match provider {
        "amocrm" => {
            checked(
                client
                    .post(
                        amo_base(config)?
                            .join("/oauth2/access_token")
                            .map_err(|_| ProviderError::permanent("invalid_provider_url"))?,
                    )
                    .json(
                        &fields
                            .iter()
                            .copied()
                            .collect::<std::collections::HashMap<_, _>>(),
                    )
                    .send()
                    .await,
            )
            .await?
        }
        "sheets" => {
            checked(
                client
                    .post(endpoint("https://oauth2.googleapis.com/token", "google")?)
                    .form(&fields)
                    .send()
                    .await,
            )
            .await?
        }
        _ => return Err(ProviderError::permanent("unsupported_integration")),
    };
    let mut token = read_json(response).await?;
    required(&token, "access_token")?;
    let ttl = token["expires_in"]
        .as_i64()
        .filter(|v| *v > 0 && *v <= 86_400 * 365)
        .ok_or_else(|| ProviderError::permanent("invalid_oauth_response"))?;
    token["expires_at"] = json!((Utc::now() + chrono::Duration::seconds(ttl)).timestamp());
    Ok(token)
}
pub async fn amocrm_search(config: &Value, phone: &str) -> Result<Value> {
    if phone.len() > 64 {
        return Err(ProviderError::permanent("invalid_phone"));
    }
    let client = http_client()?;
    let base = amo_base(config)?;
    let token = required(config, "access_token")?;
    let contacts = read_json(
        checked(
            client
                .get(base.join("/api/v4/contacts").unwrap())
                .bearer_auth(token)
                .query(&[("query", phone), ("with", "leads")])
                .send()
                .await,
        )
        .await?,
    )
    .await?;
    let mut deals = Vec::new();
    let rows = contacts["_embedded"]["contacts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    for contact in rows.iter().take(100) {
        if let Some(leads) = contact["_embedded"]["leads"].as_array() {
            for link in leads.iter().take(30) {
                if let Some(id) = link["id"].as_u64() {
                    if seen.insert(id) {
                        let deal = read_json(
                            checked(
                                client
                                    .get(base.join(&format!("/api/v4/leads/{id}")).unwrap())
                                    .bearer_auth(token)
                                    .send()
                                    .await,
                            )
                            .await?,
                        )
                        .await?;
                        deals.push(json!({"id":id.to_string(),"name":deal["name"],"status_id":deal["status_id"]}));
                    }
                }
            }
        }
    }
    Ok(
        json!({"contacts":rows.iter().map(|v| json!({"id":v["id"].to_string(),"name":v["name"]})).collect::<Vec<_>>(),"deals":deals}),
    )
}
pub async fn test_connection(kind: &str, config: &Value) -> Result<Value> {
    match kind {
        "telphin" => {
            let _ = Telphin::connect(config).await?;
        }
        "amocrm" => {
            let _ = read_json(
                checked(
                    http_client()?
                        .get(amo_base(config)?.join("/api/v4/account").unwrap())
                        .bearer_auth(required(config, "access_token")?)
                        .send()
                        .await,
                )
                .await?,
            )
            .await?;
        }
        "sheets" => {
            let id = required(config, "spreadsheet_id")?;
            let _ = read_json(checked(http_client()?.get(endpoint(&format!("https://sheets.googleapis.com/v4/spreadsheets/{}?fields=spreadsheetId",encode(id)),"google")?).bearer_auth(required(config,"access_token")?).send().await).await?).await?;
        }
        "asr" | "analysis" => {
            let provider = required(config, "provider")?;
            if provider == "demo" {
                if !matches!(std::env::var("DEMO_MODE").as_deref(), Ok("true" | "1")) {
                    return Err(ProviderError::permanent("demo_provider_not_allowed"));
                }
                return Ok(
                    json!({"ok":true,"state":"demo","message":"Демонстрационный адаптер: синтетические данные"}),
                );
            }
            let purpose = if provider == "opencode" {
                "model"
            } else {
                "model"
            };
            let mut base = endpoint(required(config, "base_url")?, purpose)?;
            if !base.path().ends_with('/') {
                let path = format!("{}/", base.path());
                base.set_path(&path);
            }
            let path = if provider == "opencode" {
                "global/health"
            } else {
                "models"
            };
            let mut request = http_client()?.request(
                Method::GET,
                base.join(path)
                    .map_err(|_| ProviderError::permanent("invalid_provider_url"))?,
            );
            if provider == "opencode" {
                request = request.basic_auth("opencode", Some(required(config, "api_key")?));
            } else {
                request = request.bearer_auth(required(config, "api_key")?);
            }
            let _ = read_json(checked(request.send().await).await?).await?;
        }
        _ => return Err(ProviderError::permanent("unsupported_provider")),
    }
    Ok(json!({"ok":true,"state":"configured","message":"Подключение проверено"}))
}

#[cfg(test)]
mod client_id_tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    struct Fixture {
        host: Url,
        requests: Arc<Mutex<Vec<(String, String, String)>>>,
        task: tokio::task::JoinHandle<()>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            self.task.abort();
        }
    }
    async fn fixture(user: Value) -> Fixture {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let host = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let captured = requests.clone();
        let task = tokio::spawn(async move {
            for reply in [
                json!({"access_token":"synthetic-token","token_type":"Bearer","expires_in":3600}),
                user,
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut bytes = Vec::new();
                let header_end = loop {
                    let mut part = [0; 1024];
                    let n = stream.read(&mut part).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&part[..n]);
                    if let Some(end) = bytes.windows(4).position(|p| p == b"\r\n\r\n") {
                        break end + 4;
                    }
                    assert!(bytes.len() < 65536);
                };
                let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(k, _)| k.eq_ignore_ascii_case("content-length"))
                            .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while bytes.len() < header_end + length {
                    let mut part = [0; 1024];
                    let n = stream.read(&mut part).await.unwrap();
                    assert!(n > 0);
                    bytes.extend_from_slice(&part[..n]);
                }
                let body =
                    String::from_utf8(bytes[header_end..header_end + length].to_vec()).unwrap();
                captured.lock().unwrap().push((
                    headers.lines().next().unwrap().to_owned(),
                    headers,
                    body,
                ));
                let data = serde_json::to_vec(&reply).unwrap();
                let header=format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",data.len());
                stream.write_all(header.as_bytes()).await.unwrap();
                stream.write_all(&data).await.unwrap();
            }
        });
        Fixture {
            host,
            requests,
            task,
        }
    }
    fn config() -> Value {
        json!({"app_id":"synthetic-app-id","app_secret":"synthetic+secret&=value","host":"apiproxy.telphin.ru"})
    }
    fn user(id: Value) -> Value {
        json!({"client_id":id,"extension_id":null,"extension_group_id":null})
    }
    async fn connect(config: &Value, f: &Fixture) -> Result<Value> {
        Ok(
            Telphin::connect_with(config, http_client()?, f.host.clone())
                .await?
                .config,
        )
    }
    fn assert_grant_and_user(f: &Fixture) {
        let captured = f.requests.lock().unwrap();
        assert_eq!(captured.len(), 2);
        assert_eq!(captured[0].0, "POST /oauth/token HTTP/1.1");
        assert!(captured[0]
            .1
            .to_lowercase()
            .contains("content-type: application/x-www-form-urlencoded"));
        let fields = url::form_urlencoded::parse(captured[0].2.as_bytes())
            .into_owned()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(fields.get("grant_type").unwrap(), "client_credentials");
        assert_eq!(fields.get("client_id").unwrap(), "synthetic-app-id");
        assert_eq!(
            fields.get("client_secret").unwrap(),
            "synthetic+secret&=value"
        );
        assert_eq!(captured[1].0, "GET /api/ver1.0/user/ HTTP/1.1");
        assert!(captured[1]
            .1
            .to_lowercase()
            .contains("authorization: bearer synthetic-token"));
        assert!(captured[1].2.is_empty());
    }
    #[tokio::test]
    async fn resolve_telphin_missing_or_blank_client_id_from_authenticated_user() {
        for blank in [None, Some(json!("")), Some(json!("   "))] {
            let mut input = config();
            if let Some(value) = blank {
                input["client_id"] = value;
            }
            let f = fixture(user(json!(42))).await;
            let resolved = connect(&input, &f).await.unwrap();
            assert_eq!(resolved["client_id"], "42");
            assert_eq!(resolved["app_id"], input["app_id"]);
            assert_eq!(resolved["app_secret"], input["app_secret"]);
            assert_ne!(input["client_id"], "42");
            assert_grant_and_user(&f);
        }
    }
    #[tokio::test]
    async fn resolve_telphin_normalizes_numeric_client_id_and_checks_explicit_match() {
        for id in [json!(42), json!("42"), json!("0042")] {
            let mut input = config();
            input["client_id"] = id;
            let f = fixture(user(json!(42))).await;
            assert_eq!(connect(&input, &f).await.unwrap()["client_id"], "42");
            assert_grant_and_user(&f);
        }
        let mut input = config();
        input["client_id"] = json!(43);
        let f = fixture(user(json!(42))).await;
        assert_eq!(
            connect(&input, &f).await.unwrap_err().code,
            "telphin_client_mismatch"
        );
        assert_grant_and_user(&f);
    }
    #[tokio::test]
    async fn resolve_telphin_rejects_extension_and_group_scope() {
        for field in ["extension_id", "extension_group_id"] {
            let mut input = config();
            input["client_id"] = json!(42);
            let mut limited = user(json!(42));
            limited[field] = json!(7);
            let f = fixture(limited).await;
            assert_eq!(
                connect(&input, &f).await.unwrap_err().code,
                "telphin_client_scope_required"
            );
            assert_grant_and_user(&f);
        }
    }
    #[tokio::test]
    async fn resolve_telphin_never_guesses_invalid_explicit_or_provider_client_id() {
        for id in [
            Value::Null,
            json!(0),
            json!(-1),
            json!(true),
            json!("invalid"),
            json!(1.5),
            json!({"id":42}),
        ] {
            let mut input = config();
            input["client_id"] = id;
            let f = fixture(user(json!(42))).await;
            assert_eq!(
                connect(&input, &f).await.unwrap_err().code,
                "invalid_client_id"
            );
            assert!(f.requests.lock().unwrap().is_empty());
        }
        for id in [
            Value::Null,
            json!(0),
            json!(false),
            json!(""),
            json!("unknown"),
        ] {
            let f = fixture(user(id)).await;
            assert_eq!(
                connect(&config(), &f).await.unwrap_err().code,
                "invalid_client_id"
            );
            assert_grant_and_user(&f);
        }
    }
    #[tokio::test]
    async fn resolve_telphin_production_has_no_loopback_or_untrusted_host_override() {
        let mut input = config();
        input["host"] = json!("127.0.0.1");
        assert_eq!(
            resolve_telphin_config(&input).await.unwrap_err().code,
            "provider_url_not_allowed"
        );
    }
}
