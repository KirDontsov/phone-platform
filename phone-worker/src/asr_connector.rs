//! Speech-to-text extension of the private Docker OpenCode connector.
//! Provider credentials are scoped to this request; native OpenCode never receives them.
use crate::{
    models::{normalize_transcript, split_audio, AudioChunk},
    providers::{self, ProviderError, Result},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{path::Path, time::Duration};

pub const MODELS: [&str; 2] = ["moondream/parakeet-ultra", "nvidia/nemotron-3.5-transcribe"];
pub const DEFAULT_URL: &str = "http://model-runner:4096";
const MAX_CHUNK: usize = 24 * 1024 * 1024;
const CHUNK_SECONDS: u32 = 300;

/// Only the operator can choose the transport destination for the runner credential.
pub fn validate_config(config: &Value) -> Result<url::Url> {
    let base = crate::models::normalized_base(config)?;
    let expected = crate::models::normalized_base(
        &json!({"base_url":std::env::var("MODEL_RUNNER_URL").unwrap_or_else(|_| DEFAULT_URL.into())}),
    )?;
    if base != expected || base.query().is_some() {
        return Err(ProviderError::permanent("asr_runner_url_mismatch"));
    }
    if !MODELS.contains(&providers::required(config, "model")?) {
        return Err(ProviderError::permanent("unsupported_asr_model"));
    }
    providers::required(config, "api_key")?;
    Ok(base)
}
fn runner_password() -> Result<String> {
    std::env::var("MODEL_RUNNER_PASSWORD")
        .ok()
        .filter(|s| s.len() >= 24 && !s.chars().any(char::is_control))
        .ok_or_else(|| ProviderError::permanent("asr_runner_auth_missing"))
}
async fn response(
    response: std::result::Result<reqwest::Response, reqwest::Error>,
) -> Result<Value> {
    let response = response.map_err(|e| {
        ProviderError::retry(
            if e.is_timeout() {
                "provider_timeout"
            } else {
                "provider_network_error"
            },
            30,
        )
    })?;
    if response.status().is_success() {
        return providers::read_json(response).await;
    }
    if response.status().is_redirection() {
        return Err(ProviderError::permanent("provider_redirect_blocked"));
    }
    let status = response.status().as_u16();
    let delay = response
        .headers()
        .get("retry-after")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.parse::<u64>().ok())
        .unwrap_or(60);
    let raw = providers::read_json(response).await.unwrap_or(Value::Null);
    let code = raw["error"]["code"].as_str().unwrap_or("");
    let error = match code {
        "runner_auth_required" => ProviderError::permanent("asr_runner_auth_failed"),
        "asr_provider_key_required" => ProviderError::permanent("configuration_missing"),
        "asr_model_unsupported" => ProviderError::permanent("unsupported_asr_model"),
        "asr_invalid_request"
        | "asr_audio_too_large"
        | "invalid_transcript_response"
        | "asr_response_too_large" => ProviderError::permanent(code),
        "asr_busy" | "provider_rate_limited" => {
            ProviderError::retry("provider_rate_limited", delay)
        }
        "provider_timeout" => ProviderError::retry("provider_timeout", 60),
        "provider_unavailable" | "connector_unavailable" => {
            ProviderError::retry("provider_unavailable", 30)
        }
        "provider_unauthorized" => ProviderError::permanent("provider_reauthorization_required"),
        "provider_forbidden" => ProviderError::permanent("provider_permission_denied"),
        _ => match status {
            401 => ProviderError::permanent("asr_runner_auth_failed"),
            403 => ProviderError::permanent("provider_permission_denied"),
            429 => ProviderError::retry("provider_rate_limited", delay),
            500..=599 => ProviderError::retry("provider_unavailable", 30),
            _ => ProviderError::permanent("provider_request_rejected"),
        },
    };
    Err(error)
}
pub async fn test_connection(config: &Value) -> Result<Value> {
    let base = validate_config(config)?;
    let password = runner_password()?;
    let result = response(
        providers::http_client()?
            .post(base.join("asr/test").unwrap())
            .basic_auth("opencode", Some(password))
            .header(
                "X-OpenRouter-Api-Key",
                providers::required(config, "api_key")?,
            )
            .timeout(Duration::from_secs(20))
            .json(&json!({"model":config["model"]}))
            .send()
            .await,
    )
    .await?;
    if result["ok"] != true {
        return Err(ProviderError::permanent("invalid_transcript_response"));
    }
    Ok(
        json!({"ok":true,"state":"configured","message":"Docker-коннектор и ключ OpenRouter проверены. Распознавание записи этой проверкой не запускалось."}),
    )
}
pub async fn transcribe(config: &Value, path: &Path, duration: f64) -> Result<Value> {
    let base = validate_config(config)?;
    let password = runner_password()?;
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|_| ProviderError::permanent("recording_file_missing"))?
        .len();
    if size > providers::MAX_AUDIO as u64 {
        return Err(ProviderError::permanent("recording_too_large"));
    }
    let chunks = if size > MAX_CHUNK as u64 || duration > f64::from(CHUNK_SECONDS) {
        split_audio(path, CHUNK_SECONDS).await?
    } else {
        vec![AudioChunk {
            path: path.to_owned(),
            offset: 0.,
            temporary: false,
        }]
    };
    let client = providers::http_client()?;
    let mut combined = vec![];
    let mut cost = Some(0.0);
    let mut seconds = Some(0.0);
    let mut input_tokens = Some(0u64);
    let mut output_tokens = Some(0u64);
    let mut language: Option<String> = None;
    for (index, chunk) in chunks.iter().enumerate() {
        let bytes = tokio::fs::read(&chunk.path)
            .await
            .map_err(|_| ProviderError::permanent("audio_chunk_missing"))?;
        if bytes.is_empty() || bytes.len() > MAX_CHUNK {
            return Err(ProviderError::permanent("audio_chunk_too_large"));
        }
        let format = chunk
            .path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !["wav", "mp3", "mp4", "flac", "m4a", "ogg", "webm", "aac"].contains(&format.as_str()) {
            return Err(ProviderError::permanent("recording_format_unknown"));
        }
        let raw = response(client.post(base.join("asr/transcriptions").unwrap())
            .basic_auth("opencode", Some(&password)).header("X-OpenRouter-Api-Key", providers::required(config,"api_key")?)
            .timeout(Duration::from_secs(65))
            .json(&json!({"model":config["model"],"input_audio":{"data":STANDARD.encode(bytes),"format":format},"response_format":"verbose_json"}))
            .send().await).await?;
        let part_duration = if chunks.len() > 1 {
            (duration - chunk.offset)
                .max(0.)
                .min(f64::from(CHUNK_SECONDS))
        } else {
            duration
        };
        let parsed = normalize_transcript(&raw, part_duration)?;
        for segment in parsed["segments"].as_array().unwrap() {
            let mut segment = segment.clone();
            segment["id"] = json!(format!("s{}", combined.len() + 1));
            if chunks.len() > 1 && segment["speaker"] != "unknown" {
                segment["speaker"] = json!(format!(
                    "chunk{}:{}",
                    index + 1,
                    segment["speaker"].as_str().unwrap_or("unknown")
                ));
            }
            segment["start"] = json!(segment["start"].as_f64().unwrap_or(0.) + chunk.offset);
            segment["end"] = json!(segment["end"].as_f64().unwrap_or(0.) + chunk.offset);
            combined.push(segment);
        }
        let valid_nonnegative = |v: &Value| v.as_f64().filter(|v| v.is_finite() && *v >= 0.);
        cost = cost
            .zip(valid_nonnegative(&raw["usage"]["cost"]))
            .map(|(sum, n)| sum + n);
        seconds = seconds
            .zip(valid_nonnegative(&raw["usage"]["seconds"]))
            .map(|(sum, n)| sum + n);
        input_tokens = input_tokens
            .zip(raw["usage"]["input_tokens"].as_u64())
            .and_then(|(sum, n)| sum.checked_add(n));
        output_tokens = output_tokens
            .zip(raw["usage"]["output_tokens"].as_u64())
            .and_then(|(sum, n)| sum.checked_add(n));
        let detected = raw["language"].as_str().unwrap_or("unknown");
        language = Some(match language {
            None => detected.into(),
            Some(old) if old == detected => old,
            _ => "unknown".into(),
        });
    }
    Ok(
        json!({"provider":"opencode","upstream_provider":"openrouter","model":config["model"],"state":"ready","language":language.unwrap_or_else(||"unknown".into()),"role_confidence":"uncertain","segments":combined,"usage":{"cost":cost,"seconds":seconds,"input_tokens":input_tokens,"output_tokens":output_tokens}}),
    )
}
