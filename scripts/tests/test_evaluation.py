"""Synthetic metric/validation/privacy tests; no real speech, calls, accounts or network."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPTS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(SCRIPTS))
import evaluation

def module(name):
    spec=importlib.util.spec_from_file_location(name,SCRIPTS/(name+'.py'))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

transcripts=module('eval-transcripts');analyses=module('eval-analysis')

def manifest(task,samples,thresholds=None):
    return {'version':1,'task':task,'samples':samples,'thresholds':thresholds or {}}

def result(score=4,abstained=False):
    return {'criteria':[{'id':'greeting','status':'scored','score':score,'evidence':[
        {'segment_id':'s1','start':0,'end':1,'quote':'Здравствуйте'}]}],
        'status_id':'consultation','tag_ids':['a','b'],'abstained':abstained}


class EvaluationTests(unittest.TestCase):
    def test_exact_edit_distance_and_unicode_normalization(self):
        self.assertEqual(evaluation.edit_distance('kitten','sitting'),3)
        self.assertEqual(evaluation.edit_distance([],['insert']),1)
        self.assertEqual(evaluation.edit_distance(['one'],[]),1)
        self.assertEqual(evaluation.normalized('ЁЖ'),evaluation.normalized('Е\u0308ж'))

    def test_wer_cer_micro_counts(self):
        sample={'reference':{'text':'МАМА мыла раму'},'prediction':{'text':'мама мыла'}}
        report=transcripts.evaluate(manifest('transcripts',[sample],{'wer':{'max':.5}}))
        self.assertAlmostEqual(report['metrics']['wer'],1/3)
        self.assertAlmostEqual(report['metrics']['cer'],1/3)
        self.assertEqual(report['counts']['reference_words'],3)
        self.assertEqual(report['counts']['word_edits'],1)
        self.assertTrue(report['acceptance_passed'])

    def test_timestamps_roles_and_segmentation_mismatch_are_explicit(self):
        a={'text':'x','segments':[{'start':0,'end':1,'role':'manager'},{'start':1,'end':2,'role':'client'}]}
        b={'text':'x','segments':[{'start':.2,'end':1.2,'role':'manager'},{'start':1.2,'end':2.2,'role':'unknown'}]}
        report=transcripts.evaluate(manifest('transcripts',[{'reference':a,'prediction':b}]))
        self.assertAlmostEqual(report['metrics']['timestamp_mae_seconds'],.2)
        self.assertEqual(report['metrics']['role_accuracy'],.5)
        b['segments'].pop()
        report=transcripts.evaluate(manifest('transcripts',[{'reference':a,'prediction':b}]))
        self.assertIsNone(report['metrics']['timestamp_mae_seconds'])
        self.assertIsNone(report['metrics']['role_accuracy'])
        self.assertEqual(report['counts']['segmentation_mismatch_samples'],1)

    def test_missing_or_empty_reference_cannot_pass_quality(self):
        for sample in [{'reference':None,'prediction':{'text':'prediction'}},
                       {'reference':{'text':''},'prediction':{'text':'prediction'}}]:
            report=transcripts.evaluate(manifest('transcripts',[sample],{'wer':{'max':1}}))
            self.assertIsNone(report['metrics']['wer'])
            self.assertIsNone(report['acceptance_passed'])
        sample={'reference':{'text':'matching'},'prediction':{'text':'matching'}}
        report=transcripts.evaluate(manifest('transcripts',[sample,{'reference':None,'prediction':{'text':'unknown'}}],{'wer':{'max':0}}))
        self.assertEqual(report['metrics']['wer'],0)
        self.assertIsNone(report['acceptance_passed'])

    def test_analysis_alignment_and_hallucinated_evidence(self):
        reference=result();prediction=result(3);prediction['status_id']='wrong';prediction['tag_ids']=['b','c']
        source=[{'id':'s1','start':0,'end':1,'text':'Здравствуйте, клиент'}]
        report=analyses.evaluate(manifest('analysis',[{'reference':reference,'prediction':prediction,'segments':source}]))
        self.assertEqual(report['metrics']['status_accuracy'],0)
        self.assertEqual(report['metrics']['tag_precision'],.5)
        self.assertEqual(report['metrics']['tag_recall'],.5)
        self.assertEqual(report['metrics']['criterion_score_mae'],1)
        self.assertEqual(report['metrics']['evidence_precision'],1)
        self.assertEqual(report['metrics']['evidence_validity'],1)
        prediction['criteria'][0]['evidence'][0]['quote']='Не произнесено'
        report=analyses.evaluate(manifest('analysis',[{'reference':reference,'prediction':prediction,'segments':source}]))
        self.assertEqual(report['metrics']['evidence_validity'],0)
        self.assertEqual(report['metrics']['evidence_recall'],0)

    def test_abstention_precision_recall_and_absent_source(self):
        samples=[{'reference':result(abstained=reference),'prediction':result(abstained=prediction)}
                 for reference,prediction in [(True,True),(False,True),(True,False)]]
        report=analyses.evaluate(manifest('analysis',samples))
        self.assertEqual(report['metrics']['abstention_precision'],.5)
        self.assertEqual(report['metrics']['abstention_recall'],.5)
        self.assertIsNone(report['metrics']['evidence_validity'])
        self.assertIsNone(report['acceptance_passed'])

    def test_strict_inputs_reject_invalid_scores_duplicate_ids_and_roles(self):
        bad=result();bad['tag_ids']=['a','a']
        with self.assertRaises(ValueError):analyses.analysis(bad)
        bad=result();bad['criteria'][0]['score']=True
        with self.assertRaises(ValueError):analyses.analysis(bad)
        bad=result();bad['criteria'].append(bad['criteria'][0])
        with self.assertRaises(ValueError):analyses.analysis(bad)
        with self.assertRaises(ValueError):transcripts.transcript({'text':'x','segments':[{'start':0,'end':1,'role':'guessed'}]})

    def test_manifest_privacy_and_unknown_threshold_rejected(self):
        with tempfile.TemporaryDirectory()as directory:
            path=Path(directory)/'manifest.json';path.write_text(json.dumps(manifest('transcripts',[{'reference':None,'prediction':{'text':'private'}}])))
            path.chmod(0o644)
            with self.assertRaises(ValueError):evaluation.load_manifest(path,'transcripts',transcripts.METRICS)
            path.chmod(0o600)
            self.assertEqual(len(evaluation.load_manifest(path,'transcripts',transcripts.METRICS)['samples']),1)
            link=Path(directory)/'symlink';link.symlink_to(path)
            with self.assertRaises(ValueError):evaluation.load_manifest(link,'transcripts',transcripts.METRICS)
            value=json.loads(path.read_text());value['thresholds']={'invented':{'max':0}};path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):evaluation.load_manifest(path,'transcripts',transcripts.METRICS)

    def test_cli_reports_no_text_and_unknown_quality(self):
        secret='НЕПЕЧАТАТЬ synthetic private dialogue'
        with tempfile.TemporaryDirectory()as directory:
            for task,script,prediction in [('transcripts','eval-transcripts.py',{'text':secret}),('analysis','eval-analysis.py',result())]:
                path=Path(directory)/(task+'.json');report=Path(directory)/(task+'-report.json')
                path.write_text(json.dumps(manifest(task,[{'reference':None,'prediction':prediction}])));path.chmod(0o600)
                completed=subprocess.run([sys.executable,str(SCRIPTS/script),'--manifest',str(path),'--report',str(report)],text=True,capture_output=True)
                self.assertEqual(completed.returncode,0,completed.stderr)
                self.assertNotIn(secret,completed.stdout+completed.stderr+report.read_text())
                self.assertNotIn('Здравствуйте',completed.stdout+report.read_text())
                self.assertIsNone(json.loads(completed.stdout)['acceptance_passed'])
                self.assertEqual(report.stat().st_mode&0o777,0o600)
                original=path.read_bytes()
                collision=subprocess.run([sys.executable,str(SCRIPTS/script),'--manifest',str(path),'--report',str(path)],text=True,capture_output=True)
                self.assertEqual(collision.returncode,1)
                self.assertEqual(path.read_bytes(),original)

    def test_failing_threshold_is_explicit_and_not_unknown(self):
        report=transcripts.evaluate(manifest('transcripts',[{'reference':{'text':'one'},'prediction':{'text':'two'}}],{'wer':{'max':0}}))
        self.assertFalse(report['acceptance_passed'])
        self.assertFalse(report['thresholds']['wer']['passed'])


if __name__=='__main__':unittest.main()
