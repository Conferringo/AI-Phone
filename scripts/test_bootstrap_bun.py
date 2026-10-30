"""Pinned, portable bootstrap behavior without a package-manager dependency."""
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile
from bootstrap_bun import install, asset_name, bun_path

class BunBootstrapTests(unittest.TestCase):
    def test_portable_assets_and_unknown_hosts(self):
        self.assertEqual(asset_name('Darwin','arm64'),'bun-darwin-aarch64')
        self.assertEqual(asset_name('Linux','x86_64'),'bun-linux-x64-baseline')
        self.assertEqual(asset_name('Windows','amd64'),'bun-windows-x64-baseline')
        with self.assertRaises(ValueError):asset_name('Unknown','arm64')

    def test_verified_download_is_atomic_and_reused(self):
        with tempfile.TemporaryDirectory() as temp:
            root=Path(temp);(root/'config').mkdir();archive=root/'fixture.zip'
            name=asset_name();executable='bun.exe' if __import__('os').name=='nt' else 'bun'
            with zipfile.ZipFile(archive,'w') as z:z.writestr(name+'/'+executable,b'fixture executable'.ljust(1024,b'x'))
            policy={'version':'1.4.4','assets':{name+'.zip':hashlib.sha256(archive.read_bytes()).hexdigest()}}
            (root/'config/bun-runtime.json').write_text(json.dumps(policy));calls=[]
            def download(url,path):calls.append(url);path.write_bytes(archive.read_bytes())
            with patch('bootstrap_bun.subprocess.check_output',return_value='1.4.4\n'):
                self.assertEqual(install(root,download),bun_path(root))
                self.assertEqual(install(root,lambda *_:self.fail('download on reuse')),bun_path(root))
            self.assertEqual(len(calls),1)
            bun_path(root).unlink();policy['assets'][name+'.zip']='0'*64
            (root/'config/bun-runtime.json').write_text(json.dumps(policy))
            with self.assertRaisesRegex(ValueError,'checksum'):install(root,download)
            self.assertFalse(bun_path(root).exists())
            self.assertEqual(list((root/'.tools/bin').iterdir()),[])

    def test_rejects_empty_and_excessive_advertised_zip_sizes_before_execution(self):
        import struct
        for size in [0,200*1024*1024,200*1024*1024+1]:
            with self.subTest(size=size),tempfile.TemporaryDirectory() as temp:
                root=Path(temp);(root/'config').mkdir();archive=root/'fixture.zip';name=asset_name();executable='bun.exe' if __import__('os').name=='nt' else 'bun'
                with zipfile.ZipFile(archive,'w') as z:z.writestr(name+'/'+executable,b'x'*1024)
                content=bytearray(archive.read_bytes());central=content.index(b'PK\x01\x02');local=content.index(b'PK\x03\x04');struct.pack_into('<I',content,central+24,size);struct.pack_into('<I',content,local+22,size)
                (root/'config/bun-runtime.json').write_text(json.dumps({'version':'1.4.4','assets':{name+'.zip':hashlib.sha256(content).hexdigest()}}))
                with patch('bootstrap_bun.subprocess.check_output',return_value='1.4.4\n'):
                    with self.assertRaisesRegex(ValueError,'executable'):install(root,lambda _,p:p.write_bytes(content))
                self.assertFalse(bun_path(root).exists())

if __name__=='__main__':unittest.main()
