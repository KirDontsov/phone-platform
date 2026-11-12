use phone_worker::models;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
static ENV_LOCK: Mutex<()> = Mutex::new(());

async fn scripted_server(
    script: Vec<(&'static str, &'static str, u16, Value)>,
) -> (
    String,
    Arc<Mutex<Vec<(String, Value)>>>,
    tokio::task::JoinHandle<()>,
) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(vec![]));
    let out = captured.clone();
    let task = tokio::spawn(async move {
        for (method, path, status, response) in script {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut bytes = vec![];
            let header_end;
            loop {
                let mut buffer = [0; 1024];
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                    header_end = end + 4;
                    break;
                }
                assert!(bytes.len() < 128 * 1024);
            }
            let headers = String::from_utf8(bytes[..header_end].to_vec()).unwrap();
            let request_line = headers.lines().next().unwrap();
            let mut parts = request_line.split_whitespace();
            assert_eq!(parts.next().unwrap(), method);
            let actual_path = percent_encoding::percent_decode_str(parts.next().unwrap())
                .decode_utf8_lossy()
                .to_string();
            assert_eq!(actual_path, path);
            let length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, v)| v.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while bytes.len() < header_end + length {
                let mut buffer = [0; 1024];
                let n = stream.read(&mut buffer).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let mut body = if length > 0 {
                serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap_or_else(
                    |_| {
                        json!(String::from_utf8_lossy(
                            &bytes[header_end..header_end + length]
                        ))
                    },
                )
            } else {
                Value::Null
            };
            if actual_path.starts_with("/asr/") || actual_path.starts_with("/local/") {
                body["_captured_headers"] = json!(headers);
            }
            out.lock().unwrap().push((actual_path, body));
            let payload = serde_json::to_vec(&response).unwrap();
            let header=format!("HTTP/1.1 {status} OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",payload.len());
            stream.write_all(header.as_bytes()).await.unwrap();
            stream.write_all(&payload).await.unwrap();
        }
    });
    (format!("http://{address}/"), captured, task)
}

#[tokio::test]
async fn opencode_session_is_new_tools_are_disabled_and_session_is_deleted() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("PROVIDER_ALLOWED_HOSTS", "127.0.0.1");
    let (url, requests, server) = scripted_server(vec![
        (
            "GET",
            "/config",
            200,
            json!({"permission":{"*":"deny"},"mcp":{}}),
        ),
        (
            "GET",
            "/experimental/tool/ids",
            200,
            json!(["bash", "read", "mcp_custom_tool"]),
        ),
        ("POST", "/session", 200, json!({"id":"synthetic-session"})),
        (
            "POST",
            "/session/synthetic-session/message",
            200,
            json!({"parts":[{"type":"text","text":"{\"summary\":\"synthetic\"}"}]}),
        ),
        ("DELETE", "/session/synthetic-session", 200, json!(true)),
    ])
    .await;
    let result=tokio::time::timeout(std::time::Duration::from_secs(5),models::analyze(&json!({"provider":"opencode","base_url":url,"api_key":"synthetic-only","model":"vendor/model"}),&json!({"transcript":{"segments":[]}}),false)).await.unwrap().unwrap();
    server.await.unwrap();
    assert_eq!(result["summary"], "synthetic");
    let rows = requests.lock().unwrap();
    let message = &rows[3].1;
    assert_eq!(message["tools"]["mcp_custom_tool"], false);
    assert_eq!(message["tools"]["bash"], false);
    assert_eq!(message["model"]["providerID"], "vendor");
    std::env::remove_var("PROVIDER_ALLOWED_HOSTS");
}
#[tokio::test]
async fn permissive_runner_is_rejected_before_creating_a_session() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("PROVIDER_ALLOWED_HOSTS", "127.0.0.1");
    let (url, requests, server) = scripted_server(vec![(
        "GET",
        "/config",
        200,
        json!({"permission":{"*":"allow"}}),
    )])
    .await;
    let error=tokio::time::timeout(std::time::Duration::from_secs(5),models::analyze(&json!({"provider":"opencode","base_url":url,"api_key":"synthetic","model":"vendor/model"}),&json!({}),false)).await.unwrap().unwrap_err();
    server.await.unwrap();
    assert_eq!(error.code, "runner_tools_not_isolated");
    assert_eq!(requests.lock().unwrap().len(), 1);
    std::env::remove_var("PROVIDER_ALLOWED_HOSTS");
}
#[tokio::test]
async fn openai_compatible_request_uses_json_mode_and_bounded_output() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("PROVIDER_ALLOWED_HOSTS", "127.0.0.1");
    let(url,requests,server)=scripted_server(vec![("POST","/v1/chat/completions",200,json!({"choices":[{"message":{"content":"{\"summary\":\"synthetic\"}"}}],"usage":{"prompt_tokens":7,"completion_tokens":4}}))]).await;
    let result=tokio::time::timeout(std::time::Duration::from_secs(5),models::analyze(&json!({"provider":"openai","base_url":format!("{url}v1"),"api_key":"synthetic","model":"configured-model","max_output_tokens":2048}),&json!({"transcript":{"segments":[]},"status_definitions":[{"id":"status-1","name":"Нужен перезвон"}],"tag_definitions":[{"id":"tag-1","name":"Запрос стоимости"}]}),false)).await.unwrap().unwrap();
    server.await.unwrap();
    assert_eq!(result["usage"]["input_tokens"], 7);
    let rows = requests.lock().unwrap();
    assert_eq!(rows[0].1["response_format"]["type"], "json_object");
    assert_eq!(rows[0].1["max_tokens"], 2048);
    let prompt = rows[0].1["messages"].to_string();
    assert!(prompt.contains("Нужен перезвон"));
    assert!(prompt.contains("Запрос стоимости"));
    std::env::remove_var("PROVIDER_ALLOWED_HOSTS");
}
#[tokio::test]
async fn runner_failure_to_delete_session_is_visible() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("PROVIDER_ALLOWED_HOSTS", "127.0.0.1");
    let (url, _, server) = scripted_server(vec![
        (
            "GET",
            "/config",
            200,
            json!({"permission":{"*":"deny"},"mcp":{}}),
        ),
        ("GET", "/experimental/tool/ids", 200, json!([])),
        ("POST", "/session", 200, json!({"id":"synthetic-session"})),
        (
            "POST",
            "/session/synthetic-session/message",
            200,
            json!({"parts":[{"type":"text","text":"{}"}]}),
        ),
        ("DELETE", "/session/synthetic-session", 500, json!({})),
    ])
    .await;
    let error=tokio::time::timeout(std::time::Duration::from_secs(5),models::analyze(&json!({"provider":"opencode","base_url":url,"api_key":"synthetic","model":"vendor/model"}),&json!({}),false)).await.unwrap().unwrap_err();
    server.await.unwrap();
    assert_eq!(error.code, "runner_cleanup_failed");
    std::env::remove_var("PROVIDER_ALLOWED_HOSTS");
}

#[tokio::test]
async fn asr_upload_uses_multipart_versioned_endpoint_and_preserves_unknown_roles() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("PROVIDER_ALLOWED_HOSTS", "127.0.0.1");
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic.wav");
    std::fs::write(&path, b"RIFF0000WAVEsynthetic-upload-not-real-speech").unwrap();
    let (url, requests, server) = scripted_server(vec![(
        "POST",
        "/v1/audio/transcriptions",
        200,
        json!({"segments":[{"start":0,"end":2,"speaker":"A","text":"Синтетический текст"}]}),
    )])
    .await;
    let transcript=tokio::time::timeout(std::time::Duration::from_secs(5),models::transcribe(&json!({"provider":"openai","base_url":format!("{url}v1"),"api_key":"synthetic","model":"test-asr","response_format":"verbose_json"}),&path,3.,false,&json!({}))).await.unwrap().unwrap();
    server.await.unwrap();
    assert_eq!(transcript["role_confidence"], "uncertain");
    assert_eq!(transcript["segments"][0]["role"], "unknown");
    let rows = requests.lock().unwrap();
    let multipart = rows[0].1.as_str().unwrap();
    assert!(multipart.contains("name=\"file\""));
    assert!(multipart.contains("RIFF0000WAVE"));
    assert!(multipart.contains("name=\"model\""));
    assert!(multipart.contains("test-asr"));
    assert!(multipart.contains("verbose_json"));
    std::env::remove_var("PROVIDER_ALLOWED_HOSTS");
}

struct AsrEnvironment(Vec<(&'static str, Option<String>)>);
impl AsrEnvironment {
    fn new(url: &str) -> Self {
        let values = [
            ("PROVIDER_ALLOWED_HOSTS", "127.0.0.1"),
            ("MODEL_RUNNER_URL", url),
            (
                "MODEL_RUNNER_PASSWORD",
                "synthetic-runner-password-at-least-24",
            ),
        ];
        let old = values
            .iter()
            .map(|(key, _)| (*key, std::env::var(key).ok()))
            .collect();
        for (key, value) in values {
            std::env::set_var(key, value);
        }
        Self(old)
    }
}
impl Drop for AsrEnvironment {
    fn drop(&mut self) {
        for (key, value) in &self.0 {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

#[tokio::test]
async fn opencode_asr_uses_exact_models_tenant_key_and_real_stt_protocol() {
    use base64::Engine;
    let _guard = ENV_LOCK.lock().unwrap();
    for (model, tenant_key) in [
        ("moondream/parakeet-ultra", "synthetic-tenant-a-key"),
        ("nvidia/nemotron-3.5-transcribe", "synthetic-tenant-b-key"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("synthetic.wav");
        let audio = b"RIFF0000WAVE-not-real-audio";
        std::fs::write(&path, audio).unwrap();
        let (url, requests, server) = scripted_server(vec![("POST", "/asr/transcriptions", 200,
            json!({"text":"Synthetic transcript","language":"ru","segments":[{"start":0.,"end":1.,"speaker":0,"text":"Synthetic one"},{"start":1.,"end":2.,"speaker":1,"text":"Synthetic two"}],"usage":{"cost":0.012,"seconds":3}}))]).await;
        let _env = AsrEnvironment::new(&url);
        let transcript = models::transcribe(
            &json!({"provider":"opencode","base_url":url,"api_key":tenant_key,"model":model}),
            &path,
            3.,
            false,
            &json!({}),
        )
        .await
        .unwrap();
        server.await.unwrap();
        assert_eq!(transcript["segments"][0]["speaker"], "0");
        assert_eq!(transcript["segments"][1]["speaker"], "1");
        assert_eq!(transcript["segments"][0]["role"], "unknown");
        assert_eq!(transcript["role_confidence"], "uncertain");
        assert_eq!(transcript["model"], model);
        assert_eq!(transcript["usage"]["cost"], 0.012);
        let rows = requests.lock().unwrap();
        assert_eq!(rows[0].1["model"], model);
        assert_eq!(rows[0].1["response_format"], "verbose_json");
        assert_eq!(rows[0].1["input_audio"]["format"], "wav");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(rows[0].1["input_audio"]["data"].as_str().unwrap())
                .unwrap(),
            audio
        );
        let headers = rows[0].1["_captured_headers"]
            .as_str()
            .unwrap()
            .to_lowercase();
        assert!(headers.contains(&format!("x-openrouter-api-key: {tenant_key}")));
        let basic = base64::engine::general_purpose::STANDARD
            .encode("opencode:synthetic-runner-password-at-least-24");
        assert!(headers.contains(&format!("authorization: basic {basic}").to_lowercase()));
        assert!(!rows[0]
            .1
            .to_string()
            .replace(&rows[0].1["_captured_headers"].to_string(), "")
            .contains(tenant_key));
    }
}

#[tokio::test]
async fn opencode_asr_rejects_url_and_model_before_transport_or_audio_reads() {
    let _guard = ENV_LOCK.lock().unwrap();
    let _env = AsrEnvironment::new("http://127.0.0.1:9/");
    let absent = std::path::Path::new("/nonexistent-synthetic-audio");
    let mut config = json!({"provider":"opencode","base_url":"http://127.0.0.1:8/","api_key":"synthetic-key","model":"moondream/parakeet-ultra"});
    assert_eq!(
        models::transcribe(&config, absent, 3., false, &json!({}))
            .await
            .unwrap_err()
            .code,
        "asr_runner_url_mismatch"
    );
    config["base_url"] = json!("http://127.0.0.1:9/");
    config["model"] = json!("unapproved/provider-model");
    assert_eq!(
        models::transcribe(&config, absent, 3., false, &json!({}))
            .await
            .unwrap_err()
            .code,
        "unsupported_asr_model"
    );
}

#[tokio::test]
async fn opencode_asr_connection_probe_and_failures_preserve_safe_codes_without_chat_or_paid_retry()
{
    let _guard = ENV_LOCK.lock().unwrap();
    let (url, requests, server) =
        scripted_server(vec![("POST", "/asr/test", 200, json!({"ok":true}))]).await;
    let _env = AsrEnvironment::new(&url);
    let config = json!({"provider":"opencode","base_url":url,"model":"moondream/parakeet-ultra","api_key":"synthetic-tenant-key"});
    assert_eq!(
        phone_worker::test_connection("asr", &config).await.unwrap()["ok"],
        true
    );
    server.await.unwrap();
    assert_eq!(requests.lock().unwrap().len(), 1);
    assert!(requests.lock().unwrap()[0].1.get("input_audio").is_none());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.wav");
    std::fs::write(&path, b"RIFF0000WAVEfixture").unwrap();
    for (status, remote_code, local_code, retryable) in [
        (401, "runner_auth_required", "asr_runner_auth_failed", false),
        (
            401,
            "provider_unauthorized",
            "provider_reauthorization_required",
            false,
        ),
        (
            422,
            "provider_request_failed",
            "provider_request_rejected",
            false,
        ),
        (429, "asr_busy", "provider_rate_limited", true),
        (
            502,
            "invalid_transcript_response",
            "invalid_transcript_response",
            false,
        ),
    ] {
        let (url, requests, server) = scripted_server(vec![(
            "POST",
            "/asr/transcriptions",
            status,
            json!({"error":{"code":remote_code,"message":"SYNTHETIC_SECRET_MUST_NOT_ESCAPE"}}),
        )])
        .await;
        let _env = AsrEnvironment::new(&url);
        let mut config = config.clone();
        config["base_url"] = json!(url);
        let error = models::transcribe(&config, &path, 3., false, &json!({}))
            .await
            .unwrap_err();
        assert_eq!(error.code, local_code);
        assert_eq!(error.retryable, retryable);
        assert!(!format!("{error:?}").contains("SYNTHETIC_SECRET"));
        server.await.unwrap();
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "no second format/provider fallback request"
        );
    }
}

#[tokio::test]
async fn local_opencode_asr_uses_runtime_pairing_and_session_tenant_without_forwarding_stale_keys()
{
    let _guard = ENV_LOCK.lock().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("synthetic.wav");
    std::fs::write(&path, b"RIFF synthetic isolated audio").unwrap();
    let (url, requests, server) = scripted_server(vec![("POST", "/asr/transcriptions", 200,
        json!({"text":"Synthetic local transcript","segments":[{"start":0.,"end":1.,"text":"Synthetic local transcript","speaker":0}],"usage":{"seconds":1}}))]).await;
    let _env = AsrEnvironment::new(&url);
    let org = "11111111-1111-4111-8111-111111111111";
    let config = json!({"provider":"opencode","base_url":url,"model":"apinex/voice-stt","local_provider":"apinex","credential_source":"local","_organization_id":org,"api_key":"synthetic-stale-external-key"});
    let transcript = models::transcribe(&config, &path, 2., false, &json!({}))
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(transcript["upstream_provider"], "apinex");
    assert_eq!(transcript["model"], "apinex/voice-stt");
    assert_eq!(transcript["segments"][0]["role"], "unknown");
    assert!(transcript["usage"]["cost"].is_null());
    let rows = requests.lock().unwrap();
    let headers = rows[0].1["_captured_headers"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert!(headers.contains(&format!("x-phone-organization: {org}")));
    assert!(!headers.contains("x-openrouter-api-key"));
    assert!(!headers.contains("synthetic-stale-external-key"));
    assert!(!rows[0]
        .1
        .to_string()
        .contains("synthetic-stale-external-key"));
}

#[tokio::test]
async fn local_catalog_only_exposes_public_model_metadata_and_uses_server_tenant() {
    let _guard = ENV_LOCK.lock().unwrap();
    let (url, requests, server) = scripted_server(vec![("GET", "/local/providers", 200,
        json!({"available":true,"api_key":"synthetic-private-key","providers":[{"id":"apinex","name":"APInex","api_key":"synthetic-private-key","models":[{"id":"apinex/free/test","name":"Test","credential":"synthetic-private-key"}]}],"asr_models":[{"id":"apinex/voice-stt","name":"Wrong invented name","provider":"apinex","api_key":"synthetic-private-key"}]}))]).await;
    let _env = AsrEnvironment::new(&url);
    let org = uuid::Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap();
    let catalog = phone_worker::asr_connector::local_catalog(org)
        .await
        .unwrap();
    server.await.unwrap();
    assert!(!catalog.to_string().contains("synthetic-private-key"));
    assert_eq!(catalog["asr_models"][0]["name"], "APInex Voice STT");
    assert_eq!(
        catalog["providers"][0]["models"][0]["id"],
        "apinex/free/test"
    );
    let rows = requests.lock().unwrap();
    assert!(rows[0].1["_captured_headers"]
        .as_str()
        .unwrap()
        .to_lowercase()
        .contains(&format!("x-phone-organization: {org}")));
}

#[tokio::test]
async fn native_opencode_payment_error_is_not_misreported_as_an_empty_model_answer() {
    let _guard = ENV_LOCK.lock().unwrap();
    std::env::set_var("PROVIDER_ALLOWED_HOSTS", "127.0.0.1");
    let (url, requests, server) = scripted_server(vec![
        ("GET", "/config", 200, json!({"permission":{"*":"deny"},"mcp":{}})),
        ("GET", "/experimental/tool/ids", 200, json!([])),
        ("POST", "/session", 200, json!({"id":"synthetic-session"})),
        ("POST", "/session/synthetic-session/message", 200, json!({"info":{"error":{"name":"APIError","data":{"statusCode":402,"message":"synthetic private provider detail"}}},"parts":[]})),
        ("DELETE", "/session/synthetic-session", 200, json!(true)),
    ]).await;
    let error = models::analyze(&json!({"provider":"opencode","base_url":url,"api_key":"synthetic-only","model":"apinex/free/test"}), &json!({"transcript":{"segments":[]}}), false).await.unwrap_err();
    assert_eq!(error.code, "provider_payment_required");
    assert!(!error.retryable);
    server.await.unwrap();
    assert_eq!(
        requests.lock().unwrap().last().unwrap().0,
        "/session/synthetic-session"
    );
    std::env::remove_var("PROVIDER_ALLOWED_HOSTS");
}
