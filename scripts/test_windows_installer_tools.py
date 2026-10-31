"""Windows installer tools are sealed resources, never user/global installations."""
import hashlib
import io
import json
from pathlib import Path,PurePosixPath
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch

from installer_tools import build,verify,environment,checked_manifest


class WindowsInstallerToolsTests(unittest.TestCase):
    def fixture(self,root):
        folder=root/'inputs';folder.mkdir();python=folder/'python.tar.gz';node=folder/'node.zip'
        with tarfile.open(python,'w:gz') as target:
            for name,data in [('python/python.exe',b'python fixture'),('python/Lib/fixture.py',b'pass\n'),('python/LICENSE.txt',b'PSF fixture')]:
                entry=tarfile.TarInfo(name);entry.mode=0o755 if name.endswith('.exe') else 0o644;entry.size=len(data);target.addfile(entry,io.BytesIO(data))
        with zipfile.ZipFile(node,'w') as target:
            for name,data in [('node.exe',b'node fixture'),('LICENSE',b'Node license'),('README.md',b'Node readme'),('node_modules/npm/private',b'not runtime')]:target.writestr('node-v22.23.4-win-x64/'+name,data)
        pins={role:{'file':path.name,'version':'3.13.18' if role=='python' else '22.23.4','sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'size_bytes':path.stat().st_size,'url':'https://example.invalid/'+path.name} for role,path in [('python',python),('node',node)]}
        return folder,pins

    def test_windows_component_owns_its_executables_and_omits_npm(self):
        with tempfile.TemporaryDirectory() as folder,patch('installer_tools.shutil.disk_usage',return_value=type('Usage',(),{'free':16*1024**3})()):
            root=Path(folder);inputs,pins=self.fixture(root);target=root/'component'
            build(inputs,target,pins=pins,host='windows-x64');manifest=verify(target,host='windows-x64')
            self.assertEqual(manifest['tools']['python']['executable'],'python/python.exe')
            self.assertEqual(manifest['tools']['node']['executable'],'node/node.exe')
            self.assertFalse(any('npm' in row['path'] for row in manifest['files']))
            env=environment(target,manifest,base_path='fixture-path')
            self.assertEqual(env['AI_PHONE_PYTHON'],str(target.resolve()/'python/python.exe'))
            self.assertEqual(env['PATH'].split(__import__('os').pathsep)[:2],[str(target.resolve()/'python'),str(target.resolve()/'node')])
            first=PurePosixPath(manifest['files'][0]['path'])
            duplicate={**manifest['files'][0],'path':first.with_name(first.name.swapcase()).as_posix()}
            with self.assertRaises(ValueError):checked_manifest({**manifest,'files':[*manifest['files'],duplicate]},'windows-x64')
            (target/'node/node.exe').write_bytes(b'corrupt')
            with self.assertRaises(ValueError):verify(target,host='windows-x64')

    def test_windows_node_rejects_duplicate_alias_or_empty_selected_files(self):
        for variant in ('duplicate','alias','empty'):
            with tempfile.TemporaryDirectory() as folder,patch('installer_tools.shutil.disk_usage',return_value=type('Usage',(),{'free':16*1024**3})()):
                root=Path(folder);inputs,pins=self.fixture(root);node=inputs/'node.zip'
                with zipfile.ZipFile(node,'w') as target:
                    for name,data in [('node.exe',b'' if variant=='empty' else b'fixture'),('LICENSE',b'license'),('README.md',b'readme')]:
                        entry=zipfile.ZipInfo('node-v22.23.4-win-x64/'+name)
                        if variant=='alias' and name=='node.exe':entry.external_attr=0o120777<<16
                        target.writestr(entry,data)
                    if variant=='duplicate':
                        with self.assertWarns(UserWarning):target.writestr('node-v22.23.4-win-x64/node.exe',b'duplicate')
                pins['node'].update(sha256=hashlib.sha256(node.read_bytes()).hexdigest(),size_bytes=node.stat().st_size)
                with self.subTest(variant=variant),self.assertRaisesRegex(ValueError,'Windows Node'):build(inputs,root/'component',pins=pins,host='windows-x64')
                self.assertFalse((root/'component').exists());self.assertEqual(list(root.glob('.tool-build-*')),[])


if __name__=='__main__':unittest.main()
