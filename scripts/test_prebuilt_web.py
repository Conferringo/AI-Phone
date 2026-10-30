"""Prebuilt web deployment runs without its source checkout or a package manager."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch
from prebuilt_web import prepare_web, web_command
from bootstrap_bun import bun_path
from client_host_entry import web_links
from check_prebuilt_web import verify, isolated_environment

ROOT=Path(__file__).resolve().parent.parent

class PrebuiltWebTests(unittest.TestCase):
    def test_isolation_keeps_windows_system_environment_without_model_credentials(self):
        with tempfile.TemporaryDirectory() as temp,patch.dict(os.environ,{'SYSTEMROOT':'C:/Windows','USERPROFILE':'C:/Users/fixture','DEEPSEEK_API_KEY':'fixture-model-key'},clear=True):
            environment=isolated_environment(Path(temp),12345)
            self.assertEqual(environment.get('SYSTEMROOT'),'C:/Windows')
            self.assertEqual(environment.get('USERPROFILE'),'C:/Users/fixture')
            self.assertNotIn('DEEPSEEK_API_KEY',environment)

    def fixture(self, root):
        (root/'config').mkdir();(root/'config/bun-runtime.json').write_bytes((ROOT/'config/bun-runtime.json').read_bytes())
        app=root/'apps/web';server=app/'.next/standalone/apps/web/server.js';server.parent.mkdir(parents=True)
        server.write_text('const nextConfig = {"images":{"unoptimized":true}};\nconsole.log("offline-web-runtime")')
        (app/'.next/static').mkdir();(app/'.next/static/chunk.js').write_text('public chunk')
        (app/'public').mkdir();(app/'public/icon.svg').write_text('<svg/>')
        return app
    def test_prebuilt_entry_runs_after_original_build_is_removed(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);self.fixture(root)
            files=prepare_web(root,bun_path(ROOT))
            for name,content,mode in files:
                p=root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(content);p.chmod(mode)
            shutil.rmtree(root/'apps')
            result=subprocess.check_output(web_command(root),cwd=root,text=True)
            self.assertEqual(result.strip(),'offline-web-runtime')
            self.assertTrue((root/'runtime/web/apps/web/.next/static/chunk.js').is_file())
            self.assertTrue((root/'runtime/web/apps/web/public/icon.svg').is_file())
            (root/'runtime/web/apps/web/server.js').write_text('tampered')
            with self.assertRaisesRegex(ValueError,'integrity'):web_command(root)
    def test_missing_build_private_files_and_escaping_links_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp)
            with self.assertRaisesRegex(ValueError,'standalone'):prepare_web(root,bun_path(ROOT))
            app=self.fixture(root);standalone=app/'.next/standalone';(standalone/'.env').write_text('private')
            with self.assertRaisesRegex(ValueError,'private'):prepare_web(root,bun_path(ROOT))
            (standalone/'.env').unlink();(standalone/'escape').symlink_to(ROOT/'README.md')
            with self.assertRaisesRegex(ValueError,'symlink'):prepare_web(root,bun_path(ROOT))
    def test_package_aliases_keep_their_original_dependency_resolution(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);app=self.fixture(root);standalone=app/'.next/standalone'
            store=standalone/'node_modules/.store/pkg/node_modules';package=store/'pkg';dependency=store/'dependency'
            package.mkdir(parents=True);dependency.mkdir()
            (package/'index.js').write_text('module.exports=require("dependency")')
            (dependency/'index.js').write_text('module.exports="package-receipt"')
            alias=standalone/'apps/web/node_modules/pkg';alias.parent.mkdir();alias.symlink_to(Path('../../../node_modules/.store/pkg/node_modules/pkg'),target_is_directory=True)
            (standalone/'apps/web/server.js').write_text('const nextConfig = {"images":{"unoptimized":true}};\nconsole.log(require("pkg"))')
            for name,content,mode in prepare_web(root,bun_path(ROOT)):
                p=root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(content);p.chmod(mode)
            web_links(root,create=True);shutil.rmtree(root/'apps')
            self.assertEqual(subprocess.check_output(web_command(root),cwd=root,text=True).strip(),'package-receipt')
            link=root/'runtime/web/apps/web/node_modules/pkg';link.unlink();link.symlink_to(ROOT,target_is_directory=True)
            with self.assertRaisesRegex(ValueError,'changed'):web_command(root)
    def test_cold_start_rejects_an_asset_that_differs_from_the_installed_bytes(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);app=self.fixture(root);server=app/'.next/standalone/apps/web/server.js'
            html='<script src="/_next/static/test.js"></script>'
            server.write_text('const nextConfig = {"images":{"unoptimized":true}};\n'+
                'require("node:http").createServer((req,res)=>res.end(req.url==="/phone"?'+json.dumps(html)+':"wrong bytes")).listen(Number(process.env.PORT),"127.0.0.1");')
            (app/'.next/static/test.js').write_bytes(b'correct bytes')
            for name,content,mode in prepare_web(root,bun_path(ROOT)):
                p=root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(content);p.chmod(mode)
            with self.assertRaisesRegex(ValueError,'differs'):verify(root)

    def test_only_build_root_configuration_is_relocated(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);app=self.fixture(root);server=app/'.next/standalone/apps/web/server.js'
            config={'images':{'unoptimized':True},'outputFileTracingRoot':str(root),'turbopack':{'root':str(root)},'distDir':'.next','keep':'business-value'}
            server.write_text('const nextConfig = '+json.dumps(config)+'\nconsole.log(JSON.stringify(nextConfig))')
            files=prepare_web(root,bun_path(ROOT));text=next(content for name,content,_ in files if name=='runtime/web/apps/web/server.js').decode()
            self.assertNotIn(str(root),text);self.assertIn('business-value',text);self.assertIn('"distDir": ".next"',text)

if __name__=='__main__':unittest.main()
