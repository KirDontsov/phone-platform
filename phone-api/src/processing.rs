//! Safe tenant-scoped read model for call processing; never returns provider configuration.
use crate::*;
use persistence::Job;
use std::collections::HashMap;

pub fn active(job: &Job) -> bool {
    matches!(job.state.as_str(), "queued" | "running" | "retry") && job.attempts < job.max_attempts
        || job.state == "running"
}
fn processing_job(job: &Job) -> bool {
    matches!(job.kind.as_str(), "transcribe" | "analyze" | "reprocess")
}
fn job_priority(job: &Job) -> (u8, DateTime<Utc>, Uuid) {
    let priority = match (active(job), job.kind.as_str() == "reprocess") {
        (true, false) => 3,
        (true, true) => 2,
        (false, false) => 1,
        (false, true) if job.state == "succeeded" => 0,
        (false, true) => 1,
    };
    (priority, job.created_at, job.id)
}
fn safe_error(error: Option<&str>) -> Value {
    let code = match error {
        None => return Value::Null,
        Some(
            code @ ("configuration_missing"
            | "demo_provider_not_allowed"
            | "connection_decryption_failed"
            | "provider_rate_limited"
            | "provider_unauthorized"
            | "provider_forbidden"
            | "provider_unavailable"
            | "provider_timeout"
            | "provider_request_failed"
            | "provider_network_error"
            | "recording_pending"
            | "recording_not_ready"
            | "recording_file_missing"
            | "recording_too_large"
            | "audio_chunking_failed"
            | "transcript_empty_or_silent"
            | "invalid_transcript_timing"
            | "transcript_required"
            | "active_rubric_required"
            | "analysis_budget_exceeded"
            | "analysis_daily_budget_reached"
            | "invalid_analysis_budget"
            | "provider_reauthorization_required"
            | "provider_permission_denied"
            | "provider_request_rejected"
            | "provider_resource_missing"
            | "recording_duration_unavailable"
            | "invalid_analysis_evidence_or_schema"
            | "invalid_analysis_json"
            | "analysis_response_missing"
            | "analysis_context_too_large"
            | "runner_tools_not_isolated"
            | "runner_cleanup_failed"
            | "attempts_exhausted"
            | "no_recording"
            | "asr_runner_auth_missing"
            | "asr_runner_auth_failed"
            | "asr_runner_url_mismatch"
            | "unsupported_asr_model"
            | "asr_invalid_request"
            | "asr_audio_too_large"
            | "invalid_transcript_response"
            | "asr_response_too_large"),
        ) => code,
        Some(_) => "processing_failed",
    };
    json!(code)
}
pub struct Snapshot {
    pub jobs: Vec<Job>,
    selected_jobs: HashMap<Uuid, usize>,
    transcripts: HashMap<Uuid, Document>,
    analyses: HashMap<Uuid, Vec<Document>>,
}
impl Snapshot {
    pub fn load(conn: &mut diesel::PgConnection, org: Uuid) -> Result<Self, ApiError> {
        let mut transcripts: HashMap<Uuid, Document> = HashMap::new();
        for d in persistence::list_documents(conn, org, "transcript")? {
            if let Some(parent) = d.parent_id {
                if transcripts
                    .get(&parent)
                    .is_none_or(|old| (old.created_at, old.id) < (d.created_at, d.id))
                {
                    transcripts.insert(parent, d);
                }
            }
        }
        let mut analyses: HashMap<Uuid, Vec<Document>> = HashMap::new();
        for d in persistence::list_documents(conn, org, "analysis")? {
            if let Some(parent) = d.parent_id {
                analyses.entry(parent).or_default().push(d);
            }
        }
        let jobs = persistence::processing_jobs(conn, org)?;
        let mut selected_jobs: HashMap<Uuid, usize> = HashMap::new();
        for (index, job) in jobs.iter().enumerate().filter(|(_, j)| processing_job(j)) {
            if let Some(call) = job.subject_id {
                if selected_jobs
                    .get(&call)
                    .is_none_or(|old| job_priority(&jobs[*old]) < job_priority(job))
                {
                    selected_jobs.insert(call, index);
                }
            }
        }
        Ok(Self {
            jobs,
            selected_jobs,
            transcripts,
            analyses,
        })
    }
    pub fn summary(&self, call: &Value) -> Value {
        let id = call["id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .unwrap_or(Uuid::nil());
        let transcript = self
            .transcripts
            .get(&id)
            .filter(|d| d.data["state"] == "ready");
        let analysis = transcript.and_then(|t| {
            self.analyses.get(&id).and_then(|rows| {
                rows.iter()
                    .filter(|a| {
                        a.data["transcript_id"].as_str() == Some(&t.id.to_string())
                            && matches!(a.data["state"].as_str(), Some("ready" | "partial"))
                    })
                    .max_by_key(|a| (a.created_at, a.id))
            })
        });
        let analysis_state = analysis
            .map(|a| {
                if a.data["state"] == "partial"
                    || a.data["score"].is_null()
                    || a.data["abstention_reason"]
                        .as_str()
                        .is_some_and(|s| !s.is_empty())
                {
                    "partial"
                } else {
                    "analyzed"
                }
            })
            .unwrap_or("not_analyzed");
        let job = self.selected_jobs.get(&id).map(|index| &self.jobs[*index]);
        let state = job
            .map(|j| match j.state.as_str() {
                "queued" | "retry" if j.attempts >= j.max_attempts => "failed",
                "queued" => "queued",
                "running" => "running",
                "retry" => "retry",
                "failed" | "uncertain" => "failed",
                _ => {
                    if call["stage"] == "skipped" {
                        "skipped"
                    } else if analysis.is_some() {
                        "completed"
                    } else {
                        "idle"
                    }
                }
            })
            .unwrap_or_else(|| {
                if call["stage"] == "skipped" {
                    "skipped"
                } else if analysis.is_some() {
                    "completed"
                } else {
                    "idle"
                }
            });
        json!({"transcript_state":if transcript.is_some(){"ready"}else{"missing"},"analysis_state":analysis_state,"analysis_score":analysis.map(|a|a.data["score"].clone()).unwrap_or(Value::Null),"processing":{
            "state":state,"step":job.map(|j|j.kind.as_str()),"job_id":job.map(|j|j.id),
            "error_code":if matches!(state,"failed"|"retry"){safe_error(job.and_then(|j|j.error_code.as_deref()))}else if state=="skipped"{safe_error(call["processing_reason"].as_str())}else{Value::Null},
            "queued_at":job.map(|j|j.created_at),"updated_at":job.map(|j|j.updated_at),"next_attempt_at":job.filter(|_|matches!(state,"queued"|"retry")).map(|j|j.next_attempt_at),
            "attempts":job.map(|j|j.attempts).unwrap_or(0),"max_attempts":job.map(|j|j.max_attempts).unwrap_or(0)
        }})
    }
    pub fn enrich(&self, calls: &mut [Value]) {
        for call in calls {
            let summary = self.summary(call);
            for key in ["transcript_state", "analysis_state", "processing"] {
                call[key] = summary[key].clone();
            }
            call["score"] = summary["analysis_score"].clone();
        }
    }
    pub fn queue(&self, call: &Value) -> Value {
        let now = Utc::now();
        let ready = |j: &&Job| {
            active(j) && matches!(j.state.as_str(), "queued" | "retry") && j.next_attempt_at <= now
        };
        let id = call["id"]
            .as_str()
            .and_then(|id| Uuid::parse_str(id).ok())
            .unwrap_or(Uuid::nil());
        let selected = self
            .selected_jobs
            .get(&id)
            .map(|index| &self.jobs[*index])
            .filter(|j| {
                active(j)
                    && matches!(j.state.as_str(), "queued" | "retry")
                    && j.next_attempt_at <= now
            });
        let ahead = selected.map(|selected| {
            self.jobs
                .iter()
                .filter(ready)
                .filter(|j| {
                    (j.next_attempt_at, j.created_at, j.id)
                        < (selected.next_attempt_at, selected.created_at, selected.id)
                })
                .count()
        });
        json!({"scope":"organization","ready_ahead":ahead,"ready_total":self.jobs.iter().filter(ready).count(),"running_total":self.jobs.iter().filter(|j|active(j)&&j.state=="running").count(),"scheduled_total":self.jobs.iter().filter(|j|active(j)&&matches!(j.state.as_str(),"queued"|"retry")&&j.next_attempt_at>now).count(),"estimated_wait_seconds":null})
    }
}

pub fn providers(
    conn: &mut diesel::PgConnection,
    org: Uuid,
    key: &[u8; 32],
    demo: bool,
    call: &Value,
) -> Result<Value, ApiError> {
    let connections = persistence::list_documents(conn, org, "connection")?;
    let mut result = json!({});
    for kind in ["asr", "analysis"] {
        let configuration = connections.iter().find(|d| d.data["kind"] == kind);
        let config = configuration
            .filter(|d| d.data["state"] != "disconnected")
            .and_then(|d| d.data["config_encrypted"].as_str())
            .and_then(|encrypted| contracts::unseal(encrypted, key).ok());
        let config = config.or_else(|| {
            if configuration.is_none() && demo {
                Some(json!({"provider":"demo","model":"demo-synthetic"}))
            } else {
                None
            }
        });
        let reason = match config.as_ref() {
            None => Some("configuration_missing"),
            Some(config) if config["provider"] == "demo" => {
                if demo && call["synthetic"] == true {
                    None
                } else {
                    Some("demo_provider_not_allowed")
                }
            }
            Some(config) => {
                let allowed = if kind == "asr" {
                    ["opencode", "openai", "http"]
                        .contains(&config["provider"].as_str().unwrap_or(""))
                } else {
                    ["opencode", "openai", "http"]
                        .contains(&config["provider"].as_str().unwrap_or(""))
                };
                let complete = ["model", "base_url", "api_key"].iter().all(|field| {
                    config[*field]
                        .as_str()
                        .is_some_and(|v| !v.trim().is_empty())
                });
                let valid_asr_connector = kind != "asr"
                    || config["provider"] != "opencode"
                    || phone_worker::asr_connector::validate_config(config).is_ok();
                if allowed
                    && complete
                    && valid_asr_connector
                    && phone_worker::models::normalized_base(config).is_ok()
                {
                    None
                } else {
                    Some("invalid_configuration")
                }
            }
        };
        result[kind] = json!({"ready":reason.is_none(),"reason":reason});
    }
    Ok(result)
}

pub fn preflight(
    conn: &mut diesel::PgConnection,
    org: Uuid,
    key: &[u8; 32],
    demo: bool,
    call: &Document,
    stage: &str,
) -> Result<(), ApiError> {
    let public = to_json(call.clone());
    let configured = providers(conn, org, key, demo, &public)?;
    let children = persistence::list_children(conn, org, call.id, None)?;
    if stage != "analyze" {
        if !children.iter().any(|d| {
            d.kind == "recording" && (d.data["ready"] == true || d.data["state"] == "ready")
        }) {
            return Err(ApiError::new(
                422,
                "recording_not_ready",
                "Готовая запись пока отсутствует. Дождитесь загрузки записи",
            ));
        }
        if configured["asr"]["ready"] != true {
            return Err(ApiError::new(
                422,
                "asr_not_configured",
                "Расшифровка не настроена для реальных звонков. Откройте Настройки → Расшифровка",
            ));
        }
    } else if !children
        .iter()
        .any(|d| d.kind == "transcript" && d.data["state"] == "ready")
    {
        return Err(ApiError::new(
            422,
            "transcript_required",
            "Сначала расшифруйте запись",
        ));
    }
    if stage != "transcribe" {
        if configured["analysis"]["ready"] != true {
            return Err(ApiError::new(
                422,
                "analysis_not_configured",
                "Анализ не настроен. Откройте Настройки → Нейросетевой анализ",
            ));
        }
        if !persistence::list_documents(conn, org, "rubric")?
            .iter()
            .any(|d| d.data["active"] == true)
        {
            return Err(ApiError::new(
                422,
                "active_rubric_required",
                "Создайте и активируйте критерии качества",
            ));
        }
    }
    Ok(())
}
