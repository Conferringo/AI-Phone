"""The real media suite runs only against the verified fixture image identity."""
import subprocess
import tempfile
from pathlib import Path
import unittest
from unittest.mock import patch
import linux_host_acceptance as host

class NativeMediaAcceptanceTests(unittest.TestCase):
    def test_verified_image_and_selected_engine_reach_the_unchanged_real_suite(self):
        with tempfile.TemporaryDirectory() as folder:
            staged=Path(folder);calls=[]
            def run(argv,**options):
                calls.append((argv,options))
                if argv[:3]==['docker','container','inspect']:
                    self.assertEqual(argv[-1],'owned-container-id')
                    return subprocess.CompletedProcess(argv,0,'sha256:'+'a'*64+'\n','')
                self.assertEqual(argv[-1],'test_desktop_media')
                self.assertEqual(options['cwd'],staged/'scripts')
                self.assertEqual(options['env']['DOCKER_HOST'],'unix:///owned.sock')
                self.assertEqual(options['env']['AI_PHONE_TEST_DESKTOP_IMAGE'],'sha256:'+'a'*64)
                self.assertEqual(options['env']['AI_PHONE_TEST_DESKTOP_MEDIA'],'1')
                self.assertNotIn('AI_PHONE_TEST_GUEST_BINARY',options['env'])
                return subprocess.CompletedProcess(argv,0)
            with patch('runtime_config.runtime_env',return_value={'DOCKER_HOST':'unix:///owned.sock'}),patch('client_host.verify_container_ownership',return_value={'ai-phone-desktop':{'id':'owned-container-id'}}),patch('linux_host_acceptance.subprocess.run',side_effect=run):
                host.verify_native_media(staged,{'AI_PHONE_TEST_GUEST_BINARY':'/unowned/program'})
            self.assertEqual(len(calls),2)

    def test_missing_or_invalid_image_never_starts_the_media_suite(self):
        for containers,identity in [({},''),({'ai-phone-desktop':{'id':'owned'}},'mutable-tag')]:
            with patch('runtime_config.runtime_env',return_value={}),patch('client_host.verify_container_ownership',return_value=containers),patch('linux_host_acceptance.subprocess.run',return_value=subprocess.CompletedProcess([],0,identity,'')) as run:
                with self.assertRaises(ValueError):host.verify_native_media(Path('/owned-fixture'),{})
                self.assertLessEqual(run.call_count,1)

if __name__=='__main__':unittest.main()
