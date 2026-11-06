"""Transitional import and web-launch contracts; native lifecycle tests live in Rust."""
from contextlib import redirect_stderr
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from linux_services import start_unit,stop_unit,unit_state


class NativeServiceAdapterTests(unittest.TestCase):
    def test_imports_deliver_literal_arguments_and_unicode_to_the_qualified_native_owner(self):
        with tempfile.TemporaryDirectory() as temporary,patch('linux_services.supervisor_command',return_value=['/fixture/native']),patch('linux_services.subprocess.run',return_value=subprocess.CompletedProcess([],0,stdout='true\n')) as run:
            root=Path(temporary).resolve();environment={'LANG':'中文'}
            self.assertTrue(start_unit(root,'worker',['/fixture/with spaces','literal'],environment))
            self.assertEqual(run.call_args.args[0],['/fixture/native','--service-control',str(root),'worker','start'])
            self.assertEqual(json.loads(run.call_args.kwargs['input']),{'command':['/fixture/with spaces','literal'],'environment':environment})
            self.assertEqual(run.call_args.kwargs['cwd'],root)
            (root/'data/services').mkdir(parents=True)
            run.return_value=subprocess.CompletedProcess([],0,stdout='{"pid":7319}\n')
            self.assertEqual(unit_state(root,'worker'),{'pid':7319});self.assertEqual(run.call_args.args[0][-1],'status')
            run.return_value=subprocess.CompletedProcess([],0,stdout='false\n')
            self.assertFalse(stop_unit(root,'worker'));self.assertEqual(run.call_args.args[0][-1],'stop')

    def test_absent_state_does_not_select_a_program_or_create_directories(self):
        with tempfile.TemporaryDirectory() as temporary,patch('linux_services.supervisor_command') as program:
            root=Path(temporary);self.assertIsNone(unit_state(root,'worker'));self.assertFalse(stop_unit(root,'worker'))
            program.assert_not_called();self.assertFalse((root/'data').exists())

    def test_native_failure_is_never_reported_as_a_successful_resume(self):
        for message in ('service_configuration_changed\n','service_owner_unknown\n'):
            with tempfile.TemporaryDirectory() as temporary,patch('linux_services.supervisor_command',return_value=['/fixture/native']),patch('linux_services.subprocess.run',return_value=subprocess.CompletedProcess([],1,stdout='',stderr=message)),redirect_stderr(io.StringIO()),self.assertRaises(ValueError):
                start_unit(Path(temporary),'worker',['/fixture/worker'],{})


class WorkbenchLaunchTests(unittest.TestCase):
    def test_prebuilt_web_and_lan_start_without_node_or_build_tools(self):
        from linux_host import start_workbench_services
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);(root/'runtime').mkdir();(root/'runtime/manifest.json').write_text('{}')
            environment={};commands=[]
            with patch('linux_host.shutil.which',return_value=None),patch('prebuilt_web.web_command',return_value=[str(root/'runtime/bin/bun'),str(root/'runtime/web/apps/web/server.js')]):
                start_workbench_services(root,environment,lambda _args:self.fail('Prebuilt client must not compile'),lambda name,argv:commands.append((name,argv)))
            self.assertEqual(commands,[('web',[str(root/'runtime/bin/bun'),str(root/'runtime/web/apps/web/server.js')]),('lan',[str(root/'runtime/bin/bun'),str(root/'scripts/lan-server.mjs')])])
            self.assertEqual(environment,{'PORT':'17860','HOSTNAME':'127.0.0.1'})

    def test_source_web_and_lan_keep_the_existing_node_commands(self):
        from linux_host import start_workbench_services
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);builds=[];commands=[]
            with patch('linux_host.shutil.which',return_value='/fixture/node'),patch('linux_host.unit_state',return_value=None):
                start_workbench_services(root,{},builds.append,lambda name,argv:commands.append((name,argv)))
            self.assertEqual(builds,[[root/'scripts/bun','run','build']])
            self.assertEqual(commands,[('web',['/fixture/node',str(root/'apps/web/node_modules/next/dist/bin/next'),'start',str(root/'apps/web'),'--hostname','127.0.0.1','--port','17860']),('lan',['/fixture/node',str(root/'scripts/lan-server.mjs')])])

    def test_source_web_still_reports_a_missing_node_before_launch(self):
        from linux_host import start_web_service
        with tempfile.TemporaryDirectory() as temporary,patch('linux_host.shutil.which',return_value=None),patch('linux_host.unit_state',return_value={}):
            with self.assertRaisesRegex(ValueError,'Node runtime unavailable'):
                start_web_service(Path(temporary),{},lambda _args:None,lambda *_args:self.fail('Source web must not launch without Node'))


if __name__=='__main__':unittest.main()
