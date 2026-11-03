"""AppImage inspection contracts; actual extractor execution belongs to Linux CI."""
import json
from pathlib import Path
import shutil
import struct
import tempfile
import unittest
from unittest.mock import patch
import client_artifacts as artifacts


class AppImageArtifactTests(unittest.TestCase):
    def fixture(self,root,machine=62):
        native=root/'apps/client/src-tauri';native.mkdir(parents=True)
        (root/'LICENSE').write_bytes(b'reviewed license')
        (native/'tauri.conf.json').write_text(json.dumps({'bundle':{'resources':{'../../../LICENSE':'legal/LICENSE'}}}))
        tree=root/'fixture-appdir';binary=tree/'usr/bin/ai-phone-client';binary.parent.mkdir(parents=True)
        header=bytearray(64);header[:6]=b'\x7fELF\x02\x01';struct.pack_into('<H',header,18,machine)
        binary.write_bytes(header);binary.chmod(0o755)
        legal=tree/'usr/lib/ai-phone/legal/LICENSE';legal.parent.mkdir(parents=True);legal.write_bytes(b'reviewed license')
        image=root/'AI Phone.AppImage';image.write_bytes(b'owned fixture archive')
        def extract(_command,*,cwd,**_options):shutil.copytree(tree,Path(cwd)/'squashfs-root',symlinks=True)
        return image,tree,extract

    def test_unpacked_resources_and_native_elf_match_the_expected_host(self):
        for arch,machine in [('amd64',62),('arm64',183)]:
            with self.subTest(arch=arch),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);image,tree,extract=self.fixture(root,machine)
                (tree/'AppRun').symlink_to('usr/bin/ai-phone-client')
                with patch.object(artifacts.subprocess,'run',side_effect=extract):
                    value=artifacts.inspect_appimage(image,arch,root)
                self.assertEqual(value['architecture'],arch)
                files={row['path'] for row in value['files']}
                self.assertIn('usr/bin/ai-phone-client',files)
                self.assertIn('usr/lib/ai-phone/legal/LICENSE',files)

    def test_wrong_elf_missing_and_stale_resources_fail(self):
        for changed in ['wrong-machine','not-elf','wrong-class','wrong-endianness','truncated-header','missing-license','stale-license']:
            with self.subTest(changed=changed),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);image,tree,extract=self.fixture(root,183 if changed=='wrong-machine' else 62)
                if changed=='not-elf':(tree/'usr/bin/ai-phone-client').write_bytes(b'broken program')
                binary=tree/'usr/bin/ai-phone-client'
                if changed=='wrong-class':binary.write_bytes(binary.read_bytes().replace(b'\x7fELF\x02',b'\x7fELF\x01',1))
                if changed=='wrong-endianness':binary.write_bytes(binary.read_bytes().replace(b'\x7fELF\x02\x01',b'\x7fELF\x02\x02',1))
                if changed=='truncated-header':binary.write_bytes(binary.read_bytes()[:19])
                legal=tree/'usr/lib/ai-phone/legal/LICENSE'
                if changed=='missing-license':legal.unlink()
                if changed=='stale-license':legal.write_bytes(b'outdated license')
                with patch.object(artifacts.subprocess,'run',side_effect=extract),self.assertRaises(ValueError):
                    artifacts.inspect_appimage(image,'amd64',root)

    def test_extracted_link_cannot_read_a_host_file_and_scratch_is_removed(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);image,tree,extract=self.fixture(root);outside=root/'outside';outside.write_bytes(b'host content')
            (tree/'escaped').symlink_to(outside)
            seen=[]
            def capture(command,*,cwd,**options):seen.append(Path(cwd));return extract(command,cwd=cwd,**options)
            with patch.object(artifacts.subprocess,'run',side_effect=capture),self.assertRaisesRegex(ValueError,'link'):
                artifacts.inspect_appimage(image,'amd64',root)
            self.assertEqual(outside.read_bytes(),b'host content')
            self.assertTrue(seen);self.assertTrue(all(not path.exists() for path in seen))

    def test_extractor_failure_is_reported_and_scratch_is_removed(self):
        import subprocess
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);image,_,_=self.fixture(root);seen=[]
            def fail(command,*,cwd,**_options):
                seen.append(Path(cwd));raise subprocess.CalledProcessError(1,command)
            with patch.object(artifacts.subprocess,'run',side_effect=fail),self.assertRaises(subprocess.CalledProcessError):
                artifacts.inspect_appimage(image,'amd64',root)
            self.assertTrue(seen);self.assertTrue(all(not path.exists() for path in seen))


if __name__=='__main__':unittest.main()
