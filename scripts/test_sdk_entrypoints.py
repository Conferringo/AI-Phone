"""Read-only Java entry-point check with isolated Android preferences."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from android_sdk import command
from runtime_config import runtime_env


class SdkEntrypointTests(unittest.TestCase):
    def test_sdkmanager_keeps_its_different_vendor_property(self):
        root=Path(__file__).resolve().parent.parent;env={**os.environ,**runtime_env(root)}
        manager=Path(env['AI_PHONE_SDK_ROOT'])/'cmdline-tools/latest/bin/sdkmanager'
        if not manager.is_file() or not (Path(env['JAVA_HOME'])/'bin/java').is_file():self.skipTest('Reviewed local SDK/JDK unavailable')
        result=subprocess.run(command(manager,env,['--version']),env=env,capture_output=True,text=True,timeout=20)
        self.assertEqual(result.returncode,0,result.stderr)
        self.assertRegex(result.stdout,r'\d+\.\d+')
    def test_avdmanager_receives_its_required_tools_property_with_spaced_paths(self):
        root=Path(__file__).resolve().parent.parent;env={**os.environ,**runtime_env(root)}
        manager=Path(env['AI_PHONE_SDK_ROOT'])/'cmdline-tools/latest/bin/avdmanager'
        if not manager.is_file() or not (Path(env['JAVA_HOME'])/'bin/java').is_file():self.skipTest('Reviewed local SDK/JDK unavailable')
        with tempfile.TemporaryDirectory(prefix='ai-phone-sdk preferences-') as temp:
            env.update(ANDROID_USER_HOME=temp,ANDROID_EMULATOR_HOME=temp,ANDROID_AVD_HOME=str(Path(temp)/'avds'),ANDROID_HOME=env['AI_PHONE_SDK_ROOT'])
            result=subprocess.run(command(manager,env,['list','avd']),env=env,cwd=temp,capture_output=True,text=True,timeout=20)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('Available Android Virtual Devices',result.stdout)


if __name__=='__main__':unittest.main()
