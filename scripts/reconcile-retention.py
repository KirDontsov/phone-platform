#!/usr/bin/env python3
"""Review/apply an exact private inventory of accidental retention soft deletions."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import subprocess
import uuid


def load_manifest(path):
    spec = importlib.util.spec_from_file_location('phone_private_files', Path(__file__).with_name('connect-routerai.py'))
    private = importlib.util.module_from_spec(spec); spec.loader.exec_module(private)
    value = private.private_read(path, limit=32*1024*1024)
    if not isinstance(value, dict): raise ValueError('invalid_manifest')
    org = str(uuid.UUID(value['organization_id']))
    entries = value.get('targets')
    if not isinstance(entries, list) or not 1 <= len(entries) <= 100000: raise ValueError('invalid_target_count')
    targets = []
    seen = set()
    for entry in entries:
        if entry.get('purged') is not False or entry.get('tombstone') or entry.get('manual_audit') or entry.get('manual_purge'): continue
        ids = tuple(str(uuid.UUID(entry[key])) for key in ['call_id','purge_job_id','retention_job_id'])
        if ids[0] in seen: raise ValueError('duplicate_call_target')
        seen.add(ids[0]); targets.append(ids)
    if not targets: raise ValueError('no_soft_deleted_targets')
    return org, targets


def recovery_sql(org, targets, apply=False):
    org = str(uuid.UUID(org))
    rows = ','.join('('+','.join("'"+str(uuid.UUID(v))+"'" for v in row)+')' for row in targets)
    receipt = str(uuid.uuid4())
    return f"""BEGIN ISOLATION LEVEL SERIALIZABLE;
SET LOCAL lock_timeout='5s'; SET LOCAL statement_timeout='120s';
DO $$ BEGIN
 PERFORM 1 FROM organizations WHERE id='{org}' FOR UPDATE;
 IF NOT FOUND THEN RAISE EXCEPTION 'organization_missing'; END IF;
 IF EXISTS(SELECT 1 FROM organizations WHERE id='{org}' AND data->'retention_enabled'='true'::jsonb)
 THEN RAISE EXCEPTION 'retention_enabled_recovery_blocked'; END IF;
END $$;
CREATE TEMP TABLE recovery_manifest(call_id uuid PRIMARY KEY,purge_job_id uuid UNIQUE,retention_job_id uuid) ON COMMIT DROP;
INSERT INTO recovery_manifest VALUES {rows};
CREATE TEMP TABLE recovery_eligible ON COMMIT DROP AS
SELECT c.id AS call_id,j.id AS job_id FROM recovery_manifest m
JOIN documents c ON c.organization_id='{org}' AND c.id=m.call_id AND c.kind='call'
JOIN jobs j ON j.organization_id=c.organization_id AND j.id=m.purge_job_id AND j.subject_id=c.id AND j.kind='purge'
JOIN jobs parent ON parent.organization_id=c.organization_id AND parent.id=m.retention_job_id AND parent.kind='retention'
WHERE j.dedup_key='retention:'||c.id::text||':'||parent.id::text
AND (j.state IN ('queued','retry') OR (j.state='running' AND j.lease_until<=now()))
AND c.data->'deleted'='true'::jsonb AND c.data->'purged' IS DISTINCT FROM 'true'::jsonb
AND jsonb_typeof(c.data->'started_at')='string' AND jsonb_typeof(c.data->'provider_id')='string'
AND btrim(c.data->>'provider_id')<>''
AND c.data->>'started_at' ~ '^[0-9]{{4}}-[0-9]{{2}}-[0-9]{{2}}T'
AND pg_input_is_valid(c.data->>'started_at','timestamp with time zone')
AND NOT EXISTS(SELECT 1 FROM deletion_tombstones t WHERE t.organization_id=c.organization_id AND (t.subject_id=c.id OR t.provider_id=c.data->>'provider_id'))
AND NOT EXISTS(SELECT 1 FROM documents a WHERE a.organization_id=c.organization_id AND a.kind='audit' AND (a.data->>'event'='call.deleted' OR a.data->>'action'='call.deleted') AND a.data->>'subject_id'=c.id::text)
AND NOT EXISTS(SELECT 1 FROM jobs manual WHERE manual.organization_id=c.organization_id AND manual.kind='purge' AND manual.subject_id=c.id AND manual.dedup_key NOT LIKE 'retention:%');
CREATE TEMP TABLE recovery_counts(restored bigint,cancelled bigint) ON COMMIT DROP;
WITH restored AS (
 UPDATE documents c SET data=jsonb_set(c.data-'deletion_origin'-'retention_job_id','{{deleted}}','false'::jsonb),version=version+1,updated_at=now()
 FROM recovery_eligible e WHERE c.organization_id='{org}' AND c.id=e.call_id RETURNING c.id
), cancelled AS (
 UPDATE jobs j SET state='cancelled',error_code='retention_disabled',lease_owner=NULL,lease_until=NULL,fencing=fencing+1,updated_at=now()
 FROM recovery_eligible e WHERE j.organization_id='{org}' AND j.id=e.job_id RETURNING j.id
) INSERT INTO recovery_counts SELECT (SELECT count(*) FROM restored),(SELECT count(*) FROM cancelled);
INSERT INTO documents(organization_id,id,kind,data)
SELECT '{org}','{receipt}','audit',jsonb_build_object('event','retention.reconciled','actor_id',NULL,'at',now(),'details',jsonb_build_object('restored',restored,'cancelled',cancelled,'source','phase20_exact_inventory')) FROM recovery_counts;
SELECT json_build_object('applied',{'true' if apply else 'false'},'targets',(SELECT count(*) FROM recovery_manifest),'restored',restored,'cancelled',cancelled) FROM recovery_counts;
{'COMMIT' if apply else 'ROLLBACK'};
"""


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest',type=Path,required=True)
    parser.add_argument('--server',required=True)
    parser.add_argument('--apply',action='store_true')
    args=parser.parse_args()
    if not re.fullmatch(r'[a-zA-Z0-9_.-]+@[a-zA-Z0-9.-]+',args.server): raise ValueError('invalid_server')
    org,targets=load_manifest(args.manifest)
    result=subprocess.run(['ssh','-o','BatchMode=yes',args.server,'docker exec -i phone-platform-db-1 psql -U phone -d phone_platform -X -Atq -v ON_ERROR_STOP=1'],input=recovery_sql(org,targets,args.apply),text=True,capture_output=True)
    if result.returncode: raise ValueError('recovery_transaction_failed')
    receipts=[json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
    if len(receipts)!=1: raise ValueError('invalid_recovery_receipt')
    print(json.dumps(receipts[0]))

if __name__=='__main__':
    try: main()
    except Exception:
        print(json.dumps({'state':'error','error':'retention_recovery_failed'}))
        raise SystemExit(1)
