"""Public CI diagnostics preserve decisions without retaining raw tool responses."""
import copy
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import audit_dependencies as audit


def fixture():
    return {'format':1,'javascript_passed':False,
            'javascript':{'glob':[{'id':1244013,'url':'https://github.com/advisories/GHSA-3g3m-2fjr-6pw4','severity':'moderate','title':'private fixture text','nodes':['/private/fixture/user'],'recommendation':'secret fixture text'}]},
            'rust':{'server':{'lock_sha256':'a'*64,'database':{'last-commit':'b'*40,'private':'private fixture text'},
                      'findings':[{'package':'rsa@0.9.10','advisory':'RUSTSEC-2023-0071','category':'vulnerability','blocking_when_active':True,'active_targets':['x86_64-unknown-linux-gnu'],'title':'private fixture text','url':'https://example.test/?token=secret-fixture'}]}},
            'blocked_targets':{target: ['server:RUSTSEC-2023-0071'] if target=='x86_64-unknown-linux-gnu' else [] for target in audit.TARGETS},
            'environment':{'credential':'secret fixture text'},'stdout':'private fixture text','blanket_ignores':[]}


class DependencyReportingTests(unittest.TestCase):
    def test_failed_advisories_are_named_and_safe_report_preserves_decisions(self):
        raw=fixture();before=copy.deepcopy(raw)
        with tempfile.TemporaryDirectory() as directory,contextlib.redirect_stdout(io.StringIO()) as output:
            path=Path(directory)/'audit.json';audit.publish_report(raw,path)
            report=json.loads(path.read_text())
        self.assertEqual(raw,before)
        self.assertTrue(report['completed'])
        self.assertFalse(report['javascript_passed'])
        self.assertEqual(report['blocked_targets'],raw['blocked_targets'])
        self.assertIn('glob [moderate] GHSA-3g3m-2fjr-6pw4',output.getvalue())
        self.assertIn('rsa@0.9.10 [vulnerability] RUSTSEC-2023-0071',output.getvalue())
        self.assertEqual(report['rust']['server']['findings'][0]['active_targets'],['x86_64-unknown-linux-gnu'])
        for secret in ('private fixture text','secret fixture text','/private/fixture','secret-fixture','recommendation','environment','stdout'):
            self.assertNotIn(secret,json.dumps(report)+output.getvalue())

    def test_advisory_query_and_untrusted_identifiers_cannot_enter_logs_or_artifact(self):
        raw=fixture();raw['javascript']['glob'][0]['url']+='?token=secret-fixture'
        raw['javascript']['line\n::error::private fixture']=[{'id':'private fixture text','url':'https://user:secret-fixture@example.test/private','severity':'critical\nprivate fixture'}]
        raw['rust']['server']['findings'][0]['package']='private fixture text'
        raw['rust']['server']['findings'][0]['category']='private fixture text'
        with tempfile.TemporaryDirectory() as directory,contextlib.redirect_stdout(io.StringIO()) as output:
            path=Path(directory)/'audit.json';audit.publish_report(raw,path)
            text=path.read_text()+output.getvalue()
        for value in ('secret-fixture','private fixture','::error::','user:'):
            self.assertNotIn(value,text)
        self.assertIn('GHSA-3g3m-2fjr-6pw4',text)
        self.assertIn('unknown',text)

    def test_cli_keeps_javascript_and_target_blocking_exit_status(self):
        for js_passed,blocked,target,expected in [(False,False,None,1),(True,True,'x86_64-unknown-linux-gnu',1),(True,True,'aarch64-apple-darwin',0),(True,False,None,0)]:
            raw=fixture();raw['javascript_passed']=js_passed
            if not blocked:raw['blocked_targets']={key:[] for key in audit.TARGETS}
            with self.subTest(js_passed=js_passed,blocked=blocked,target=target),tempfile.TemporaryDirectory() as directory,patch.object(audit,'audit',return_value=raw),contextlib.redirect_stdout(io.StringIO()):
                args=['--output',str(Path(directory)/'audit.json')]+(['--target',target] if target else [])
                self.assertEqual(audit.main(args),expected)

    def test_unrecognized_remote_shape_stays_failed_without_publishing_its_body(self):
        for response in ([], {'glob': {}}, {'glob': [None]}):
            raw=fixture();raw['javascript']=response
            with self.subTest(response=response),tempfile.TemporaryDirectory() as directory,patch.object(audit,'audit',return_value=raw),contextlib.redirect_stdout(io.StringIO()):
                path=Path(directory)/'audit.json'
                with self.assertRaises(ValueError):audit.main(['--output',str(path)])
                self.assertFalse(json.loads(path.read_text())['completed'])

    def test_incomplete_audit_retains_only_a_sanitized_failure_receipt(self):
        with tempfile.TemporaryDirectory() as directory,patch.object(audit,'audit',side_effect=RuntimeError('secret fixture text')),contextlib.redirect_stdout(io.StringIO()) as output:
            path=Path(directory)/'audit.json'
            with self.assertRaisesRegex(RuntimeError,'secret fixture text'):audit.main(['--output',str(path)])
            report=json.loads(path.read_text())
        self.assertEqual(report,{'format':2,'completed':False,'error':'Dependency audit did not complete','exception':'RuntimeError'})
        self.assertNotIn('secret fixture text',output.getvalue())


if __name__=='__main__':unittest.main()
