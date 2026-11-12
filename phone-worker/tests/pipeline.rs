use chrono::Utc;
use diesel::{
    sql_types::{Text, Uuid as SqlUuid},
    RunQueryDsl,
};
use phone_worker::{Worker, WorkerConfig};
use serde_json::json;
use uuid::Uuid;
static PG_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tokio::test]
async fn demo_reprocess_preserves_manual_fields_versions_and_purge_removes_artifacts() {
    let _serial = PG_TEST_LOCK.lock().unwrap();
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIP worker PostgreSQL acceptance: TEST_DATABASE_URL not configured");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let org = Uuid::new_v4();
    let foreign = Uuid::new_v4();
    let call_id = Uuid::new_v4();
    let rec_id = Uuid::new_v4();
    let status = Uuid::new_v4();
    let tag = Uuid::new_v4();
    let worker = Worker::new(WorkerConfig {
        database_url: url,
        encryption_key: [8; 32],
        media_root: directory.path().to_owned(),
        demo_mode: true,
        rabbitmq_url: None,
        public_url: "http://localhost:8031".into(),
        poll_seconds: 1,
    })
    .unwrap();
    let relative = format!("{org}/synthetic.wav");
    let path = directory.path().join(&relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"RIFF0000WAVEsynthetic-test-not-uploaded").unwrap();
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        for id in [org, foreign] {
            diesel::sql_query("INSERT INTO organizations(id,name) VALUES($1,$2)")
                .bind::<SqlUuid, _>(id)
                .bind::<Text, _>("Worker synthetic test")
                .execute(&mut conn)
                .unwrap();
        }
        persistence::save_document(
            &mut conn,
            org,
            status,
            "status",
            None,
            json!({"name":"Ручной статус","archived":false}),
            None,
        )
        .unwrap();
        persistence::save_document(
            &mut conn,
            org,
            tag,
            "tag",
            None,
            json!({"name":"Ручной тег","archived":false}),
            None,
        )
        .unwrap();
        persistence::save_document(&mut conn,org,Uuid::new_v4(),"rubric",None,json!({"name":"Тестовая шкала","rubric_version":7,"active":true,"criteria":[{"id":"greeting","name":"Приветствие","weight":1,"required":true},{"id":"next_step","name":"Следующий шаг","weight":2,"required":true}]}),None).unwrap();
        persistence::save_document(&mut conn,org,call_id,"call",None,json!({"provider_id":"synthetic-provider-call","synthetic":true,"started_at":Utc::now(),"phone":"+70000000000","contact_name":"Синтетический клиент","duration":60,"provider_result":"bridged","human_connected":true,"direction":"in","stage":"imported","has_recording":true,"manual_lock":true,"status_id":status,"tag_ids":[tag],"note":"Ручная заметка","contact_kind":"repeat","coverage_complete":true}),None).unwrap();
        persistence::save_document(&mut conn,org,rec_id,"recording",Some(call_id),json!({"call_id":call_id,"path":relative,"state":"ready","ready":true,"mime":"audio/wav","duration":60,"sha256":"synthetic-checksum"}),None).unwrap();
        assert!(persistence::enqueue(
            &mut conn,
            foreign,
            "transcribe",
            Some(call_id),
            "foreign-invalid",
            json!({})
        )
        .is_err());
        persistence::enqueue(
            &mut conn,
            org,
            "transcribe",
            Some(call_id),
            "initial",
            json!({}),
        )
        .unwrap();
    }
    assert!(worker.run_once().await.unwrap());
    assert!(worker.run_once().await.unwrap());
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        let transcripts =
            persistence::list_children(&mut conn, org, call_id, Some("transcript")).unwrap();
        let analyses =
            persistence::list_children(&mut conn, org, call_id, Some("analysis")).unwrap();
        assert_eq!(transcripts.len(), 1);
        assert_eq!(analyses.len(), 1);
        assert_eq!(analyses[0].data["rubric_version"], 7);
        assert_eq!(analyses[0].data["provider"], "demo");
        assert_eq!(analyses[0].data["score"], 80.);
        let call = persistence::get_document(&mut conn, org, call_id)
            .unwrap()
            .unwrap();
        assert_eq!(call.data["stage"], "ready");
        assert_eq!(call.data["status_id"], status.to_string());
        assert_eq!(call.data["tag_ids"], json!([tag]));
        assert_eq!(call.data["note"], "Ручная заметка");
        assert_eq!(call.data["contact_kind"], "repeat");
        persistence::enqueue(
            &mut conn,
            org,
            "reprocess",
            Some(call_id),
            "second-version",
            json!({"stage":"all"}),
        )
        .unwrap();
    }
    for _ in 0..3 {
        assert!(worker.run_once().await.unwrap());
    }
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        let versions =
            persistence::list_children(&mut conn, org, call_id, Some("transcript")).unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().any(|v| v.data["version"] == 1));
        assert!(versions.iter().any(|v| v.data["version"] == 2));
        assert_eq!(
            persistence::list_children(&mut conn, org, call_id, Some("analysis"))
                .unwrap()
                .len(),
            2
        );
        let mut call = persistence::get_document(&mut conn, org, call_id)
            .unwrap()
            .unwrap();
        call.data["deleted"] = json!(true);
        persistence::save_document(&mut conn, org, call_id, "call", None, call.data, None).unwrap();
        persistence::enqueue(
            &mut conn,
            org,
            "purge",
            Some(call_id),
            "delete-artifacts",
            json!({}),
        )
        .unwrap();
    }
    assert!(worker.run_once().await.unwrap());
    assert!(!path.exists());
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        assert!(persistence::list_children(&mut conn, org, call_id, None)
            .unwrap()
            .is_empty());
        let stub = persistence::get_document(&mut conn, org, call_id)
            .unwrap()
            .unwrap();
        assert_eq!(stub.data["purged"], true);
        assert!(stub.data.get("phone").is_none());
        assert!(persistence::is_tombstoned(
            &mut conn,
            org,
            call_id,
            Some("synthetic-provider-call")
        )
        .unwrap());
        assert!(persistence::save_document(
            &mut conn,
            org,
            call_id,
            "call",
            None,
            json!({"phone":"must-not-resurrect"}),
            None
        )
        .is_err());
        persistence::enqueue(
            &mut conn,
            org,
            "transcribe",
            Some(call_id),
            "late-transcribe",
            json!({}),
        )
        .unwrap();
    }
    assert!(worker.run_once().await.unwrap());
    worker.write_health("ready", None).await.unwrap();
    let health = directory.path().join(".worker-health.json");
    assert!(health.exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            health.metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        assert!(persistence::list_children(&mut conn, org, call_id, None)
            .unwrap()
            .is_empty());
        for id in [org, foreign] {
            diesel::sql_query("DELETE FROM organizations WHERE id=$1")
                .bind::<SqlUuid, _>(id)
                .execute(&mut conn)
                .unwrap();
        }
    }
}

#[tokio::test]
async fn missing_production_asr_configuration_is_visible_and_never_fabricates_transcript() {
    let _serial = PG_TEST_LOCK.lock().unwrap();
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIP worker PostgreSQL acceptance: TEST_DATABASE_URL not configured");
        return;
    };
    let tmp = tempfile::tempdir().unwrap();
    let org = Uuid::new_v4();
    let call = Uuid::new_v4();
    let worker = Worker::new(WorkerConfig {
        database_url: url,
        encryption_key: [8; 32],
        media_root: tmp.path().into(),
        demo_mode: false,
        rabbitmq_url: None,
        public_url: "http://localhost".into(),
        poll_seconds: 1,
    })
    .unwrap();
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        diesel::sql_query(
            "INSERT INTO organizations(id,name) VALUES($1,'No external credentials')",
        )
        .bind::<SqlUuid, _>(org)
        .execute(&mut conn)
        .unwrap();
        persistence::save_document(&mut conn,org,call,"call",None,json!({"has_recording":true,"stage":"imported","manual_lock":true,"note":"Do not alter"}),None).unwrap();
        persistence::save_document(
            &mut conn,
            org,
            Uuid::new_v4(),
            "recording",
            Some(call),
            json!({"ready":true,"path":format!("{org}/record.wav")}),
            None,
        )
        .unwrap();
        persistence::enqueue(
            &mut conn,
            org,
            "transcribe",
            Some(call),
            "no-provider",
            json!({}),
        )
        .unwrap();
    }
    // This test uses a distinct DB/serial test run to avoid claiming another test's global jobs.
    for _ in 0..10 {
        worker.run_once().await.unwrap();
        let mut conn = persistence::connection(&worker.pool).unwrap();
        if persistence::get_document(&mut conn, org, call)
            .unwrap()
            .unwrap()
            .data["stage"]
            == "failed"
        {
            break;
        }
    }
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        let doc = persistence::get_document(&mut conn, org, call)
            .unwrap()
            .unwrap();
        assert_eq!(doc.data["stage"], "failed");
        assert_eq!(doc.data["processing_reason"], "configuration_missing");
        assert_eq!(doc.data["note"], "Do not alter");
        assert!(
            persistence::list_children(&mut conn, org, call, Some("transcript"))
                .unwrap()
                .is_empty()
        );
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(org)
            .execute(&mut conn)
            .unwrap();
    }
}

#[tokio::test]
async fn manual_retry_cannot_reuse_a_previous_budget_reservation() {
    let _serial = PG_TEST_LOCK.lock().unwrap();
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIP worker PostgreSQL acceptance: TEST_DATABASE_URL not configured");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let org = Uuid::new_v4();
    let call = Uuid::new_v4();
    let key = [8; 32];
    let worker = Worker::new(WorkerConfig {
        database_url: url,
        encryption_key: key,
        media_root: directory.path().into(),
        demo_mode: false,
        rabbitmq_url: None,
        public_url: "http://localhost".into(),
        poll_seconds: 1,
    })
    .unwrap();
    let job = {
        let mut c = persistence::connection(&worker.pool).unwrap();
        diesel::sql_query(
            "INSERT INTO organizations(id,name) VALUES($1,'Budget regression synthetic')",
        )
        .bind::<SqlUuid, _>(org)
        .execute(&mut c)
        .unwrap();
        persistence::save_document(&mut c,org,call,"call",None,json!({"phone":"+70000000000","started_at":Utc::now(),"manual_lock":true,"stage":"ready"}),None).unwrap();
        persistence::save_document(
            &mut c,
            org,
            Uuid::new_v4(),
            "transcript",
            Some(call),
            phone_worker::models::demo_transcript(&json!({"synthetic":true,"duration":30})),
            None,
        )
        .unwrap();
        persistence::save_document(&mut c,org,Uuid::new_v4(),"rubric",None,json!({"active":true,"rubric_version":1,"criteria":[{"id":"greeting","name":"Приветствие","weight":1,"required":true}]}),None).unwrap();
        // This URL is rejected locally; the test never contacts any external account.
        let config = json!({"provider":"openai","base_url":"http://not-allowed.invalid","api_key":"synthetic","model":"synthetic","budget_daily":1.0,"estimated_cost_per_run":0.75});
        persistence::save_document(&mut c,org,Uuid::new_v4(),"connection",None,json!({"kind":"analysis","state":"configured","config_encrypted":contracts::seal(&config,&key).unwrap()}),None).unwrap();
        persistence::enqueue(
            &mut c,
            org,
            "analyze",
            Some(call),
            "budget-retry",
            json!({}),
        )
        .unwrap()
    };
    worker.run_once().await.unwrap();
    {
        let mut c = persistence::connection(&worker.pool).unwrap();
        assert_eq!(
            persistence::list_documents(&mut c, org, "usage")
                .unwrap()
                .len(),
            1
        );
        assert!(persistence::retry_job(&mut c, org, job.id).unwrap());
    }
    worker.run_once().await.unwrap();
    {
        let mut c = persistence::connection(&worker.pool).unwrap();
        let jobs = persistence::list_jobs(&mut c, org, 10).unwrap();
        let retried = jobs.iter().find(|j| j.id == job.id).unwrap();
        assert_eq!(
            retried.error_code.as_deref(),
            Some("analysis_daily_budget_reached")
        );
        assert_eq!(
            persistence::list_documents(&mut c, org, "usage")
                .unwrap()
                .len(),
            1
        );
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(org)
            .execute(&mut c)
            .unwrap();
    }
}

#[tokio::test]
async fn purge_unlinks_shared_demo_audio_only_after_last_recording_reference() {
    let _serial = PG_TEST_LOCK.lock().unwrap();
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIP worker PostgreSQL acceptance: TEST_DATABASE_URL not configured");
        return;
    };
    let directory = tempfile::tempdir().unwrap();
    let org = Uuid::new_v4();
    let calls = [Uuid::new_v4(), Uuid::new_v4()];
    let relative = format!("{org}/shared-demo.wav");
    let path = directory.path().join(&relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"RIFF0000WAVEsynthetic-shared").unwrap();
    let worker = Worker::new(WorkerConfig {
        database_url: url,
        encryption_key: [8; 32],
        media_root: directory.path().into(),
        demo_mode: false,
        rabbitmq_url: None,
        public_url: "http://localhost".into(),
        poll_seconds: 1,
    })
    .unwrap();
    {
        let mut c = persistence::connection(&worker.pool).unwrap();
        diesel::sql_query(
            "INSERT INTO organizations(id,name) VALUES($1,'Synthetic shared media test')",
        )
        .bind::<SqlUuid, _>(org)
        .execute(&mut c)
        .unwrap();
        for call in calls {
            persistence::save_document(
                &mut c,
                org,
                call,
                "call",
                None,
                json!({"synthetic":true,"has_recording":true}),
                None,
            )
            .unwrap();
            persistence::save_document(
                &mut c,
                org,
                Uuid::new_v4(),
                "recording",
                Some(call),
                json!({"path":relative,"ready":true,"state":"ready"}),
                None,
            )
            .unwrap();
        }
    }
    for (index, call) in calls.into_iter().enumerate() {
        {
            let mut c = persistence::connection(&worker.pool).unwrap();
            let mut doc = persistence::get_document(&mut c, org, call)
                .unwrap()
                .unwrap();
            doc.data["deleted"] = json!(true);
            persistence::save_document(
                &mut c,
                org,
                call,
                "call",
                None,
                doc.data,
                Some(doc.version),
            )
            .unwrap();
            persistence::enqueue(
                &mut c,
                org,
                "purge",
                Some(call),
                &format!("purge-shared:{index}"),
                json!({}),
            )
            .unwrap();
        }
        worker.run_once().await.unwrap();
        assert_eq!(
            path.exists(),
            index == 0,
            "First purge preserves other recording; final purge removes shared audio"
        );
    }
    let mut c = persistence::connection(&worker.pool).unwrap();
    diesel::sql_query("DELETE FROM organizations WHERE id=$1")
        .bind::<SqlUuid, _>(org)
        .execute(&mut c)
        .unwrap();
}

fn review_worker(demo_mode: bool) -> Option<(Worker, tempfile::TempDir, Uuid)> {
    let Ok(database_url) = std::env::var("TEST_DATABASE_URL") else {
        eprintln!("SKIP PostgreSQL review regression: TEST_DATABASE_URL not configured");
        return None;
    };
    let directory = tempfile::tempdir().unwrap();
    let org = Uuid::new_v4();
    let worker = Worker::new(WorkerConfig {
        database_url,
        encryption_key: [9; 32],
        media_root: directory.path().into(),
        demo_mode,
        rabbitmq_url: None,
        public_url: "http://localhost".into(),
        poll_seconds: 1,
    })
    .unwrap();
    let mut conn = persistence::connection(&worker.pool).unwrap();
    diesel::sql_query(
        "INSERT INTO organizations(id,name) VALUES($1,'Review regression synthetic')",
    )
    .bind::<SqlUuid, _>(org)
    .execute(&mut conn)
    .unwrap();
    Some((worker, directory, org))
}

#[tokio::test]
async fn disconnected_demo_provider_is_not_reenabled_by_fallback() {
    let _serial = PG_TEST_LOCK.lock().unwrap();
    let Some((worker, directory, org)) = review_worker(true) else {
        return;
    };
    let call = Uuid::new_v4();
    let relative = format!("{org}/synthetic.wav");
    std::fs::create_dir(directory.path().join(org.to_string())).unwrap();
    std::fs::write(directory.path().join(&relative), b"RIFF0000WAVEsynthetic").unwrap();
    {
        let mut conn = persistence::connection(&worker.pool).unwrap();
        persistence::save_document(
            &mut conn,
            org,
            call,
            "call",
            None,
            json!({"synthetic":true,"has_recording":true,"duration":30,"stage":"imported"}),
            None,
        )
        .unwrap();
        persistence::save_document(
            &mut conn,
            org,
            Uuid::new_v4(),
            "recording",
            Some(call),
            json!({"path":relative,"ready":true,"state":"ready","duration":30}),
            None,
        )
        .unwrap();
        persistence::save_document(
            &mut conn,
            org,
            Uuid::new_v4(),
            "connection",
            None,
            json!({"kind":"asr","state":"disconnected","config":{}}),
            None,
        )
        .unwrap();
        persistence::enqueue(
            &mut conn,
            org,
            "transcribe",
            Some(call),
            "disabled-demo",
            json!({}),
        )
        .unwrap();
    }
    assert!(worker.run_once().await.unwrap());
    let mut conn = persistence::connection(&worker.pool).unwrap();
    let current = persistence::get_document(&mut conn, org, call)
        .unwrap()
        .unwrap();
    assert_eq!(current.data["processing_reason"], "configuration_missing");
    assert!(
        persistence::list_children(&mut conn, org, call, Some("transcript"))
            .unwrap()
            .is_empty()
    );
    diesel::sql_query("DELETE FROM organizations WHERE id=$1")
        .bind::<SqlUuid, _>(org)
        .execute(&mut conn)
        .unwrap();
}

#[tokio::test]
async fn ambiguous_delivery_survives_configuration_failure_and_manual_retry() {
    let _serial = PG_TEST_LOCK.lock().unwrap();
    let Some((worker, _directory, org)) = review_worker(false) else {
        return;
    };
    for state in ["uncertain", "attempting"] {
        let call = Uuid::new_v4();
        let (job, delivery) = {
            let mut conn = persistence::connection(&worker.pool).unwrap();
            persistence::save_document(
                &mut conn,
                org,
                call,
                "call",
                None,
                json!({"stage":"ready"}),
                None,
            )
            .unwrap();
            let job = persistence::enqueue(
                &mut conn,
                org,
                "export",
                Some(call),
                state,
                json!({"provider":"amocrm"}),
            )
            .unwrap();
            let delivery = Uuid::new_v5(&job.id, b"delivery");
            persistence::save_document(
                &mut conn,
                org,
                delivery,
                "delivery",
                Some(call),
                json!({"state":state,"marker":"stable-review-marker","analysis_id":null}),
                None,
            )
            .unwrap();
            (job, delivery)
        };
        for attempt in 0..2 {
            if attempt > 0 {
                let mut conn = persistence::connection(&worker.pool).unwrap();
                assert!(persistence::retry_job(&mut conn, org, job.id).unwrap());
            }
            assert!(worker.run_once().await.unwrap());
            let mut conn = persistence::connection(&worker.pool).unwrap();
            let delivery = persistence::get_document(&mut conn, org, delivery)
                .unwrap()
                .unwrap();
            assert_eq!(delivery.data["state"], "uncertain");
            assert_eq!(delivery.data["requires_reconciliation"], true);
            assert_eq!(delivery.data["marker"], "stable-review-marker");
            assert_eq!(delivery.data["error_code"], "configuration_missing");
        }
    }
    let mut conn = persistence::connection(&worker.pool).unwrap();
    diesel::sql_query("DELETE FROM organizations WHERE id=$1")
        .bind::<SqlUuid, _>(org)
        .execute(&mut conn)
        .unwrap();
}
