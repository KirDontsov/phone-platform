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
