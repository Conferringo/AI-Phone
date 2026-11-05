"""Legacy launcher delegates to Rust; input recovery policy is tested in Rust."""
import ast
import os
from pathlib import Path
import subprocess
import sys
import types
import unittest
from unittest.mock import patch
import tempfile
import android_input
HERE=Path(__file__).resolve().parent

class NativeDelegationTests(unittest.TestCase):
    def test_proto_input_and_fixed_component_retain_unicode_paths_and_source_resolvers(self):
        with tempfile.TemporaryDirectory(prefix='aiphone-输入-') as temporary:
            root=Path(temporary); program=root/'native.py'; apk=root/'input.apk'; probe=root/'probe.jar'
            for path in (apk,probe):path.write_bytes(b'fixture')
            program.write_text('import sys\nfrom pathlib import Path\nPath(sys.argv[1]).write_bytes(sys.stdin.buffer.read())\nsys.stdout.buffer.write(b"\\x00\\x00\\x00\\x02\\x08\\x01")\n')
            captured=root/'captured'
            with patch('prebuilt_services.input_setup_command',return_value=[sys.executable,str(program),str(captured)]),patch('prebuilt_phone.installer_apk',return_value=apk),patch('prebuilt_phone.notification_helper',return_value=probe):
                android_input.setup_input(root/'sdk/adb','emulator-fixture',root)
            body=captured.read_bytes();self.assertEqual(int.from_bytes(body[:4],'big'),len(body)-4)
            self.assertIn(str(root).encode(),body);self.assertIn(b'emulator-fixture',body);self.assertIn(str(probe).encode(),body)
    def test_native_failure_is_not_success_and_no_local_pending_owner_is_created(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            with patch('prebuilt_services.input_setup_command',return_value=[sys.executable,'-c','raise SystemExit(7)']),patch('prebuilt_phone.installer_apk',return_value=root/'apk'),patch('prebuilt_phone.notification_helper',return_value=root/'probe'):
                with self.assertRaises(RuntimeError):android_input.setup_input(root/'adb','emulator-fixture',root)
            self.assertFalse((root/'data').exists())

    def test_successful_exit_without_a_native_receipt_is_not_readiness(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            with patch('prebuilt_services.input_setup_command',return_value=[sys.executable,'-c','pass']),patch('prebuilt_phone.installer_apk',return_value=root/'apk'),patch('prebuilt_phone.notification_helper',return_value=root/'probe'):
                with self.assertRaisesRegex(RuntimeError,'invalid receipt'):
                    android_input.setup_input(root/'adb','emulator-fixture',root)

def extracted(name,function,environment):
    tree=ast.parse((HERE/name).read_text())
    node=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name==function)
    exec(compile(ast.Module(body=[node],type_ignores=[]),name,'exec'),environment)
    return environment[function]


class CallerDelegationTests(unittest.TestCase):
    def test_linux_delegates_input_once_without_its_own_package_decision(self):
        run=[];queried=[]
        def query(argv,**_):queried.append(argv);return types.SimpleNamespace(returncode=0,stdout='package:fixture')
        function=extracted('linux_host.py','start_guest',{'subprocess':types.SimpleNamespace(run=query),'emit_snapshot':lambda _phase:None})
        function(Path('/fixture'),{},run.append,lambda *_:False)
        self.assertEqual(sum(str(a[-1]).endswith('/android-install-input.sh') for a in run),1)
        self.assertFalse(any('dev.aiphone.input' in a for a in queried))
        self.assertEqual({a[-1] for a in queried},{'dev.aiphone.home','com.github.metacubex.clash.meta'})

    def test_mac_delegates_input_once_and_keeps_legacy_apps_untouched(self):
        run=[];queried=[];services=[('dev.aiphone.emulator.p2','android-start.sh')]
        env={'Path':Path,'os':os,'SERVICES':services,'owned_jobs':lambda *_:{services[0][0]:True},'runtime_env':lambda *_:{},'subprocess':subprocess}
        function=extracted('service_start.py','_start_missing',env)
        with patch.dict(sys.modules,{'managed_engine':types.SimpleNamespace(ensure_running=lambda *_:None)}):
            function('/fixture',execute=run.append,package_installed=lambda p:queried.append(p) or True)
        self.assertEqual(run,['android-install-input.sh'])
        self.assertEqual(queried,['dev.aiphone.home','com.github.metacubex.clash.meta'])


if __name__=='__main__':unittest.main()
