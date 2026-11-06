"""Linux builds and inspects one DEB; APT reuses that package."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import build_client as client

class LinuxClientPackageTests(unittest.TestCase):
    def fixture(self,root,arch):
        source=root/f'apps/client/src-tauri/target/release/bundle/deb/AI Phone_fixture_{arch}.deb'
        source.parent.mkdir(parents=True);source.write_bytes(b'deb');(root/'dist').mkdir()

    def test_compiles_only_deb_and_inspects_it_before_reporting(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.fixture(root,'amd64');env={'CI':'true'}
            with patch.object(client.platform,'system',return_value='Linux'),patch.object(client.platform,'machine',return_value='x86_64'),patch.object(client.subprocess,'run') as run,patch.object(client,'inspect_deb',return_value={}) as inspect,patch.object(client,'record',return_value={'server_included':True,'complete_device_bundle':True}) as record:
                value=client.build_linux(root,'fixture',env,['node','owned-tauri.js'],False)
                arguments=run.call_args.args[0]
                self.assertEqual(arguments[arguments.index('--bundles')+1],'deb')
                self.assertEqual(run.call_args.kwargs['env'],env)
                inspect.assert_called_once_with(root/'dist/AI-Phone-fixture-linux-amd64.deb',root)
                record.assert_called_once();self.assertEqual(len(value),1)
                self.assertTrue(value[0]['complete_device_bundle'])

    def test_existing_arm_deb_does_not_recompile(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);self.fixture(root,'arm64')
            with patch.object(client.platform,'system',return_value='Linux'),patch.object(client.platform,'machine',return_value='aarch64'),patch.object(client.subprocess,'run') as run,patch.object(client,'inspect_deb',return_value={}),patch.object(client,'record',return_value={'server_included':False,'complete_device_bundle':False}):
                value=client.build_linux(root,'fixture',{},['node','owned-tauri.js'],True)
                run.assert_not_called();self.assertEqual(len(value),1)
                self.assertEqual((root/'dist/AI-Phone-fixture-linux-arm64.deb').read_bytes(),b'deb')

if __name__=='__main__':unittest.main()
