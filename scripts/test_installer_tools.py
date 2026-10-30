"""A bundled installer starts without host tools and keeps immutable runtimes."""
import hashlib
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from installer_tools import build,verify,select,selected_environment,prepare_resources


class InstallerToolTests(unittest.TestCase):
    def _build(self,*args,**kwargs):
        # Synthetic fixtures abstract physical disk size. The guard is tested
        # explicitly below; real component acceptance uses the physical reserve.
        with patch('installer_tools.shutil.disk_usage',return_value=type('Usage',(),{'free':16*1024**3})()):
            return build(*args,**kwargs)

    def fixture(self,root):
        archives=root/'archives';archives.mkdir();pins={}
        for role,entries in {
            'python':[('python/bin/python3.13',b'python fixture',0o755),('python/lib/library.py',b'pass\n',0o644),('python/LICENSE',b'Python licence fixture',0o644)],
            'node':[('node-version/bin/node',b'node fixture',0o755),('node-version/LICENSE',b'Node licence fixture',0o644),('node-version/lib/node_modules/npm/index.js',b'not a runtime dependency',0o644)]}.items():
            path=archives/(role+'.tar.gz')
            with tarfile.open(path,'w:gz') as target:
                for name,data,mode in entries:
                    entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=mode;target.addfile(entry,io.BytesIO(data))
                if role=='python':
                    entry=tarfile.TarInfo('python/bin/python3');entry.type=tarfile.SYMTYPE;entry.linkname='python3.13';target.addfile(entry)
            pins[role]={'file':path.name,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'size_bytes':path.stat().st_size,'version':'3.13.18' if role=='python' else '22.23.4','url':'https://example.invalid/'+role}
        return archives,pins

    def test_build_materializes_owned_aliases_without_npm_or_user_files(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archives,pins=self.fixture(root);target=root/'component'
            self._build(archives,target,pins=pins,host='macos-arm64')
            manifest=verify(target,host='macos-arm64')
            self.assertEqual((target/'python/bin/python3').read_bytes(),b'python fixture');self.assertFalse((target/'python/bin/python3').is_symlink())
            self.assertFalse((target/'node/lib/node_modules').exists());self.assertEqual(set(manifest['tools']),{'python','node'})
            self.assertEqual(manifest['tools']['python']['version'],'3.13.18')
            self.assertEqual({row['path'] for row in manifest['files']},{p.relative_to(target).as_posix() for p in target.rglob('*') if p.is_file()}-{'manifest.json'})

    def test_unknown_or_changed_runtime_never_replaces_a_selection(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archives,pins=self.fixture(root);target=root/'component';self._build(archives,target,pins=pins,host='macos-arm64')
            installation=root/'installation';installation.mkdir();(installation/'user-chat').write_text('keep')
            with patch('installer_tools.host_key',return_value='macos-arm64'):
                env=select(installation,target);self.assertEqual(env['AI_PHONE_TOOL_RUNTIME'],str(target.resolve()))
                self.assertEqual(selected_environment(installation),env);self.assertEqual(env['PYTHONDONTWRITEBYTECODE'],'1')
                before=(installation/'data/deployment/tool-runtime.json').read_bytes()
                (target/'node/bin/node').write_bytes(b'changed')
                with self.assertRaises(ValueError):select(installation,target)
                with self.assertRaises(ValueError):selected_environment(installation)
                self.assertEqual((installation/'data/deployment/tool-runtime.json').read_bytes(),before)
            self.assertEqual((installation/'user-chat').read_text(),'keep')

    def test_service_path_prefers_verified_tools_over_host_and_project_executables(self):
        import os
        from runtime_config import runtime_env
        with tempfile.TemporaryDirectory() as folder,patch('installer_tools.host_key',return_value='macos-arm64'):
            root=Path(folder);archives,pins=self.fixture(root);target=root/'component';self._build(archives,target,pins=pins,host='macos-arm64');select(root,target)
            with patch.dict(os.environ,{'PATH':'/untrusted/host','AI_PHONE_MANAGED_BIN':'/owned/docker-wrapper'}):env=runtime_env(root)
            self.assertEqual(env['PATH'].split(os.pathsep)[:4],['/owned/docker-wrapper',str(target.resolve()/'python/bin'),str(target.resolve()/'node/bin'),str(root/'.tools/bin')])

    def test_alias_escapes_cycles_and_unsupported_members_leave_no_output(self):
        for kind in ('escape','cycle','device','path'):
            with tempfile.TemporaryDirectory() as folder:
                root=Path(folder);archives,pins=self.fixture(root);archive=archives/'python.tar.gz'
                with tarfile.open(archive,'w:gz') as target:
                    if kind=='cycle':
                        for name,to in [('one','two'),('two','one')]:
                            entry=tarfile.TarInfo('python/'+name);entry.type=tarfile.SYMTYPE;entry.linkname=to;target.addfile(entry)
                    else:
                        entry=tarfile.TarInfo('python/../outside' if kind=='path' else 'python/alias');entry.type=tarfile.CHRTYPE if kind=='device' else tarfile.SYMTYPE;entry.linkname='/tmp/outside';target.addfile(entry)
                pins['python'].update(size_bytes=archive.stat().st_size,sha256=hashlib.sha256(archive.read_bytes()).hexdigest())
                with self.assertRaises(ValueError):self._build(archives,root/'component',pins=pins,host='macos-arm64')
                self.assertFalse((root/'component').exists());self.assertEqual(list(root.glob('.tool-build-*')),[])

    def test_manifest_paths_types_counts_and_permissions_are_admission_boundaries(self):
        from installer_tools import checked_manifest
        from copy import deepcopy
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archives,pins=self.fixture(root);target=root/'component';self._build(archives,target,pins=pins,host='macos-arm64');manifest=json.loads((target/'manifest.json').read_text())
            for changed in (None,[],{}, {**manifest,'format':True},{**manifest,'tools':{}},{**manifest,'files':[]},{**manifest,'files':manifest['files']*2}):
                with self.assertRaises(ValueError):checked_manifest(changed,'macos-arm64')
            for path in ('../private','/absolute','python//file','other/bin/tool','python/./file','python\\escape'):
                changed=deepcopy(manifest);changed['files'][0]['path']=path
                with self.assertRaises(ValueError):checked_manifest(changed,'macos-arm64')
            for field,value in [('mode',0o777),('size',True),('sha256','not-a-hash')]:
                changed=deepcopy(manifest);changed['files'][0][field]=value
                with self.assertRaises(ValueError):checked_manifest(changed,'macos-arm64')
            for descriptor in (None,[],{'version':False},{'version':'1.2'},{'version':'3.13.18','executable':[]},{'version':'3.13.18','executable':'node/bin/node'}):
                changed=deepcopy(manifest);changed['tools']['python']=descriptor
                with self.assertRaises(ValueError):checked_manifest(changed,'macos-arm64')

    def test_failed_resource_copy_reclaims_only_new_generated_files(self):
        with tempfile.TemporaryDirectory() as folder,patch('installer_tools.host_key',return_value='macos-arm64'):
            root=Path(folder);archives,pins=self.fixture(root);target=root/'component';self._build(archives,target,pins=pins,host='macos-arm64');generated=root/'generated';generated.mkdir()
            for partial in (False,True):
                def copy(*_,**__):
                    if partial:(generated/'tools/partial').write_text('incomplete')
                    raise OSError('fixture copy failure')
                with patch('installer_tools.shutil.copytree',side_effect=copy),self.assertRaisesRegex(OSError,'copy failure'):prepare_resources(generated,target)
                self.assertFalse((generated/'tools').exists());verify(target,host='macos-arm64')

    def test_unknown_extra_files_and_foreign_hosts_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archives,pins=self.fixture(root);target=root/'component';self._build(archives,target,pins=pins,host='macos-arm64')
            with self.assertRaises(ValueError):verify(target,host='linux-x64')
            (target/'unlisted').write_bytes(b'unknown')
            with self.assertRaises(ValueError):verify(target,host='macos-arm64')

    def test_bad_archive_checksum_leaves_no_partial_component(self):
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archives,pins=self.fixture(root);(archives/'python.tar.gz').write_bytes(b'bad')
            with self.assertRaises(ValueError):self._build(archives,root/'component',pins=pins,host='macos-arm64')
            self.assertFalse((root/'component').exists());self.assertEqual(list(root.glob('.tool-build-*')),[])

    def test_physical_reserve_blocks_build_and_copy_without_changing_old_files(self):
        from installer_tools import RESERVE,MAX_BYTES
        with tempfile.TemporaryDirectory() as folder,patch('installer_tools.host_key',return_value='macos-arm64'):
            root=Path(folder);archives,pins=self.fixture(root)
            with patch('installer_tools.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE+MAX_BYTES-1})()):
                with self.assertRaisesRegex(ValueError,'4 GiB'):build(archives,root/'blocked',pins=pins,host='macos-arm64')
            self.assertFalse((root/'blocked').exists())
            target=root/'component';self._build(archives,target,pins=pins,host='macos-arm64');generated=root/'generated';generated.mkdir()
            with patch('installer_tools.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE-1})()):
                with self.assertRaisesRegex(ValueError,'4 GiB'):prepare_resources(generated,target)
            self.assertFalse((generated/'tools').exists());verify(target,host='macos-arm64')
            with patch('installer_tools.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE+MAX_BYTES})()):build(archives,root/'exact-space',pins=pins,host='macos-arm64')
            size=sum(row['size'] for row in verify(target,host='macos-arm64')['files'])
            with patch('installer_tools.shutil.disk_usage',return_value=type('Usage',(),{'free':RESERVE+size})()):prepare_resources(generated,target)

    def test_prepared_resource_inventory_has_one_manifest_and_never_overwrites(self):
        with tempfile.TemporaryDirectory() as folder,patch('installer_tools.host_key',return_value='macos-arm64'):
            root=Path(folder);archives,pins=self.fixture(root);target=root/'component';self._build(archives,target,pins=pins,host='macos-arm64')
            generated=root/'generated';generated.mkdir();digest=prepare_resources(generated,target)
            self.assertEqual(set(digest),{'tools/manifest.json'});self.assertEqual(prepare_resources(generated,target),digest)
            (generated/'tools/python/bin/python3').write_bytes(b'changed')
            with self.assertRaises(ValueError):prepare_resources(generated,target)

    def test_archive_identity_fields_are_independently_required(self):
        from installer_tools import archive_inputs
        from copy import deepcopy
        with tempfile.TemporaryDirectory() as folder:
            root=Path(folder);archives,pins=self.fixture(root)
            changed=deepcopy(pins);changed['python']['size_bytes']+=1
            with self.assertRaises(ValueError):archive_inputs(archives,changed)
            original=archives/'python.tar.gz';alias=archives/'alias.tar.gz';alias.symlink_to(original)
            changed=deepcopy(pins);changed['python']['file']=alias.name
            with self.assertRaises(ValueError):archive_inputs(archives,changed)
            changed=deepcopy(pins);changed['python']['file']='../archives/python.tar.gz'
            with self.assertRaises(ValueError):archive_inputs(archives,changed)

    def test_default_factory_policy_and_nested_output_are_used(self):
        with tempfile.TemporaryDirectory() as folder,patch('installer_tools.host_key',return_value='macos-arm64'):
            root=Path(folder);archives,pins=self.fixture(root);(root/'config').mkdir();(root/'config/installer-tools.json').write_text(json.dumps({'hosts':{'macos-arm64':pins}}))
            with patch('installer_tools.__file__',str(root/'scripts/installer_tools.py')):
                result=self._build(archives,root/'new/nested/component')
            self.assertEqual(result['host'],'macos-arm64');verify(root/'new/nested/component',host='macos-arm64')

    def test_inventory_header_and_byte_limits_are_inclusive_and_early(self):
        from installer_tools import bounded_entries,entry_name,inventory_records,tree_inventory
        directory=tarfile.TarInfo('python');directory.type=tarfile.DIRTYPE
        entry=tarfile.TarInfo('python/file');entry.size=4
        with patch('installer_tools.MAX_BYTES',4),patch('installer_tools.MAX_FILES',2):
            self.assertEqual(len(list(bounded_entries([directory,entry],'python'))),2)
            with self.assertRaises(ValueError):list(bounded_entries([directory,directory],'python'))
            with self.assertRaises(ValueError):list(bounded_entries([directory,entry,tarfile.TarInfo('python/extra')],'python'))
            for size in (-1,5):
                entry.size=size
                with self.assertRaises(ValueError):list(bounded_entries([entry],'python'))
            row={'path':'python/data','size':4,'sha256':'0'*64,'mode':0o644}
            self.assertEqual(len(inventory_records([row])),1)
            with self.assertRaises(ValueError):inventory_records([])
            self.assertEqual(len(inventory_records([{**row,'size':2},{**row,'path':'node/data','size':2}])),2)
            with self.assertRaises(ValueError):inventory_records([{**row,'size':5}])
            with tempfile.TemporaryDirectory() as folder:
                root=Path(folder);(root/'one').touch();(root/'two').touch();self.assertEqual(tree_inventory(root),{'one','two'})
                (root/'three').touch();(root/'four').touch();self.assertEqual(len(tree_inventory(root)),4)
                (root/'five').touch()
                with self.assertRaises(ValueError):tree_inventory(root)
        with self.assertRaises(ValueError):entry_name(tarfile.TarInfo('wrong/file'),'python','wrong')
        with self.assertRaises(ValueError):entry_name(tarfile.TarInfo('python'),'python','python')
        from installer_tools import tool_path
        deepest='python/'+'/'.join(['part']*63);self.assertEqual(len(tool_path(deepest).parts),64)
        with self.assertRaises(ValueError):tool_path(deepest+'/extra')

    def test_manifest_size_and_selection_shapes_fail_without_host_fallback(self):
        from installer_tools import read_manifest
        with tempfile.TemporaryDirectory() as folder,patch('installer_tools.host_key',return_value='macos-arm64'):
            root=Path(folder);archives,pins=self.fixture(root);component=root/'component';self._build(archives,component,pins=pins,host='macos-arm64')
            path=component/'manifest.json';data=path.read_bytes();limit=4*1024**2;path.write_bytes(data+b' '*(limit-len(data)));read_manifest(component)
            with path.open('ab') as file:file.write(b' ')
            with self.assertRaises(ValueError):read_manifest(component)
            path.write_bytes(data);select(root,component);selection=root/'data/deployment/tool-runtime.json';original=selection.read_text()
            for value in (None,[],{}, {'format':True,'path':str(component)},{'format':1,'path':False},{'format':1,'path':'relative'}):
                selection.write_text(json.dumps(value))
                with self.assertRaises(ValueError):selected_environment(root)
            changed=json.loads(original);changed['sha256']='0'*64;selection.write_text(json.dumps(changed))
            with self.assertRaises(ValueError):selected_environment(root)


if __name__=='__main__':unittest.main()
