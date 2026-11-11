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
            let body = if length > 0 {
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
    let result=tokio::time::timeout(std::time::Duration::from_secs(5),models::analyze(&json!({"provider":"openai","base_url":format!("{url}v1"),"api_key":"synthetic","model":"configured-model","max_output_tokens":2048}),&json!({"transcript":{"segments":[]}}),false)).await.unwrap().unwrap();
    server.await.unwrap();
    assert_eq!(result["usage"]["input_tokens"], 7);
    let rows = requests.lock().unwrap();
    assert_eq!(rows[0].1["response_format"]["type"], "json_object");
    assert_eq!(rows[0].1["max_tokens"], 2048);
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
