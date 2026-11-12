-- Pending identities are not users or tenants and cannot log into the service.
CREATE TABLE registration_challenges (
 email TEXT PRIMARY KEY CHECK(email=lower(email) AND length(email)<=254),
 id UUID NOT NULL UNIQUE,
 name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 120),
 organization_name TEXT NOT NULL CHECK(length(organization_name) BETWEEN 1 AND 160),
 password_hash TEXT NOT NULL,
 code_hash TEXT NOT NULL,
 browser_hash TEXT NOT NULL,
 expires_at TIMESTAMPTZ NOT NULL,
 attempts INTEGER NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 5),
 send_state TEXT NOT NULL CHECK(send_state IN ('sending','sent','failed')),
 sent_at TIMESTAMPTZ,
 updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX registration_expiry_idx ON registration_challenges(expires_at);
