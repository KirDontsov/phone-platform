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
        expected={'removed_documents':3,'ledger_entries':1,'media':[],'resurrected_calls':0}
        with mock.patch.object(backup,'psql',return_value=json.dumps(expected))as psql:
            result=backup.replay_ledger(self.env,config,'phone_restore_test',ledger)
        self.assertEqual(result,expected)
        statement=psql.call_args.args[3]
        encoded=statement.split('COPY replay_input FROM STDIN;\n',1)[1].split('\n',1)[0]
        self.assertEqual(json.loads(base64.b64decode(encoded)),backup.validated_ledger(ledger)['entries'])
        self.assertNotIn(ledger['entries'][0]['provider_id'],statement)
        with mock.patch.object(backup,'psql',return_value=json.dumps(dict(expected,resurrected_calls=1))):
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
            if 'COPY replay_input FROM STDIN' in sql:return json.dumps({'removed_documents':2,'ledger_entries':1,'media':[deleted],'resurrected_calls':0})
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
        mutating=[(database,sql)for database,sql in statements if 'DELETE FROM sessions' in sql or 'COPY replay_input' in sql]
        self.assertTrue(all(database==created for database,sql in mutating))
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


if __name__ == '__main__':
    unittest.main()
