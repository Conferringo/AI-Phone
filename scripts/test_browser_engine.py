"""Only exact owned identities cross the Docker command boundary."""
import csv
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch,Mock
from browser_engine import DockerEngine
from browser_lease import BrowserContext

ID='b'*64


class BrowserEngineTests(unittest.TestCase):
    def test_command_adapter_captures_real_child_output(self):
        import sys
        self.assertEqual(DockerEngine(sys.executable).execute(['-c','print("fixture output")']),'fixture output')

    def test_command_failure_and_oversized_output_are_not_an_absent_container(self):
        engine=DockerEngine('selected-docker')
        with patch('browser_engine.subprocess.run',return_value=Mock(returncode=0,stdout=' result ')) as run:
            self.assertEqual(engine.execute(['version']),'result');self.assertEqual(run.call_args.args[0],['selected-docker','version'])
        with patch('browser_engine.subprocess.run',return_value=Mock(returncode=0,stdout='x'*1024**2)):self.assertEqual(len(engine.execute(['version'])),1024**2)
        for result in (Mock(returncode=1,stdout=''),Mock(returncode=0,stdout='x'*(1024**2+1))):
            with patch('browser_engine.subprocess.run',return_value=result),self.assertRaises((RuntimeError,ValueError)):engine.execute(['inspect'])

    def test_component_check_requires_a_valid_image_and_an_actual_browser_version(self):
        calls=[];engine=DockerEngine('selected-docker',runner=lambda args:(calls.append(args) or 'Chromium 156.0.8078.47 fixture'))
        self.assertIn('156.0',engine.check('sha256:'+'a'*64));self.assertEqual(calls[0][calls[0].index('--network')+1],'none')
        with self.assertRaises(ValueError):engine.check('image:latest')
        with self.assertRaises(ValueError):DockerEngine(runner=lambda args:'not a browser').check('sha256:'+'a'*64)

    def test_inspection_schema_and_profile_user_mapping_are_checked_independently(self):
        from browser_engine import checked_container
        row={'Id':ID,'Name':'/expected','Config':{}}
        for value in ({},[],[row,row],[None],[{**row,'Config':None}]):
            with self.assertRaises(ValueError):checked_container(value,ID,None)
        with self.assertRaises(ValueError):checked_container([row],ID,'different')
        for uid in (0,2**32-2):
            self.assertEqual(DockerEngine(runner=lambda args:json.dumps([uid,uid])).profile_user(type('Context',(),{'profile':Path('/profile'),'image_id':'sha256:'+'a'*64})()),f'{uid}:{uid}')
        for ids in (None,{},[],[1],[1,2,3],[True,1],[-1,0],[2**32-1,0],[0,1.5]):
            with self.assertRaises(ValueError):DockerEngine(runner=lambda args:json.dumps(ids)).profile_user(type('Context',(),{'profile':Path('/profile'),'image_id':'sha256:'+'a'*64})())

    def test_running_browser_gets_a_graceful_stop_before_container_removal(self):
        row={'Id':ID,'Name':'/owned','Image':'sha256:'+'c'*64,'Config':{'Labels':{}},'State':{'Running':True}};calls=[]
        def run(args):
            calls.append(args)
            if args[0]=='container':return ID
            if args[0]=='inspect':return json.dumps([row])
            if args[0]=='stop':row['State']['Running']=False
            return ID
        DockerEngine('docker',runner=run).remove(ID)
        self.assertLess(next(i for i,a in enumerate(calls) if a[0]=='stop'),next(i for i,a in enumerate(calls) if a[0]=='rm'))

    def test_absent_container_is_distinct_from_daemon_failure_or_invalid_output(self):
        calls=[]
        def run(args):calls.append(args);return ''
        engine=DockerEngine('docker',runner=run)
        self.assertIsNone(engine.inspect(name='ai-phone-browser-'+'a'*24))
        self.assertIn('name=^/ai-phone-browser-'+'a'*24+'$',calls[0])
        with self.assertRaises(OSError):DockerEngine('docker',runner=lambda args:(_ for _ in ()).throw(OSError('offline'))).inspect(identity=ID)
        for output in ('invalid','\n'.join([ID,ID])):
            with self.assertRaises(ValueError):DockerEngine('docker',runner=lambda args:output).inspect(identity=ID)

    def test_inspection_must_return_the_selected_immutable_identity(self):
        row={'Id':ID,'Name':'/owned','Image':'sha256:'+'c'*64,'Config':{'Labels':{}}}
        def run(args):return ID if args[0]=='container' else json.dumps([row])
        engine=DockerEngine('docker',runner=run)
        self.assertEqual(engine.inspect(identity=ID)['Id'],ID)
        row['Id']='d'*64
        with self.assertRaises(ValueError):engine.inspect(identity=ID)
        for args in ({},{'identity':'not-an-id'},{'name':'another-project'},{'name':'ai-phone-browser-'+'a'*24,'identity':ID}):
            with self.assertRaises(ValueError):engine.inspect(**args)

    def test_create_has_no_debug_port_or_host_socket_and_csv_preserves_profile_paths(self):
        with tempfile.TemporaryDirectory(prefix='aiphone, browser-') as temporary:
            context=BrowserContext(Path(temporary),'fixture','ai-phone-bundle/desktop:sha-'+'a'*64);calls=[]
            engine=DockerEngine('docker',runner=lambda args:(calls.append(args) or ('[1000,1000]' if args[0]=='run' else ID)),network='none')
            self.assertEqual(engine.create(context,{'token':'e'*64},['--user-agent=fixture']),ID)
            command=next(args for args in calls if args[0]=='create');self.assertIn(context.image_id,command);self.assertEqual(command[command.index('--pull')+1],'never')
            self.assertEqual(command[command.index('--user')+1],'1000:1000')
            self.assertEqual(command[command.index('--hostname')+1],context.name)
            probe=next(args for args in calls if args[0]=='run');self.assertIn('readonly',next(csv.reader(io.StringIO(probe[probe.index('--mount')+1]))))
            mount=next(csv.reader(io.StringIO(command[command.index('--mount')+1])))
            self.assertIn('source='+str(context.profile),mount);self.assertIn('target=/profile',mount)
            self.assertNotIn('--publish',command);self.assertFalse(any('docker.sock' in arg for arg in command))
            self.assertEqual(command[command.index('--network')+1],'none')


if __name__=='__main__':unittest.main()
