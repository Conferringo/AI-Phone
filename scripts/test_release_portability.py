from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import os
from release_package import select_files
import test_source_export
from source_export import export

class ReleasePortabilityTests(unittest.TestCase):
    def test_source_manifest_order_is_independent_of_windows_path_comparison(self):
        # Windows Path ordering folds case; the same reviewed files must retain
        # the POSIX manifest order without rewriting any reviewed bytes.
        class WindowsOrderedPath(type(Path())):
            def __lt__(self,other):
                return tuple(p.casefold() for p in self.parts)<tuple(p.casefold() for p in other.parts)
        names=['README.md','apps/alpha.txt','LICENSE','apps/web/a.txt','apps/Zeta.txt']
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)
            for name in names:
                path=root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(name.encode()+b'\n')
            with patch('release_package.source_candidates',return_value=[WindowsOrderedPath(root/name) for name in names]):
                files=select_files(root)
            self.assertEqual([name for name,_,_ in files],['LICENSE','README.md','apps/Zeta.txt','apps/alpha.txt','apps/web/a.txt'])
            for name,content,_ in files:self.assertEqual(content,name.encode()+b'\n')

    def test_ignored_build_trees_are_never_traversed(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'apps/web/node_modules/private').mkdir(parents=True);(root/'apps/web/ui.ts').write_bytes(b'public\n')
            original=os.scandir
            def guarded(path):
                if 'node_modules' in Path(path).parts:raise AssertionError('Private dependency tree was traversed')
                return original(path)
            with patch('os.scandir',side_effect=guarded):files=select_files(root)
            self.assertEqual([f[0] for f in files],['apps/web/ui.ts'])
    def test_windows_checkout_keeps_reviewed_source_and_notice_bytes(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d)/'source';root.mkdir();test_source_export.SourceExportTests().fixture(root)
            (root/'.gitattributes').write_bytes(b'* -text\n')
            code=root/'apps/client/src-tauri/vendor/source.rs';code.parent.mkdir(parents=True);code.write_bytes(b'// reviewed source\nfn test() {}\n')
            legal=root/'docs/public/licenses/native/grant.txt';legal.parent.mkdir(parents=True);legal.write_bytes(b'Original grant\r\nCopyright\r\n')
            review=Path(d)/'review';export(root,review,draft=True,git=True)
            checkout=Path(d)/'windows-checkout';subprocess.run(['git','clone','--quiet','--no-checkout',str(review),str(checkout)],check=True)
            subprocess.run(['git','config','core.autocrlf','true'],cwd=checkout,check=True)
            subprocess.run(['git','checkout','--quiet','--force','master'],cwd=checkout,check=True)
            for p in [code,legal]:self.assertEqual((checkout/p.relative_to(root)).read_bytes(),p.read_bytes())
            self.assertTrue((checkout/'.gitattributes').exists())
    def test_source_names_are_canonical_for_protocol_and_manifests(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'apps/web').mkdir(parents=True);(root/'apps/web/ui.ts').write_bytes(b'public\n')
            self.assertEqual([f[0] for f in select_files(root)],['apps/web/ui.ts'])

if __name__=='__main__':unittest.main()
