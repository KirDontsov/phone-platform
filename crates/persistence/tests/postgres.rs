//! Run only against an isolated TEST_DATABASE_URL: cargo test -p persistence --test postgres -- --ignored.
use persistence::diesel::Connection as DieselConnection;
use persistence::diesel::{
    prelude::{QueryableByName, RunQueryDsl},
    sql_types::{BigInt, Text, Uuid as SqlUuid},
};
use persistence::*;
use serde_json::json;
use uuid::Uuid;

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type=BigInt)]
    n: i64,
}
fn count(conn: &mut PgConnection, sql: &str, id: Uuid) -> i64 {
    diesel::sql_query(sql)
        .bind::<SqlUuid, _>(id)
        .get_result::<Count>(conn)
        .unwrap()
        .n
}

#[test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
fn migration_tenant_atomicity_leases_and_purge_are_enforced() {
    let pool = create_pool(
        &std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
    )
    .unwrap();
    migrate(&pool).unwrap();
    migrate(&pool).unwrap();
    let mut conn = connection(&pool).unwrap();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let id = Uuid::new_v4();
    for org in [a, b] {
        diesel::sql_query(
            "INSERT INTO organizations(id,name) VALUES($1,'Synthetic integration test')",
        )
        .bind::<SqlUuid, _>(org)
        .execute(&mut conn)
        .unwrap();
    }
    let first = save_document(
        &mut conn,
        a,
        id,
        "call",
        None,
        json!({"provider_id":"synthetic-provider","note":"first"}),
        None,
    )
    .unwrap();
    assert_eq!(first.version, 1);
    assert!(get_document(&mut conn, b, id).unwrap().is_none());
    assert_eq!(list_documents(&mut conn, b, "call").unwrap().len(), 0);
    assert!(save_document(
        &mut conn,
        b,
        Uuid::new_v4(),
        "transcript",
        Some(id),
        json!({}),
        None
    )
    .is_err());
    let second = save_document(
        &mut conn,
        a,
        id,
        "call",
        None,
        json!({"provider_id":"synthetic-provider","note":"second"}),
        Some(1),
    )
    .unwrap();
    assert_eq!(second.version, 2);
    assert_eq!(
        save_document(
            &mut conn,
            a,
            id,
            "call",
            None,
            json!({"note":"stale"}),
            Some(1)
        )
        .unwrap_err(),
        DbError::Conflict
    );
    assert_eq!(
        get_document(&mut conn, a, id).unwrap().unwrap().data["note"],
        "second"
    );

    let dedup = Uuid::new_v4().to_string();
    let job = enqueue(&mut conn, a, "test-job", Some(id), &dedup, json!({})).unwrap();
    assert_eq!(
        enqueue(&mut conn, a, "test-job", Some(id), &dedup, json!({}))
            .unwrap()
            .id,
        job.id
    );
    assert_eq!(
        count(
            &mut conn,
            "SELECT count(*)::bigint AS n FROM outbox_events WHERE job_id=$1",
            job.id
        ),
        1
    );
    let rollback_id = Uuid::new_v4().to_string();
    let rollback_result: Result<()> = conn.transaction(|c| {
        enqueue(c, a, "rollback", Some(id), &rollback_id, json!({}))?;
        Err(DbError::Conflict)
    });
    assert!(rollback_result.is_err());
    let absent = diesel::sql_query(
        "SELECT count(*)::bigint AS n FROM jobs WHERE organization_id=$1 AND dedup_key=$2",
    )
    .bind::<SqlUuid, _>(a)
    .bind::<Text, _>(&rollback_id)
    .get_result::<Count>(&mut conn)
    .unwrap()
    .n;
    assert_eq!(absent, 0);
    let lease = claim_job(&mut conn, "worker-a", &["test-job".into()], 30)
        .unwrap()
        .unwrap();
    assert_eq!(lease.id, job.id);
    assert_eq!(lease.attempts, 1);
    assert_eq!(lease.fencing, 1);
    assert!(heartbeat(&mut conn, lease.id, lease.fencing, "worker-a", 30).unwrap());
    assert!(claim_job(&mut conn, "worker-b", &["test-job".into()], 30)
        .unwrap()
        .is_none());
    diesel::sql_query("UPDATE jobs SET lease_until=now()-interval '1 second' WHERE id=$1")
        .bind::<SqlUuid, _>(lease.id)
        .execute(&mut conn)
        .unwrap();
    let recovered = claim_job(&mut conn, "worker-b", &["test-job".into()], 30)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.fencing, 2);
    assert!(!finish_job(
        &mut conn,
        lease.id,
        lease.fencing,
        "worker-a",
        "succeeded",
        None,
        0
    )
    .unwrap());
    assert!(finish_job(
        &mut conn,
        recovered.id,
        recovered.fencing,
        "worker-b",
        "succeeded",
        None,
        0
    )
    .unwrap());
    assert!(!heartbeat(&mut conn, recovered.id, recovered.fencing, "worker-b", 30).unwrap());
    // Explicit retry of uncertain delivery permits reconciliation and preserves its marker/payload.
    let uncertain = enqueue(&mut conn, a, "export", Some(id), "uncertain-delivery", json!({"marker":"stable-marker"})).unwrap();
    let attempt = claim_job(&mut conn, "delivery-worker", &["export".into()], 30).unwrap().unwrap();
    assert_eq!(attempt.id, uncertain.id);
    assert!(finish_job(&mut conn, attempt.id, attempt.fencing, "delivery-worker", "uncertain", Some("delivery_outcome_unknown"), 0).unwrap());
    assert!(!retry_job(&mut conn, b, uncertain.id).unwrap());
    assert!(retry_job(&mut conn, a, uncertain.id).unwrap());
    assert!(!retry_job(&mut conn, a, uncertain.id).unwrap());
    let reconcile = claim_job(&mut conn, "reconcile-worker", &["export".into()], 30).unwrap().unwrap();
    assert_eq!(reconcile.payload, json!({"marker":"stable-marker"}));
    assert_eq!(reconcile.attempts, 1);
    assert!(reconcile.fencing > attempt.fencing);
    assert!(finish_job(&mut conn, reconcile.id, reconcile.fencing, "reconcile-worker", "succeeded", None, 0).unwrap());
    // Composite outbox foreign key rejects linking another organization's job.
    assert!(diesel::sql_query(
        "INSERT INTO outbox_events(organization_id,job_id,payload) VALUES($1,$2,'{}')"
    )
    .bind::<SqlUuid, _>(b)
    .bind::<SqlUuid, _>(job.id)
    .execute(&mut conn)
    .is_err());

    tombstone(&mut conn, a, id, Some("synthetic-provider")).unwrap();
    assert!(delete_document(&mut conn, a, id).unwrap());
    assert_eq!(
        save_document(&mut conn, a, id, "call", None, json!({}), None).unwrap_err(),
        DbError::Conflict
    );
    assert_eq!(
        save_document(
            &mut conn,
            a,
            Uuid::new_v4(),
            "call",
            None,
            json!({"provider_id":"synthetic-provider"}),
            None
        )
        .unwrap_err(),
        DbError::Conflict
    );
    assert!(save_document(
        &mut conn,
        b,
        id,
        "call",
        None,
        json!({"provider_id":"synthetic-provider"}),
        None
    )
    .is_ok());
    for org in [a, b] {
        diesel::sql_query("DELETE FROM organizations WHERE id=$1")
            .bind::<SqlUuid, _>(org)
            .execute(&mut conn)
            .unwrap();
    }
}

fn phase20_connection() -> persistence::Connection {
    let pool = create_pool(
        &std::env::var("TEST_DATABASE_URL").expect("isolated TEST_DATABASE_URL required"),
    )
    .unwrap();
    migrate(&pool).unwrap();
    let mut conn = connection(&pool).unwrap();
    conn.begin_test_transaction().unwrap();
    conn
}
fn phase20_org(conn: &mut PgConnection) -> Uuid {
    let org = Uuid::new_v4();
    diesel::sql_query("INSERT INTO organizations(id,name) VALUES($1,'Phase20 synthetic queue')")
        .bind::<SqlUuid, _>(org)
        .execute(conn)
        .unwrap();
    org
}
fn phase20_job(conn: &mut PgConnection, org: Uuid, kind: &str, age_hours: i32) -> Job {
    let job = enqueue(
        conn,
        org,
        kind,
        None,
        &Uuid::new_v4().to_string(),
        json!({}),
    )
    .unwrap();
    diesel::sql_query("UPDATE jobs SET next_attempt_at=now()-make_interval(hours=>$2) WHERE id=$1")
        .bind::<SqlUuid, _>(job.id)
        .bind::<diesel::sql_types::Integer, _>(age_hours)
        .execute(conn)
        .unwrap();
    job
}

#[test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
fn phase20_cooldown_skips_jobs_without_attempts_and_preserves_tenant_and_expired_lease() {
    let mut conn = phase20_connection();
    let blocked = phase20_org(&mut conn);
    let foreign = phase20_org(&mut conn);
    save_document(
        &mut conn,
        blocked,
        Uuid::new_v4(),
        "connection",
        None,
        json!({"kind":"telphin","next_request_at":chrono::Utc::now().timestamp()+3600}),
        None,
    )
    .unwrap();
    let sync = phase20_job(&mut conn, blocked, "sync", 4);
    let recording = phase20_job(&mut conn, blocked, "recording", 4);
    diesel::sql_query("UPDATE jobs SET payload=jsonb_build_object('recovery_manifest_hash',repeat('a',64)) WHERE id=$1")
        .bind::<SqlUuid, _>(recording.id).execute(&mut conn).unwrap();
    diesel::sql_query("UPDATE jobs SET state='running',attempts=1,fencing=1,lease_owner='crashed',lease_until=now()-interval '1 minute' WHERE id=$1")
        .bind::<SqlUuid,_>(recording.id).execute(&mut conn).unwrap();
    let allowed = phase20_job(&mut conn, foreign, "recording", 1);
    let kinds = ["sync".into(), "recording".into()];
    let claimed = claim_job(&mut conn, "phase20", &kinds, 30)
        .unwrap()
        .unwrap();
    assert_eq!(
        claimed.id, allowed.id,
        "another tenant must proceed while this tenant cools down"
    );
    assert!(claim_job(&mut conn, "phase20", &kinds, 30)
        .unwrap()
        .is_none());
    assert_eq!(
        count(
            &mut conn,
            "SELECT attempts::bigint AS n FROM jobs WHERE id=$1",
            sync.id
        ),
        0
    );
    assert_eq!(
        count(
            &mut conn,
            "SELECT attempts::bigint AS n FROM jobs WHERE id=$1",
            recording.id
        ),
        1
    );
    assert_eq!(
        count(
            &mut conn,
            "SELECT fencing AS n FROM jobs WHERE id=$1",
            recording.id
        ),
        1
    );
    diesel::sql_query("UPDATE documents SET data=jsonb_set(data,'{next_request_at}',to_jsonb(0)) WHERE organization_id=$1 AND kind='connection'")
        .bind::<SqlUuid,_>(blocked).execute(&mut conn).unwrap();
    let resumed = claim_job(&mut conn, "phase20", &kinds, 30)
        .unwrap()
        .unwrap();
    assert_eq!(resumed.id, sync.id);
    assert!(finish_job(
        &mut conn,
        resumed.id,
        resumed.fencing,
        "phase20",
        "succeeded",
        None,
        0
    )
    .unwrap());
    let recovered = claim_job(&mut conn, "phase20", &kinds, 30)
        .unwrap()
        .unwrap();
    assert_eq!(recovered.id, recording.id);
    assert_eq!(recovered.fencing, 2);
    assert_eq!(recovered.attempts, 2);
    assert!(!finish_job(&mut conn, recording.id, 1, "crashed", "succeeded", None, 0).unwrap());
}

#[test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
fn phase20_control_and_history_are_not_starved_by_recording_backlog() {
    let mut conn = phase20_connection();
    let org = phase20_org(&mut conn);
    let recording = phase20_job(&mut conn, org, "recording", 4);
    let recovered = phase20_job(&mut conn, org, "recording", 0);
    diesel::sql_query("UPDATE jobs SET payload=jsonb_build_object('recovery_manifest_hash',repeat('a',64)) WHERE id=$1")
        .bind::<SqlUuid, _>(recovered.id).execute(&mut conn).unwrap();
    let invalid_recovery = phase20_job(&mut conn, org, "recording", 0);
    diesel::sql_query("UPDATE jobs SET payload=jsonb_build_object('recovery_manifest_hash','invalid') WHERE id=$1")
        .bind::<SqlUuid, _>(invalid_recovery.id).execute(&mut conn).unwrap();
    let future_recovery = phase20_job(&mut conn, org, "recording", -1);
    diesel::sql_query("UPDATE jobs SET payload=jsonb_build_object('recovery_manifest_hash',repeat('a',64)) WHERE id=$1")
        .bind::<SqlUuid, _>(future_recovery.id).execute(&mut conn).unwrap();
    let sync = phase20_job(&mut conn, org, "sync", 2);
    let purge = phase20_job(&mut conn, org, "purge", 1);
    let retention = phase20_job(&mut conn, org, "retention", 0);
    let future = phase20_job(&mut conn, org, "purge", -1);
    let exhausted = phase20_job(&mut conn, org, "purge", 5);
    diesel::sql_query("UPDATE jobs SET attempts=max_attempts WHERE id=$1")
        .bind::<SqlUuid, _>(exhausted.id)
        .execute(&mut conn)
        .unwrap();
    let kinds = [
        "sync".into(),
        "recording".into(),
        "purge".into(),
        "retention".into(),
    ];
    for expected in [purge.id, retention.id, sync.id, recovered.id, recording.id, invalid_recovery.id] {
        let job = claim_job(&mut conn, "phase20", &kinds, 30)
            .unwrap()
            .unwrap();
        assert_eq!(
            job.id, expected,
            "control/history work must precede bulk recordings"
        );
        assert!(finish_job(
            &mut conn,
            job.id,
            job.fencing,
            "phase20",
            "succeeded",
            None,
            0
        )
        .unwrap());
    }
    assert!(claim_job(&mut conn, "phase20", &kinds, 30)
        .unwrap()
        .is_none());
    assert_eq!(
        count(
            &mut conn,
            "SELECT attempts::bigint AS n FROM jobs WHERE id=$1",
            future.id
        ),
        0
    );
    assert_eq!(count(&mut conn,"SELECT count(*)::bigint AS n FROM jobs WHERE id=$1 AND state='failed' AND error_code='attempts_exhausted'",exhausted.id),1);
}

#[test]
#[ignore = "requires isolated PostgreSQL TEST_DATABASE_URL"]
fn phase20_cooldown_json_is_safe_and_does_not_block_other_kinds() {
    let mut conn = phase20_connection();
    for value in [
        json!(null),
        json!("9999999999999999999999"),
        json!({"bad":true}),
        json!([]),
        json!(true),
        json!(-1),
        json!(1e30),
    ] {
        let org = phase20_org(&mut conn);
        save_document(
            &mut conn,
            org,
            Uuid::new_v4(),
            "connection",
            None,
            json!({"kind":"telphin","next_request_at":value}),
            None,
        )
        .unwrap();
        let expected = phase20_job(&mut conn, org, "sync", 1);
        let job = claim_job(&mut conn, "phase20", &["sync".into()], 30)
            .unwrap()
            .unwrap();
        assert_eq!(job.id, expected.id);
        finish_job(
            &mut conn,
            job.id,
            job.fencing,
            "phase20",
            "succeeded",
            None,
            0,
        )
        .unwrap();
    }
    let org = phase20_org(&mut conn);
    save_document(
        &mut conn,
        org,
        Uuid::new_v4(),
        "connection",
        None,
        json!({"kind":"telphin","next_request_at":chrono::Utc::now().timestamp()+3600}),
        None,
    )
    .unwrap();
    let export = phase20_job(&mut conn, org, "export", 1);
    let job = claim_job(&mut conn, "phase20", &["export".into()], 30)
        .unwrap()
        .unwrap();
    assert_eq!(job.id, export.id);
}
