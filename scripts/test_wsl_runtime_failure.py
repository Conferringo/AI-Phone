"""A failed desktop probe keeps its closed error code, not private command output."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import wsl_initialize_diagnostics as diag
import wsl_runtime_fixture as fixture
import wsl_runtime_worker as worker


class RuntimeFailureTests(unittest.TestCase):
    def test_closed_desktop_and_rpc_errors_are_retained(self):
        for raw in (b'qualification_provision_failed\n','qualification_rpc_desktop/observe_500\n'):
            result=diag.runtime_failure(subprocess.CalledProcessError(1,['PRIVATE_ARGUMENT'],stderr=raw))
            self.assertEqual(result,{'error':'command_failed','returncode':1,'probe':raw.strip().decode() if isinstance(raw,bytes) else raw.strip()})
            diag.checked_runtime_failure(result)

    def test_unknown_output_and_timeout_cannot_export_private_text(self):
        for error in (subprocess.CalledProcessError(1,['PRIVATE_ARGUMENT'],stderr=b'PRIVATE_TOKEN /private/path'),subprocess.TimeoutExpired(['PRIVATE_ARGUMENT'],300),ValueError('PRIVATE_TOKEN')):
            result=diag.runtime_failure(error)
            self.assertEqual(result['probe'],'unclassified');self.assertNotIn('PRIVATE',json.dumps(result));diag.checked_runtime_failure(result)
        for code in ('qualification_rpc_private/token_500','qualification_rpc_desktop/observe_999','qualification_provision_failed\nPRIVATE_TOKEN'):
            self.assertEqual(diag.runtime_failure(subprocess.CalledProcessError(1,[],stderr=code))['probe'],'unclassified')

    def test_main_records_a_post_initialization_failure_without_reclassifying_initialization(self):
        captured=[]
        def perform(value,nonce,phase):
            value['stage']='desktop';value['initialization']['step']='complete'
            raise subprocess.CalledProcessError(1,['PRIVATE_ARGUMENT'],stderr=b'qualification_provision_failed\n')
        def checkpoint(value,stage):value['stage']=stage;captured.append(json.loads(json.dumps(value)))
        with tempfile.TemporaryDirectory() as temporary,patch.object(worker,'STATE',Path(temporary)),patch.object(worker,'fixture_identity',return_value='a'*32),patch.object(worker,'perform',side_effect=perform),patch.object(worker,'checkpoint',side_effect=checkpoint):
            self.assertEqual(worker.main(),1)
        result=captured[-1]
        self.assertEqual(result['failure']['probe'],'qualification_provision_failed')
        self.assertEqual(result['initialization']['error'],'none')
        self.assertEqual(result['initialization']['step'],'complete')
        fixture.worker_result(result,'a'*32,1)
        for mutation in ({'probe':'PRIVATE_TOKEN'},{'returncode':True},{'error':'none'},{'extra':'PRIVATE_TOKEN'}):
            changed=json.loads(json.dumps(result));changed['failure'].update(mutation)
            with self.assertRaises(ValueError):fixture.worker_result(changed,'a'*32,1)


if __name__=='__main__':unittest.main()
