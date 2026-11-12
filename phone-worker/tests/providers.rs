use chrono::{TimeZone, Utc};
use phone_worker::{models, providers};
use serde_json::json;
use uuid::Uuid;

#[test]
fn telphin_history_rejects_cross_tenant_rows_and_out_of_window_timestamps() {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    let end = start + chrono::Duration::days(1);
    let mut value = json!({"page":1,"per_page":100,"order":"asc","call_history":[{"call_uuid":"synthetic-id","client_owner_id":42,"init_time_gmt":"2026-01-01 12:00:00.123","result":"not answered","flow":"in","cdr":[]}]});
    assert!(providers::validate_history(&value, "42", start, end, 1).is_ok());
    value["call_history"][0]["client_owner_id"] = json!(43);
    assert_eq!(
        providers::validate_history(&value, "42", start, end, 1)
            .unwrap_err()
            .code,
        "call_owner_mismatch"
    );
    value["call_history"][0]["client_owner_id"] = json!(42);
    value["call_history"][0]["init_time_gmt"] = json!("2026-02-01 00:00:00");
    assert_eq!(
        providers::validate_history(&value, "42", start, end, 1)
            .unwrap_err()
            .code,
        "call_outside_requested_window"
    );
}
#[test]
fn malformed_history_never_passes_as_an_empty_final_page() {
    let t = Utc::now();
    for value in [
        json!({"page":1,"per_page":100,"order":"asc"}),
        json!({"page":2,"per_page":100,"order":"asc","call_history":[]}),
        json!({"page":1,"per_page":0,"order":"asc","call_history":[]}),
    ] {
        assert!(providers::validate_history(&value, "42", t, t, 1).is_err());
    }
}
#[test]
fn unknown_speaker_roles_are_never_guessed() {
    let normalized=models::normalize_transcript(&json!({"language":"ru","segments":[{"speaker":"A","role":"manager","start":0.,"end":2.,"text":"Добрый день"}]}),3.).unwrap();
    assert_eq!(normalized["role_confidence"], "uncertain");
    assert_eq!(normalized["segments"][0]["role"], "unknown");
    let coarse =
        models::normalize_transcript(&json!({"text":"Текст без временных меток"}), 20.).unwrap();
    assert_eq!(coarse["segments"][0]["timing"], "whole_recording_only");
}
#[test]
fn invalid_transcript_timestamps_and_silence_are_not_ready() {
    assert!(models::normalize_transcript(&json!({"text":""}), 10.).is_err());
    assert!(models::normalize_transcript(
        &json!({"segments":[{"start":-1.,"end":1.,"text":"x"}]}),
        10.
    )
    .is_err());
    assert!(models::normalize_transcript(
        &json!({"segments":[{"start":0.,"end":300.,"text":"x"}]}),
        10.
    )
    .is_err());
}
#[test]
fn demo_analysis_has_real_evidence_and_domain_rejects_fabricated_quote() {
    let transcript = models::demo_transcript(
        &json!({"synthetic":true,"duration":60,"contact_name":"Синтетический клиент"}),
    );
    let rubric =
        json!({"criteria":[{"id":"greeting","name":"Приветствие","weight":2,"required":true}]});
    let mut analysis = models::demo_analysis(&transcript, &rubric);
    domain::validate_analysis(&mut analysis, &transcript, &rubric, &[], &[]).unwrap();
    assert_eq!(analysis["score"], 80.);
    analysis["criteria"][0]["evidence"][0]["quote"] = json!("Этого не было в разговоре");
    assert!(domain::validate_analysis(&mut analysis, &transcript, &rubric, &[], &[]).is_err());
}
#[tokio::test]
async fn production_never_uses_synthetic_models_without_explicit_mode_and_call_label() {
    let config = json!({"provider":"demo"});
    let path = std::path::Path::new("/nonexistent");
    assert_eq!(
        models::transcribe(&config, path, 10., false, &json!({"synthetic":true}))
            .await
            .unwrap_err()
            .code,
        "demo_provider_not_allowed"
    );
    assert_eq!(
        models::transcribe(&config, path, 10., true, &json!({"synthetic":false}))
            .await
            .unwrap_err()
            .code,
        "demo_provider_not_allowed"
    );
    assert_eq!(
        models::analyze(&config, &json!({"call":{"synthetic":true}}), false)
            .await
            .unwrap_err()
            .code,
        "demo_provider_not_allowed"
    );
}
#[test]
fn provider_urls_fail_closed_and_join_versioned_base_correctly() {
    for url in [
        "http://api.openai.com/v1/",
        "https://api.openai.com@127.0.0.1/v1/",
        "https://evil.invalid/v1/",
        "file:///etc/passwd",
    ] {
        assert!(providers::endpoint(url, "model").is_err());
    }
    assert_eq!(
        models::normalized_base(&json!({"base_url":"https://api.openai.com/v1"}))
            .unwrap()
            .join("models")
            .unwrap()
            .as_str(),
        "https://api.openai.com/v1/models"
    );
    assert!(providers::endpoint("http://model-runner:4096", "model").is_ok());
    assert!(providers::endpoint("https://not-telphin.invalid", "telphin").is_err());
}
#[test]
fn oauth_links_bind_state_and_encoded_redirect() {
    let state = "synthetic-state-12345678901234567890";
    let config = json!({"client_id":"synthetic","client_secret":"never-in-url","redirect_uri":"http://127.0.0.1:8030/api/integrations/sheets/callback"});
    let url = phone_worker::oauth_authorize_url("sheets", &config, state).unwrap();
    assert!(url.contains(state));
    assert!(!url.contains("never-in-url"));
    assert!(url.contains("response_type=code"));
    assert!(phone_worker::oauth_authorize_url("sheets", &config, "short").is_err());
}
#[test]
fn media_paths_are_tenant_scoped_and_cannot_escape() {
    let root = tempfile::tempdir().unwrap();
    let org = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    assert!(phone_worker::pipeline::safe_media_path(
        root.path(),
        org,
        &format!("{org}/recording.wav")
    )
    .is_ok());
    for value in [
        format!("{foreign}/recording.wav"),
        format!("{org}/../secrets"),
        "/etc/passwd".into(),
    ] {
        assert!(phone_worker::pipeline::safe_media_path(root.path(), org, &value).is_err());
    }
}
#[test]
fn phone_normalization_does_not_merge_internal_or_anonymous_callers() {
    assert_eq!(
        phone_worker::pipeline::normalize_phone("8 (999) 000-00-01"),
        "+79990000001"
    );
    assert_eq!(
        phone_worker::pipeline::normalize_phone("+49 1234567890"),
        "+491234567890"
    );
    assert!(phone_worker::pipeline::normalize_phone("0005*101").is_empty());
    assert!(phone_worker::pipeline::normalize_phone("anonymous").is_empty());
}

#[tokio::test]
async fn demo_transcript_uses_recording_duration_not_call_wait_time() {
    let transcript = models::transcribe(
        &json!({"provider":"demo"}),
        std::path::Path::new("/not-read-in-demo"),
        32.,
        true,
        &json!({"synthetic":true,"duration":152}),
    )
    .await
    .unwrap();
    let last = transcript["segments"].as_array().unwrap().last().unwrap();
    assert_eq!(last["end"], 32.);
}

#[test]
fn runner_configuration_must_deny_all_tools_and_mcp() {
    assert!(models::validate_runner_config(&json!({"permission":{"*":"deny"},"mcp":{}})).is_ok());
    for unsafe_config in [
        json!({}),
        json!({"permission":{"*":"ask"}}),
        json!({"permission":{"*":"deny","read":"allow"}}),
        json!({"permission":{"*":"deny"},"mcp":{"remote":{}}}),
        json!({"permission":{"*":"deny"},"plugin":["untrusted-plugin"]}),
        json!({"permission":{"*":"deny"},"mcp":[]}),
    ] {
        assert!(models::validate_runner_config(&unsafe_config).is_err());
    }
}

#[test]
fn sheets_mapping_keeps_tenant_key_in_a_and_rejects_unknown_or_duplicate_columns() {
    use phone_worker::integrations::sheets_columns;
    let call = json!({"id":"synthetic-call","started_at":"2026-01-01","phone":"=SUM(1,2)","score":85,"tag_ids":["tag-1"]});
    let config = json!({"column_mapping":{"call_id":"A","phone":"C","score":"D","tag_ids":"E","call_link":"F"}});
    let values = sheets_columns(&config, &call, None, "https://phone.example", "marker").unwrap();
    assert_eq!(
        values,
        json!([
            "",
            "=SUM(1,2)",
            85,
            "[\"tag-1\"]",
            "https://phone.example/calls/synthetic-call"
        ])
        .as_array()
        .unwrap()
        .clone()
    );
    for mapping in [
        json!({"call_id":"B"}),
        json!({"call_id":"A","phone":"A"}),
        json!({"call_id":"A","phone":"C","score":"C"}),
        json!({"call_id":"A","token":"B"}),
        json!({"call_id":"A","phone":"BA"}),
    ] {
        assert_eq!(
            sheets_columns(&json!({"column_mapping":mapping}), &call, None, "", "")
                .unwrap_err()
                .code,
            "invalid_column_mapping"
        );
    }
}

#[test]
fn sheets_exports_agreements_next_steps_and_explicit_criterion_verdicts() {
    let config = json!({"column_mapping":{"call_id":"A","summary":"B","agreements":"C","next_steps":"D","criteria":"E"}});
    let analysis = json!({"summary":"Synthetic summary","agreements":["Send invoice"],"next_steps":["Call tomorrow"],"criteria":[{"id":"greet","name":"Greeting","evaluation_mode":"checklist","fulfilled":false,"score":0,"status":"scored","reason":"Synthetic evidence"}]});
    let values = phone_worker::integrations::sheets_columns(
        &config,
        &json!({"id":"synthetic-call"}),
        Some(&analysis),
        "",
        "",
    )
    .unwrap();
    assert_eq!(values[0], "Synthetic summary");
    assert_eq!(values[1], analysis["agreements"].to_string());
    assert_eq!(values[2], analysis["next_steps"].to_string());
    let criteria: serde_json::Value = serde_json::from_str(values[3].as_str().unwrap()).unwrap();
    assert_eq!(criteria[0]["fulfilled"], false);
    assert_eq!(criteria[0]["name"], "Greeting");
}

#[test]
fn demo_checklist_result_respects_mode_and_never_infers_a_scaled_verdict() {
    let transcript = models::demo_transcript(&json!({"synthetic":true,"duration":60}));
    let rubric = json!({"criteria":[{"id":"greet","name":"Greeting","weight":1,"required":true,"evaluation_mode":"checklist"}]});
    let mut result = models::demo_analysis(&transcript, &rubric);
    assert_eq!(result["criteria"][0]["fulfilled"], true);
    assert_eq!(result["criteria"][0]["score"], 5);
    domain::validate_analysis(&mut result, &transcript, &rubric, &[], &[]).unwrap();
    assert_eq!(result["score"], 100.);
}

#[test]
fn delivery_write_distinguishes_definite_rejection_from_ambiguous_commit() {
    use phone_worker::{integrations::delivery_write_error, ProviderError};
    let quota = delivery_write_error(ProviderError::retry("provider_rate_limited", 120));
    assert_eq!(quota.code, "provider_rate_limited");
    assert!(quota.retryable);
    assert_eq!(quota.retry_after_seconds, Some(120));
    for code in [
        "provider_network_error",
        "provider_unavailable",
        "provider_body_interrupted",
    ] {
        let error = delivery_write_error(ProviderError::retry(code, 30));
        assert_eq!(error.code, "delivery_uncertain");
        assert!(!error.retryable);
    }
}
