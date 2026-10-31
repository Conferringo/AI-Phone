"""Prebuilt JS launches are source-bound and never fall back after corruption."""
import hashlib
import json
import os
from pathlib import Path,PurePosixPath
import shutil
import tempfile
import unittest
from host_platform import host_key


class JavaScriptRuntimeTests(unittest.TestCase):
    def test_aliases_are_created_by_staging_and_rechecked_on_reuse(self):
        from prebuilt_js import PREFIX,MANIFEST
        from client_host_entry import component_links,stage_source
        from prepare_client_host import prepare
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();value=self.fixture(root);name=PREFIX+'node_modules/.store/library/index.js';path=root/name;path.parent.mkdir(parents=True);path.write_text('module.exports=1;')
            value['files'].append({'path':name,'size':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'mode':0o644})
            value['links']=[{'path':PREFIX+'node_modules/library','target':'.store/library'}];(root/MANIFEST).write_text(json.dumps(value))
            cache=root/'.tools/js-services';shutil.copytree(root/'runtime/js-services',cache/'runtime/js-services');component_links(cache,MANIFEST,PREFIX.rstrip('/'),create=True)
            shutil.copyfile(Path(__file__).parent/'client_host_entry.py',root/'scripts/client_host_entry.py')
            prepare(root,with_js_services=True);archive=root/'apps/client/src-tauri/gen/host/source.payload';home=root/'installation';staged=stage_source(archive,home);(home/'user/data/chat').write_text('keep')
            alias=staged/PREFIX/'node_modules/library';self.assertEqual((alias/'index.js').read_text(),'module.exports=1;');self.assertEqual(stage_source(archive,home),staged)
            alias.unlink();alias.symlink_to(root,target_is_directory=True)
            with self.assertRaises(ValueError):stage_source(archive,home)
            self.assertEqual((home/'user/data/chat').read_text(),'keep')

    def test_failed_or_cancelled_private_copy_never_publishes_partial_bun(self):
        from prebuilt_js import private_bun,RESERVE
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root)
            with patch('prebuilt_js.shutil.disk_usage',return_value=type('Space',(),{'free':RESERVE})()):
                with self.assertRaises(ValueError):private_bun(root,create=True)
            for error in (OSError('copy failed'),KeyboardInterrupt()):
                with patch('prebuilt_js.shutil.copyfileobj',side_effect=error):
                    with self.assertRaises(type(error)):private_bun(root,create=True)
                self.assertEqual(list((root/'.tools/bin').iterdir()),[])

    def test_native_target_and_windows_permission_intent_are_explicit(self):
        from prebuilt_js import validate_native,resource_mode,permissions_match
        from unittest.mock import patch
        from types import SimpleNamespace
        name='runtime/js-services/node_modules/addon.node'
        with patch('prebuilt_js.host_key',return_value='macos-arm64'),patch('macos_target.minimum_version',return_value='14.4'):
            validate_native(Path('/fixture'),{name}, {name:'11.0'})
            for value in ({},{name:'26.0'},{name:True},{name:'invalid'}):
                with self.assertRaises(ValueError):validate_native(Path('/fixture'),{name},value)
        path=SimpleNamespace(suffix='.js',stat=lambda:SimpleNamespace(st_mode=0o666));self.assertEqual(resource_mode(path,'nt'),0o644);self.assertTrue(permissions_match(path,0o644,'nt'))
        readonly=SimpleNamespace(stat=lambda:SimpleNamespace(st_mode=0o444));self.assertFalse(permissions_match(readonly,0o644,'nt'))

    def test_source_snapshot_is_frozen_and_excludes_private_environment(self):
        from prebuilt_js import snapshot_sources,source_fingerprint
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root);(root/'apps/channels/.env').write_text('private environment')
            frozen=root/'frozen';snapshot_sources(root,frozen);self.assertFalse((frozen/'apps/channels/.env').exists())
            self.assertEqual(source_fingerprint(frozen),source_fingerprint(root));original=(frozen/'apps/channels/server.ts').read_text();(root/'apps/channels/server.ts').write_text('changed during build')
            self.assertEqual((frozen/'apps/channels/server.ts').read_text(),original);self.assertNotEqual(source_fingerprint(frozen),source_fingerprint(root))

    def test_browser_helper_changes_invalidate_the_compiled_consumer(self):
        from prebuilt_js import snapshot_sources,source_fingerprint
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root);helper=root/'scripts/channel_browser.py';helper.write_text('original browser contract')
            frozen=root/'frozen';snapshot_sources(root,frozen);self.assertEqual((frozen/'scripts/channel_browser.py').read_text(),'original browser contract')
            self.assertEqual(source_fingerprint(root),source_fingerprint(frozen));helper.write_text('changed browser contract')
            self.assertNotEqual(source_fingerprint(root),source_fingerprint(frozen))

    def test_prebuilt_dependencies_skip_package_install_and_preserve_existing_tools(self):
        from prebuilt_js import private_bun
        from install_state import InstallState
        from release import install_source_components,bootstrap_valid
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root)
            self.assertIsNone(private_bun(root))
            with patch('release.run') as run,patch('release.native_apps',return_value=True),patch('release.messaging_browser',return_value=True):
                with InstallState(root) as state:install_source_components(state,root,{})
                run.assert_not_called()
            self.assertTrue(bootstrap_valid(root));self.assertFalse((root/'apps/web/node_modules').exists())
            binary=private_bun(root);binary.write_text('unowned tool')
            with self.assertRaises(ValueError):private_bun(root,create=True)
            self.assertEqual(binary.read_text(),'unowned tool')

    def fixture(self,root):
        from prebuilt_js import ENTRIES,PREFIX,source_fingerprint
        (root/'config').mkdir();(root/'config/bun-runtime.json').write_text('{"version":"1.4.4"}')
        for source in ENTRIES.values():
            path=root/source;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('// source fixture\n')
        folder=root/PREFIX;folder.mkdir(parents=True)
        records=[]
        for source in ENTRIES.values():
            name=PREFIX+str(PurePosixPath(source).with_suffix('.js'));path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('// bundled fixture\n');records.append({'path':name,'size':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'mode':0o644})
        value={'format':1,'host':host_key(),'bun_version':'1.4.4','source_sha256':source_fingerprint(root),'entries':{role:PREFIX+str(PurePosixPath(source).with_suffix('.js')) for role,source in ENTRIES.items()},'files':records,'links':[],'native_targets':{}}
        (folder/'manifest.json').write_text(json.dumps(value))
        binary=root/('runtime/bin/bun.exe' if os.name=='nt' else 'runtime/bin/bun');binary.parent.mkdir(parents=True);binary.write_text('fixture runtime');binary.chmod(0o755)
        web={'format':1,'host':host_key(),'services':['web'],'files':[{'path':binary.relative_to(root).as_posix(),'size':binary.stat().st_size,'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'mode':0o755}],'links':[]}
        server=root/'runtime/web/apps/web/server.js';server.parent.mkdir(parents=True);server.write_text('// web fixture');web['files'].append({'path':server.relative_to(root).as_posix(),'size':server.stat().st_size,'sha256':hashlib.sha256(server.read_bytes()).hexdigest(),'mode':0o644})
        (root/'runtime/manifest.json').write_text(json.dumps(web));return value

    def test_source_path_and_present_component_have_one_explicit_selection(self):
        from prebuilt_js import launch_spec
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.assertFalse(launch_spec(root,'media')['prebuilt'])
            self.fixture(root);spec=launch_spec(root,'media');self.assertTrue(spec['prebuilt']);self.assertEqual(spec['argv'][1:],["--no-install",str(root/'runtime/js-services/apps/device-stream/server.js')])
            (root/'runtime/js-services/apps/device-stream/server.js').write_text('changed')
            with self.assertRaises(ValueError):launch_spec(root,'channels')
            with self.assertRaises(ValueError):launch_spec(root,'unknown')

    def test_sources_and_unlisted_resources_cannot_reuse_a_component(self):
        from prebuilt_js import launch_spec
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root);source=root/'apps/channels/server.ts';original=source.read_text();source.write_text('changed')
            with self.assertRaises(ValueError):launch_spec(root,'channels')
            source.write_text(original);extra=root/'runtime/js-services/unexpected';extra.write_text('unexpected')
            with self.assertRaises(ValueError):launch_spec(root,'media')



class JavaScriptBuildTests(unittest.TestCase):
    fixture=JavaScriptRuntimeTests.fixture
    def test_compiler_failure_and_changed_sources_never_publish_a_component(self):
        from prebuilt_js import build,ENTRIES,PREFIX,RESERVE,MAX_BYTES,MAX_SOURCE
        from unittest.mock import patch
        from types import SimpleNamespace
        import subprocess
        for mode in ('compiler-failure','source-changed','prepublication-change','missing-protobuf','success'):
            with self.subTest(mode=mode),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary).resolve();self.fixture(root)
                config=root/'apps/client/src-tauri/tauri.conf.json';config.parent.mkdir(parents=True);config.write_text('{"bundle":{"macOS":{"minimumSystemVersion":"14.4"}}}')
                for name in ('apps/channels','apps/device-stream','apps/device-network'):(root/name/'package.json').write_text('{}')
                (root/'package.json').write_text('{"devDependencies":{"@bufbuild/protobuf":"2"}}');package=root/'node_modules/@bufbuild/protobuf';package.mkdir(parents=True);(package/'package.json').write_text('{"name":"@bufbuild/protobuf","version":"2.0.0"}')
                if mode=='missing-protobuf':shutil.rmtree(root/'node_modules')
                def compile_fixture(source,work,bun):
                    if mode=='compiler-failure':raise subprocess.CalledProcessError(1,['fixture-compiler'])
                    for entry in ENTRIES.values():
                        target=work/PREFIX/PurePosixPath(entry).with_suffix('.js');target.parent.mkdir(parents=True,exist_ok=True);target.write_text('// compiled snapshot\n')
                    if mode=='source-changed':(root/'apps/channels/server.ts').write_text('new edit')
                def version_probe(command,**kwargs):return '1.4.4\n' if kwargs.get('text') else b'1.4.4\n'
                original_manifest=__import__('prebuilt_js').build_manifest
                def manifest_fixture(source,work,graph,identity):
                    result=original_manifest(source,work,graph,identity)
                    if mode=='prepublication-change':(root/'apps/channels/server.ts').write_text('edit before publication')
                    return result
                with patch('prebuilt_js.build_manifest',side_effect=manifest_fixture),patch('bootstrap_bun.bun_path',return_value=root/'runtime/bin/bun'),patch('prebuilt_js.subprocess.check_output',side_effect=version_probe),patch('prebuilt_js.bundle',side_effect=compile_fixture),patch('prebuilt_js.shutil.disk_usage',return_value=SimpleNamespace(free=RESERVE+MAX_BYTES+MAX_SOURCE)):
                    if mode=='success':
                        value=build(root);self.assertEqual(len(value['entries']),5);self.assertEqual(build(root),value)
                    else:
                        with self.assertRaises((ValueError,subprocess.CalledProcessError)):build(root)
                        self.assertFalse((root/'.tools/js-services').exists())
                    self.assertEqual(list((root/'.tools').glob('.js-services-*')),[])

    def test_manifest_malformed_modes_and_missing_files_fail_without_source_fallback(self):
        from prebuilt_js import launch_spec,MANIFEST
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();value=self.fixture(root);path=root/MANIFEST
            for changes in ({'format':True},{'host':'other'},{'bun_version':'other'},{'source_sha256':'other'},{'entries':{}},{'files':None},{'files':[]},{'files':[value['files'][0],value['files'][0]]}):
                path.write_text(json.dumps({**value,**changes}))
                with self.assertRaises(ValueError):launch_spec(root,'media')
            path.write_text(json.dumps(value));entry=root/value['files'][0]['path'];entry.unlink()
            with self.assertRaises(ValueError):launch_spec(root,'channels')

    def test_source_and_component_budgets_and_compiler_pin_fail_early(self):
        from prebuilt_js import snapshot_sources,file_records,build,MAX_BYTES,RESERVE,MAX_SOURCE
        from unittest.mock import patch
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();value=self.fixture(root)
            with patch('prebuilt_js.MAX_SOURCE',0):
                with self.assertRaises(ValueError):snapshot_sources(root,root/'snapshot')
            with patch('prebuilt_js.MAX_BYTES',0):
                with self.assertRaises(ValueError):file_records(value['files'])
            for free,version in ((RESERVE+MAX_BYTES+MAX_SOURCE-1,'1.4.4'),(RESERVE+MAX_BYTES+MAX_SOURCE,'0.1.0')):
                with patch('bootstrap_bun.bun_path',return_value=root/'runtime/bin/bun'),patch('prebuilt_js.subprocess.check_output',return_value=version),patch('prebuilt_js.shutil.disk_usage',return_value=SimpleNamespace(free=free)):
                    with self.assertRaisesRegex(ValueError,'Keep 4 GiB free' if version=='1.4.4' else 'Use the pinned Bun'):build(root)
                self.assertFalse((root/'.tools/js-services').exists())
                self.assertEqual(list((root/'.tools').glob('.js-services-*')),[])


    def test_copy_inventory_and_staging_budgets_accept_the_exact_boundary(self):
        from prebuilt_js import copy_dependencies,file_records,snapshot_sources,source_inputs,actual_members
        from unittest.mock import patch
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();value=self.fixture(root);package=root/'node_modules/vendor';package.mkdir(parents=True);(package/'a.js').write_bytes(b'a');(package/'b.js').write_bytes(b'bb')
            graph=SimpleNamespace(source=root,packages={package:{'name':'vendor','version':'1'}},package_source=lambda path:path)
            for limit,count,passes in ((3,7,True),(2,7,False),(3,6,False)):
                work=root/f'work-{limit}-{count}'
                with patch('prebuilt_js.MAX_BYTES',limit),patch('prebuilt_js.MAX_FILES',count):
                    if passes:copy_dependencies(root,work,graph);self.assertEqual((work/'runtime/js-services/node_modules/vendor/b.js').read_bytes(),b'bb')
                    else:
                        with self.assertRaises(ValueError):copy_dependencies(root,work,graph)
            rows=[{**row,'size':0} for row in value['files']]
            with patch('prebuilt_js.MAX_FILES',len(rows)),patch('prebuilt_js.MAX_BYTES',0):self.assertEqual(len(file_records(rows)),len(rows))
            with self.assertRaises(ValueError):file_records([*rows,{**rows[0],'path':'runtime/web/unrelated.js'}])
            total=sum(len(data) for _,data,_ in source_inputs(root))
            with patch('prebuilt_js.MAX_SOURCE',total):snapshot_sources(root,root/'frozen')
            folder=root/'runtime/js-services';
            with patch('prebuilt_js.MAX_FILES',2):
                bounded=root/'bounded';bounded.mkdir()
                for i in range(6):(bounded/str(i)).touch()
                self.assertEqual(len(actual_members(bounded)),6);(bounded/'seventh').touch()
                with self.assertRaises(ValueError):actual_members(bounded)

    def test_generated_aliases_and_compiler_exit_status_are_preserved(self):
        from prebuilt_js import bundle,build_manifest,PREFIX,entries
        from unittest.mock import patch
        from types import SimpleNamespace
        import subprocess,sys
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root);work=root/'work';work.mkdir()
            compiler=root/'compiler.py';compiler.write_text('import pathlib,sys\np=next(x.removeprefix("--outfile=") for x in sys.argv if x.startswith("--outfile="))\npathlib.Path(p).write_text("// compiled")\nraise SystemExit(7 if "fail" in sys.argv else 0)\n')
            run=subprocess.run
            def invoke(command,**kwargs):
                self.assertIn('--no-install',command);self.assertIn('--packages=external',command)
                return run([sys.executable,str(compiler),*command[1:]],**kwargs)
            with patch('prebuilt_js.subprocess.run',side_effect=invoke):bundle(root,work,Path('fixture-compiler'));bundle(root,work,Path('fixture-compiler'))
            native=work/PREFIX/'node_modules/addon.node';native.parent.mkdir(parents=True);native.write_bytes(b'fixture-native-library')
            alias='apps/channels/node_modules/addon';graph=SimpleNamespace(aliases={alias:'node_modules/addon.node'})
            with patch('macos_target.verify_targets',side_effect=lambda r,paths:{name:'11.0' for name in paths}):value=build_manifest(root,work,graph,'0'*64)
            self.assertEqual((work/PREFIX/alias).read_bytes(),b'fixture-native-library');self.assertEqual(set(entries().values())-set(row['path'] for row in value['files']),set())
            def failed(command,**kwargs):return run([sys.executable,str(compiler),*command[1:],'fail'],**kwargs)
            with patch('prebuilt_js.subprocess.run',side_effect=failed):
                with self.assertRaises(subprocess.CalledProcessError):bundle(root,work,Path('fixture-compiler'))

    def test_private_bun_exact_reserve_and_binary_selection(self):
        from prebuilt_js import private_bun,launch_spec,RESERVE
        from unittest.mock import patch
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root);name='bun.exe' if os.name=='nt' else 'bun';source=root/'runtime/bin'/name
            with patch('prebuilt_js.shutil.disk_usage',return_value=SimpleNamespace(free=RESERVE+source.stat().st_size)):binary=private_bun(root,create=True)
            self.assertEqual(binary.name,name);self.assertEqual(launch_spec(root,'media')['argv'][0],str(source));self.assertEqual(binary.read_bytes(),source.read_bytes())
            if os.name!='nt':
                binary.chmod(0o644)
                with self.assertRaises(ValueError):private_bun(root)
            binary.unlink();binary.symlink_to(root/'missing')
            with self.assertRaises(ValueError):private_bun(root,create=True)

    def test_oversized_or_aliased_manifest_and_native_equal_target(self):
        from prebuilt_js import launch_spec,validate_native,resource_mode
        from unittest.mock import patch
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();value=self.fixture(root);manifest=root/'runtime/js-services/manifest.json';body=manifest.read_text();manifest.write_text(body+' '*(4*1024**2-len(body)));self.assertTrue(launch_spec(root,'channels')['prebuilt'])
            manifest.write_text(manifest.read_text()+' ')
            with self.assertRaises(ValueError):launch_spec(root,'channels')
            manifest.unlink();outside=root/'other-manifest';outside.write_text(body);manifest.symlink_to(outside)
            with self.assertRaises(ValueError):launch_spec(root,'channels')
        name='runtime/js-services/lib.node'
        with patch('prebuilt_js.host_key',return_value='macos-arm64'),patch('macos_target.minimum_version',return_value='14.4'):validate_native(Path('/fixture'),{name},{name:'14.4'})
        executable=SimpleNamespace(suffix='.exe',stat=lambda:SimpleNamespace(st_mode=0o644));self.assertEqual(resource_mode(executable,'nt'),0o755)
        non_windows=SimpleNamespace(suffix='.js',stat=lambda:SimpleNamespace(st_mode=0o755));self.assertEqual(resource_mode(non_windows,'posix'),0o755)


    def test_private_tool_publication_tolerates_already_released_temporary_name(self):
        from prebuilt_js import private_bun
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();self.fixture(root);link=os.link
            def release_name(source,destination):
                link(source,destination);Path(source).unlink()
            with patch('prebuilt_js.os.link',side_effect=release_name):binary=private_bun(root,create=True)
            self.assertEqual(binary.read_text(),'fixture runtime');self.assertEqual(list(binary.parent.glob('.bun-*')),[])


if __name__=='__main__':unittest.main()
