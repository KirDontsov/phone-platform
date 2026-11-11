#!/usr/bin/env python3
"""Consistent operator backup and isolated restore verification for this Compose project."""
import argparse, base64, datetime, hashlib, json, os, pathlib, re, secrets, subprocess, sys, tarfile, uuid
ROOT=pathlib.Path(__file__).resolve().parent.parent

def configuration(path):
    p=pathlib.Path(path)
    if not p.is_file() or p.is_symlink() or p.stat().st_mode&0o077:
        raise RuntimeError('private_env_required')
    return dict(line.split('=',1)for line in p.read_text().splitlines()if line and not line.startswith('#'))

def invoke(command,**kw):
    result=subprocess.run(command,stderr=subprocess.PIPE,**kw)
    if result.returncode:raise RuntimeError('backup_command_failed')
    return result

def compose(env):return ['docker','compose','--env-file',str(env),'-f',str(ROOT/'infra/compose.yml')]
def db_command(env,args):return compose(env)+['exec','-T','db']+args

def digest(path):
    h=hashlib.sha256()
    with path.open('rb')as f:
        for block in iter(lambda:f.read(1024*1024),b''):h.update(block)
    return h.hexdigest()
def volume_name(env,service,target):
    container=invoke(compose(env)+['ps','-a','-q',service],stdout=subprocess.PIPE,text=True).stdout.strip()
    info=json.loads(invoke(['docker','inspect',container],stdout=subprocess.PIPE,text=True).stdout)[0]
    return next(m['Name']for m in info['Mounts']if m['Destination']==target and m['Type']=='volume')
def write_private(path,content):
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,'w')as f:f.write(content)

def backup(env,directory):
    config=configuration(env);directory=directory.absolute()
    if directory.exists() or directory.is_symlink():raise RuntimeError('backup_directory_must_be_new')
    directory.mkdir(mode=0o700,parents=True);os.chmod(directory,0o700)
    running=invoke(compose(env)+['ps','--services','--status','running'],stdout=subprocess.PIPE,text=True).stdout.split()
    services=[s for s in ['worker','api']if s in running]
    try:
        if services:invoke(compose(env)+['stop',*services],stdout=subprocess.PIPE)
        dump=directory/'database.dump'
        fd=os.open(dump,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
        with os.fdopen(fd,'wb')as f:
            invoke(db_command(env,['pg_dump','-U',config['POSTGRES_USER'],'-d',config['POSTGRES_DB'],'-Fc','--no-owner','--no-acl']),stdout=f)
        counts=invoke(db_command(env,['psql','-U',config['POSTGRES_USER'],'-d',config['POSTGRES_DB'],'-Atc',"SELECT json_build_object('organizations',(SELECT count(*) FROM organizations),'users',(SELECT count(*) FROM users),'documents',(SELECT count(*) FROM documents),'jobs',(SELECT count(*) FROM jobs))"]),stdout=subprocess.PIPE,text=True).stdout.strip()
        volume=volume_name(env,'api','/app/media')
        media=directory/'media.tar.gz';fd=os.open(media,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
        with os.fdopen(fd,'wb')as f:invoke(['docker','run','--rm','-v',volume+':/source:ro','busybox:1.37','tar','-czf','-','-C','/source','.'],stdout=f)
        manifest={'version':1,'project':config.get('COMPOSE_PROJECT_NAME','phone-platform'),'sha256':{p.name:digest(p)for p in [dump,media]},'counts':json.loads(counts),'key_fingerprint':hashlib.sha256(config['ENCRYPTION_KEY'].encode()).hexdigest(),'quiesced_services':services,'restore_requires_original_encryption_key':True}
        write_private(directory/'manifest.json',json.dumps(manifest,indent=2))
        print(json.dumps({'backup_created':True,'counts':manifest['counts']}))
    finally:
        if services:invoke(compose(env)+['start',*services],stdout=subprocess.PIPE)

COUNTS_SQL = "SELECT json_build_object('organizations',(SELECT count(*) FROM organizations),'users',(SELECT count(*) FROM users),'documents',(SELECT count(*) FROM documents),'jobs',(SELECT count(*) FROM jobs))"
LEDGER_SQL = "SELECT COALESCE(json_agg(json_build_object('organization_id',organization_id,'subject_id',subject_id,'provider_id',provider_id,'deleted_at',deleted_at) ORDER BY organization_id,subject_id),'[]'::json) FROM deletion_tombstones"

def psql(env,config,database,sql):
    return invoke(db_command(env,['psql','-X','-q','-A','-t','-U',config['POSTGRES_USER'],'-d',database,'-v','ON_ERROR_STOP=1']),input=sql,text=True,stdout=subprocess.PIPE).stdout.strip()

def validated_ledger(value):
    if not isinstance(value,dict)or type(value.get('version'))is not int or value.get('version')!=1 or set(value)!={'version','exported_at','entries'}:raise RuntimeError('invalid_deletion_ledger')
    def timestamp(raw):
        if not isinstance(raw,str)or len(raw)>64:raise RuntimeError('invalid_deletion_ledger')
        try:parsed=datetime.datetime.fromisoformat(raw.replace('Z','+00:00'))
        except ValueError:raise RuntimeError('invalid_deletion_ledger')from None
        if parsed.tzinfo is None:raise RuntimeError('invalid_deletion_ledger')
        return parsed.isoformat()
    exported=timestamp(value['exported_at']);entries=value['entries']
    if not isinstance(entries,list)or len(entries)>1000000:raise RuntimeError('invalid_deletion_ledger')
    seen={};clean=[]
    for entry in entries:
        if not isinstance(entry,dict)or set(entry)!={'organization_id','subject_id','provider_id','deleted_at'}:raise RuntimeError('invalid_deletion_ledger')
        row={}
        for field in ['organization_id','subject_id']:
            raw=entry[field]
            if not isinstance(raw,str)or len(raw)!=36:raise RuntimeError('invalid_deletion_ledger')
            try:row[field]=str(uuid.UUID(raw))
            except ValueError:raise RuntimeError('invalid_deletion_ledger')from None
            if row[field]!=raw.lower():raise RuntimeError('invalid_deletion_ledger')
        provider=entry['provider_id']
        if provider is not None and(not isinstance(provider,str)or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_.:@/+==\-]{0,511}',provider)):raise RuntimeError('invalid_deletion_ledger')
        row['provider_id']=provider;row['deleted_at']=timestamp(entry['deleted_at'])
        key=(row['organization_id'],row['subject_id'])
        if key in seen:
            if seen[key]!=row:raise RuntimeError('conflicting_deletion_ledger')
            continue
        seen[key]=row;clean.append(row)
    return {'version':1,'exported_at':exported,'entries':clean}

def current_ledger(env,config):
    has_restored=psql(env,config,config['POSTGRES_DB'],"SELECT to_regclass('public.restore_deletion_ledger') IS NOT NULL;")=='t'
    query=LEDGER_SQL
    if has_restored:
        query="SELECT COALESCE(json_agg(json_build_object('organization_id',organization_id,'subject_id',subject_id,'provider_id',provider_id,'deleted_at',deleted_at) ORDER BY organization_id,subject_id),'[]'::json) FROM (SELECT DISTINCT ON(organization_id,subject_id) * FROM (SELECT * FROM deletion_tombstones UNION ALL SELECT * FROM restore_deletion_ledger) all_entries ORDER BY organization_id,subject_id,deleted_at DESC) latest"
    entries=json.loads(psql(env,config,config['POSTGRES_DB'],query))
    return validated_ledger({'version':1,'exported_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'entries':entries})

def load_ledger(path):
    if path.is_symlink()or not path.is_file()or path.stat().st_mode&0o077 or path.stat().st_size>64*1024*1024:raise RuntimeError('private_deletion_ledger_required')
    return validated_ledger(json.loads(path.read_text()))

def export_ledger(env,path):
    config=configuration(env);ledger=current_ledger(env,config)
    path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    write_private(path,json.dumps(ledger,indent=2)+'\n')
    print(json.dumps({'deletion_ledger_exported':True,'entries':len(ledger['entries'])}))

REPLAY_SQL = r"""
BEGIN;
CREATE TEMP TABLE replay_input(encoded TEXT) ON COMMIT DROP;
COPY replay_input FROM STDIN;
{payload}
\.
CREATE TEMP TABLE incoming_ledger ON COMMIT DROP AS
 SELECT x.* FROM replay_input p CROSS JOIN LATERAL jsonb_to_recordset(convert_from(decode(p.encoded,'base64'),'UTF8')::jsonb)
 AS x(organization_id UUID,subject_id UUID,provider_id TEXT,deleted_at TIMESTAMPTZ);
CREATE TABLE IF NOT EXISTS restore_deletion_ledger(
 organization_id UUID NOT NULL,subject_id UUID NOT NULL,provider_id TEXT,deleted_at TIMESTAMPTZ NOT NULL,
 PRIMARY KEY(organization_id,subject_id));
INSERT INTO restore_deletion_ledger SELECT * FROM deletion_tombstones WHERE true
 ON CONFLICT(organization_id,subject_id) DO NOTHING;
INSERT INTO restore_deletion_ledger SELECT * FROM incoming_ledger WHERE true
 ON CONFLICT(organization_id,subject_id) DO UPDATE SET
 provider_id=COALESCE(EXCLUDED.provider_id,restore_deletion_ledger.provider_id),
 deleted_at=GREATEST(EXCLUDED.deleted_at,restore_deletion_ledger.deleted_at);
INSERT INTO deletion_tombstones SELECT l.* FROM restore_deletion_ledger l JOIN organizations o ON o.id=l.organization_id WHERE true
 ON CONFLICT(organization_id,subject_id) DO UPDATE SET
 provider_id=COALESCE(EXCLUDED.provider_id,deletion_tombstones.provider_id),
 deleted_at=GREATEST(EXCLUDED.deleted_at,deletion_tombstones.deleted_at);
CREATE TEMP TABLE purge_documents ON COMMIT DROP AS
 WITH RECURSIVE targets AS(
  SELECT d.organization_id,d.id FROM documents d JOIN restore_deletion_ledger t ON t.organization_id=d.organization_id
   AND(t.subject_id=d.id OR(t.provider_id IS NOT NULL AND t.provider_id=d.data->>'provider_id')) WHERE d.kind='call'
  UNION
  SELECT d.organization_id,d.id FROM documents d JOIN targets p ON d.organization_id=p.organization_id AND d.parent_id=p.id
 ) SELECT * FROM targets;
CREATE TEMP TABLE purge_media ON COMMIT DROP AS
 SELECT d.organization_id,d.data->>'path' AS path FROM documents d JOIN purge_documents p USING(organization_id,id)
 WHERE d.kind='recording' AND d.data->>'path' IS NOT NULL;
DELETE FROM documents d USING purge_documents p WHERE d.organization_id=p.organization_id AND d.id=p.id;
SELECT json_build_object('removed_documents',(SELECT count(*) FROM purge_documents),
 'ledger_entries',(SELECT count(*) FROM restore_deletion_ledger),
 'media',COALESCE((SELECT json_agg(json_build_object('organization_id',organization_id,'path',path)) FROM purge_media),'[]'::json),
 'resurrected_calls',(SELECT count(*) FROM documents d JOIN restore_deletion_ledger t ON t.organization_id=d.organization_id
 AND(t.subject_id=d.id OR(t.provider_id IS NOT NULL AND t.provider_id=d.data->>'provider_id')) WHERE d.kind='call'));
COMMIT;
"""

def replay_ledger(env,config,database,ledger):
    clean=validated_ledger(ledger)
    # A single base64 COPY field carries JSON as data; no provider IDs or user text enter SQL syntax.
    payload=base64.b64encode(json.dumps(clean['entries'],separators=(',',':')).encode()).decode()
    result=json.loads(psql(env,config,database,REPLAY_SQL.format(payload=payload)))
    if result['resurrected_calls']!=0:raise RuntimeError('deletion_ledger_replay_failed')
    return result

def media_path(row):
    if not isinstance(row,dict):raise RuntimeError('invalid_media_reference')
    org=row.get('organization_id');raw=row.get('path')
    try:valid_org=isinstance(org,str)and str(uuid.UUID(org))==org
    except ValueError:valid_org=False
    if not valid_org or not isinstance(raw,str)or len(raw)>4096 or any(ord(c)<32 or ord(c)==127 for c in raw):raise RuntimeError('invalid_media_reference')
    path=pathlib.PurePosixPath(raw)
    if path.is_absolute()or len(path.parts)<2 or '..'in path.parts or path.parts[0]!=org:raise RuntimeError('invalid_media_reference')
    return str(path)

MEDIA_PLAN_SH = r'''
set -eu
while IFS="$(printf '\t')" read -r action relative; do
 case "$action" in
  D) rm -f -- "/restore/$relative"; test ! -e "/restore/$relative" ;;
  F) test -f "/restore/$relative" ;;
  *) exit 1 ;;
 esac
done
'''

def apply_media_plan(volume,deleted,ready):
    ready_paths={media_path(row)for row in ready}
    deleted_paths={media_path(row)for row in deleted}
    # A physical recording can have multiple live references (e.g. shared synthetic demo WAV).
    # Removing one subject must not remove an artifact still required by a retained call.
    remove_paths=deleted_paths-ready_paths
    plan=''.join('D\t'+path+'\n'for path in sorted(remove_paths))+''.join('F\t'+path+'\n'for path in sorted(ready_paths))
    # One isolated helper handles every path. Paths arrive on stdin and are always shell-quoted.
    invoke(['docker','run','--rm','--network','none','-i','-v',volume+':/restore','busybox:1.37','sh','-c',MEDIA_PLAN_SH],input=plan,text=True,stdout=subprocess.PIPE)
    return {'removed_paths':len(remove_paths),'shared_media_retained':len(deleted_paths&ready_paths)}

def verify(env,directory,deletion_ledger=None):
    config=configuration(env);manifest=json.loads((directory/'manifest.json').read_text())
    if hashlib.sha256(config['ENCRYPTION_KEY'].encode()).hexdigest()!=manifest['key_fingerprint']:raise RuntimeError('encryption_key_mismatch')
    if set(manifest.get('sha256',{}))!={'database.dump','media.tar.gz'}:raise RuntimeError('incomplete_backup_manifest')
    for name,expected in manifest['sha256'].items():
        if digest(directory/name)!=expected:raise RuntimeError('backup_checksum_mismatch')
    with tarfile.open(directory/'media.tar.gz')as archive:
        for m in archive.getmembers():
            if not(m.isfile()or m.isdir())or m.issym()or m.islnk()or m.name.startswith('/')or'..'in pathlib.PurePosixPath(m.name).parts:raise RuntimeError('unsafe_media_archive')
    ledger=load_ledger(deletion_ledger)if deletion_ledger is not None else current_ledger(env,config)
    suffix=secrets.token_hex(8);database='phone_restore_'+suffix;volume='phone-platform-restore-'+suffix
    invoke(db_command(env,['createdb','-U',config['POSTGRES_USER'],database]),stdout=subprocess.PIPE)
    try:
        with (directory/'database.dump').open('rb')as f:invoke(db_command(env,['pg_restore','-U',config['POSTGRES_USER'],'-d',database,'--no-owner','--no-acl','--exit-on-error']),stdin=f,stdout=subprocess.PIPE)
        psql(env,config,database,'DELETE FROM sessions; UPDATE token_grants SET consumed_at=now();')
        counts=json.loads(psql(env,config,database,COUNTS_SQL))
        if counts!=manifest['counts']:raise RuntimeError('restored_counts_mismatch')
        replay=replay_ledger(env,config,database,ledger)
        invoke(['docker','volume','create',volume],stdout=subprocess.PIPE)
        with (directory/'media.tar.gz').open('rb')as f:invoke(['docker','run','--rm','-i','-v',volume+':/restore','busybox:1.37','tar','-xzf','-','-C','/restore'],stdin=f,stdout=subprocess.PIPE)
        rows=psql(env,config,database,"SELECT json_build_object('organization_id',organization_id,'path',data->>'path') FROM documents WHERE kind='recording' AND ((data->>'ready')='true' OR (data->>'state')='ready')")
        ready=[json.loads(raw)for raw in rows.splitlines()if raw]
        media_result=apply_media_plan(volume,replay['media'],ready)
        print(json.dumps({'isolated_restore_verified':True,'counts':counts,'ready_media_verified':len(ready),
         'sessions_revoked':True,'restored_into_original_database':False,'deletion_ledger_entries':replay['ledger_entries'],
         'documents_removed_after_backup':replay['removed_documents'],'unreferenced_deleted_media_absent':True,
         'shared_media_retained':media_result['shared_media_retained'],'no_resurrection':True}))
    finally:
        try:invoke(db_command(env,['dropdb','-U',config['POSTGRES_USER'],'--if-exists',database]),stdout=subprocess.PIPE)
        finally:subprocess.run(['docker','volume','rm',volume],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('action',choices=['create','verify','ledger'])
    p.add_argument('--env-file',type=pathlib.Path,default=ROOT/'.env.platform');p.add_argument('--directory',type=pathlib.Path)
    p.add_argument('--deletion-ledger',type=pathlib.Path,help='Latest private ledger instead of current source DB during verify')
    p.add_argument('--ledger-file',type=pathlib.Path,help='New private output file for ledger action')
    args=p.parse_args();os.umask(0o077)
    if args.action=='ledger'and args.ledger_file is None:p.error('--ledger-file is required')
    if args.action!='ledger'and args.directory is None:p.error('--directory is required')
    try:
        if args.action=='ledger':export_ledger(args.env_file,args.ledger_file)
        elif args.action=='create':backup(args.env_file,args.directory)
        else:verify(args.env_file,args.directory,args.deletion_ledger)
        return 0
    except (RuntimeError,OSError,ValueError,KeyError,IndexError,TypeError,StopIteration,tarfile.TarError):
        print('{"error":"backup_or_restore_failed"}',file=sys.stderr);return 1

if __name__=='__main__':sys.exit(main())
