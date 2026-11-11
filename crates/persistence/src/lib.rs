//! Tenant-scoped persistence. Raw SQL binds all caller-controlled values.
use chrono::{DateTime, Utc};
pub use diesel;
pub use diesel::PgConnection;
use diesel::Connection as DieselConnection;
use diesel::{
    connection::SimpleConnection,
    prelude::*,
    r2d2::{self, ConnectionManager, PooledConnection},
    sql_types::{
        Array, BigInt, Bool, Integer, Jsonb, Nullable, Text, Timestamptz, Uuid as SqlUuid,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{fmt, time::Duration};
use uuid::Uuid;

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type Connection = PooledConnection<ConnectionManager<PgConnection>>;
pub type Result<T> = std::result::Result<T, DbError>;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DbError {
    NotFound,
    Conflict,
    Validation(&'static str),
    Database,
    Unavailable,
}
impl fmt::Display for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Validation(c) => c,
            Self::Database => "database_error",
            Self::Unavailable => "database_unavailable",
        })
    }
}
impl fmt::Debug for DbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for DbError {}
impl From<diesel::result::Error> for DbError {
    fn from(error: diesel::result::Error) -> Self {
        use diesel::result::{DatabaseErrorKind, Error};
        match error {
            Error::NotFound => Self::NotFound,
            Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _) => Self::Conflict,
            Error::DatabaseError(DatabaseErrorKind::ForeignKeyViolation, _) => {
                Self::Validation("invalid_reference")
            }
            Error::DatabaseError(DatabaseErrorKind::CheckViolation, _) => {
                Self::Validation("constraint_violation")
            }
            Error::DatabaseError(DatabaseErrorKind::SerializationFailure, _) => Self::Conflict,
            _ => Self::Database,
        }
    }
}
pub fn create_pool(url: &str) -> Result<Pool> {
    Pool::builder()
        .max_size(12)
        .connection_timeout(Duration::from_secs(10))
        .build(ConnectionManager::<PgConnection>::new(url))
        .map_err(|_| DbError::Unavailable)
}
pub fn connection(pool: &Pool) -> Result<Connection> {
    pool.get().map_err(|_| DbError::Unavailable)
}

#[derive(QueryableByName)]
struct IntRow {
    #[diesel(sql_type=Integer)]
    value: i32,
}
#[derive(QueryableByName)]
struct BoolRow {
    #[diesel(sql_type=Bool)]
    value: bool,
}

pub fn migrate(pool: &Pool) -> Result<()> {
    let mut conn = connection(pool)?;
    conn.transaction::<_,DbError,_>(|conn|{
        conn.batch_execute("SELECT pg_advisory_xact_lock(726843197); CREATE TABLE IF NOT EXISTS schema_migrations(version INTEGER PRIMARY KEY, applied_at TIMESTAMPTZ NOT NULL DEFAULT now());")?;
        let exists=diesel::sql_query("SELECT count(*)::integer AS value FROM schema_migrations WHERE version=1").get_result::<IntRow>(conn)?.value>0;
        if !exists {
            conn.batch_execute(include_str!("../migrations/0001_access/up.sql"))?;
            diesel::sql_query("INSERT INTO schema_migrations(version) VALUES(1)").execute(conn)?;
        }
        Ok(())
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, QueryableByName)]
pub struct Document {
    #[diesel(sql_type=SqlUuid)]
    pub organization_id: Uuid,
    #[diesel(sql_type=SqlUuid)]
    pub id: Uuid,
    #[diesel(sql_type=Text)]
    pub kind: String,
    #[diesel(sql_type=Nullable<SqlUuid>)]
    pub parent_id: Option<Uuid>,
    #[diesel(sql_type=Jsonb)]
    pub data: Value,
    #[diesel(sql_type=BigInt)]
    pub version: i64,
    #[diesel(sql_type=Timestamptz)]
    pub created_at: DateTime<Utc>,
    #[diesel(sql_type=Timestamptz)]
    pub updated_at: DateTime<Utc>,
}
pub fn document_json(doc: &Document) -> Value {
    let mut data = doc.data.clone();
    if let Some(m) = data.as_object_mut() {
        m.insert("id".into(), json!(doc.id));
        m.insert("version".into(), json!(doc.version));
        m.entry("created_at").or_insert(json!(doc.created_at));
    }
    data
}
pub fn get_document(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<Option<Document>> {
    diesel::sql_query("SELECT * FROM documents WHERE organization_id=$1 AND id=$2")
        .bind::<SqlUuid, _>(org)
        .bind::<SqlUuid, _>(id)
        .get_result(conn)
        .optional()
        .map_err(Into::into)
}
pub fn list_documents(conn: &mut PgConnection, org: Uuid, kind: &str) -> Result<Vec<Document>> {
    diesel::sql_query("SELECT * FROM documents WHERE organization_id=$1 AND kind=$2 ORDER BY created_at DESC,id DESC")
        .bind::<SqlUuid,_>(org).bind::<Text,_>(kind).load(conn).map_err(Into::into)
}
pub fn list_children(
    conn: &mut PgConnection,
    org: Uuid,
    parent: Uuid,
    kind: Option<&str>,
) -> Result<Vec<Document>> {
    diesel::sql_query("SELECT * FROM documents WHERE organization_id=$1 AND parent_id=$2 AND ($3::text IS NULL OR kind=$3) ORDER BY created_at,id")
        .bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(parent).bind::<Nullable<Text>,_>(kind).load(conn).map_err(Into::into)
}
/// expected_version provides optimistic concurrency. Kind and tenant never change on conflict.
pub fn save_document(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    kind: &str,
    parent: Option<Uuid>,
    data: Value,
    expected_version: Option<i64>,
) -> Result<Document> {
    if !data.is_object() || kind.is_empty() || kind.len() > 80 {
        return Err(DbError::Validation("invalid_document"));
    }
    let provider_id = data
        .get("provider_id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    diesel::sql_query("INSERT INTO documents(organization_id,id,kind,parent_id,data)
        SELECT $1,$2,$3,$4,$5 WHERE
        NOT EXISTS(SELECT 1 FROM deletion_tombstones t WHERE t.organization_id=$1 AND
            (t.subject_id=$2 OR ($3 IN ('call','calls') AND $7::text IS NOT NULL AND t.provider_id=$7)))
        AND ($6::bigint IS NULL OR EXISTS(SELECT 1 FROM documents WHERE organization_id=$1 AND id=$2 AND version=$6))
        ON CONFLICT(organization_id,id) DO UPDATE SET data=EXCLUDED.data,parent_id=EXCLUDED.parent_id,
            version=documents.version+1,updated_at=now()
        WHERE documents.kind=EXCLUDED.kind AND ($6::bigint IS NULL OR documents.version=$6)
        RETURNING documents.*")
        .bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(id).bind::<Text,_>(kind)
        .bind::<Nullable<SqlUuid>,_>(parent).bind::<Jsonb,_>(data).bind::<Nullable<BigInt>,_>(expected_version)
        .bind::<Nullable<Text>,_>(provider_id).get_result(conn)
        .map_err(|e| if e==diesel::result::Error::NotFound {DbError::Conflict}else{e.into()})
}
pub fn delete_document(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<bool> {
    diesel::sql_query("DELETE FROM documents WHERE organization_id=$1 AND id=$2")
        .bind::<SqlUuid, _>(org)
        .bind::<SqlUuid, _>(id)
        .execute(conn)
        .map(|v| v > 0)
        .map_err(Into::into)
}
pub fn tombstone(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    provider_id: Option<&str>,
) -> Result<()> {
    diesel::sql_query("INSERT INTO deletion_tombstones(organization_id,subject_id,provider_id) VALUES($1,$2,$3) ON CONFLICT(organization_id,subject_id) DO NOTHING")
        .bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(id).bind::<Nullable<Text>,_>(provider_id).execute(conn)?;
    Ok(())
}
pub fn is_tombstoned(
    conn: &mut PgConnection,
    org: Uuid,
    id: Uuid,
    provider_id: Option<&str>,
) -> Result<bool> {
    let row=diesel::sql_query("SELECT EXISTS(SELECT 1 FROM deletion_tombstones WHERE organization_id=$1 AND (subject_id=$2 OR ($3::text IS NOT NULL AND provider_id=$3))) AS value")
        .bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(id).bind::<Nullable<Text>,_>(provider_id).get_result::<BoolRow>(conn)?;
    Ok(row.value)
}
pub fn append_audit(
    conn: &mut PgConnection,
    org: Uuid,
    actor: Option<Uuid>,
    event: &str,
    subject: Option<Uuid>,
    details: Value,
) -> Result<Document> {
    // Callers pass redacted metadata, never token/provider payload/transcript bodies.
    save_document(
        conn,
        org,
        Uuid::new_v4(),
        "audit",
        None,
        json!({"actor_id":actor,"event":event,"subject_id":subject,"details":details,"at":Utc::now()}),
        None,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize, QueryableByName)]
pub struct Job {
    #[diesel(sql_type=SqlUuid)]
    pub id: Uuid,
    #[diesel(sql_type=SqlUuid)]
    pub organization_id: Uuid,
    #[diesel(sql_type=Text)]
    pub kind: String,
    #[diesel(sql_type=Nullable<SqlUuid>)]
    pub subject_id: Option<Uuid>,
    #[diesel(sql_type=Text)]
    pub dedup_key: String,
    #[diesel(sql_type=Jsonb)]
    pub payload: Value,
    #[diesel(sql_type=Text)]
    pub state: String,
    #[diesel(sql_type=Integer)]
    pub attempts: i32,
    #[diesel(sql_type=Integer)]
    pub max_attempts: i32,
    #[diesel(sql_type=Nullable<Text>)]
    pub lease_owner: Option<String>,
    #[diesel(sql_type=Nullable<Timestamptz>)]
    pub lease_until: Option<DateTime<Utc>>,
    #[diesel(sql_type=BigInt)]
    pub fencing: i64,
    #[diesel(sql_type=Timestamptz)]
    pub next_attempt_at: DateTime<Utc>,
    #[diesel(sql_type=Nullable<Text>)]
    pub error_code: Option<String>,
    #[diesel(sql_type=Timestamptz)]
    pub created_at: DateTime<Utc>,
    #[diesel(sql_type=Timestamptz)]
    pub updated_at: DateTime<Utc>,
}
pub fn enqueue(
    conn: &mut PgConnection,
    org: Uuid,
    kind: &str,
    subject: Option<Uuid>,
    dedup_key: &str,
    payload: Value,
) -> Result<Job> {
    if kind.is_empty()
        || kind.len() > 80
        || dedup_key.is_empty()
        || dedup_key.len() > 300
        || !payload.is_object()
    {
        return Err(DbError::Validation("invalid_job"));
    }
    conn.transaction::<_,DbError,_>(|conn| {
        let job=diesel::sql_query("INSERT INTO jobs(organization_id,kind,subject_id,dedup_key,payload) VALUES($1,$2,$3,$4,$5)
            ON CONFLICT(organization_id,kind,dedup_key) DO UPDATE SET dedup_key=EXCLUDED.dedup_key RETURNING *")
            .bind::<SqlUuid,_>(org).bind::<Text,_>(kind).bind::<Nullable<SqlUuid>,_>(subject)
            .bind::<Text,_>(dedup_key).bind::<Jsonb,_>(payload).get_result::<Job>(conn)?;
        diesel::sql_query("INSERT INTO outbox_events(organization_id,job_id,payload) VALUES($1,$2,$3) ON CONFLICT(job_id) DO NOTHING")
            .bind::<SqlUuid,_>(org).bind::<SqlUuid,_>(job.id)
            .bind::<Jsonb,_>(json!({"job_id":job.id,"organization_id":org,"kind":kind})).execute(conn)?;
        Ok(job)
    })
}
/// SKIP LOCKED + atomically incremented fencing permits concurrent workers and crash recovery.
pub fn claim_job(
    conn: &mut PgConnection,
    worker: &str,
    kinds: &[String],
    lease_seconds: i32,
) -> Result<Option<Job>> {
    if worker.is_empty() || !(5..=3600).contains(&lease_seconds) {
        return Err(DbError::Validation("invalid_lease"));
    }
    // Exhausted crashed attempts become dead jobs, rather than remaining running forever.
    diesel::sql_query("UPDATE jobs SET state='failed',error_code='attempts_exhausted',lease_owner=NULL,lease_until=NULL,updated_at=now()
        WHERE attempts>=max_attempts AND ((state='running' AND lease_until<now()) OR state IN ('queued','retry'))").execute(conn)?;
    diesel::sql_query("WITH candidate AS (
          SELECT id FROM jobs WHERE attempts<max_attempts AND (cardinality($2::text[])=0 OR kind=ANY($2))
          AND ((state IN ('queued','retry') AND next_attempt_at<=now()) OR (state='running' AND lease_until<now()))
          ORDER BY next_attempt_at,created_at FOR UPDATE SKIP LOCKED LIMIT 1
        ) UPDATE jobs j SET state='running',attempts=j.attempts+1,lease_owner=$1,
          lease_until=now()+make_interval(secs=>$3),fencing=j.fencing+1,updated_at=now()
          FROM candidate c WHERE j.id=c.id RETURNING j.*")
        .bind::<Text,_>(worker).bind::<Array<Text>,_>(kinds).bind::<Integer,_>(lease_seconds)
        .get_result(conn).optional().map_err(Into::into)
}
pub fn heartbeat(
    conn: &mut PgConnection,
    id: Uuid,
    fencing: i64,
    worker: &str,
    lease_seconds: i32,
) -> Result<bool> {
    if !(5..=3600).contains(&lease_seconds) {
        return Err(DbError::Validation("invalid_lease"));
    }
    diesel::sql_query(
        "UPDATE jobs SET lease_until=now()+make_interval(secs=>$4),updated_at=now()
        WHERE id=$1 AND fencing=$2 AND lease_owner=$3 AND state='running' AND lease_until>now()",
    )
    .bind::<SqlUuid, _>(id)
    .bind::<BigInt, _>(fencing)
    .bind::<Text, _>(worker)
    .bind::<Integer, _>(lease_seconds)
    .execute(conn)
    .map(|n| n > 0)
    .map_err(Into::into)
}
pub fn finish_job(
    conn: &mut PgConnection,
    id: Uuid,
    fencing: i64,
    worker: &str,
    state: &str,
    error: Option<&str>,
    delay_seconds: i32,
) -> Result<bool> {
    if !["succeeded", "failed", "retry", "uncertain"].contains(&state)
        || delay_seconds < 0
        || error.is_some_and(|s| {
            s.len() > 120
                || !s
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
    {
        return Err(DbError::Validation("invalid_job_finish"));
    }
    diesel::sql_query("UPDATE jobs SET state=CASE WHEN $4='retry' AND attempts>=max_attempts THEN 'failed' ELSE $4 END,
        error_code=$5,next_attempt_at=now()+make_interval(secs=>$6),lease_owner=NULL,lease_until=NULL,updated_at=now()
        WHERE id=$1 AND fencing=$2 AND lease_owner=$3 AND state='running' AND lease_until>now()")
        .bind::<SqlUuid,_>(id).bind::<BigInt,_>(fencing).bind::<Text,_>(worker).bind::<Text,_>(state)
        .bind::<Nullable<Text>,_>(error).bind::<Integer,_>(delay_seconds).execute(conn).map(|n|n>0).map_err(Into::into)
}
pub fn list_jobs(conn: &mut PgConnection, org: Uuid, limit: i64) -> Result<Vec<Job>> {
    diesel::sql_query(
        "SELECT * FROM jobs WHERE organization_id=$1 ORDER BY created_at DESC LIMIT $2",
    )
    .bind::<SqlUuid, _>(org)
    .bind::<BigInt, _>(limit.clamp(1, 1000))
    .load(conn)
    .map_err(Into::into)
}
pub fn retry_job(conn: &mut PgConnection, org: Uuid, id: Uuid) -> Result<bool> {
    diesel::sql_query(
        "UPDATE jobs SET state='queued',attempts=0,next_attempt_at=now(),error_code=NULL,
        lease_owner=NULL,lease_until=NULL,fencing=fencing+1,updated_at=now()
        WHERE organization_id=$1 AND id=$2 AND state IN ('failed','retry','uncertain')",
    )
    .bind::<SqlUuid, _>(org)
    .bind::<SqlUuid, _>(id)
    .execute(conn)
    .map(|n| n > 0)
    .map_err(Into::into)
}
#[derive(Debug, Clone, Serialize, Deserialize, QueryableByName)]
pub struct OutboxEvent {
    #[diesel(sql_type=SqlUuid)]
    pub id: Uuid,
    #[diesel(sql_type=SqlUuid)]
    pub organization_id: Uuid,
    #[diesel(sql_type=SqlUuid)]
    pub job_id: Uuid,
    #[diesel(sql_type=Jsonb)]
    pub payload: Value,
    #[diesel(sql_type=Nullable<Timestamptz>)]
    pub published_at: Option<DateTime<Utc>>,
    #[diesel(sql_type=Integer)]
    pub attempts: i32,
    #[diesel(sql_type=Timestamptz)]
    pub created_at: DateTime<Utc>,
}
pub fn pending_outbox(conn: &mut PgConnection, limit: i64) -> Result<Vec<OutboxEvent>> {
    diesel::sql_query(
        "SELECT * FROM outbox_events WHERE published_at IS NULL ORDER BY created_at LIMIT $1",
    )
    .bind::<BigInt, _>(limit.clamp(1, 1000))
    .load(conn)
    .map_err(Into::into)
}
pub fn mark_outbox_published(conn: &mut PgConnection, id: Uuid) -> Result<bool> {
    diesel::sql_query("UPDATE outbox_events SET published_at=now(),attempts=attempts+1 WHERE id=$1 AND published_at IS NULL")
        .bind::<SqlUuid,_>(id).execute(conn).map(|n|n>0).map_err(Into::into)
}
