"""Backup boundary tests use disposable files and mocked Docker, never project credentials."""
import base64
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import uuid
from unittest import mock

SOURCE = Path(__file__).resolve().parents[1] / 'backup.py'
spec = importlib.util.spec_from_file_location('phone_backup', SOURCE)
backup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(backup)


class BackupSafetyTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.env = self.root / 'local.env'
        self.env.write_text('POSTGRES_USER=testuser\nPOSTGRES_DB=phone_fixture\nENCRYPTION_KEY=test-only-key\n')
        self.env.chmod(0o600)
        self.directory = self.root / 'backup'
        self.directory.mkdir(mode=0o700)
        (self.directory / 'database.dump').write_bytes(b'synthetic dump; never a real database')
        self.archive()
        self.manifest()

    def archive(self, member=None):
        with tarfile.open(self.directory / 'media.tar.gz', 'w:gz') as archive:
            if member is not None:
                archive.addfile(member, io.BytesIO(b'x') if member.isfile() else None)

    def manifest(self, **changes):
        value = {'version': 1, 'project': 'test-project',
                 'sha256': {name: backup.digest(self.directory / name)
                            for name in ('database.dump', 'media.tar.gz')},
                 'key_fingerprint': hashlib.sha256(b'test-only-key').hexdigest(),
                 'counts': {'organizations': 1, 'users': 1, 'documents': 0, 'jobs': 0}}
        value.update(changes)
        (self.directory / 'manifest.json').write_text(json.dumps(value))
        return value

    def rejected_before_docker(self, error):
        with mock.patch.object(backup, 'invoke', side_effect=AssertionError('Docker must not run')) as invoke:
            with self.assertRaisesRegex(RuntimeError, error):
                backup.verify(self.env, self.directory)
            invoke.assert_not_called()

    def test_env_must_be_private_regular_file(self):
        self.env.chmod(0o644)
        with self.assertRaisesRegex(RuntimeError, 'private_env_required'):
            backup.configuration(self.env)
        self.env.chmod(0o600)
        link = self.root / 'env-link'
        link.symlink_to(self.env)
        with self.assertRaisesRegex(RuntimeError, 'private_env_required'):
            backup.configuration(link)

    def test_media_volume_lookup_includes_stopped_api_container(self):
        def invoke(command,**kwargs):
            if 'ps' in command:
                return subprocess.CompletedProcess(command,0,stdout='stopped-api' if '-a' in command else '')
            self.assertEqual(command,['docker','inspect','stopped-api'])
            return subprocess.CompletedProcess(command,0,stdout=json.dumps([{'Mounts':[{'Destination':'/app/media','Type':'volume','Name':'synthetic-media-volume'}]}]))
        with mock.patch.object(backup,'invoke',side_effect=invoke):
            self.assertEqual(backup.volume_name(self.env,'api','/app/media'),'synthetic-media-volume')

    def test_private_output_does_not_overwrite_existing_file(self):
        target = self.root / 'private-output'
        backup.write_private(target, 'first')
        self.assertEqual(target.stat().st_mode & 0o777, 0o600)
        with self.assertRaises(FileExistsError):
            backup.write_private(target, 'replacement')
        self.assertEqual(target.read_text(), 'first')

    def test_wrong_encryption_key_rejected_before_restore(self):
        self.manifest(key_fingerprint='0' * 64)
        self.rejected_before_docker('encryption_key_mismatch')

    def test_tampered_dump_rejected_before_restore(self):
        (self.directory / 'database.dump').write_bytes(b'changed')
        self.rejected_before_docker('backup_checksum_mismatch')

    def test_manifest_requires_both_exact_checksum_entries(self):
        for entries in ({}, {'database.dump': backup.digest(self.directory / 'database.dump')}):
            with self.subTest(entries=list(entries)):
                self.manifest(sha256=entries)
                self.rejected_before_docker('manifest|checksum')

    def test_traversal_symlink_and_special_archive_members_rejected(self):
        fixtures = []
        traversal = tarfile.TarInfo('../escape'); traversal.size = 1; fixtures.append(traversal)
        symlink = tarfile.TarInfo('tenant/link'); symlink.type = tarfile.SYMTYPE; symlink.linkname = '/etc/passwd'; fixtures.append(symlink)
        fifo = tarfile.TarInfo('tenant/fifo'); fifo.type = tarfile.FIFOTYPE; fixtures.append(fifo)
        device = tarfile.TarInfo('tenant/device'); device.type = tarfile.CHRTYPE; device.devmajor = 1; device.devminor = 3; fixtures.append(device)
        for member in fixtures:
            with self.subTest(member=member.name):
                self.archive(member)
                self.manifest()
                self.rejected_before_docker('unsafe_media_archive')

    def test_partially_failed_quiesce_restarts_previously_running_services(self):
        destination = self.root / 'new-backup'
        commands = []
        def invoke(command, **kwargs):
            commands.append(command)
            if command[-4:] == ['ps', '--services', '--status', 'running']:
                return subprocess.CompletedProcess(command, 0, stdout='api\nworker\n')
            if 'stop' in command:
                raise RuntimeError('simulated_partial_stop_failure')
            return subprocess.CompletedProcess(command, 0, stdout='')
        with mock.patch.object(backup, 'invoke', side_effect=invoke):
            with self.assertRaises(RuntimeError):
                backup.backup(self.env, destination)
        restarts = [command for command in commands if 'start' in command]
        self.assertTrue(restarts, 'A failed stop can still stop one service; finally must restart both.')
        self.assertEqual(set(restarts[-1][-2:]), {'api', 'worker'})

    def ledger(self, **changes):
        value={'version':1,'exported_at':'2026-11-11T00:00:00+00:00','entries':[
            {'organization_id':'11111111-1111-4111-8111-111111111111',
             'subject_id':'22222222-2222-4222-8222-222222222222',
             'provider_id':'provider:alpha_01==','deleted_at':'2026-11-10T00:00:00+00:00'}]}
        value.update(changes)
        return value

    def test_ledger_validation_rejects_untrusted_ids_and_time(self):
        for field,value in [('organization_id','not-a-uuid'),('subject_id',"x'); DROP TABLE documents;--"),
                            ('provider_id',"x'); DROP TABLE documents;--"),('deleted_at','2026-11-11')]:
            with self.subTest(field=field):
                ledger=self.ledger();ledger['entries'][0][field]=value
                with self.assertRaisesRegex(RuntimeError,'invalid_deletion_ledger'):
                    backup.validated_ledger(ledger)
        ledger=self.ledger();ledger['entries'].append(dict(ledger['entries'][0],provider_id='different'))
        with self.assertRaisesRegex(RuntimeError,'conflicting_deletion_ledger'):
            backup.validated_ledger(ledger)

    def test_private_ledger_file_validation(self):
        path=self.root/'ledger.json';path.write_text(json.dumps(self.ledger()));path.chmod(0o644)
        with self.assertRaisesRegex(RuntimeError,'private_deletion_ledger_required'):
            backup.load_ledger(path)
        path.chmod(0o600)
        self.assertEqual(len(backup.load_ledger(path)['entries']),1)
        link=self.root/'ledger-link';link.symlink_to(path)
        with self.assertRaisesRegex(RuntimeError,'private_deletion_ledger_required'):
            backup.load_ledger(link)

    def test_replay_transmits_validated_json_as_copy_data(self):
        ledger=self.ledger();config=backup.configuration(self.env)
        expected={'removed_documents':3,'ledger_entries':1,'media':[],'resurrected_calls':0,'resurrected_subjects':0}
        with mock.patch.object(backup,'psql',return_value=json.dumps(expected))as psql:
            result=backup.replay_ledger(self.env,config,'phone_restore_test',ledger)
        self.assertEqual(result,expected)
        statement=psql.call_args.args[3]
        encoded=statement.split('COPY replay_input FROM STDIN;\n',1)[1].split('\n',1)[0]
        self.assertEqual(json.loads(base64.b64decode(encoded)),backup.validated_ledger(ledger)['entries'])
        self.assertNotIn(ledger['entries'][0]['provider_id'],statement)
        for changed in ({'resurrected_calls':1},{'resurrected_subjects':1}):
            with self.subTest(changed=changed),mock.patch.object(backup,'psql',return_value=json.dumps(dict(expected,**changed))):
                with self.assertRaisesRegex(RuntimeError,'deletion_ledger_replay_failed'):
                    backup.replay_ledger(self.env,config,'phone_restore_test',ledger)

    def test_media_plan_is_one_container_and_paths_are_stdin_data(self):
        org=self.ledger()['entries'][0]['organization_id']
        deleted=[{'organization_id':org,'path':org+'/delete;literal.wav'}]
        ready=[{'organization_id':org,'path':org+'/record '+str(i)+'.wav'}for i in range(100)]
        with mock.patch.object(backup,'invoke')as invoke:
            backup.apply_media_plan('isolated-restore-volume',deleted,ready)
        invoke.assert_called_once()
        command=invoke.call_args.args[0];plan=invoke.call_args.kwargs['input']
        self.assertEqual(command[0:2],['docker','run'])
        self.assertNotIn(deleted[0]['path'],' '.join(command))
        self.assertEqual(len(plan.splitlines()),101)
        self.assertTrue(plan.startswith('D\t'+deleted[0]['path']))
        for path in ['../outside',org+'/../outside','/absolute',org+'/bad\nline',org+'/bad\tfield']:
            with self.subTest(path=path),mock.patch.object(backup,'invoke')as invoke:
                with self.assertRaisesRegex(RuntimeError,'invalid_media_reference'):
                    backup.apply_media_plan('isolated-restore-volume',[{'organization_id':org,'path':path}],[])
                invoke.assert_not_called()

    def test_shared_media_retained_while_live_recording_reference_exists(self):
        org=self.ledger()['entries'][0]['organization_id']
        shared={'organization_id':org,'path':org+'/demo-signal.wav'}
        deleted_only={'organization_id':org,'path':org+'/deleted-only.wav'}
        with mock.patch.object(backup,'invoke')as invoke:
            result=backup.apply_media_plan('isolated-restore-volume',[shared,deleted_only],[shared,shared])
        plan=invoke.call_args.kwargs['input']
        self.assertNotIn('D\t'+shared['path'],plan)
        self.assertIn('D\t'+deleted_only['path'],plan)
        self.assertEqual(plan.count('F\t'+shared['path']),1)
        self.assertEqual(result,{'removed_paths':1,'shared_media_retained':1})

    def test_restore_isolated_database_replays_ledger_and_cleans_resources(self):
        commands=[];statements=[];manifest=self.manifest();ledger=self.ledger()
        org=ledger['entries'][0]['organization_id']
        deleted={'organization_id':org,'path':org+'/deleted.wav'}
        ready={'organization_id':org,'path':org+'/retained.wav'}
        def invoke(command,**kwargs):
            commands.append((command,kwargs))
            return subprocess.CompletedProcess(command,0,stdout='')
        def psql(env,config,database,sql):
            statements.append((database,sql))
            if sql==backup.LEDGER_SQL:return json.dumps(ledger['entries'])
            if sql==backup.COUNTS_SQL:return json.dumps(manifest['counts'])
            if 'COPY replay_input FROM STDIN' in sql:return json.dumps({'removed_documents':2,'ledger_entries':1,'media':[deleted],'resurrected_calls':0,'resurrected_subjects':0})
            if "kind='recording'" in sql:return json.dumps(ready)
            return ''
        with mock.patch.object(backup,'invoke',side_effect=invoke),mock.patch.object(backup,'psql',side_effect=psql), \
             mock.patch.object(backup.subprocess,'run')as cleanup,mock.patch('builtins.print')as output:
            backup.verify(self.env,self.directory)
        created=next(command for command,kw in commands if 'createdb' in command)[-1]
        self.assertTrue(created.startswith('phone_restore_'))
        self.assertNotEqual(created,'phone_fixture')
        restores=[command for command,kw in commands if 'pg_restore' in command]
        self.assertEqual(restores[0][restores[0].index('-d')+1],created)
        mutating=[(database,sql)for database,sql in statements if 'DELETE FROM sessions' in sql or 'COPY replay_input' in sql or sql==backup.ASSISTANT_REVOKE_SQL]
        self.assertTrue(all(database==created for database,sql in mutating))
        self.assertIn((created,backup.ASSISTANT_REVOKE_SQL),statements,'Restore must execute assistant authority reset in the isolated database')
        baseline=next(i for i,(_,sql)in enumerate(statements)if sql==backup.COUNTS_SQL)
        replay=next(i for i,(_,sql)in enumerate(statements)if 'COPY replay_input' in sql)
        self.assertLess(baseline,replay)
        helpers=[kw['input']for command,kw in commands if command[-1]==backup.MEDIA_PLAN_SH]
        self.assertEqual(helpers,['D\t'+deleted['path']+'\nF\t'+ready['path']+'\n'])
        self.assertTrue(any('dropdb' in command and command[-1]==created for command,kw in commands))
        result=json.loads(output.call_args.args[0]);self.assertTrue(result['no_resurrection'])
        self.assertEqual(result['documents_removed_after_backup'],2)
        cleanup.assert_called_once()
        self.assertEqual(cleanup.call_args.args[0][0:3],['docker','volume','rm'])


    def test_snapshot_media_archive_never_mounts_assistant_auth_volume(self):
        """Auth-cache exclusion is checked at the archive boundary, not a manifest claim."""
        destination=self.root/'assistant-backup'
        commands=[]
        def invoke(command,**kwargs):
            commands.append(command)
            if command[-4:]==['ps','--services','--status','running']:
                return subprocess.CompletedProcess(command,0,stdout='api\nworker\nassistant-runner\n')
            if 'pg_dump' in command:
                kwargs['stdout'].write(b'synthetic database dump')
            elif 'psql' in command:
                return subprocess.CompletedProcess(command,0,stdout=json.dumps({'organizations':1,'users':1,'documents':0,'jobs':0}))
            elif command[:2]==['docker','run']:
                kwargs['stdout'].write(b'synthetic media archive')
            return subprocess.CompletedProcess(command,0,stdout='')
        with mock.patch.object(backup,'invoke',side_effect=invoke),mock.patch.object(backup,'volume_name',return_value='synthetic-media')as volume,mock.patch('builtins.print'):
            backup.backup(self.env,destination)
        volume.assert_called_once_with(self.env,'api','/app/media')
        archive_commands=[command for command in commands if command[:2]==['docker','run']]
        self.assertEqual(len(archive_commands),1)
        command=archive_commands[0]
        self.assertEqual(command[command.index('-v')+1],'synthetic-media:/source:ro')
        self.assertFalse(any('assistant-auth' in arg or 'auth.json' in arg for arg in command))
        self.assertEqual(set(json.loads((destination/'manifest.json').read_text())['sha256']),{'database.dump','media.tar.gz'})


@unittest.skipUnless(os.environ.get('PHONE_BACKUP_PG_TESTS')=='1','requires explicit isolated PostgreSQL backup fixture')
class AssistantRestorePostgresTests(unittest.TestCase):
    """Execute actual replay/reset SQL in a newly created disposable DB, never source DB."""
    @classmethod
    def setUpClass(cls):
        cls.env=backup.ROOT/'.env.platform'
        cls.config=backup.configuration(cls.env)
        cls.database='phone_assistant_restore_'+uuid.uuid4().hex[:12]
        if cls.database==cls.config['POSTGRES_DB'] or not cls.database.startswith('phone_assistant_restore_'):
            raise RuntimeError('isolated_assistant_restore_database_required')
        backup.invoke(backup.db_command(cls.env,['createdb','-U',cls.config['POSTGRES_USER'],cls.database]),stdout=subprocess.PIPE)
        try:
            migration=(backup.ROOT/'crates/persistence/migrations/0001_access/up.sql').read_text()
            backup.psql(cls.env,cls.config,cls.database,migration)
        except BaseException:
            cls.tearDownClass()
            raise

    @classmethod
    def tearDownClass(cls):
        backup.invoke(backup.db_command(cls.env,['dropdb','-U',cls.config['POSTGRES_USER'],'--if-exists',cls.database]),stdout=subprocess.PIPE)

    def sql(self,statement):
        self.assertNotEqual(self.database,self.config['POSTGRES_DB'])
        return backup.psql(self.env,self.config,self.database,statement)

    def setUp(self):
        self.sql('TRUNCATE organizations,users CASCADE;')
        self.org=str(uuid.uuid4());self.foreign=str(uuid.uuid4());self.user=str(uuid.uuid4())
        self.sql("INSERT INTO organizations(id,name) VALUES('"+self.org+"','Synthetic assistant restore'),('"+self.foreign+"','Foreign restore canary');"+
                 "INSERT INTO users(id,email,name,password_hash) VALUES('"+self.user+"','synthetic@example.test','Synthetic','unused');"+
                 "INSERT INTO memberships(organization_id,user_id,role) VALUES('"+self.org+"','"+self.user+"','owner');")

    def test_restore_revokes_pending_registration_proofs_without_touching_users(self):
        # Legacy snapshots: safe when no table exists. New snapshots: no OTP revival.
        self.sql(backup.REGISTRATION_REVOKE_SQL)
        self.sql((backup.ROOT/'crates/persistence/migrations/0002_registration/up.sql').read_text())
        before=self.sql("SELECT json_build_object('users',(SELECT count(*) FROM users),'orgs',(SELECT count(*) FROM organizations));")
        self.sql("INSERT INTO registration_challenges(email,id,name,organization_name,password_hash,code_hash,browser_hash,expires_at,send_state) VALUES('pending@example.test',gen_random_uuid(),'Synthetic','Synthetic','unused-password-hash','unused-code-hash','unused-browser-hash',now()+interval '15 minutes','sent');")
        self.sql(backup.REGISTRATION_REVOKE_SQL)
        self.assertEqual(self.sql('SELECT count(*) FROM registration_challenges;'),'0')
        self.assertEqual(self.sql("SELECT json_build_object('users',(SELECT count(*) FROM users),'orgs',(SELECT count(*) FROM organizations));"),before)

    def docs(self,rows):
        payload=base64.b64encode(json.dumps(rows).encode()).decode()
        self.sql("INSERT INTO documents(organization_id,id,kind,parent_id,data) SELECT organization_id,id,kind,parent_id,data FROM jsonb_to_recordset(convert_from(decode('"+payload+"','base64'),'UTF8')::jsonb) AS x(organization_id UUID,id UUID,kind TEXT,parent_id UUID,data JSONB);")

    def doc(self,id,document_kind,parent=None,org=None,**data):
        return {'organization_id':org or self.org,'id':id,'kind':document_kind,'parent_id':parent,'data':data}

    def test_deleted_assistant_subjects_and_children_do_not_resurrect_or_cross_tenants(self):
        ids={name:str(uuid.uuid4())for name in ['conversation','message','run','receipt','child_credential','standalone_credential','call','live_chat','foreign_chat','foreign_call','old_call_subject']}
        provider='shared-provider-fixture'
        self.docs([
            self.doc(ids['conversation'],'assistant_conversation',text='Deleted synthetic history'),
            self.doc(ids['message'],'assistant_message',ids['conversation'],text='Deleted synthetic message'),
            self.doc(ids['run'],'assistant_run',ids['conversation'],state='completed'),
            self.doc(ids['receipt'],'assistant_tool_receipt',ids['run'],state='completed'),
            self.doc(ids['child_credential'],'assistant_credential',ids['conversation'],config_encrypted='deleted-ciphertext'),
            self.doc(ids['standalone_credential'],'assistant_credential',config_encrypted='deleted-independent-ciphertext'),
            self.doc(ids['call'],'call',provider_id=provider),
            self.doc(ids['live_chat'],'assistant_conversation',provider_id=provider,text='Must survive provider-ID matching'),
            self.doc(ids['foreign_chat'],'assistant_conversation',org=self.foreign,provider_id=provider,text='Foreign private history'),
            self.doc(ids['foreign_call'],'call',org=self.foreign,provider_id=provider),
        ])
        entries=[{'organization_id':self.org,'subject_id':ids[name],'provider_id':None,'deleted_at':'2026-11-12T00:00:00+00:00'}for name in ['conversation','standalone_credential']]
        entries.append({'organization_id':self.org,'subject_id':ids['old_call_subject'],'provider_id':provider,'deleted_at':'2026-11-12T00:00:00+00:00'})
        ledger={'version':1,'exported_at':'2026-11-12T01:00:00+00:00','entries':entries}
        result=backup.replay_ledger(self.env,self.config,self.database,ledger)
        self.assertEqual(result['removed_documents'],7)
        remaining=set(json.loads(self.sql("SELECT COALESCE(json_agg(id::text),'[]') FROM documents;")))
        self.assertEqual(remaining,{ids['live_chat'],ids['foreign_chat'],ids['foreign_call']})
        again=backup.replay_ledger(self.env,self.config,self.database,ledger)
        self.assertEqual(again['removed_documents'],0)
        self.assertEqual(again['ledger_entries'],3)

    def test_restore_revokes_assistant_authority_auth_and_temporary_ciphertext(self):
        ids={name:str(uuid.uuid4())for name in ['conversation','message','run','credential','attempt','control','connection']}
        self.docs([
            self.doc(ids['conversation'],'assistant_conversation',user_id=self.user,active_run_id=ids['run']),
            self.doc(ids['message'],'assistant_message',ids['conversation'],user_id=self.user,role='assistant',status='running',text='Durable synthetic partial',run_id=ids['run']),
            self.doc(ids['run'],'assistant_run',ids['conversation'],user_id=self.user,state='running',capability_hash='stale-turn-hash',assistant_message_id=ids['message'],auth_generation=7),
            self.doc(ids['credential'],'assistant_credential',ids['conversation'],user_id=self.user,config_encrypted='temporary-ciphertext-must-be-erased'),
            self.doc(ids['attempt'],'assistant_auth_attempt',ids['control'],user_id=self.user,state='pending',capability_hash='stale-auth-hash',generation=7),
            self.doc(ids['control'],'assistant_auth_control',user_id=self.user,authenticated=True,confirmed_generation=7,generation=7),
            self.doc(ids['connection'],'connection',kind='telphin',config_encrypted='permanent-provider-ciphertext-must-remain'),
        ])
        self.sql("INSERT INTO sessions(token_hash,user_id,organization_id,csrf_token,expires_at) VALUES('synthetic-session','"+self.user+"','"+self.org+"','synthetic-csrf',now()+interval '1 hour');"+
                 "INSERT INTO token_grants(token_hash,kind,user_id,expires_at) VALUES('synthetic-invite','invite','"+self.user+"',now()+interval '1 hour');")
        statement=getattr(backup,'ASSISTANT_REVOKE_SQL',None)
        self.assertIsInstance(statement,str,'Restore must expose and execute the same complete authority-reset SQL tested here')
        self.sql('DELETE FROM sessions; UPDATE token_grants SET consumed_at=now();')
        self.sql(statement)
        rows=json.loads(self.sql("SELECT COALESCE(json_agg(json_build_object('id',id,'kind',kind,'data',data)),'[]') FROM documents;"))
        by_id={row['id']:row['data']for row in rows}
        self.assertEqual(by_id[ids['run']]['state'],'interrupted')
        self.assertNotIn('capability_hash',by_id[ids['run']])
        self.assertIsNone(by_id[ids['conversation']].get('active_run_id'))
        self.assertEqual(by_id[ids['message']]['status'],'failed')
        self.assertEqual(by_id[ids['message']]['text'],'Durable synthetic partial')
        self.assertNotIn('temporary-ciphertext-must-be-erased',json.dumps(rows))
        if ids['attempt']in by_id:
            self.assertNotEqual(by_id[ids['attempt']].get('state'),'pending')
            self.assertNotIn('capability_hash',by_id[ids['attempt']])
        if ids['control']in by_id:
            self.assertIsNot(by_id[ids['control']].get('authenticated'),True)
            self.assertIsNone(by_id[ids['control']].get('confirmed_generation'))
        self.assertEqual(by_id[ids['connection']]['config_encrypted'],'permanent-provider-ciphertext-must-remain')
        sessions=json.loads(self.sql("SELECT json_build_object('sessions',(SELECT count(*) FROM sessions),'unused_grants',(SELECT count(*) FROM token_grants WHERE consumed_at IS NULL));"))
        self.assertEqual(sessions,{'sessions':0,'unused_grants':0})


if __name__ == '__main__':
    unittest.main()
