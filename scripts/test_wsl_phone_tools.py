"""Offline guest phone resources, independent of Windows boot or user state."""
import hashlib
from pathlib import Path
import tempfile
import unittest
import zipfile
from unittest.mock import patch

import wsl_backend
import test_wsl_backend as fixture

ROOT = Path(__file__).resolve().parent.parent
SDK = 'opt/aiphone/native-phone-bridge/'
SCHEMA = SDK + 'emulator/lib/emulator_controller.proto'


class WslPhoneAdmissionTests(unittest.TestCase):
    def test_schema_is_data_but_other_emulator_paths_remain_forbidden(self):
        wsl_backend.permitted(SCHEMA)
        for name in (SDK+'emulator/emulator', SDK+'emulator/lib/other.proto',
                     'opt/other/emulator/lib/emulator_controller.proto'):
            with self.subTest(name=name), self.assertRaises(ValueError):
                wsl_backend.permitted(name)

    def test_rootfs_requires_both_linux_adb_and_the_media_schema(self):
        with tempfile.TemporaryDirectory() as temporary:
            target = Path(temporary)/'rootfs.tar.gz'
            fixture.rootfs(target,phone_tools=False)
            # Older backend-only archives cannot advertise guest phone support.
            with self.assertRaisesRegex(ValueError, 'Incomplete'):
                wsl_backend.inspect_rootfs(target)


def archive(folder):
    path = folder/'tools.zip'
    rows = {'platform-tools/adb': b'\x7fELF\x02\x01\x01'+b'\0'*11+b'\x3e\0'+b'fixture-linux-x64',
            'platform-tools/source.properties': b'Pkg.Revision=37.0.1\n',
            'platform-tools/NOTICE.txt': b'fixture-third-party-notices'}
    with zipfile.ZipFile(path, 'w') as source:
        for name, body in rows.items():
            entry = zipfile.ZipInfo(name)
            entry.external_attr = (0o100755 if name.endswith('/adb') else 0o100644)<<16
            source.writestr(entry, body)
    pin = {'version': '37.0.1', 'size_bytes': path.stat().st_size,
           'sha256': hashlib.sha256(path.read_bytes()).hexdigest()}
    return path, pin


class WslPhoneProducerTests(unittest.TestCase):
    def test_publisher_is_offline_and_includes_versioned_tools_schema_and_notices(self):
        import wsl_phone_tools as tools
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary); source,pin=archive(root); output=root/'guest-sdk'
            with patch.object(tools, 'platform_tools_pin', return_value=pin), patch('urllib.request.urlopen', side_effect=AssertionError('offline producer')):
                result=tools.build(ROOT,source,output)
                self.assertEqual(tools.verify(ROOT,output), result)
                self.assertEqual(result['platform_tools_sha256'],pin['sha256'])
                self.assertEqual((output/'platform-tools/adb').stat().st_mode&0o777,0o755)
                self.assertEqual((output/'platform-tools/NOTICE.txt').read_bytes(),b'fixture-third-party-notices')
                self.assertEqual((output/'emulator/lib/emulator_controller.proto').read_bytes(),(ROOT/'packages/protocol/vendor/android-emulator/emulator_controller.proto').read_bytes())
                with self.assertRaises(FileExistsError):tools.build(ROOT,source,output)
                (output/'platform-tools/adb').write_bytes(b'changed')
                with self.assertRaisesRegex(ValueError,'integrity'):tools.verify(ROOT,output)

    def test_bad_pin_wrong_revision_and_non_linux_binary_do_not_publish(self):
        import wsl_phone_tools as tools
        for kind in ('hash','version','machine'):
            with self.subTest(kind=kind),tempfile.TemporaryDirectory() as temporary:
                root=Path(temporary);source,pin=archive(root)
                if kind=='hash':pin['sha256']='0'*64
                else:
                    pin['version']='0.0.0' if kind=='version' else pin['version']
                    if kind=='machine':
                        with zipfile.ZipFile(source,'a') as target:target.writestr('platform-tools/foreign',b'fixture')
                        # Hash-valid archive with the wrong target architecture.
                        # Rewrite normally so CRCs are valid.
                        with zipfile.ZipFile(source) as incoming:rows={name:incoming.read(name) for name in incoming.namelist()}
                        rows['platform-tools/adb']=rows['platform-tools/adb'].replace(b'\x3e\0fixture',b'\xb7\0fixture')
                        with zipfile.ZipFile(source,'w') as target:
                            for name,data in rows.items():target.writestr(name,data)
                        pin.update(size_bytes=source.stat().st_size,sha256=hashlib.sha256(source.read_bytes()).hexdigest())
                with patch.object(tools,'platform_tools_pin',return_value=pin),self.assertRaises(ValueError):tools.build(ROOT,source,root/'output')
                self.assertFalse((root/'output').exists())
                self.assertFalse(list(root.glob('.phone-tools-*')))


if __name__=='__main__':unittest.main()
