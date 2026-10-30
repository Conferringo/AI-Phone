"""Locked package resolution preserves aliases, cycles and private native resources."""
import json
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


class DependencyTreeTests(unittest.TestCase):
    def test_non_ascii_package_metadata_uses_utf8_on_a_legacy_windows_locale(self):
        from js_dependencies import collect
        description='\u0101 - 中文 - العربية - 日本語'
        native_open=io.open
        def windows_open(file, mode='r', buffering=-1, encoding=None, *args, **kwargs):
            if 'b' not in mode and encoding in (None,'locale'):
                encoding='cp1252'
            return native_open(file,mode,buffering,encoding,*args,**kwargs)
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();app=root/'app';app.mkdir()
            (app/'package.json').write_bytes(json.dumps({'description':description,'dependencies':{'sample':'1'}},ensure_ascii=False).encode('utf-8'))
            dependency=root/'node_modules/sample';dependency.mkdir(parents=True)
            (dependency/'package.json').write_bytes(json.dumps({'name':'sample','version':'1','description':description},ensure_ascii=False).encode('utf-8'))
            with patch('io.open',side_effect=windows_open):
                graph=collect(root,['app'])
            self.assertEqual(graph.packages[dependency]['description'],description)

    def test_development_templates_are_not_runtime_environment_files(self):
        from js_dependencies import package_files
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();(root/'index.js').write_text('runtime');(root/'.env.example').write_text('PUBLIC_TEMPLATE=');(root/'index.js.map').write_text('{}')
            self.assertEqual([p.name for p in package_files(root)],['index.js'])

    def package(self,root,name,version='1.0.0',dependencies=None,**values):
        folder=root/'node_modules/.bun'/(name.replace('/','+')+'@'+version)/'node_modules'/name
        folder.mkdir(parents=True);(folder/'package.json').write_text(json.dumps({'name':name,'version':version,'dependencies':dependencies or {},**values}));(folder/'index.js').write_text('module.exports={};')
        return folder

    def link(self,base,name,target):
        path=base/'node_modules'/name;path.parent.mkdir(parents=True,exist_ok=True);path.symlink_to(target,target_is_directory=True)

    def test_alias_cycle_and_two_versions_keep_original_resolution(self):
        from js_dependencies import collect
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();app=root/'apps/sample';app.mkdir(parents=True);(app/'package.json').write_text(json.dumps({'dependencies':{'a':'1','legacy':'npm:c@1'}}))
            a=self.package(root,'a',dependencies={'b':'1'});b=self.package(root,'b',dependencies={'a':'1','c':'2'});one=self.package(root,'c');two=self.package(root,'c','2.0.0')
            self.link(app,'a',a);self.link(app,'legacy',one);self.link(a,'b',b);self.link(b,'a',a);self.link(b,'c',two)
            graph=collect(root,['apps/sample']);self.assertEqual(len(graph.packages),4)
            self.assertEqual(graph.aliases[(app/'node_modules/legacy').relative_to(root).as_posix()],one.relative_to(root).as_posix())

    def test_missing_required_escape_and_wrong_package_are_rejected(self):
        from js_dependencies import collect
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();app=root/'app';app.mkdir();(app/'package.json').write_text('{"dependencies":{"a":"1"}}')
            with self.assertRaises(ValueError):collect(root,['app'])
            other=root.parent/(root.name+'-outside');other.mkdir()
            try:
                self.link(app,'a',other)
                with self.assertRaises(ValueError):collect(root,['app'])
                (app/'node_modules/a').unlink();wrong=self.package(root,'different');self.link(app,'a',wrong)
                with self.assertRaises(ValueError):collect(root,['app'])
            finally:other.rmdir()

    def test_optional_platform_and_peers_do_not_hide_required_packages(self):
        from js_dependencies import collect
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();app=root/'app';app.mkdir();(app/'package.json').write_text('{"dependencies":{"a":"1"}}')
            a=self.package(root,'a',optionalDependencies={'missing':'1','foreign':'1'},peerDependencies={'peer':'1','optional-peer':'1'},peerDependenciesMeta={'optional-peer':{'optional':True}})
            foreign=self.package(root,'foreign',os=['unsupported-platform']);peer=self.package(root,'peer');self.link(app,'a',a);self.link(a,'foreign',foreign)
            with self.assertRaises(ValueError):collect(root,['app'])
            self.link(a,'peer',peer);graph=collect(root,['app']);self.assertEqual(set(graph.packages),{a,peer})



class DependencyBoundaryTests(unittest.TestCase):
    def test_metadata_platform_and_dependency_shapes(self):
        from js_dependencies import package_data,dependencies,matches,platform_tags,expected_name
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            for body in ('[]','null'):
                (root/'package.json').write_text(body)
                with self.assertRaises(ValueError):package_data(root)
            for value in ({'dependencies':[]},{'dependencies':{'../foreign':'1'}},{'dependencies':{'valid':1}},{'peerDependencies':{'a':'1'},'peerDependenciesMeta':{'a':True}}):
                with self.assertRaises(ValueError):dependencies(value)
        self.assertTrue(matches(['linux','!darwin'],'linux'));self.assertFalse(matches(['linux','!darwin'],'darwin'));self.assertTrue(matches(['!linux'],'darwin'));self.assertFalse(matches(['win32'],'linux'))
        for value in ('linux',[1]):
            with self.assertRaises(ValueError):matches(value,'linux')
        self.assertEqual(expected_name('legacy','npm:@scope/name@1'),'@scope/name');self.assertEqual(expected_name('legacy','npm:real'),'real')
        with patch('js_dependencies.platform.system',return_value='Linux'),patch('js_dependencies.platform.machine',return_value='x86_64'),patch('js_dependencies.platform.libc_ver',return_value=('glibc','2.39')):self.assertEqual(platform_tags(),{'os':'linux','cpu':'x64','libc':'glibc'})
        with patch('js_dependencies.platform.system',return_value='unknown'):
            with self.assertRaises(ValueError):platform_tags()

    def test_workspace_files_use_snapshot_and_resource_escape_fails(self):
        from js_dependencies import DependencyTree,package_files
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();source=root/'snapshot';(source/'packages/protocol').mkdir(parents=True);(source/'packages/protocol/package.json').write_text('{"name":"protocol","version":"1"}')
            tree=DependencyTree(root,source);self.assertEqual(tree.package_source(root/'packages/protocol'),source/'packages/protocol')
            folder=root/'dependency';folder.mkdir();outside=root/'outside';outside.write_text('other');(folder/'index.js').symlink_to(outside)
            with self.assertRaises(ValueError):list(package_files(folder))
            (folder/'index.js').unlink();(folder/'directory').symlink_to(root,target_is_directory=True)
            with self.assertRaises(ValueError):list(package_files(folder))

    def test_metadata_limit_alias_and_graph_capacity_are_exact(self):
        from js_dependencies import package_data,expected_name,DependencyTree
        self.assertEqual(expected_name('alias','npm:@scope/name'),'@scope/name')
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary).resolve();body='{"name":"a","version":"1"}';path=root/'package.json';path.write_text(body+' '*(1024**2-len(body)));self.assertEqual(package_data(root)['name'],'a')
            path.write_text(path.read_text()+' ')
            with self.assertRaises(ValueError):package_data(root)
            path.unlink();other=root/'other.json';other.write_text(body);path.symlink_to(other)
            with self.assertRaises(ValueError):package_data(root)
            path.unlink();path.write_text(body);tree=DependencyTree(root)
            with patch('js_dependencies.MAX_PACKAGES',0):
                with self.assertRaises(ValueError):tree.add(root)


if __name__=='__main__':unittest.main()
