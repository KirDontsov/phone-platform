use crate::{
    integrations, models,
    providers::{self, ProviderError, Result, Telphin},
};
use base64::Engine;
use chrono::{DateTime, NaiveDate, Utc};
use diesel::{
    sql_types::{Integer, Uuid as SqlUuid},
    Connection, RunQueryDsl,
};
use persistence::{Document, Job, Pool};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;

impl From<persistence::DbError> for ProviderError {
    fn from(error: persistence::DbError) -> Self {
        match error {
            persistence::DbError::Conflict => Self::retry("stale_or_conflicting_document", 1),
            persistence::DbError::NotFound => Self::permanent("subject_missing"),
            persistence::DbError::Validation(_) => Self::permanent("invalid_job_reference"),
            _ => Self::retry("worker_database_unavailable", 30),
        }
    }
}
impl From<diesel::result::Error> for ProviderError {
    fn from(_: diesel::result::Error) -> Self {
        Self::retry("worker_database_unavailable", 30)
    }
}

/// New exports use the current transcript's result. A delivery that has already
/// attempted a write keeps its exact analysis ID, including an explicitly pinned null.
pub fn select_export_analysis(
    conn: &mut diesel::PgConnection,
    org: Uuid,
    call: Uuid,
    delivery: Option<&Value>,
) -> Result<Option<Document>> {
    let analyses = persistence::list_children(conn, org, call, Some("analysis"))?;
    if let Some(pinned) = delivery.and_then(|d| d.get("analysis_id")) {
        if pinned.is_null() {
            return Ok(None);
        }
        let id = pinned
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .ok_or_else(|| ProviderError::permanent("pinned_analysis_missing"))?;
        return analyses
            .into_iter()
            .find(|a| a.id == id)
            .map(Some)
            .ok_or_else(|| ProviderError::permanent("pinned_analysis_missing"));
    }
    let transcript = persistence::list_children(conn, org, call, Some("transcript"))?
        .into_iter()
        .max_by_key(|d| (d.created_at, d.id));
    let Some(transcript) = transcript.filter(|t| t.data["state"] == "ready") else {
        return Ok(None);
    };
    Ok(analyses
        .into_iter()
        .filter(|a| {
            a.data["transcript_id"].as_str() == Some(&transcript.id.to_string())
                && matches!(a.data["state"].as_str(), Some("ready" | "partial"))
        })
        .max_by_key(|a| (a.created_at, a.id)))
}
pub fn export_call_data(call: &Value, analysis: Option<&Value>) -> Value {
    let mut call = call.clone();
    call["score"] = analysis.map(|a| a["score"].clone()).unwrap_or(Value::Null);
    call
}
#[derive(Clone)]
pub struct WorkerConfig {
    pub database_url: String,
    pub encryption_key: [u8; 32],
    pub media_root: PathBuf,
    pub demo_mode: bool,
    pub rabbitmq_url: Option<String>,
    pub public_url: String,
    pub poll_seconds: u64,
}
impl WorkerConfig {
    pub fn from_env() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL")
            .map_err(|_| ProviderError::permanent("database_configuration_missing"))?;
        let encoded = std::env::var("ENCRYPTION_KEY")
            .map_err(|_| ProviderError::permanent("encryption_key_missing"))?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded.trim())
            .map_err(|_| ProviderError::permanent("invalid_encryption_key"))?;
        let encryption_key = bytes
            .try_into()
            .map_err(|_| ProviderError::permanent("invalid_encryption_key"))?;
        Ok(Self {
            database_url,
            encryption_key,
            media_root: std::env::var("MEDIA_ROOT")
                .unwrap_or_else(|_| "storage/media".into())
                .into(),
            demo_mode: matches!(std::env::var("DEMO_MODE").as_deref(), Ok("true" | "1")),
            rabbitmq_url: std::env::var("RABBITMQ_URL").ok(),
            public_url: std::env::var("PUBLIC_URL")
                .unwrap_or_else(|_| "http://localhost:8031".into()),
            poll_seconds: 2,
        })
    }
}
/// A previous ambiguous write stays ambiguous even if a later read/refresh fails.
fn delivery_requires_reconciliation(data: &Value) -> bool {
    data["requires_reconciliation"] == true
        || matches!(data["state"].as_str(), Some("attempting" | "uncertain"))
}
fn preserve_delivery_failure(data: &mut Value, code: &str) {
    let uncertain = delivery_requires_reconciliation(data) || code == "delivery_uncertain";
    data["requires_reconciliation"] = json!(uncertain);
    data["state"] = json!(if uncertain { "uncertain" } else { "failed" });
}
fn analysis_taxonomy(documents: Vec<Document>) -> Vec<Value> {
    documents
        .into_iter()
        .filter(|d| d.data["archived"] != true)
        .map(|d| json!({"id":d.id,"name":d.data["name"]}))
        .collect()
}

#[derive(Clone)]
pub struct Worker {
    pub pool: Pool,
    pub config: WorkerConfig,
    pub worker_id: String,
}
impl Worker {
    pub fn new(config: WorkerConfig) -> Result<Self> {
        let pool = persistence::create_pool(&config.database_url)?;
        persistence::migrate(&pool)?;
        Ok(Self {
            pool,
            config,
            worker_id: format!("worker-{}", Uuid::new_v4()),
        })
    }
    async fn db<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut persistence::PgConnection) -> Result<T> + Send + 'static,
    {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = persistence::connection(&pool)?;
            operation(&mut conn)
        })
        .await
        .map_err(|_| ProviderError::retry("worker_database_task_failed", 30))?
    }
    async fn guarded<T, F>(&self, job: &Job, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut persistence::PgConnection) -> Result<T> + Send + 'static,
    {
        let job = job.clone();
        let worker = self.worker_id.clone();
        self.db(move |conn| {
            conn.transaction::<_, ProviderError, _>(|conn| {
                if !persistence::heartbeat(conn, job.id, job.fencing, &worker, 300)? {
                    return Err(ProviderError::permanent("stale_job_lease"));
                }
                if let Some(id) = job.subject_id {
                    if let Some(subject) = persistence::get_document(conn, job.organization_id, id)?
                    {
                        if subject.data["deleted"] == true && job.kind != "purge" {
                            return Err(ProviderError::permanent("subject_deleted"));
                        }
                    } else if job.kind != "purge" {
                        return Err(ProviderError::permanent("subject_missing"));
                    }
                }
                operation(conn)
            })
        })
        .await
    }
    async fn final_write<T, F>(&self, job: &Job, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut persistence::PgConnection) -> Result<T> + Send + 'static,
    {
        let job_copy = job.clone();
        let worker = self.worker_id.clone();
        self.guarded(job, move |conn| {
            let result = operation(conn)?;
            if !persistence::finish_job(
                conn,
                job_copy.id,
                job_copy.fencing,
                &worker,
                "succeeded",
                None,
                0,
            )? {
                return Err(ProviderError::permanent("stale_job_lease"));
            }
            Ok(result)
        })
        .await
    }
    async fn document(&self, org: Uuid, id: Uuid, kind: &str) -> Result<Document> {
        let kind = kind.to_string();
        self.db(move |conn| {
            let doc = persistence::get_document(conn, org, id)?
                .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
            if doc.kind != kind || doc.data["deleted"] == true {
                return Err(ProviderError::permanent("subject_unavailable"));
            }
            Ok(doc)
        })
        .await
    }
    async fn documents(&self, org: Uuid, kind: &str) -> Result<Vec<Document>> {
        let kind = kind.to_owned();
        self.db(move |conn| Ok(persistence::list_documents(conn, org, &kind)?))
            .await
    }
    async fn connection_config(&self, org: Uuid, kind: &str) -> Result<(Option<Document>, Value)> {
        let doc = self
            .documents(org, "connection")
            .await?
            .into_iter()
            .find(|d| d.data["kind"] == kind);
        if let Some(doc) = doc {
            if doc.data["state"] == "disconnected" {
                return Err(ProviderError::permanent("configuration_missing"));
            }
            if let Some(next) = doc.data["next_request_at"]
                .as_i64()
                .filter(|v| *v > Utc::now().timestamp())
            {
                return Err(ProviderError::retry(
                    "provider_rate_limited",
                    (next - Utc::now().timestamp()).max(1) as u64,
                ));
            }
            let encrypted = doc.data["config_encrypted"]
                .as_str()
                .ok_or_else(|| ProviderError::permanent("configuration_missing"))?;
            let mut config = contracts::unseal(encrypted, &self.config.encryption_key)
                .map_err(|_| ProviderError::permanent("connection_decryption_failed"))?;
            if matches!(kind, "asr" | "analysis") {
                config["_organization_id"] = json!(org);
            }
            return Ok((Some(doc), config));
        }
        if self.config.demo_mode && matches!(kind, "asr" | "analysis") {
            return Ok((None, json!({"provider":"demo","model":"demo-synthetic"})));
        }
        Err(ProviderError::permanent("configuration_missing"))
    }
    async fn integration_config(&self, org: Uuid, provider: &str) -> Result<Value> {
        let (document, mut config) = self.connection_config(org, provider).await?;
        if config["expires_at"]
            .as_i64()
            .is_some_and(|v| v < Utc::now().timestamp() + 60)
        {
            // Serialize token refresh across worker processes with a DB advisory lock in a dedicated connection.
            // The lock is held through HTTP and the encrypted update; credentials never enter job payloads.
            let pool = self.pool.clone();
            let id = document.as_ref().unwrap().id;
            let key = self.config.encryption_key;
            let lock_key = i64::from_be_bytes(id.as_bytes()[0..8].try_into().unwrap());
            let mut conn = tokio::task::spawn_blocking(move || persistence::connection(&pool))
                .await
                .map_err(|_| ProviderError::retry("worker_database_task_failed", 30))??;
            diesel::sql_query("SELECT pg_advisory_lock($1)")
                .bind::<diesel::sql_types::BigInt, _>(lock_key)
                .execute(&mut conn)?;
            let mut token_lock = TokenLock {
                connection: conn,
                key: lock_key,
            };
            let latest = persistence::get_document(&mut token_lock.connection, org, id)?
                .ok_or_else(|| ProviderError::permanent("configuration_missing"))?;
            let fresh =
                contracts::unseal(providers::required(&latest.data, "config_encrypted")?, &key)
                    .map_err(|_| ProviderError::permanent("connection_decryption_failed"))?;
            config = fresh;
            if config["expires_at"]
                .as_i64()
                .is_some_and(|v| v < Utc::now().timestamp() + 60)
            {
                let refreshed = providers::oauth_refresh(provider, &config).await;
                match refreshed {
                    Ok(tokens) => {
                        for (k, v) in tokens
                            .as_object()
                            .ok_or_else(|| ProviderError::permanent("invalid_oauth_response"))?
                        {
                            config[k] = v.clone();
                        }
                        let mut data = latest.data.clone();
                        data["config_encrypted"] =
                            json!(contracts::seal(&config, &key).map_err(|_| {
                                ProviderError::permanent("connection_encryption_failed")
                            })?);
                        persistence::save_document(
                            &mut token_lock.connection,
                            org,
                            id,
                            "connection",
                            None,
                            data,
                            Some(latest.version),
                        )?;
                    }
                    Err(error) => {
                        return Err(error);
                    }
                }
            }
            drop(token_lock);
        }
        Ok(config)
    }
    pub async fn run_once(&self) -> Result<bool> {
        let worker = self.worker_id.clone();
        let kinds = vec![
            "sync",
            "recording",
            "transcribe",
            "analyze",
            "reprocess",
            "export",
            "purge",
            "retention",
        ]
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
        let claimed = self
            .db(move |conn| Ok(persistence::claim_job(conn, &worker, &kinds, 300)?))
            .await?;
        let Some(job) = claimed else { return Ok(false) };
        let heartbeat_worker = self.clone();
        let heartbeat_job = job.clone();
        let heartbeat = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(30)).await;
                let worker = heartbeat_worker.worker_id.clone();
                let j = heartbeat_job.clone();
                let _ = heartbeat_worker.write_health("working", None).await;
                if !heartbeat_worker
                    .db(move |conn| {
                        Ok(persistence::heartbeat(conn, j.id, j.fencing, &worker, 300)?)
                    })
                    .await
                    .unwrap_or(false)
                {
                    break;
                }
            }
        });
        let _ = self.write_health("working", None).await;
        let result = self.execute(&job).await;
        if let Err(error) = &result {
            let _ = self.record_failure(&job, error).await;
        }
        heartbeat.abort();
        let worker = self.worker_id.clone();
        let finished = job.clone();
        let (state, error, delay) = match result {
            Ok(()) => ("succeeded", None, 0),
            Err(e) => {
                let state = if e.code == "delivery_uncertain" {
                    "uncertain"
                } else if e.retryable {
                    "retry"
                } else {
                    "failed"
                };
                tracing::warn!(job_id=%job.id,code=%e.code,"Job ended without success");
                (
                    state,
                    Some(e.code),
                    e.retry_after_seconds.unwrap_or(60).min(86_400) as i32,
                )
            }
        };
        let _ = self.write_health("idle", error.as_deref()).await;
        self.db(move |conn| {
            let _ = persistence::finish_job(
                conn,
                finished.id,
                finished.fencing,
                &worker,
                state,
                error.as_deref(),
                delay,
            )?;
            Ok(())
        })
        .await?;
        Ok(true)
    }
    async fn record_failure(&self, job: &Job, error: &ProviderError) -> Result<()> {
        let org = job.organization_id;
        let subject = job.subject_id;
        let kind = job.kind.clone();
        let code = error.code.clone();
        let retry = error.retryable;
        let delay = error.retry_after_seconds.unwrap_or(60).min(86400) as i64;
        let provider_kind = match job.kind.as_str() {
            "sync" | "recording" => "telphin",
            "transcribe" => "asr",
            "analyze" => "analysis",
            "export" => job.payload["provider"].as_str().unwrap_or(""),
            _ => "",
        }
        .to_string();
        let delivery_id = job.payload["delivery_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(|| Uuid::new_v5(&job.id, b"delivery"));
        self.guarded(job, move |conn| {
            if let Some(id) = subject {
                if let Some(mut doc) = persistence::get_document(conn, org, id)? {
                    if doc.kind == "call"
                        && doc.data["deleted"] != true
                        && matches!(kind.as_str(), "transcribe" | "analyze" | "reprocess")
                    {
                        doc.data["stage"] = json!("failed");
                        doc.data["processing_reason"] = json!(code);
                        doc.data["retry_pending"] = json!(retry);
                        persistence::save_document(
                            conn,
                            org,
                            id,
                            "call",
                            doc.parent_id,
                            doc.data,
                            Some(doc.version),
                        )?;
                    } else if doc.kind == "recording" {
                        doc.data["state"] = json!(if retry { "pending" } else { "failed" });
                        doc.data["last_error"] = json!(code);
                        persistence::save_document(
                            conn,
                            org,
                            id,
                            "recording",
                            doc.parent_id,
                            doc.data,
                            Some(doc.version),
                        )?;
                    } else if doc.kind == "connection" {
                        doc.data["last_error"] = json!(code);
                        if !retry {
                            doc.data["state"] = json!("error");
                        }
                        persistence::save_document(
                            conn,
                            org,
                            id,
                            "connection",
                            None,
                            doc.data,
                            Some(doc.version),
                        )?;
                    }
                }
                if kind == "export" {
                    let mut doc = persistence::get_document(conn, org, delivery_id)?
                        .map(|d| d.data)
                        .unwrap_or_else(|| json!({"call_id":id}));
                    preserve_delivery_failure(&mut doc, &code);
                    doc["error_code"] = json!(code);
                    persistence::save_document(
                        conn,
                        org,
                        delivery_id,
                        "delivery",
                        Some(id),
                        doc,
                        None,
                    )?;
                }
            }
            if code == "provider_rate_limited" && !provider_kind.is_empty() {
                if let Some(mut provider) = persistence::list_documents(conn, org, "connection")?
                    .into_iter()
                    .find(|d| d.data["kind"] == provider_kind)
                {
                    provider.data["next_request_at"] = json!(Utc::now().timestamp() + delay);
                    provider.data["last_error"] = json!(code);
                    persistence::save_document(
                        conn,
                        org,
                        provider.id,
                        "connection",
                        None,
                        provider.data,
                        Some(provider.version),
                    )?;
                }
            }
            Ok(())
        })
        .await
    }
    pub async fn run(&self) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        let mut health_interval = tokio::time::interval(Duration::from_secs(10));
        self.write_health("ready", None).await?;
        loop {
            tokio::select! {
                _=tokio::signal::ctrl_c()=>return Ok(()),
                _=health_interval.tick()=>{let _=self.write_health("idle",None).await;},
                _=interval.tick()=>{if let Err(e)=self.schedule().await{tracing::warn!(code=%e.code,"Scheduler retry pending");}let _=self.relay_outbox().await;},
                _=tokio::time::sleep(Duration::from_secs(self.config.poll_seconds))=>{if let Err(e)=self.run_once().await{tracing::warn!(code=%e.code,"Worker polling error");}},
            }
        }
    }
    async fn execute(&self, job: &Job) -> Result<()> {
        match job.kind.as_str() {
            "sync" => self.sync(job).await,
            "recording" => self.recording(job).await,
            "transcribe" => self.transcribe(job).await,
            "analyze" => self.analyze(job).await,
            "reprocess" => self.reprocess(job).await,
            "export" => self.export(job).await,
            "purge" => self.purge(job).await,
            "retention" => self.retention(job).await,
            _ => Err(ProviderError::permanent("unsupported_job_kind")),
        }
    }
    async fn sync(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let connection = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        let connection_doc = self.document(org, connection, "connection").await?;
        if connection_doc.data["kind"] != "telphin"
            || connection_doc.data["state"] == "disconnected"
        {
            return Err(ProviderError::permanent("connection_disconnected"));
        }
        let (_, config) = self.connection_config(org, "telphin").await?;
        let mut telphin = Telphin::connect(&config).await?;
        let run_id = Uuid::new_v5(&job.id, b"sync-run");
        let existing = self
            .db(move |conn| Ok(persistence::get_document(conn, org, run_id)?))
            .await?;
        let mut run = existing.map(|v| v.data).unwrap_or_else(
            || json!({"state":"running","window":0,"page":1,"imported":0,"page_hashes":[]}),
        );
        if run["state"] == "completed" {
            return Ok(());
        }
        let all = job.payload["all_available"].as_bool().unwrap_or(false);
        let (start, end) = sync_range(&config, &job.payload, &run, Utc::now())?;
        let coverage_from = if run.get("coverage_goal_from").is_some() {
            run["coverage_goal_from"]
                .as_str()
                .and_then(|t| providers::parse_utc(t).ok())
        } else {
            verified_coverage_goal(&connection_doc.data, &config, start, end, all)
        };
        let complete_history = coverage_from.is_some();
        run["coverage_goal_from"] = json!(coverage_from);
        let mut windows = vec![];
        let mut upper = end;
        while upper > start {
            let lower = (upper - chrono::Duration::days(7)).max(start);
            windows.push((lower, upper));
            upper = if lower == start {
                start
            } else {
                lower + chrono::Duration::seconds(1)
            };
        }
        run["from"] = json!(start);
        run["to"] = json!(end);
        run["all_available"] = json!(all);
        run["lower_bound"] = json!(if complete_history {
            "operator_verified"
        } else {
            "unknown"
        });
        run["coverage_complete"] = json!(false);
        let window = run["window"].as_u64().unwrap_or(0) as usize;
        for (index, (lower, upper)) in windows.iter().enumerate().skip(window) {
            let mut page = if index == window {
                run["page"].as_u64().unwrap_or(1) as u32
            } else {
                1
            };
            loop {
                if page > 10_000 {
                    return Err(ProviderError::permanent("sync_page_limit"));
                }
                let result = telphin.history(*lower, *upper, page, 100).await;
                let data = match result {
                    Ok(v) => v,
                    Err(error) => {
                        let mut failed = run.clone();
                        failed["state"] = json!("retry_pending");
                        failed["last_error"] = json!(error.code);
                        failed["next_attempt_at"] = json!(
                            Utc::now()
                                + chrono::Duration::seconds(
                                    error.retry_after_seconds.unwrap_or(60) as i64
                                )
                        );
                        self.guarded(job, move |conn| {
                            persistence::save_document(
                                conn,
                                org,
                                run_id,
                                "sync_run",
                                Some(connection),
                                failed,
                                None,
                            )?;
                            Ok(())
                        })
                        .await?;
                        return Err(error);
                    }
                };
                let calls = data["call_history"].as_array().unwrap();
                let empty = calls.is_empty();
                let hash = hex::encode(Sha256::digest(
                    serde_json::to_vec(calls).unwrap_or_default(),
                ));
                let hashes = run["page_hashes"].as_array().cloned().unwrap_or_default();
                if !empty && hashes.iter().any(|v| v.as_str() == Some(&hash)) {
                    return Err(ProviderError::permanent("repeated_history_page"));
                }
                if run["effective_per_page"]
                    .as_u64()
                    .is_some_and(|size| size != data["per_page"].as_u64().unwrap_or(0))
                {
                    return Err(ProviderError::permanent("history_page_size_changed"));
                }
                let imported = calls.len();
                let rows = calls.clone();
                let job_id = job.id;
                let import_config = config.clone();
                run["state"] = json!("running");
                run["effective_per_page"] = data["per_page"].clone();
                run["imported"] = json!(run["imported"].as_u64().unwrap_or(0) + imported as u64);
                if empty {
                    run["window"] = json!(index + 1);
                    run["page"] = json!(1);
                    run["page_hashes"] = json!([]);
                    run["effective_per_page"] = Value::Null;
                } else {
                    let mut h = hashes;
                    h.push(json!(hash));
                    run["page_hashes"] = json!(h);
                    run["page"] = json!(page + 1);
                }
                let checkpoint = run.clone();
                self.guarded(job, move |conn| {
                    for source in rows {
                        import_call(
                            conn,
                            org,
                            connection,
                            job_id,
                            &source,
                            false,
                            &import_config,
                        )?;
                    }
                    persistence::save_document(
                        conn,
                        org,
                        run_id,
                        "sync_run",
                        Some(connection),
                        checkpoint,
                        None,
                    )?;
                    Ok(())
                })
                .await?;
                if empty {
                    break;
                }
                page += 1;
            }
        }
        run["state"] = json!("completed");
        run["coverage_complete"] = json!(complete_history);
        run["completed_at"] = json!(Utc::now());
        run["last_error"] = Value::Null;
        self.final_write(job, move |conn| {
            persistence::save_document(conn, org, run_id, "sync_run", Some(connection), run, None)?;
            if complete_history {
                for mut call in persistence::list_documents(conn, org, "call")? {
                    if call.data["source_connection_id"] == connection.to_string()
                        && call.data["deleted"] != true
                        && call.data["started_at"]
                            .as_str()
                            .and_then(|t| providers::parse_utc(t).ok())
                            .is_some_and(|t| t >= start && t <= end)
                    {
                        call.data["coverage_complete"] = json!(true);
                        persistence::save_document(
                            conn,
                            org,
                            call.id,
                            "call",
                            call.parent_id,
                            call.data,
                            Some(call.version),
                        )?;
                    }
                }
            }
            if let Some(mut doc) = persistence::get_document(conn, org, connection)? {
                if let Some(from) = coverage_from {
                    let current = &doc.data["history_coverage"];
                    let current_from = current["from"]
                        .as_str()
                        .and_then(|t| providers::parse_utc(t).ok());
                    let current_to = current["to"]
                        .as_str()
                        .and_then(|t| providers::parse_utc(t).ok());
                    let (from, to) = if current["verified"] == true
                        && current_from
                            .zip(current_to)
                            .is_some_and(|(a, b)| a <= end && b >= from)
                    {
                        (
                            from.min(current_from.unwrap()),
                            end.max(current_to.unwrap()),
                        )
                    } else {
                        (from, end)
                    };
                    doc.data["history_coverage"] = json!({"from":from,"to":to,"verified":true});
                }
                doc.data["last_sync_at"] = json!(end);
                doc.data["last_error"] = Value::Null;
                if doc.data["state"] == "error" {
                    doc.data["state"] = json!("configured");
                }
                persistence::save_document(
                    conn,
                    org,
                    connection,
                    "connection",
                    None,
                    doc.data,
                    Some(doc.version),
                )?;
            }
            recompute_contact_kinds(conn, org)?;
            persistence::append_audit(
                conn,
                org,
                None,
                "sync.completed",
                Some(connection),
                json!({"coverage_complete":complete_history}),
            )?;
            Ok(())
        })
        .await
    }
    async fn recording(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let id = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        let recording = self.document(org, id, "recording").await?;
        let call_id = recording
            .parent_id
            .ok_or_else(|| ProviderError::permanent("invalid_recording_reference"))?;
        let _ = self.document(org, call_id, "call").await?;
        if recording.data["ready"] == true {
            return Ok(());
        }
        let (_, config) = self.connection_config(org, "telphin").await?;
        let mut telphin = Telphin::connect(&config).await?;
        let bytes = telphin
            .recording(providers::required(&recording.data, "record_uuid")?)
            .await;
        let (bytes, mime) = match bytes {
            Ok(v) => v,
            Err(e) if e.code == "provider_resource_missing" => {
                return Err(ProviderError::retry("recording_not_ready", 120))
            }
            Err(e) => return Err(e),
        };
        let extension = audio_extension(&bytes, &mime)
            .ok_or_else(|| ProviderError::permanent("recording_format_unknown"))?;
        let relative = format!("{org}/{id}-{}-{}.{extension}", job.id, job.fencing);
        let path = safe_media_path(&self.config.media_root, org, &relative)?;
        let mut artifact = MediaArtifact {
            path: path.clone(),
            committed: false,
        };
        write_private(&path, &bytes).await?;
        let checksum = hex::encode(Sha256::digest(&bytes));
        let duration = probe_audio_duration(&path)
            .await
            .ok_or_else(|| ProviderError::permanent("recording_duration_unavailable"))?;
        let mut data = recording.data;
        data["path"] = json!(relative);
        data["mime"] = json!(if mime == "application/octet-stream" {
            match extension {
                "wav" => "audio/wav",
                "mp3" => "audio/mpeg",
                "ogg" => "audio/ogg",
                _ => "audio/mp4",
            }
        } else {
            &mime
        });
        data["size"] = json!(bytes.len());
        data["sha256"] = json!(checksum);
        data["state"] = json!("ready");
        data["ready"] = json!(true);
        data["duration"] = json!(duration);
        self.final_write(job, move |conn| {
            let call = persistence::get_document(conn, org, call_id)?
                .filter(|d| d.data["deleted"] != true)
                .ok_or_else(|| ProviderError::permanent("subject_deleted"))?;
            persistence::save_document(conn, org, id, "recording", Some(call_id), data, None)?;
            persistence::enqueue(
                conn,
                org,
                "transcribe",
                Some(call.id),
                &format!("recording:{id}:{checksum}"),
                json!({"recording_id":id}),
            )?;
            Ok(())
        })
        .await?;
        artifact.committed = true;
        Ok(())
    }
    async fn transcribe(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let call_id = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        let transcript_id = Uuid::new_v5(&job.id, b"transcript");
        if self
            .db(move |conn| Ok(persistence::get_document(conn, org, transcript_id)?.is_some()))
            .await?
        {
            return Ok(());
        }
        let call = self.document(org, call_id, "call").await?;
        let recordings = self
            .db(move |conn| {
                Ok(persistence::list_children(
                    conn,
                    org,
                    call_id,
                    Some("recording"),
                )?)
            })
            .await?;
        let recording = recordings
            .iter()
            .find(|r| r.data["ready"] == true || r.data["state"] == "ready")
            .cloned();
        if recording.is_none() && call.data["has_recording"] != true {
            return self
                .guarded(job, move |conn| {
                    let mut doc = persistence::get_document(conn, org, call_id)?
                        .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
                    doc.data["stage"] = json!("skipped");
                    doc.data["processing_reason"] = json!("no_recording");
                    persistence::save_document(
                        conn,
                        org,
                        call_id,
                        "call",
                        doc.parent_id,
                        doc.data,
                        Some(doc.version),
                    )?;
                    Ok(())
                })
                .await;
        }
        let recording = recording.ok_or_else(|| ProviderError::retry("recording_pending", 120))?;
        let (_, config) = self.connection_config(org, "asr").await?;
        let path = safe_media_path(
            &self.config.media_root,
            org,
            providers::required(&recording.data, "path")?,
        )?;
        let duration = recording.data["duration"]
            .as_f64()
            .unwrap_or_else(|| call.data["duration"].as_f64().unwrap_or(0.));
        self.set_stage(job, "transcribing").await?;
        let mut transcript =
            models::transcribe(&config, &path, duration, self.config.demo_mode, &call.data).await?;
        if config["roles_verified"] == true {
            if let Some(segments) = transcript["segments"].as_array_mut() {
                for segment in segments {
                    if let Some(role) = segment["speaker"]
                        .as_str()
                        .and_then(|speaker| config["speaker_roles"][speaker].as_str())
                        .filter(|s| matches!(*s, "manager" | "client"))
                    {
                        segment["role"] = json!(role);
                    }
                }
            }
            if transcript["segments"].as_array().is_some_and(|rows| {
                rows.iter()
                    .all(|r| matches!(r["role"].as_str(), Some("manager" | "client")))
            }) {
                transcript["role_confidence"] = json!("known");
            }
        }
        let versions = self
            .db(move |conn| {
                Ok(persistence::list_children(conn, org, call_id, Some("transcript"))?.len())
            })
            .await?;
        transcript["version"] = json!(versions + 1);
        transcript["id"] = json!(transcript_id);
        transcript["created_at"] = json!(Utc::now());
        transcript["recording_id"] = json!(recording.id);
        transcript["sha256"] = recording.data["sha256"].clone();
        self.final_write(job, move |conn| {
            persistence::save_document(
                conn,
                org,
                transcript_id,
                "transcript",
                Some(call_id),
                transcript,
                None,
            )?;
            let mut call = persistence::get_document(conn, org, call_id)?
                .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
            call.data["stage"] = json!("analyzing");
            call.data["processing_reason"] = Value::Null;
            call.data["retry_pending"] = json!(false);
            persistence::save_document(
                conn,
                org,
                call_id,
                "call",
                call.parent_id,
                call.data,
                Some(call.version),
            )?;
            persistence::enqueue(
                conn,
                org,
                "analyze",
                Some(call_id),
                &format!("transcript:{transcript_id}"),
                json!({"transcript_id":transcript_id}),
            )?;
            persistence::append_audit(
                conn,
                org,
                None,
                "transcript.created",
                Some(call_id),
                json!({"transcript_id":transcript_id}),
            )?;
            Ok(())
        })
        .await
    }
    async fn set_stage(&self, job: &Job, stage: &str) -> Result<()> {
        let org = job.organization_id;
        let id = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        let stage = stage.to_string();
        self.guarded(job, move |conn| {
            if let Some(mut doc) = persistence::get_document(conn, org, id)? {
                if doc.kind == "call" {
                    doc.data["stage"] = json!(stage);
                    doc.data["processing_reason"] = Value::Null;
                    doc.data["retry_pending"] = json!(false);
                    persistence::save_document(
                        conn,
                        org,
                        id,
                        "call",
                        doc.parent_id,
                        doc.data,
                        Some(doc.version),
                    )?;
                }
            }
            Ok(())
        })
        .await
    }
    async fn analyze(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let call_id = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        let analysis_id = Uuid::new_v5(&job.id, b"analysis");
        if self
            .db(move |conn| Ok(persistence::get_document(conn, org, analysis_id)?.is_some()))
            .await?
        {
            return Ok(());
        }
        let call = self.document(org, call_id, "call").await?;
        let transcripts = self
            .db(move |conn| {
                Ok(persistence::list_children(
                    conn,
                    org,
                    call_id,
                    Some("transcript"),
                )?)
            })
            .await?;
        let transcript = if let Some(id) = job.payload["transcript_id"]
            .as_str()
            .and_then(|v| Uuid::parse_str(v).ok())
        {
            transcripts.iter().find(|d| d.id == id).cloned()
        } else {
            transcripts.into_iter().max_by_key(|d| d.created_at)
        }
        .ok_or_else(|| ProviderError::permanent("transcript_required"))?;
        let rubrics = self.documents(org, "rubric").await?;
        let rubric = rubrics
            .into_iter()
            .filter(|r| r.data["active"] == true)
            .max_by_key(|r| r.created_at)
            .ok_or_else(|| ProviderError::permanent("active_rubric_required"))?;
        let tag_definitions = analysis_taxonomy(self.documents(org, "tag").await?);
        let status_definitions = analysis_taxonomy(self.documents(org, "status").await?);
        let tags = tag_definitions
            .iter()
            .filter_map(|d| d["id"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let statuses = status_definitions
            .iter()
            .filter_map(|d| d["id"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let (_, config) = self.connection_config(org, "analysis").await?;
        self.reserve_budget(job, &config).await?;
        self.set_stage(job, "analyzing").await?;
        let history = self
            .documents(org, "call")
            .await?
            .into_iter()
            .filter(|c| c.data["phone"] == call.data["phone"] && c.data["deleted"] != true)
            .map(|d| persistence::document_json(&d))
            .collect::<Vec<_>>();
        let input = json!({"call":call.data,"transcript":transcript.data,"rubric":rubric.data,"allowed_statuses":statuses,"allowed_tags":tags,"status_definitions":status_definitions,"tag_definitions":tag_definitions});
        let mut result = models::analyze(&config, &input, self.config.demo_mode).await?;
        domain::validate_analysis(
            &mut result,
            &input["transcript"],
            &input["rubric"],
            &statuses,
            &tags,
        )
        .map_err(|_| ProviderError::permanent("invalid_analysis_evidence_or_schema"))?;
        result["id"] = json!(analysis_id);
        result["state"] = json!(if result["score"].is_null() {
            "partial"
        } else {
            "ready"
        });
        result["transcript_id"] = json!(transcript.id);
        result["provider"] = config["provider"].clone();
        result["model"] = config["model"].clone();
        result["prompt_version"] = json!("call-quality-v2-checklist");
        result["rubric_version"] = input["rubric"]
            .get("rubric_version")
            .cloned()
            .unwrap_or_else(|| input["rubric"]["version"].clone());
        result["synthetic"] = json!(config["provider"] == "demo");
        result["rubric_id"] = json!(rubric.id);
        result["created_at"] = json!(Utc::now());
        result["input_hash"] = json!(hex::encode(Sha256::digest(
            serde_json::to_vec(&input).unwrap_or_default()
        )));
        let transcript_id = transcript.id;
        let mut call_for_rule = call.data.clone();
        call_for_rule["id"] = json!(call_id);
        let classification = domain::contact_kind(&history, &call_for_rule);
        self.final_write(job, move |conn| {
            persistence::save_document(
                conn,
                org,
                analysis_id,
                "analysis",
                Some(call_id),
                result.clone(),
                None,
            )?;
            let latest = persistence::list_children(conn, org, call_id, Some("transcript"))?
                .into_iter()
                .max_by_key(|d| d.created_at);
            let mut current = persistence::get_document(conn, org, call_id)?
                .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
            // A result for an obsolete transcript remains in history and never becomes the active result.
            if latest.as_ref().map(|d| d.id) == Some(transcript_id) {
                current.data["stage"] = result["state"].clone();
                current.data["processing_reason"] = Value::Null;
                current.data["retry_pending"] = json!(false);
                current.data["score"] = result["score"].clone();
                if current.data["manual_lock"] != true {
                    current.data["status_id"] = result["suggested_status_id"].clone();
                    current.data["tag_ids"] = result["suggested_tag_ids"].clone();
                    current.data["contact_kind"] = json!(classification);
                }
                persistence::save_document(
                    conn,
                    org,
                    call_id,
                    "call",
                    current.parent_id,
                    current.data,
                    Some(current.version),
                )?;
            }
            persistence::append_audit(
                conn,
                org,
                None,
                "analysis.created",
                Some(call_id),
                json!({"analysis_id":analysis_id,"transcript_id":transcript_id}),
            )?;
            Ok(())
        })
        .await
    }
    async fn reserve_budget(&self, job: &Job, config: &Value) -> Result<()> {
        if config["provider"] == "demo" {
            return Ok(());
        }
        let daily = config["budget_daily"].as_f64().unwrap_or(10.);
        let estimate = config["estimated_cost_per_run"].as_f64().unwrap_or(0.25);
        if !daily.is_finite()
            || daily <= 0.
            || !estimate.is_finite()
            || estimate <= 0.
            || estimate > daily
        {
            return Err(ProviderError::permanent("invalid_analysis_budget"));
        }
        let org = job.organization_id;
        let job_id = job.id;
        let id = Uuid::new_v5(
            &job.id,
            format!("usage-reservation:{}", job.fencing).as_bytes(),
        );
        let today = Utc::now().format("%Y-%m-%d").to_string();
        self.guarded(job,move|conn|{
            if persistence::get_document(conn,org,id)?.is_some(){return Ok(());}
            let lock=i64::from_be_bytes(org.as_bytes()[0..8].try_into().unwrap());
            diesel::sql_query("SELECT pg_advisory_xact_lock($1)").bind::<diesel::sql_types::BigInt,_>(lock).execute(conn)?;
            let used=persistence::list_documents(conn,org,"usage")?.iter().filter(|d|d.data["date"]==today).map(|d|d.data["reserved_cost"].as_f64().unwrap_or(0.)).sum::<f64>();
            if used+estimate>daily{return Err(ProviderError::permanent("analysis_daily_budget_reached"));}
            persistence::save_document(conn,org,id,"usage",None,json!({"date":today,"reserved_cost":estimate,"job_id":job_id,"cost_is_estimate":true}),None)?;Ok(())
        }).await
    }
    async fn reprocess(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let call = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        let stage = job.payload["stage"].as_str().unwrap_or("all");
        let job_id = job.id;
        let kind = if stage == "analyze" {
            "analyze"
        } else {
            "transcribe"
        };
        let dedup = format!("manual:{}:{kind}", job.id);
        self.final_write(job, move |conn| {
            persistence::enqueue(
                conn,
                org,
                kind,
                Some(call),
                &dedup,
                json!({"requested_by_job":job_id}),
            )?;
            Ok(())
        })
        .await
    }
    async fn export(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let call_id = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        // One outbound delivery per call/provider at a time across worker processes.
        // A nonblocking session advisory lock avoids two concurrent missing-key reads/appends.
        let provider_for_lock = providers::required(&job.payload, "provider")?;
        let lock_id = Uuid::new_v5(
            &org,
            format!("export:{provider_for_lock}:{call_id}").as_bytes(),
        );
        let lock_key = i64::from_be_bytes(lock_id.as_bytes()[0..8].try_into().unwrap());
        let pool = self.pool.clone();
        let mut lock_conn = tokio::task::spawn_blocking(move || persistence::connection(&pool))
            .await
            .map_err(|_| ProviderError::retry("worker_database_task_failed", 30))??;
        #[derive(diesel::QueryableByName)]
        struct LockResult {
            #[diesel(sql_type=diesel::sql_types::Bool)]
            locked: bool,
        }
        let locked = diesel::sql_query("SELECT pg_try_advisory_lock($1) AS locked")
            .bind::<diesel::sql_types::BigInt, _>(lock_key)
            .get_result::<LockResult>(&mut lock_conn)?
            .locked;
        if !locked {
            return Err(ProviderError::retry("integration_delivery_busy", 5));
        }
        let _delivery_lock = TokenLock {
            connection: lock_conn,
            key: lock_key,
        };
        let call = self.document(org, call_id, "call").await?;
        let provider = providers::required(&job.payload, "provider")?;
        if !matches!(provider, "amocrm" | "sheets") {
            return Err(ProviderError::permanent("unsupported_integration"));
        }
        let config = self.integration_config(org, provider).await?;
        let delivery_id = job.payload["delivery_id"]
            .as_str()
            .and_then(|s| Uuid::parse_str(s).ok())
            .unwrap_or_else(|| Uuid::new_v5(&job.id, b"delivery"));
        let existing = self
            .db(move |conn| Ok(persistence::get_document(conn, org, delivery_id)?))
            .await?;
        let pinned_delivery = existing.as_ref().map(|d| d.data.clone());
        let analysis = self
            .db(move |conn| select_export_analysis(conn, org, call_id, pinned_delivery.as_ref()))
            .await?;
        let export_call = export_call_data(&call.data, analysis.as_ref().map(|a| &a.data));
        if existing
            .as_ref()
            .is_some_and(|d| d.data["state"] == "delivered")
        {
            return Ok(());
        }
        let uncertain = existing
            .as_ref()
            .is_some_and(|d| delivery_requires_reconciliation(&d.data));
        let marker = existing
            .as_ref()
            .and_then(|d| d.data["marker"].as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                integrations::delivery_marker(org, call_id, analysis.as_ref().map(|a| a.id))
            });
        enum DeliveryRequest {
            Amo(String, String),
            Sheets(Vec<Value>),
        }
        // Validate local prerequisites before recording that an external write may start.
        let request = if provider == "amocrm" {
            let links = self.documents(org, "external_link").await?;
            let link = links
                .iter()
                .find(|link| link.parent_id == Some(call_id) && link.data["provider"] == "amocrm")
                .ok_or_else(|| ProviderError::permanent("deal_link_required"))?;
            let summary = analysis
                .as_ref()
                .and_then(|a| a.data["summary"].as_str())
                .unwrap_or("Анализ ещё не выполнен");
            let text = format!(
                "Звонок {}\nОценка: {}\n{}\n{}/calls/{}",
                call.data["started_at"].as_str().unwrap_or(""),
                export_call["score"],
                summary,
                self.config.public_url.trim_end_matches('/'),
                call_id
            );
            DeliveryRequest::Amo(providers::required(&link.data, "deal_id")?.to_owned(), text)
        } else {
            let mut call_data = export_call;
            call_data["id"] = json!(call_id);
            let columns = integrations::sheets_columns(
                &config,
                &call_data,
                analysis.as_ref().map(|a| &a.data),
                &self.config.public_url,
                &marker,
            )?;
            DeliveryRequest::Sheets(columns)
        };
        let provider_owned = provider.to_string();
        let delivery = json!({"call_id":call_id,"provider":provider,"state":"attempting","requires_reconciliation":uncertain,"marker":marker,"attempted_at":Utc::now(),"job_id":job.id,"analysis_id":analysis.as_ref().map(|a|a.id)});
        self.guarded(job, move |conn| {
            persistence::save_document(
                conn,
                org,
                delivery_id,
                "delivery",
                Some(call_id),
                delivery,
                None,
            )?;
            Ok(())
        })
        .await?;
        let result = match request {
            DeliveryRequest::Amo(deal_id, text) => {
                integrations::deliver_amocrm(&config, &deal_id, &marker, &text, uncertain).await
            }
            DeliveryRequest::Sheets(columns) => {
                integrations::deliver_sheets(
                    &config,
                    &format!("{org}:{call_id}"),
                    columns,
                    uncertain,
                )
                .await
            }
        };
        let mut data = existing.map(|d| d.data).unwrap_or_else(
            || json!({"call_id":call_id,"provider":provider_owned,"job_id":job.id,"marker":marker,"analysis_id":analysis.as_ref().map(|a|a.id)}),
        );
        match result {
            Ok(result) => {
                for (k, v) in result.as_object().unwrap() {
                    data[k] = v.clone();
                }
                data["delivered_at"] = json!(Utc::now());
                data["requires_reconciliation"] = json!(false);
                self.final_write(job, move |conn| {
                    persistence::save_document(
                        conn,
                        org,
                        delivery_id,
                        "delivery",
                        Some(call_id),
                        data,
                        None,
                    )?;
                    persistence::append_audit(
                        conn,
                        org,
                        None,
                        "integration.delivered",
                        Some(call_id),
                        json!({"delivery_id":delivery_id}),
                    )?;
                    Ok(())
                })
                .await
            }
            Err(error) => {
                if uncertain {
                    data["requires_reconciliation"] = json!(true);
                }
                preserve_delivery_failure(&mut data, &error.code);
                data["error_code"] = json!(error.code);
                self.guarded(job, move |conn| {
                    persistence::save_document(
                        conn,
                        org,
                        delivery_id,
                        "delivery",
                        Some(call_id),
                        data,
                        None,
                    )?;
                    Ok(())
                })
                .await?;
                Err(error)
            }
        }
    }
    async fn purge(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let call_id = job
            .subject_id
            .ok_or_else(|| ProviderError::permanent("subject_missing"))?;
        let children = self
            .db(move |conn| Ok(persistence::list_children(conn, org, call_id, None)?))
            .await?;
        let shared_paths = self
            .documents(org, "recording")
            .await?
            .into_iter()
            .filter(|d| d.parent_id != Some(call_id))
            .filter_map(|d| d.data["path"].as_str().map(str::to_owned))
            .collect::<std::collections::HashSet<_>>();
        for doc in children.iter().filter(|d| d.kind == "recording") {
            if let Some(relative) = doc.data["path"].as_str() {
                // Synthetic seed and legacy imports can share a physical file. Removing one
                // call must not remove another tenant-local recording's referenced artifact.
                if shared_paths.contains(relative) {
                    continue;
                }
                let path = safe_media_path(&self.config.media_root, org, relative)?;
                match tokio::fs::remove_file(path).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => return Err(ProviderError::retry("media_purge_failed", 60)),
                }
            }
        }
        self.final_write(job,move|conn|{
            let call=persistence::get_document(conn,org,call_id)?;
            let provider=call.as_ref().and_then(|d|d.data["provider_id"].as_str());persistence::tombstone(conn,org,call_id,provider)?;
            // Preserve the redacted call tombstone so the job's subject FK remains valid.
            // Child data/recordings/transcripts/analyses/raw snapshots are actually removed.
            for child in persistence::list_children(conn,org,call_id,None)?{persistence::delete_document(conn,org,child.id)?;}
            if call.is_some(){
                diesel::sql_query("UPDATE documents SET data=$3,version=version+1,updated_at=now() WHERE organization_id=$1 AND id=$2 AND kind='call'")
                    .bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(call_id).bind::<diesel::sql_types::Jsonb,_>(json!({"deleted":true,"purged":true,"purged_at":Utc::now()})).execute(conn)?;
            }
            persistence::append_audit(conn,org,None,"call.purged",Some(call_id),json!({"artifacts_deleted":true}))?;Ok(())
        }).await
    }
    async fn retention(&self, job: &Job) -> Result<()> {
        let org = job.organization_id;
        let job_id = job.id;
        let days = job.payload["retention_days"]
            .as_i64()
            .unwrap_or(90)
            .clamp(1, 3650);
        let before = Utc::now() - chrono::Duration::days(days);
        let references = self
            .documents(org, "recording")
            .await?
            .into_iter()
            .filter_map(|d| d.data["path"].as_str().map(str::to_owned))
            .collect::<std::collections::HashSet<_>>();
        let media_root = self.config.media_root.clone();
        tokio::task::spawn_blocking(move || {
            cleanup_orphan_media(&media_root, org, &references, std::time::SystemTime::now())
        })
        .await
        .map_err(|_| ProviderError::retry("media_cleanup_failed", 60))??;
        self.final_write(job, move |conn| {
            let calls = persistence::list_documents(conn, org, "call")?;
            for mut call in calls {
                if call.data["deleted"] == true {
                    continue;
                }
                if call.data["started_at"]
                    .as_str()
                    .and_then(|s| providers::parse_utc(s).ok())
                    .is_some_and(|t| t < before)
                {
                    call.data["deleted"] = json!(true);
                    persistence::save_document(
                        conn,
                        org,
                        call.id,
                        "call",
                        call.parent_id,
                        call.data.clone(),
                        Some(call.version),
                    )?;
                    persistence::enqueue(
                        conn,
                        org,
                        "purge",
                        Some(call.id),
                        &format!("retention:{}:{}", call.id, job_id),
                        json!({}),
                    )?;
                }
            }
            Ok(())
        })
        .await
    }
    pub async fn schedule(&self) -> Result<()> {
        #[derive(diesel::QueryableByName)]
        struct Org {
            #[diesel(sql_type=SqlUuid)]
            id: Uuid,
            #[diesel(sql_type=Integer)]
            retention_days: i32,
        }
        self.db(move|conn|{
            let orgs=diesel::sql_query("SELECT id,retention_days FROM organizations").load::<Org>(conn)?;
            for org in orgs{
                let bucket=Utc::now().timestamp()/300;
                for connection in persistence::list_documents(conn,org.id,"connection")?{
                    if connection.data["kind"]=="telphin"&&connection.data["state"]=="configured"{
                        persistence::enqueue(conn,org.id,"sync",Some(connection.id),&format!("scheduled:{}:{bucket}",connection.id),json!({"from":Utc::now()-chrono::Duration::days(2),"to":Utc::now(),"all_available":false}))?;
                    }
                }
                persistence::enqueue(conn,org.id,"retention",None,&format!("retention:{}",Utc::now().format("%Y-%m-%d")),json!({"retention_days":org.retention_days}))?;
            }Ok(())
        }).await
    }
    pub async fn write_health(&self, state: &str, error: Option<&str>) -> Result<()> {
        let value = json!({"timestamp":Utc::now(),"worker_id":self.worker_id,"state":state,"error_code":error});
        write_private(
            &self.config.media_root.join(".worker-health.json"),
            &serde_json::to_vec(&value)
                .map_err(|_| ProviderError::permanent("health_serialization_failed"))?,
        )
        .await
    }
    pub async fn relay_outbox(&self) -> Result<()> {
        let Some(url) = &self.config.rabbitmq_url else {
            return Ok(());
        };
        let events = self
            .db(move |conn| Ok(persistence::pending_outbox(conn, 100)?))
            .await?;
        if events.is_empty() {
            return Ok(());
        }
        use lapin::{options::*, types::FieldTable, BasicProperties};
        let connection = lapin::Connection::connect(url, lapin::ConnectionProperties::default())
            .await
            .map_err(|_| ProviderError::retry("broker_unavailable", 30))?;
        let channel = connection
            .create_channel()
            .await
            .map_err(|_| ProviderError::retry("broker_unavailable", 30))?;
        channel
            .confirm_select(ConfirmSelectOptions::default())
            .await
            .map_err(|_| ProviderError::retry("broker_unavailable", 30))?;
        channel
            .queue_declare(
                "phone.jobs",
                QueueDeclareOptions {
                    durable: true,
                    ..Default::default()
                },
                FieldTable::default(),
            )
            .await
            .map_err(|_| ProviderError::retry("broker_unavailable", 30))?;
        for event in events {
            let payload = serde_json::to_vec(
                &json!({"job_id":event.job_id,"organization_id":event.organization_id}),
            )
            .map_err(|_| ProviderError::permanent("invalid_outbox_event"))?;
            let confirmation = channel
                .basic_publish(
                    "",
                    "phone.jobs",
                    BasicPublishOptions::default(),
                    &payload,
                    BasicProperties::default().with_delivery_mode(2),
                )
                .await
                .map_err(|_| ProviderError::retry("broker_unavailable", 30))?
                .await
                .map_err(|_| ProviderError::retry("broker_unavailable", 30))?;
            if !confirmation.is_ack() {
                return Err(ProviderError::retry("broker_publish_unconfirmed", 30));
            }
            let id = event.id;
            self.db(move |conn| {
                persistence::mark_outbox_published(conn, id)?;
                Ok(())
            })
            .await?;
        }
        for _ in 0..100 {
            let notification = channel
                .basic_get("phone.jobs", BasicGetOptions::default())
                .await
                .map_err(|_| ProviderError::retry("broker_unavailable", 30))?;
            let Some(notification) = notification else {
                break;
            };
            self.run_once().await?;
            notification
                .delivery
                .ack(BasicAckOptions::default())
                .await
                .map_err(|_| ProviderError::retry("broker_unavailable", 30))?;
        }
        let _ = connection.close(200, "done").await;
        Ok(())
    }
}

/// Coverage is extended only when a completed import overlaps the previously verified interval.
/// A polling gap never becomes evidence of a complete history.
fn verified_coverage_goal(
    connection: &Value,
    config: &Value,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    all: bool,
) -> Option<DateTime<Utc>> {
    if all
        && config["history_start_verified"] == true
        && config["history_start"]
            .as_str()
            .map(str::trim)
            .and_then(|t| parse_input_date(t).ok())
            == Some(start)
    {
        return Some(start);
    }
    let coverage = &connection["history_coverage"];
    let from = coverage["from"]
        .as_str()
        .and_then(|t| providers::parse_utc(t).ok())?;
    let to = coverage["to"]
        .as_str()
        .and_then(|t| providers::parse_utc(t).ok())?;
    (coverage["verified"] == true && from <= start && start <= to && to <= end).then_some(from)
}
/// Resolve optional requested bounds while preserving persisted checkpoint bounds.
fn sync_range(
    config: &Value,
    payload: &Value,
    run: &Value,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let end = if let Some(v) = run["to"].as_str() {
        providers::parse_utc(v)?
    } else {
        payload["to"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(parse_input_date)
            .transpose()?
            .unwrap_or_else(|| DateTime::from_timestamp(now.timestamp(), 0).unwrap())
    };
    let all = payload["all_available"].as_bool().unwrap_or(false);
    let start = if let Some(v) = run["from"].as_str() {
        providers::parse_utc(v)?
    } else if all {
        config["history_start"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(parse_input_date)
            .transpose()?
            .unwrap_or(end - chrono::Duration::days(364))
    } else {
        payload["from"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(parse_input_date)
            .transpose()?
            .unwrap_or(end - chrono::Duration::days(30))
    };
    if start >= end || end - start > chrono::Duration::days(3650) {
        return Err(ProviderError::permanent("invalid_sync_period"));
    }
    Ok((start, end))
}
fn parse_input_date(value: &str) -> Result<DateTime<Utc>> {
    use chrono::Timelike;
    let t = if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        date.and_hms_opt(0, 0, 0).unwrap().and_utc()
    } else {
        providers::parse_utc(value)?
    };
    Ok(t.with_nanosecond(0).unwrap())
}
pub fn safe_media_path(root: &Path, org: Uuid, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    let mut components = path.components();
    if components.next().and_then(|p| p.as_os_str().to_str()) != Some(&org.to_string())
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(ProviderError::permanent("invalid_media_path"));
    }
    let mut joined = root.to_owned();
    if root
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err(ProviderError::permanent("media_symlink_blocked"));
    }
    for component in path.components() {
        joined.push(component);
        if joined
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(ProviderError::permanent("media_symlink_blocked"));
        }
    }
    Ok(joined)
}
async fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ProviderError::permanent("invalid_media_path"))?;
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|_| ProviderError::retry("media_storage_unavailable", 30))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tokio::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
            .await
            .map_err(|_| ProviderError::permanent("media_permissions_error"))?;
    }
    let tmp = parent.join(format!(".pending-{}", Uuid::new_v4()));
    let _cleanup = PendingFile(tmp.clone());
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&tmp)
        .await
        .map_err(|_| ProviderError::retry("media_storage_unavailable", 30))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|_| ProviderError::permanent("media_permissions_error"))?;
    }
    file.write_all(bytes)
        .await
        .map_err(|_| ProviderError::retry("media_storage_unavailable", 30))?;
    file.sync_all()
        .await
        .map_err(|_| ProviderError::retry("media_storage_unavailable", 30))?;
    drop(file);
    tokio::fs::rename(&tmp, path)
        .await
        .map_err(|_| ProviderError::retry("media_storage_unavailable", 30))?;
    let dir = std::fs::File::open(parent)
        .map_err(|_| ProviderError::retry("media_storage_unavailable", 30))?;
    dir.sync_all()
        .map_err(|_| ProviderError::retry("media_storage_unavailable", 30))?;
    Ok(())
}
fn audio_extension(bytes: &[u8], _mime: &str) -> Option<&'static str> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE") {
        Some("wav")
    } else if bytes.starts_with(b"ID3")
        || (bytes.first() == Some(&0xff) && bytes.get(1).is_some_and(|b| b & 0xe0 == 0xe0))
    {
        Some("mp3")
    } else if bytes.starts_with(b"OggS") {
        Some("ogg")
    } else if bytes.get(4..8) == Some(b"ftyp") {
        Some("m4a")
    } else if bytes.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        Some("webm")
    } else {
        None
    }
}
async fn probe_audio_duration(path: &Path) -> Option<f64> {
    let mut command = tokio::process::Command::new("ffprobe");
    command
        .env_clear()
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
        )
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .ok()?
        .ok()?;
    if output.stdout.len() > 128 {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.)
}
/// Only explicit payload numbers or operator-configured ID→number mappings are trusted.
fn extension_candidates(source: &Value, config: &Value) -> Vec<(String, Option<String>)> {
    fn scalar(value: &Value) -> Option<String> {
        value
            .as_str()
            .map(str::to_owned)
            .or_else(|| value.as_u64().map(|v| v.to_string()))
            .filter(|s| !s.is_empty() && s.len() <= 32 && s.bytes().all(|b| b.is_ascii_digit()))
    }
    let legs = source["cdr"].as_array().cloned().unwrap_or_default();
    let answered = legs
        .iter()
        .filter(|l| matches!(l["result"].as_str(), Some("answered" | "bridged")))
        .collect::<Vec<_>>();
    let selected = if answered.is_empty() {
        legs.iter().collect::<Vec<_>>()
    } else {
        answered
    };
    let selected = if selected.is_empty() {
        vec![source]
    } else {
        selected
    };
    let mut values = std::collections::BTreeMap::new();
    for leg in selected {
        let id = scalar(&leg["extension_id"]);
        let explicit = scalar(&leg["extension_number"]);
        let number = explicit.or_else(|| {
            id.as_ref()
                .and_then(|id| scalar(&config["extension_mapping"][id]))
        });
        if let Some(id) = id.or_else(|| number.clone()) {
            values
                .entry(id)
                .and_modify(|existing: &mut Option<String>| {
                    if existing.is_none() {
                        *existing = number.clone();
                    }
                })
                .or_insert(number);
        }
    }
    values.into_iter().collect()
}
fn import_call(
    conn: &mut persistence::PgConnection,
    org: Uuid,
    connection: Uuid,
    job: Uuid,
    source: &Value,
    coverage: bool,
    config: &Value,
) -> Result<()> {
    let provider = providers::required(source, "call_uuid")?;
    let id = Uuid::new_v5(&org, format!("telphin:{connection}:{provider}").as_bytes());
    if persistence::is_tombstoned(conn, org, id, Some(provider))? {
        return Ok(());
    }
    // Serialize imports of the same previously unseen call; annotations remain protected
    // by the optimistic row version below once the call exists.
    let lock = i64::from_be_bytes(id.as_bytes()[0..8].try_into().unwrap());
    diesel::sql_query("SELECT pg_advisory_xact_lock($1)")
        .bind::<diesel::sql_types::BigInt, _>(lock)
        .execute(conn)?;
    let existing = persistence::get_document(conn, org, id)?;
    if existing.as_ref().is_some_and(|d| d.data["deleted"] == true) {
        return Ok(());
    }
    let legs = source["cdr"].as_array().cloned().unwrap_or_default();
    let internal = source["from_username"]
        .as_str()
        .is_some_and(|s| s.contains('*'))
        && source["to_username"]
            .as_str()
            .is_some_and(|s| s.contains('*'));
    let direction = if internal {
        "internal"
    } else {
        source["flow"]
            .as_str()
            .filter(|v| matches!(*v, "in" | "out"))
            .unwrap_or("internal")
    };
    let phone_raw = if direction == "in" {
        source["from_username"].as_str()
    } else {
        source["to_username"].as_str()
    }
    .unwrap_or("");
    let mut data=existing.as_ref().map(|d|d.data.clone()).unwrap_or_else(||json!({"status_id":null,"tag_ids":[],"note":"","manual_lock":false,"contact_kind":"unknown","score":null,"stage":"imported"}));
    let raw_result = source["result"].as_str().unwrap_or("unknown");
    let result = if [
        "busy",
        "answered",
        "bridged",
        "not answered",
        "rejected",
        "failed",
    ]
    .contains(&raw_result)
    {
        raw_result
    } else {
        "unknown"
    };
    data["provider_id"] = json!(provider);
    data["source_connection_id"] = json!(connection);
    data["started_at"] = json!(providers::parse_utc(providers::required(
        source,
        "init_time_gmt"
    )?)?);
    data["phone"] = json!(normalize_phone(phone_raw));
    data["contact_name"] = json!(if direction == "in" {
        source["from_screen_name"].as_str().unwrap_or("")
    } else {
        ""
    });
    let extensions = extension_candidates(source, config);
    data["employee_extensions"] = json!(extensions
        .iter()
        .map(|(id, number)| json!({"extension_id":id,"extension_number":number}))
        .collect::<Vec<_>>());
    data["employee_extension_id"] = if extensions.len() == 1 {
        json!(extensions[0].0)
    } else {
        Value::Null
    };
    data["employee_extension_number"] = if extensions.len() == 1 {
        json!(extensions[0].1)
    } else {
        Value::Null
    };
    let numbers = extensions
        .iter()
        .filter_map(|(_, number)| number.clone())
        .collect::<Vec<_>>();
    #[derive(diesel::QueryableByName)]
    struct Employee {
        #[diesel(sql_type=SqlUuid)]
        id: Uuid,
        #[diesel(sql_type=diesel::sql_types::Text)]
        name: String,
    }
    let employees = if numbers.is_empty() {
        vec![]
    } else {
        diesel::sql_query("SELECT DISTINCT u.id,u.name FROM memberships m JOIN users u ON u.id=m.user_id WHERE m.organization_id=$1 AND m.extension=ANY($2)").bind::<SqlUuid,_>(org).bind::<diesel::sql_types::Array<diesel::sql_types::Text>,_>(&numbers).load::<Employee>(conn)?
    };
    data["employee_id"] = if employees.len() == 1 && extensions.len() == 1 {
        json!(employees[0].id)
    } else {
        Value::Null
    };
    data["employee"] = json!(if employees.len() == 1 && extensions.len() == 1 {
        employees[0].name.clone()
    } else if employees.len() > 1 || extensions.len() > 1 {
        "Несколько добавочных: требуется сопоставление".to_string()
    } else if let Some(number) = numbers.first() {
        format!("Добавочный {number} (не сопоставлен)")
    } else if let Some((id, _)) = extensions.first() {
        format!("Добавочный ID {id} (номер неизвестен)")
    } else {
        source["extension_name"]
            .as_str()
            .unwrap_or("Сотрудник не определён")
            .to_string()
    });
    data["direction"] = json!(direction);
    data["provider_result"] = json!(result);
    data["human_connected"] = json!(result == "bridged");
    data["duration"] = json!(if result == "bridged" {
        source["bridged_duration"].as_i64().unwrap_or(0).max(0)
    } else {
        source["duration"].as_i64().unwrap_or(0).max(0)
    });
    data["coverage_complete"] = json!(coverage || data["coverage_complete"] == true);
    data["has_recording"] = json!(legs.iter().any(|l| l["record_uuid"].is_string()));
    persistence::save_document(
        conn,
        org,
        id,
        "call",
        None,
        data,
        existing.as_ref().map(|d| d.version),
    )?;
    for old in persistence::list_children(conn, org, id, Some("leg"))? {
        persistence::delete_document(conn, org, old.id)?;
    }
    for (index, leg) in legs.iter().enumerate() {
        persistence::save_document(
            conn,
            org,
            Uuid::new_v5(&id, format!("leg:{index}").as_bytes()),
            "leg",
            Some(id),
            leg.clone(),
            None,
        )?;
        if let Some(record) = leg["record_uuid"].as_str() {
            let recording_id = Uuid::new_v5(&id, format!("recording:{record}").as_bytes());
            if persistence::get_document(conn, org, recording_id)?.is_none() {
                persistence::save_document(
                    conn,
                    org,
                    recording_id,
                    "recording",
                    Some(id),
                    json!({"call_id":id,"record_uuid":record,"state":"pending","ready":false,"size":leg["record_file_size"]}),
                    None,
                )?;
                persistence::enqueue(
                    conn,
                    org,
                    "recording",
                    Some(recording_id),
                    &format!("recording:{recording_id}"),
                    json!({}),
                )?;
            }
        }
    }
    let hash = hex::encode(Sha256::digest(
        serde_json::to_vec(source).unwrap_or_default(),
    ));
    persistence::save_document(
        conn,
        org,
        Uuid::new_v5(&job, format!("source:{provider}:{hash}").as_bytes()),
        "source_snapshot",
        Some(id),
        json!({"provider":"telphin","payload":source,"observed_at":Utc::now()}),
        None,
    )?;
    Ok(())
}
pub fn normalize_phone(raw: &str) -> String {
    if raw
        .chars()
        .any(|c| c.is_ascii_alphabetic() || c == '*' || c == '@')
    {
        return String::new();
    }
    let digits = raw.chars().filter(char::is_ascii_digit).collect::<String>();
    if digits.len() == 11 && digits.starts_with('8') {
        format!("+7{}", &digits[1..])
    } else if (10..=15).contains(&digits.len()) {
        format!("+{digits}")
    } else {
        String::new()
    }
}

struct TokenLock {
    connection: persistence::Connection,
    key: i64,
}
impl Drop for TokenLock {
    fn drop(&mut self) {
        let _ = diesel::sql_query("SELECT pg_advisory_unlock($1)")
            .bind::<diesel::sql_types::BigInt, _>(self.key)
            .execute(&mut self.connection);
    }
}
pub fn healthcheck() -> bool {
    let root = std::env::var("MEDIA_ROOT").unwrap_or_else(|_| "storage/media".into());
    let path = Path::new(&root).join(".worker-health.json");
    if path
        .symlink_metadata()
        .map(|m| m.file_type().is_symlink() || m.len() > 4096)
        .unwrap_or(true)
    {
        return false;
    }
    std::fs::read(path)
        .ok()
        .and_then(|v| serde_json::from_slice::<Value>(&v).ok())
        .and_then(|v| {
            v["timestamp"]
                .as_str()
                .and_then(|t| providers::parse_utc(t).ok())
        })
        .is_some_and(|t| (Utc::now() - t).num_seconds() <= 90)
}

fn recompute_contact_kinds(conn: &mut persistence::PgConnection, org: Uuid) -> Result<()> {
    let calls = persistence::list_documents(conn, org, "call")?
        .into_iter()
        .filter(|c| c.data["deleted"] != true)
        .collect::<Vec<_>>();
    let mut earliest = std::collections::HashMap::<String, DateTime<Utc>>::new();
    for call in &calls {
        if let (Some(phone), Some(time)) = (
            call.data["phone"].as_str().filter(|v| !v.is_empty()),
            call.data["started_at"]
                .as_str()
                .and_then(|v| providers::parse_utc(v).ok()),
        ) {
            earliest
                .entry(phone.to_string())
                .and_modify(|t| *t = (*t).min(time))
                .or_insert(time);
        }
    }
    for mut call in calls {
        if call.data["manual_lock"] == true {
            continue;
        }
        let phone = call.data["phone"].as_str().unwrap_or("");
        let time = call.data["started_at"]
            .as_str()
            .and_then(|v| providers::parse_utc(v).ok());
        let kind = if phone.is_empty() || time.is_none() {
            "unknown"
        } else if earliest.get(phone).is_some_and(|t| Some(*t) < time) {
            "repeat"
        } else if call.data["coverage_complete"] == true {
            "primary"
        } else {
            "unknown"
        };
        if call.data["contact_kind"] != kind {
            call.data["contact_kind"] = json!(kind);
            persistence::save_document(
                conn,
                org,
                call.id,
                "call",
                call.parent_id,
                call.data,
                Some(call.version),
            )?;
        }
    }
    Ok(())
}

struct PendingFile(PathBuf);
impl Drop for PendingFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Crashes between filesystem fsync and DB commit can leave an unreferenced final artifact.
/// Only old flat files in this tenant directory are removed; recent in-flight writes are preserved.
fn cleanup_orphan_media(
    root: &Path,
    org: Uuid,
    references: &std::collections::HashSet<String>,
    now: std::time::SystemTime,
) -> Result<usize> {
    let probe = safe_media_path(root, org, &format!("{org}/.orphan-probe"))?;
    let directory = probe.parent().unwrap();
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err(ProviderError::retry("media_cleanup_failed", 60)),
    };
    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|_| ProviderError::retry("media_cleanup_failed", 60))?;
        let metadata = entry
            .path()
            .symlink_metadata()
            .map_err(|_| ProviderError::retry("media_cleanup_failed", 60))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            continue;
        }
        let relative = format!("{org}/{}", entry.file_name().to_string_lossy());
        if !references.contains(&relative)
            && metadata
                .modified()
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|age| age > Duration::from_secs(86400))
        {
            std::fs::remove_file(entry.path())
                .map_err(|_| ProviderError::retry("media_cleanup_failed", 60))?;
            count += 1;
        }
    }
    Ok(count)
}
struct MediaArtifact {
    path: PathBuf,
    committed: bool,
}
impl Drop for MediaArtifact {
    fn drop(&mut self) {
        if !self.committed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
#[cfg(test)]
mod media_tests {
    use super::*;
    #[test]
    fn sync_range_blank_history_start_uses_unverified_default() {
        let now = providers::parse_utc("2026-11-12T12:34:56.789Z").unwrap();
        let end = parse_input_date("2026-11-12T12:34:56Z").unwrap();
        for value in [Value::Null, json!(""), json!(" \t\n")] {
            let config = json!({"history_start":value,"history_start_verified":true});
            let (start, actual_end) =
                sync_range(&config, &json!({"all_available":true}), &json!({}), now).unwrap();
            assert_eq!(
                (start, actual_end),
                (end - chrono::Duration::days(364), end)
            );
            assert_eq!(
                verified_coverage_goal(&json!({}), &config, start, end, true),
                None
            );
        }
    }
    #[test]
    fn sync_range_blank_optional_bounds_use_default_period() {
        let now = parse_input_date("2026-11-12T12:34:56Z").unwrap();
        for value in [Value::Null, json!(""), json!(" \t\n")] {
            let payload = json!({"from":value,"to":value});
            assert_eq!(
                sync_range(&json!({}), &payload, &json!({}), now).unwrap(),
                (now - chrono::Duration::days(30), now)
            );
        }
    }
    #[test]
    fn sync_range_preserves_explicit_dates_and_checkpoint_precedence() {
        let now = parse_input_date("2026-11-12").unwrap();
        let start = parse_input_date("2026-01-01").unwrap();
        let end = parse_input_date("2026-02-01").unwrap();
        assert_eq!(
            sync_range(
                &json!({}),
                &json!({"from":"2026-01-01","to":"2026-02-01T03:00:00.999+03:00"}),
                &json!({}),
                now
            )
            .unwrap(),
            (start, end)
        );
        let run = json!({"from":start,"to":end});
        assert_eq!(
            sync_range(
                &json!({"history_start":"bad"}),
                &json!({"all_available":true,"from":"bad","to":"bad"}),
                &run,
                now
            )
            .unwrap(),
            (start, end)
        );
        assert_eq!(
            sync_range(
                &json!({"history_start":" 2026-01-01 "}),
                &json!({"all_available":true,"to":" 2026-02-01 "}),
                &json!({}),
                now
            )
            .unwrap(),
            (start, end)
        );
    }
    #[test]
    fn sync_range_rejects_bad_nonempty_inputs_and_strict_checkpoints() {
        let now = parse_input_date("2026-11-12").unwrap();
        for (config, payload, run) in [
            (
                json!({"history_start":"broken"}),
                json!({"all_available":true}),
                json!({}),
            ),
            (json!({}), json!({"from":"broken"}), json!({})),
            (json!({}), json!({"to":"broken"}), json!({})),
            (json!({}), json!({}), json!({"from":""})),
            (json!({}), json!({}), json!({"to":""})),
            (json!({}), json!({}), json!({"from":"broken"})),
            (json!({}), json!({}), json!({"to":"broken"})),
        ] {
            assert_eq!(
                sync_range(&config, &payload, &run, now).unwrap_err().code,
                "invalid_provider_timestamp"
            );
        }
    }
    #[test]
    fn sync_range_enforces_order_and_maximum_duration() {
        let now = parse_input_date("2026-11-12").unwrap();
        for from in [
            now,
            now + chrono::Duration::seconds(1),
            now - chrono::Duration::days(3651),
        ] {
            assert_eq!(
                sync_range(&json!({}), &json!({"from":from,"to":now}), &json!({}), now)
                    .unwrap_err()
                    .code,
                "invalid_sync_period"
            );
        }
    }

    #[test]
    fn imported_employee_uses_only_local_membership_mapping_and_preserves_manual_fields() {
        let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
            eprintln!("SKIP worker import PostgreSQL acceptance: TEST_DATABASE_URL not configured");
            return;
        };
        let pool = persistence::create_pool(&url).unwrap();
        persistence::migrate(&pool).unwrap();
        let mut c = persistence::connection(&pool).unwrap();
        let org = Uuid::new_v4();
        let foreign = Uuid::new_v4();
        let person = Uuid::new_v4();
        let outsider = Uuid::new_v4();
        let connection = Uuid::new_v4();
        let job = Uuid::new_v4();
        let provider = Uuid::new_v4().to_string();
        for (tenant, user, name) in [
            (org, person, "Local employee"),
            (foreign, outsider, "Foreign employee"),
        ] {
            diesel::sql_query(
                "INSERT INTO organizations(id,name) VALUES($1,'Synthetic import test')",
            )
            .bind::<SqlUuid, _>(tenant)
            .execute(&mut c)
            .unwrap();
            diesel::sql_query("INSERT INTO users(id,email,name,password_hash) VALUES($1,$2,$3,'unused-test-hash')").bind::<SqlUuid,_>(user).bind::<diesel::sql_types::Text,_>(format!("{user}@example.test")).bind::<diesel::sql_types::Text,_>(name).execute(&mut c).unwrap();
            diesel::sql_query("INSERT INTO memberships(organization_id,user_id,role,extension) VALUES($1,$2,'manager','123')").bind::<SqlUuid,_>(tenant).bind::<SqlUuid,_>(user).execute(&mut c).unwrap();
        }
        let source = json!({"call_uuid":provider,"init_time_gmt":"2026-01-01 12:00:00","flow":"in","from_username":"+79990000000","result":"bridged","bridged_duration":30,"cdr":[{"extension_id":8,"result":"answered"}]});
        let config = json!({"extension_mapping":{"8":"123"}});
        c.transaction::<_, ProviderError, _>(|c| {
            import_call(c, org, connection, job, &source, false, &config)
        })
        .unwrap();
        let id = Uuid::new_v5(&org, format!("telphin:{connection}:{provider}").as_bytes());
        let mut call = persistence::get_document(&mut c, org, id).unwrap().unwrap();
        assert_eq!(call.data["employee"], "Local employee");
        assert_eq!(call.data["employee_id"], person.to_string());
        assert_eq!(call.data["employee_extension_id"], "8");
        assert_eq!(call.data["employee_extension_number"], "123");
        call.data["manual_lock"] = json!(true);
        call.data["note"] = json!("Keep manual note");
        call.data["contact_kind"] = json!("repeat");
        call.data["coverage_complete"] = json!(true);
        persistence::save_document(&mut c, org, id, "call", None, call.data, Some(call.version))
            .unwrap();
        c.transaction::<_, ProviderError, _>(|c| {
            import_call(c, org, connection, job, &source, false, &config)
        })
        .unwrap();
        let call = persistence::get_document(&mut c, org, id).unwrap().unwrap();
        assert_eq!(call.data["note"], "Keep manual note");
        assert_eq!(call.data["contact_kind"], "repeat");
        assert_eq!(call.data["coverage_complete"], true);
        for tenant in [org, foreign] {
            diesel::sql_query("DELETE FROM organizations WHERE id=$1")
                .bind::<SqlUuid, _>(tenant)
                .execute(&mut c)
                .unwrap();
        }
        for user in [person, outsider] {
            diesel::sql_query("DELETE FROM users WHERE id=$1")
                .bind::<SqlUuid, _>(user)
                .execute(&mut c)
                .unwrap();
        }
    }
    #[test]
    fn extension_mapping_does_not_guess_an_internal_number_from_provider_id() {
        let call = json!({"cdr":[{"extension_id":7,"result":"answered elsewhere"},{"extension_id":8,"result":"answered"}]});
        assert_eq!(
            extension_candidates(&call, &json!({})),
            vec![("8".into(), None)]
        );
        assert_eq!(
            extension_candidates(&call, &json!({"extension_mapping":{"8":"123"}})),
            vec![("8".into(), Some("123".into()))]
        );
        let direct =
            json!({"cdr":[{"extension_id":8,"extension_number":"124","result":"answered"}]});
        assert_eq!(
            extension_candidates(&direct, &json!({"extension_mapping":{"8":"123"}})),
            vec![("8".into(), Some("124".into()))]
        );
    }
    #[test]
    fn coverage_extends_completed_overlap_but_never_a_gap_or_unverified_boundary() {
        let date = |s: &str| providers::parse_utc(s).unwrap();
        let start = date("2026-01-01T00:00:00Z");
        let covered = date("2026-02-01T00:00:00Z");
        let end = date("2026-02-02T00:00:00Z");
        let conn = json!({"history_coverage":{"from":start,"to":covered,"verified":true}});
        assert_eq!(
            verified_coverage_goal(
                &conn,
                &json!({}),
                covered - chrono::Duration::days(1),
                end,
                false
            ),
            Some(start)
        );
        assert_eq!(
            verified_coverage_goal(
                &conn,
                &json!({}),
                covered + chrono::Duration::hours(1),
                end,
                false
            ),
            None
        );
        assert_eq!(
            verified_coverage_goal(
                &json!({}),
                &json!({"history_start":"2026-01-01","history_start_verified":false}),
                start,
                end,
                true
            ),
            None
        );
        assert_eq!(
            verified_coverage_goal(
                &json!({}),
                &json!({"history_start":"2026-01-01","history_start_verified":true}),
                start,
                end,
                true
            ),
            Some(start)
        );
        assert_eq!(
            verified_coverage_goal(
                &json!({}),
                &json!({"history_start":"2026-01-02","history_start_verified":true}),
                start,
                end,
                true
            ),
            None
        );
    }
    #[test]
    fn mime_without_audio_magic_is_never_trusted() {
        assert_eq!(
            audio_extension(b"<html>expired recording</html>", "audio/wav"),
            None
        );
        assert_eq!(
            audio_extension(b"RIFF0000WAVEdata", "application/octet-stream"),
            Some("wav")
        );
    }
    #[test]
    fn orphan_cleanup_preserves_referenced_and_recent_media_and_tenant_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let org = Uuid::new_v4();
        let other = Uuid::new_v4();
        for tenant in [org, other] {
            std::fs::create_dir(directory.path().join(tenant.to_string())).unwrap();
        }
        let keep = format!("{org}/keep.wav");
        let orphan = format!("{org}/orphan.wav");
        let foreign = format!("{other}/private.wav");
        for relative in [&keep, &orphan, &foreign] {
            std::fs::write(directory.path().join(relative), b"synthetic").unwrap();
        }
        let references = std::collections::HashSet::from([keep.clone()]);
        assert_eq!(
            cleanup_orphan_media(
                directory.path(),
                org,
                &references,
                std::time::SystemTime::now()
            )
            .unwrap(),
            0
        );
        assert_eq!(
            cleanup_orphan_media(
                directory.path(),
                org,
                &references,
                std::time::SystemTime::now() + Duration::from_secs(172800)
            )
            .unwrap(),
            1
        );
        assert!(directory.path().join(keep).exists());
        assert!(directory.path().join(foreign).exists());
        assert!(!directory.path().join(orphan).exists());
    }
    #[test]
    fn unpublished_media_is_removed_on_failed_commit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("audio.wav");
        std::fs::write(&path, b"synthetic").unwrap();
        {
            let _artifact = MediaArtifact {
                path: path.clone(),
                committed: false,
            };
        }
        assert!(!path.exists());
    }
    #[test]
    fn reconciliation_failures_never_clear_prior_ambiguous_writes() {
        for original in [
            json!({"state":"uncertain","marker":"stable"}),
            json!({"state":"attempting","marker":"stable"}),
            json!({"state":"failed","requires_reconciliation":true,"marker":"stable"}),
        ] {
            for error in [
                "provider_rate_limited",
                "provider_timeout",
                "configuration_missing",
                "provider_permission_denied",
            ] {
                let mut delivery = original.clone();
                preserve_delivery_failure(&mut delivery, error);
                preserve_delivery_failure(&mut delivery, error); // export handler followed by record_failure.
                assert_eq!(delivery["state"], "uncertain");
                assert!(delivery_requires_reconciliation(&delivery));
                assert_eq!(delivery["marker"], "stable");
            }
        }
        let mut rejected = json!({"state":"failed"});
        preserve_delivery_failure(&mut rejected, "provider_rate_limited");
        assert_eq!(rejected["state"], "failed");
        assert!(!delivery_requires_reconciliation(&rejected));
        preserve_delivery_failure(&mut rejected, "delivery_uncertain");
        assert!(delivery_requires_reconciliation(&rejected));
    }
    #[test]
    fn analysis_receives_taxonomy_labels_and_excludes_archived_values() {
        let make = |name: &str, archived: bool| Document {
            organization_id: Uuid::new_v4(),
            id: Uuid::new_v4(),
            kind: "tag".into(),
            parent_id: None,
            data: json!({"name":name,"archived":archived}),
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let active = make("Запрос стоимости", false);
        let archived = make("Архивный тег", true);
        let id = active.id;
        let result = analysis_taxonomy(vec![active, archived]);
        assert_eq!(result, vec![json!({"id":id,"name":"Запрос стоимости"})]);
    }
}
