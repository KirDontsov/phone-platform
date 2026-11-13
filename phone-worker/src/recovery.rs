//! Operator-only, manifest-scoped recovery of accidental retention purges.
//! This module is never exposed over HTTP or to an AI tool.
use crate::{
    pipeline,
    providers::{self, ProviderError, Result, Telphin},
    WorkerConfig,
};
use chrono::Utc;
use diesel::{
    sql_types::{Jsonb, Text, Uuid as SqlUuid},
    Connection, RunQueryDsl,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::Path,
};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
struct Target {
    call_id: Uuid,
    purge_job_id: Uuid,
    retention_job_id: Uuid,
    provider_id: String,
    purged: bool,
    deleted: bool,
    tombstone: bool,
    manual_audit: bool,
    manual_purge: bool,
    purge_state: String,
}
#[derive(Deserialize)]
struct Manifest {
    organization_id: Uuid,
    connection_id: Uuid,
    targets: Vec<Target>,
}
#[derive(Clone, Deserialize, Serialize)]
struct Guard {
    version: i64,
    deleted_at: String,
}
#[derive(Deserialize, Serialize)]
struct RecoveredSource {
    guard: Guard,
    source: Value,
}
#[derive(Deserialize, Serialize)]
struct Stage {
    version: u32,
    organization_id: Uuid,
    connection_id: Uuid,
    manifest_hash: String,
    records: BTreeMap<Uuid, RecoveredSource>,
}
#[derive(diesel::QueryableByName)]
struct JsonRow {
    #[diesel(sql_type=Jsonb)]
    data: Value,
}

fn invalid(code: &str) -> ProviderError {
    ProviderError::permanent(code)
}
fn private_read(path: &Path, max: u64) -> Result<Vec<u8>> {
    let before =
        fs::symlink_metadata(path).map_err(|_| invalid("recovery_private_file_missing"))?;
    if !before.is_file() || before.permissions().mode() & 0o077 != 0 || before.len() > max {
        return Err(invalid("recovery_unsafe_private_file"));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|_| invalid("recovery_private_file_unreadable"))?;
    let after = file
        .metadata()
        .map_err(|_| invalid("recovery_private_file_unreadable"))?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || after.permissions().mode() & 0o077 != 0
    {
        return Err(invalid("recovery_unsafe_private_file"));
    }
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| invalid("recovery_private_file_unreadable"))?;
    if bytes.len() as u64 > max {
        return Err(invalid("recovery_private_file_too_large"));
    }
    Ok(bytes)
}
fn private_write(path: &Path, value: &Stage) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| invalid("recovery_private_directory_missing"))?;
    let meta =
        fs::symlink_metadata(parent).map_err(|_| invalid("recovery_private_directory_missing"))?;
    if !meta.is_dir() || meta.permissions().mode() & 0o077 != 0 {
        return Err(invalid("recovery_unsafe_private_directory"));
    }
    let bytes = serde_json::to_vec(value).map_err(|_| invalid("recovery_stage_invalid"))?;
    if bytes.len() > 64 * 1024 * 1024 {
        return Err(invalid("recovery_stage_too_large"));
    }
    let temporary = parent.join(format!(".recovery-{}", Uuid::new_v4()));
    let outcome = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(invalid("recovery_stage_write_failed"));
    }
    Ok(())
}
fn manifest(path: &Path) -> Result<(Manifest, String)> {
    let bytes = private_read(path, 16 * 1024 * 1024)?;
    let mut m: Manifest =
        serde_json::from_slice(&bytes).map_err(|_| invalid("recovery_manifest_invalid"))?;
    m.targets.retain(|t| t.purged);
    if m.targets.is_empty() || m.targets.len() > 200 {
        return Err(invalid("recovery_manifest_scope_invalid"));
    }
    let mut calls = HashSet::new();
    let mut providers = HashSet::new();
    for t in &m.targets {
        if !t.deleted
            || !t.tombstone
            || t.manual_audit
            || t.manual_purge
            || t.purge_state != "succeeded"
            || !providers::valid_call_uuid(&t.provider_id)
            || !calls.insert(t.call_id)
            || !providers.insert(t.provider_id.clone())
            || Uuid::new_v5(
                &m.organization_id,
                format!("telphin:{}:{}", m.connection_id, t.provider_id).as_bytes(),
            ) != t.call_id
        {
            return Err(invalid("recovery_manifest_scope_invalid"));
        }
    }
    Ok((m, hex::encode(Sha256::digest(&bytes))))
}
fn current_connection(conn: &mut persistence::PgConnection, m: &Manifest) -> Result<Value> {
    let row = diesel::sql_query("SELECT data FROM organizations WHERE id=$1")
        .bind::<SqlUuid, _>(m.organization_id)
        .get_result::<JsonRow>(conn)?;
    if row.data["retention_enabled"] == true {
        return Err(invalid("recovery_retention_must_be_disabled"));
    }
    let doc = persistence::get_document(conn, m.organization_id, m.connection_id)?
        .filter(|d| {
            d.kind == "connection"
                && d.data["kind"] == "telphin"
                && d.data["state"] != "disconnected"
        })
        .ok_or_else(|| invalid("recovery_connection_missing"))?;
    if let Some(next) = doc.data["next_request_at"]
        .as_i64()
        .filter(|v| *v > Utc::now().timestamp())
    {
        return Err(ProviderError::cached_rate_limit(
            (next - Utc::now().timestamp()).max(1) as u64,
        ));
    }
    Ok(doc.data)
}
fn target_guard(
    conn: &mut persistence::PgConnection,
    m: &Manifest,
    t: &Target,
    hash: &str,
    lock: bool,
) -> Result<Option<Guard>> {
    let recovered=diesel::sql_query("SELECT jsonb_build_object('ok',EXISTS(SELECT 1 FROM documents WHERE organization_id=$1 AND kind='audit' AND data->>'event'='retention.recovered' AND data->>'subject_id'=$2::text AND data->'details'->>'manifest_hash'=$3)) AS data")
        .bind::<SqlUuid,_>(m.organization_id).bind::<SqlUuid,_>(t.call_id).bind::<Text,_>(hash).get_result::<JsonRow>(conn)?.data;
    if recovered["ok"] == true {
        return Ok(None);
    }
    if lock {
        diesel::sql_query("SELECT id FROM documents WHERE organization_id=$1 AND id=$2 FOR UPDATE")
            .bind::<SqlUuid, _>(m.organization_id)
            .bind::<SqlUuid, _>(t.call_id)
            .execute(conn)?;
    }
    let data=diesel::sql_query("SELECT jsonb_build_object('version',c.version,'purged',c.data->'purged','deleted',c.data->'deleted',
      'deleted_at',(SELECT deleted_at::text FROM deletion_tombstones WHERE organization_id=$1 AND subject_id=$2 AND provider_id=$3),
      'purge_ok',EXISTS(SELECT 1 FROM jobs WHERE organization_id=$1 AND id=$4 AND subject_id=$2 AND kind='purge' AND state='succeeded' AND dedup_key=$5),
      'manual',EXISTS(SELECT 1 FROM documents a WHERE a.organization_id=$1 AND a.kind='audit' AND (a.data->>'event'='call.deleted' OR a.data->>'action'='call.deleted') AND a.data->>'subject_id'=$2::text)
          OR EXISTS(SELECT 1 FROM jobs WHERE organization_id=$1 AND subject_id=$2 AND kind='purge' AND dedup_key NOT LIKE 'retention:%')) AS data
      FROM documents c WHERE c.organization_id=$1 AND c.id=$2 AND c.kind='call'")
      .bind::<SqlUuid,_>(m.organization_id).bind::<SqlUuid,_>(t.call_id).bind::<Text,_>(&t.provider_id).bind::<SqlUuid,_>(t.purge_job_id).bind::<Text,_>(format!("retention:{}:{}",t.call_id,t.retention_job_id)).get_result::<JsonRow>(conn)?.data;
    if data["purged"] != true
        || data["deleted"] != true
        || data["purge_ok"] != true
        || data["manual"] != false
    {
        return Err(invalid("recovery_target_guard_failed"));
    }
    Ok(Some(Guard {
        version: data["version"]
            .as_i64()
            .ok_or_else(|| invalid("recovery_target_guard_failed"))?,
        deleted_at: data["deleted_at"]
            .as_str()
            .ok_or_else(|| invalid("recovery_target_guard_failed"))?
            .into(),
    }))
}
fn record_quota(
    conn: &mut persistence::PgConnection,
    m: &Manifest,
    error: &ProviderError,
) -> Result<()> {
    if error.code != "provider_rate_limited" || error.cached_backoff {
        return Ok(());
    }
    conn.transaction::<_, ProviderError, _>(|c| {
        let mut d = persistence::get_document(c, m.organization_id, m.connection_id)?
            .ok_or_else(|| invalid("recovery_connection_missing"))?;
        d.data["next_request_at"] = json!((Utc::now().timestamp()
            + error.retry_after_seconds.unwrap_or(60).min(86400) as i64)
            .max(d.data["next_request_at"].as_i64().unwrap_or(0)));
        d.data["last_error"] = json!("provider_rate_limited");
        persistence::save_document(
            c,
            m.organization_id,
            d.id,
            "connection",
            None,
            d.data,
            Some(d.version),
        )?;
        Ok(())
    })
}
fn checked_stage(m: &Manifest, hash: &str, path: &Path, create: bool) -> Result<Stage> {
    let stage = if create && !path.exists() {
        Stage {
            version: 1,
            organization_id: m.organization_id,
            connection_id: m.connection_id,
            manifest_hash: hash.into(),
            records: BTreeMap::new(),
        }
    } else {
        serde_json::from_slice(&private_read(path, 64 * 1024 * 1024)?)
            .map_err(|_| invalid("recovery_stage_invalid"))?
    };
    if stage.version != 1
        || stage.organization_id != m.organization_id
        || stage.connection_id != m.connection_id
        || stage.manifest_hash != hash
        || stage
            .records
            .keys()
            .any(|id| !m.targets.iter().any(|t| t.call_id == *id))
    {
        return Err(invalid("recovery_stage_scope_invalid"));
    }
    Ok(stage)
}
async fn fetch(
    config: &WorkerConfig,
    conn: &mut persistence::PgConnection,
    m: &Manifest,
    hash: &str,
    path: &Path,
) -> Result<Value> {
    let mut stage = checked_stage(m, hash, path, true)?;
    if stage.records.len() == m.targets.len() {
        return Ok(
            json!({"action":"fetch","target_count":m.targets.len(),"sources_saved":stage.records.len(),"database_calls_restored":0,"audio_downloads":0}),
        );
    }
    let data = current_connection(conn, m)?;
    for t in &m.targets {
        if !stage.records.contains_key(&t.call_id) {
            target_guard(conn, m, t, hash, false)?
                .ok_or_else(|| invalid("recovery_already_applied_without_stage"))?;
        }
    }
    let encrypted = providers::required(&data, "config_encrypted")?;
    let cfg = contracts::unseal(encrypted, &config.encryption_key)
        .map_err(|_| invalid("connection_decryption_failed"))?;
    let mut provider = match Telphin::connect(&cfg).await {
        Ok(p) => p,
        Err(e) => {
            record_quota(conn, m, &e)?;
            return Err(e);
        }
    };
    for t in &m.targets {
        if stage.records.contains_key(&t.call_id) {
            continue;
        }
        current_connection(conn, m)?;
        let guard = target_guard(conn, m, t, hash, false)?
            .ok_or_else(|| invalid("recovery_already_applied_without_stage"))?;
        let source = match provider.single_call(&t.provider_id).await {
            Ok(v) => v,
            Err(e) => {
                record_quota(conn, m, &e)?;
                return Err(e);
            }
        };
        stage
            .records
            .insert(t.call_id, RecoveredSource { guard, source });
        private_write(path, &stage)?;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    Ok(
        json!({"action":"fetch","target_count":m.targets.len(),"sources_saved":stage.records.len(),"database_calls_restored":0,"audio_downloads":0}),
    )
}
fn apply(
    config: &WorkerConfig,
    conn: &mut persistence::PgConnection,
    m: &Manifest,
    hash: &str,
    path: &Path,
    dry: bool,
) -> Result<Value> {
    let stage = checked_stage(m, hash, path, false)?;
    if stage.records.len() != m.targets.len() {
        return Err(invalid("recovery_source_fetch_incomplete"));
    }
    // Apply is offline: a provider pause must not prevent restoring already-fetched real data.
    let data = persistence::get_document(conn, m.organization_id, m.connection_id)?
        .filter(|d| d.kind == "connection" && d.data["kind"] == "telphin")
        .ok_or_else(|| invalid("recovery_connection_missing"))?
        .data;
    let cfg = contracts::unseal(
        providers::required(&data, "config_encrypted")?,
        &config.encryption_key,
    )
    .map_err(|_| invalid("connection_decryption_failed"))?;
    let mut restored = 0usize;
    let mut already = 0usize;
    for t in &m.targets {
        let item = stage
            .records
            .get(&t.call_id)
            .ok_or_else(|| invalid("recovery_source_fetch_incomplete"))?;
        let result=conn.transaction::<_,ProviderError,_>(|c| {
            diesel::sql_query("SELECT id FROM organizations WHERE id=$1 AND data->'retention_enabled' IS DISTINCT FROM 'true'::jsonb FOR UPDATE").bind::<SqlUuid,_>(m.organization_id).execute(c)?;
            let policy=diesel::sql_query("SELECT data FROM organizations WHERE id=$1").bind::<SqlUuid,_>(m.organization_id).get_result::<JsonRow>(c)?.data;
            if policy["retention_enabled"]==true {return Err(invalid("recovery_retention_must_be_disabled"));}
            let Some(now)=target_guard(c,m,t,hash,true)? else {return Ok(false)};
            if now.version!=item.guard.version || now.deleted_at!=item.guard.deleted_at {return Err(invalid("recovery_target_changed"));}
            providers::validate_single_call(&item.source,providers::required(&cfg,"client_id")?,&t.provider_id)?;
            if dry {return Ok(true);}
            let changed=diesel::sql_query("DELETE FROM deletion_tombstones WHERE organization_id=$1 AND subject_id=$2 AND provider_id=$3 AND deleted_at::text=$4")
                .bind::<SqlUuid,_>(m.organization_id).bind::<SqlUuid,_>(t.call_id).bind::<Text,_>(&t.provider_id).bind::<Text,_>(&item.guard.deleted_at).execute(c)?;
            if changed!=1 {return Err(invalid("recovery_target_changed"));}
            let baseline=json!({"status_id":null,"tag_ids":[],"note":"","manual_lock":false,"contact_kind":"unknown","score":null,"stage":"imported","retention_recovery_missing_annotations":true});
            let updated=diesel::sql_query("UPDATE documents SET data=$3,version=version+1,updated_at=now() WHERE organization_id=$1 AND id=$2 AND version=$4")
                .bind::<SqlUuid,_>(m.organization_id).bind::<SqlUuid,_>(t.call_id).bind::<Jsonb,_>(baseline).bind::<diesel::sql_types::BigInt,_>(now.version).execute(c)?;
            if updated!=1 {return Err(invalid("recovery_target_changed"));}
            pipeline::import_retention_recovery_source(c,m.organization_id,m.connection_id,t.purge_job_id,&item.source,&cfg)?;
            // Only fresh, unclaimed recording jobs created by this exact recovery.
            // Keep attempts/state/deadline unchanged; normal quota guards still apply.
            diesel::sql_query("UPDATE jobs SET payload=payload||jsonb_build_object('recovery_manifest_hash',$3::text),updated_at=now()
                WHERE organization_id=$1 AND kind='recording' AND state='queued' AND attempts=0
                AND subject_id IN (SELECT id FROM documents WHERE organization_id=$1 AND kind='recording' AND parent_id=$2)")
                .bind::<SqlUuid,_>(m.organization_id).bind::<SqlUuid,_>(t.call_id).bind::<Text,_>(hash).execute(c)?;
            persistence::append_audit(c,m.organization_id,None,"retention.recovered",Some(t.call_id),json!({"manifest_hash":hash,"source":"telphin","manual_annotations_recovered":false}))?;
            Ok(true)
        })?;
        if result {
            restored += 1;
        } else {
            already += 1;
        }
    }
    Ok(
        json!({"action":if dry{"dry-run"}else{"apply"},"targets":m.targets.len(),"metadata_restored":if dry{0}else{restored},"eligible":restored,"already_recovered":already,"audio_downloads":0,"manual_annotations_recovered":false}),
    )
}
/// Only explicit operator invocation can enter this path; no automatic worker job/HTTP API.
pub async fn run_cli(config: WorkerConfig, args: &[String]) -> Result<Value> {
    if args.len() < 3
        || args.len() > 4
        || !["fetch", "apply"].contains(&args[0].as_str())
        || (args.len() == 4 && (args[0] != "apply" || args[3] != "--dry-run"))
    {
        return Err(invalid("recovery_usage_fetch_or_apply_manifest_stage"));
    }
    let (m, hash) = manifest(Path::new(&args[1]))?;
    let pool = persistence::create_pool(&config.database_url)?;
    let mut conn = persistence::connection(&pool)?;
    if args[0] == "fetch" {
        fetch(&config, &mut conn, &m, &hash, Path::new(&args[2])).await
    } else {
        apply(
            &config,
            &mut conn,
            &m,
            &hash,
            Path::new(&args[2]),
            args.len() == 4,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn target(org: Uuid, connection: Uuid) -> Target {
        let provider_id = Uuid::new_v4().simple().to_string();
        Target {
            call_id: Uuid::new_v5(
                &org,
                format!("telphin:{connection}:{provider_id}").as_bytes(),
            ),
            purge_job_id: Uuid::new_v4(),
            retention_job_id: Uuid::new_v4(),
            provider_id,
            purged: true,
            deleted: true,
            tombstone: true,
            manual_audit: false,
            manual_purge: false,
            purge_state: "succeeded".into(),
        }
    }
    fn write_manifest(path: &Path, m: &Manifest) {
        let body = json!({"organization_id":m.organization_id,"connection_id":m.connection_id,"targets":m.targets});
        fs::write(path, serde_json::to_vec(&body).unwrap()).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    #[test]
    fn manifest_rejects_manual_foreign_identity_and_public_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("manifest.json");
        let org = Uuid::new_v4();
        let connection = Uuid::new_v4();
        let mut m = Manifest {
            organization_id: org,
            connection_id: connection,
            targets: vec![target(org, connection)],
        };
        write_manifest(&path, &m);
        assert!(manifest(&path).is_ok());
        m.targets[0].manual_audit = true;
        write_manifest(&path, &m);
        assert!(manifest(&path).is_err());
        m.targets[0].manual_audit = false;
        m.organization_id = Uuid::new_v4();
        write_manifest(&path, &m);
        assert!(manifest(&path).is_err());
        m.organization_id = org;
        write_manifest(&path, &m);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(manifest(&path).is_err());
    }
    #[test]
    fn stage_is_atomic_private_and_bound_to_manifest() {
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let org = Uuid::new_v4();
        let connection = Uuid::new_v4();
        let m = Manifest {
            organization_id: org,
            connection_id: connection,
            targets: vec![target(org, connection)],
        };
        let path = dir.path().join("stage.json");
        let stage = checked_stage(&m, "hash", &path, true).unwrap();
        private_write(&path, &stage).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(checked_stage(&m, "changed", &path, false).is_err());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
    #[test]
    #[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
    fn actual_pg_recovery_is_guarded_atomic_offline_and_idempotent() {
        let db = std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL");
        let pool = persistence::create_pool(&db).unwrap();
        persistence::migrate(&pool).unwrap();
        let mut c = persistence::connection(&pool).unwrap();
        let org = Uuid::new_v4();
        let connection = Uuid::new_v4();
        let t = target(org, connection);
        let m = Manifest {
            organization_id: org,
            connection_id: connection,
            targets: vec![t.clone()],
        };
        let hash = "synthetic-manifest-hash";
        diesel::sql_query("INSERT INTO organizations(id,name) VALUES($1,'Synthetic recovery')")
            .bind::<SqlUuid, _>(org)
            .execute(&mut c)
            .unwrap();
        let cfg = json!({"client_id":"42","host":"apiproxy.telphin.ru","app_id":"synthetic","app_secret":"synthetic"});
        let key = [7; 32];
        persistence::save_document(&mut c,org,connection,"connection",None,json!({"kind":"telphin","state":"configured","config_encrypted":contracts::seal(&cfg,&key).unwrap(),"next_request_at":Utc::now().timestamp()+3600}),None).unwrap();
        persistence::save_document(
            &mut c,
            org,
            t.call_id,
            "call",
            None,
            json!({"deleted":true,"purged":true}),
            None,
        )
        .unwrap();
        persistence::tombstone(&mut c, org, t.call_id, Some(&t.provider_id)).unwrap();
        diesel::sql_query("INSERT INTO jobs(id,organization_id,kind,subject_id,dedup_key,state) VALUES($1,$2,'purge',$3,$4,'succeeded')").bind::<SqlUuid,_>(t.purge_job_id).bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(t.call_id).bind::<Text,_>(format!("retention:{}:{}",t.call_id,t.retention_job_id)).execute(&mut c).unwrap();
        let guard = target_guard(&mut c, &m, &t, hash, false).unwrap().unwrap();
        let source = json!({"call_uuid":t.provider_id,"client_owner_id":42,"init_time_gmt":"2026-02-01 00:00:00","flow":"in","result":"not answered","duration":0,"cdr":[{"client_owner_id":42,"record_uuid":"33333333-3333-4333-8333-333333333333","record_file_size":5}]});
        let stage = Stage {
            version: 1,
            organization_id: org,
            connection_id: connection,
            manifest_hash: hash.into(),
            records: BTreeMap::from([(t.call_id, RecoveredSource { guard, source })]),
        };
        let dir = tempfile::tempdir().unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let path = dir.path().join("stage.json");
        private_write(&path, &stage).unwrap();
        let config = WorkerConfig {
            database_url: db,
            encryption_key: key,
            media_root: dir.path().into(),
            demo_mode: false,
            rabbitmq_url: None,
            public_url: "http://localhost".into(),
            poll_seconds: 1,
        };
        assert!(
            current_connection(&mut c, &m).unwrap_err().cached_backoff,
            "fetch respects quota before networking"
        );
        let dry = apply(&config, &mut c, &m, hash, &path, true).unwrap();
        assert_eq!(dry["metadata_restored"], 0);
        assert!(persistence::is_tombstoned(&mut c, org, t.call_id, Some(&t.provider_id)).unwrap());
        // A manual deletion made after fetch must prevent recovery, even under valid scope.
        let audit = persistence::append_audit(
            &mut c,
            org,
            None,
            "call.deleted",
            Some(t.call_id),
            json!({}),
        )
        .unwrap();
        assert!(apply(&config, &mut c, &m, hash, &path, false).is_err());
        assert!(persistence::is_tombstoned(&mut c, org, t.call_id, Some(&t.provider_id)).unwrap());
        persistence::delete_document(&mut c, org, audit.id).unwrap();
        let api_audit=persistence::save_document(&mut c,org,Uuid::new_v4(),"audit",None,json!({"action":"call.deleted","subject_id":t.call_id}),None).unwrap();
        assert!(apply(&config,&mut c,&m,hash,&path,false).is_err(),"real API action-shaped manual deletion cannot be recovered");
        assert!(persistence::is_tombstoned(&mut c,org,t.call_id,Some(&t.provider_id)).unwrap());
        persistence::delete_document(&mut c,org,api_audit.id).unwrap();
        let restored = apply(&config, &mut c, &m, hash, &path, false).unwrap();
        assert_eq!(restored["metadata_restored"], 1);
        assert!(!persistence::is_tombstoned(&mut c, org, t.call_id, Some(&t.provider_id)).unwrap());
        let call = persistence::get_document(&mut c, org, t.call_id)
            .unwrap()
            .unwrap();
        assert_eq!(call.data["provider_id"], t.provider_id);
        assert_eq!(call.data["retention_recovery_missing_annotations"], true);
        assert_eq!(call.data["note"], "");
        assert_eq!(
            persistence::list_children(&mut c, org, t.call_id, Some("recording"))
                .unwrap()
                .len(),
            1
        );
        let jobs=persistence::list_jobs(&mut c,org,20).unwrap();
        let audio_job=jobs.iter().find(|j|j.kind=="recording").unwrap();
        assert_eq!(audio_job.payload["recovery_manifest_hash"],hash);
        assert_eq!(audio_job.attempts,0);
        assert_eq!(audio_job.state,"queued");
        assert_eq!(
            apply(&config, &mut c, &m, hash, &path, false).unwrap()["already_recovered"],
            1
        );
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(org)
            .execute(&mut c)
            .unwrap();
    }
}
