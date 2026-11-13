import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import unittest
import uuid

spec=importlib.util.spec_from_file_location('retention_reconcile',Path(__file__).resolve().parents[1]/'reconcile-retention.py')
subject=importlib.util.module_from_spec(spec);spec.loader.exec_module(subject)

@unittest.skipUnless(os.environ.get('TEST_DATABASE_URL'),'requires isolated TEST_DATABASE_URL')
class ReconcilePostgresTests(unittest.TestCase):
    def sql(self,query,ok=True):
        result=subprocess.run([shutil.which('psql') or '/opt/homebrew/bin/psql',os.environ['TEST_DATABASE_URL'],'-X','-Atq','-v','ON_ERROR_STOP=1'],input=query,text=True,capture_output=True)
        if ok:self.assertEqual(result.returncode,0,result.stderr)
        return result
    def test_only_manifest_retention_soft_deletions_are_restored_atomically_and_idempotently(self):
        org,foreign,parent=[str(uuid.uuid4())for _ in range(3)]
        targets=[tuple(str(uuid.uuid4())for _ in range(2))+(parent,) for _ in range(8)]
        setup=f"INSERT INTO organizations(id,name) VALUES('{org}','Synthetic'),('{foreign}','Other'); INSERT INTO jobs(id,organization_id,kind,dedup_key) VALUES('{parent}','{org}','retention','parent');"
        for index,(call,job,_) in enumerate(targets):
            tenant=foreign if index==5 else org
            data={'deleted':True,'provider_id':str(uuid.uuid4()),'started_at':'2025-01-01T00:00:00Z','note':'keep synthetic manual note','purged':index==4}
            if index==6:data['started_at']='malformed date'
            if index==7:data['provider_id']='  '
            setup+=f"INSERT INTO documents(organization_id,id,kind,data) VALUES('{tenant}','{call}','call','{json.dumps(data)}'); INSERT INTO jobs(id,organization_id,kind,subject_id,dedup_key) VALUES('{job}','{tenant}','purge','{call}','retention:{call}:{parent}');"
            if index==1:setup+=f"INSERT INTO documents(organization_id,id,kind,data) VALUES('{org}','{uuid.uuid4()}','audit','{json.dumps({'event':'call.deleted','subject_id':call})}');"
            if index==2:setup+=f"INSERT INTO jobs(organization_id,kind,subject_id,dedup_key) VALUES('{org}','purge','{call}','purge:{call}');"
            if index==3:setup+=f"INSERT INTO deletion_tombstones(organization_id,subject_id) VALUES('{org}','{call}');"
        self.sql(setup)
        try:
            def run(apply):
                output=self.sql(subject.recovery_sql(org,targets,apply)).stdout
                return json.loads(next(line for line in output.splitlines()if line.startswith('{')))
            dry=run(False);self.assertEqual((dry['restored'],dry['cancelled']),(1,1))
            self.assertEqual(self.sql(f"SELECT count(*) FROM documents WHERE organization_id='{org}' AND kind='call' AND data->'deleted'='true';").stdout.strip(),'7')
            applied=run(True);self.assertEqual((applied['restored'],applied['cancelled']),(1,1))
            row=json.loads(self.sql(f"SELECT data FROM documents WHERE organization_id='{org}' AND id='{targets[0][0]}';").stdout)
            self.assertFalse(row['deleted']);self.assertEqual(row['note'],'keep synthetic manual note')
            self.assertEqual(run(True)['restored'],0)
            self.sql(f"UPDATE organizations SET data='{{\"retention_enabled\":true}}' WHERE id='{org}';")
            blocked=self.sql(subject.recovery_sql(org,targets,True),ok=False);self.assertNotEqual(blocked.returncode,0)
        finally:self.sql(f"DELETE FROM organizations WHERE id IN ('{org}','{foreign}');")

if __name__=='__main__':unittest.main()
