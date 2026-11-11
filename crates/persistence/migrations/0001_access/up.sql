CREATE TABLE organizations (
 id UUID PRIMARY KEY DEFAULT gen_random_uuid(), name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 160),
 timezone TEXT NOT NULL DEFAULT 'Europe/Moscow', callback_minutes INTEGER NOT NULL DEFAULT 120 CHECK(callback_minutes BETWEEN 1 AND 43200),
 retention_days INTEGER NOT NULL DEFAULT 90 CHECK(retention_days BETWEEN 1 AND 3650),
 data JSONB NOT NULL DEFAULT '{}' CHECK(jsonb_typeof(data)='object'), created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE TABLE users (
 id UUID PRIMARY KEY DEFAULT gen_random_uuid(), email TEXT NOT NULL UNIQUE, name TEXT NOT NULL,
 password_hash TEXT NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(), CHECK(email=lower(email))
);
CREATE TABLE memberships (
 organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
 user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
 role TEXT NOT NULL CHECK(role IN ('owner','manager','reviewer')), extension TEXT,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), PRIMARY KEY(organization_id,user_id)
);
CREATE TABLE sessions (
 id UUID PRIMARY KEY DEFAULT gen_random_uuid(), token_hash TEXT NOT NULL UNIQUE,
 user_id UUID NOT NULL, organization_id UUID NOT NULL, csrf_token TEXT NOT NULL,
 expires_at TIMESTAMPTZ NOT NULL, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 FOREIGN KEY(organization_id,user_id) REFERENCES memberships(organization_id,user_id) ON DELETE CASCADE
);
CREATE INDEX sessions_user_idx ON sessions(user_id);
CREATE INDEX sessions_expiry_idx ON sessions(expires_at);
CREATE TABLE token_grants (
 id UUID PRIMARY KEY DEFAULT gen_random_uuid(), token_hash TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL CHECK(kind IN ('reset','invite','oauth')),
 organization_id UUID REFERENCES organizations(id) ON DELETE CASCADE,
 user_id UUID REFERENCES users(id) ON DELETE CASCADE, email TEXT,
 role TEXT CHECK(role IS NULL OR role IN ('owner','manager','reviewer')),
 payload JSONB NOT NULL DEFAULT '{}' CHECK(jsonb_typeof(payload)='object'),
 expires_at TIMESTAMPTZ NOT NULL, consumed_at TIMESTAMPTZ,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX grants_expiry_idx ON token_grants(expires_at);
CREATE TABLE documents (
 organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
 id UUID NOT NULL, kind TEXT NOT NULL CHECK(length(kind) BETWEEN 1 AND 80), parent_id UUID,
 data JSONB NOT NULL CHECK(jsonb_typeof(data)='object'), version BIGINT NOT NULL DEFAULT 1 CHECK(version>0),
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,id),
 FOREIGN KEY(organization_id,parent_id) REFERENCES documents(organization_id,id) ON DELETE CASCADE,
 CHECK(parent_id IS NULL OR parent_id<>id)
);
CREATE INDEX documents_kind_idx ON documents(organization_id,kind,created_at DESC,id);
CREATE INDEX documents_parent_idx ON documents(organization_id,parent_id,kind);
CREATE INDEX documents_data_idx ON documents USING GIN(data jsonb_path_ops);
CREATE INDEX calls_started_idx ON documents(organization_id,(data->>'started_at') DESC,id DESC) WHERE kind IN ('call','calls');
CREATE UNIQUE INDEX connection_kind_unique ON documents(organization_id,(data->>'kind')) WHERE kind IN ('connection','connections');
CREATE TABLE jobs (
 id UUID PRIMARY KEY DEFAULT gen_random_uuid(), organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
 kind TEXT NOT NULL, subject_id UUID, dedup_key TEXT NOT NULL, payload JSONB NOT NULL DEFAULT '{}',
 state TEXT NOT NULL DEFAULT 'queued' CHECK(state IN ('queued','running','retry','succeeded','failed','uncertain','cancelled')),
 attempts INTEGER NOT NULL DEFAULT 0, max_attempts INTEGER NOT NULL DEFAULT 5 CHECK(max_attempts BETWEEN 1 AND 20),
 lease_owner TEXT, lease_until TIMESTAMPTZ, fencing BIGINT NOT NULL DEFAULT 0,
 next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(), error_code TEXT,
 created_at TIMESTAMPTZ NOT NULL DEFAULT now(), updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(organization_id,kind,dedup_key), UNIQUE(organization_id,id),
 FOREIGN KEY(organization_id,subject_id) REFERENCES documents(organization_id,id) ON DELETE CASCADE
);
CREATE INDEX jobs_claim_idx ON jobs(state,next_attempt_at,lease_until);
CREATE TABLE outbox_events (
 id UUID PRIMARY KEY DEFAULT gen_random_uuid(), organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
 job_id UUID NOT NULL, payload JSONB NOT NULL,
 published_at TIMESTAMPTZ, attempts INTEGER NOT NULL DEFAULT 0, created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 UNIQUE(job_id), FOREIGN KEY(organization_id,job_id) REFERENCES jobs(organization_id,id) ON DELETE CASCADE
);
CREATE INDEX outbox_pending_idx ON outbox_events(created_at) WHERE published_at IS NULL;
CREATE TABLE deletion_tombstones (
 organization_id UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
 subject_id UUID NOT NULL, provider_id TEXT, deleted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
 PRIMARY KEY(organization_id,subject_id)
);
CREATE INDEX tombstones_provider_idx ON deletion_tombstones(organization_id,provider_id);
