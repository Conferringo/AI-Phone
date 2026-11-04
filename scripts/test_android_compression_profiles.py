"""CI compression can be faster without changing a single SDK/TAR content byte."""
import lzma
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import android_component_factory as factory
from android_runtime import stage,verify_sdk
import test_android_macos_factory as fixtures


class AndroidCompressionProfiles(unittest.TestCase):
    def test_encoding_profiles_preserve_sdk_bytes_and_existing_user_state(self):
        with tempfile.TemporaryDirectory() as temp,patch('android_runtime.host_key',return_value='macos-arm64'),patch('android_component_factory.host_key',return_value='macos-arm64'):
            root=Path(temp);fixtures.fixture(root)
            normal,distribution=factory.build(root,root/'distribution')
            self.assertEqual(distribution['compression_preset'],6)
            first=stage(normal,root/'installation')
            state=root/'installation/user/chat'
            state.parent.mkdir(parents=True);state.write_bytes(b'existing chat')
            for preset in (3,9):
                with self.subTest(preset=preset):
                    payload,receipt=factory.build(root,root/str(preset),compression_preset=preset)
                    self.assertEqual(receipt['compression_preset'],preset)
                    self.assertEqual(lzma.decompress(normal.read_bytes()),lzma.decompress(payload.read_bytes()))
                    selected=stage(payload,root/'installation')
                    self.assertEqual(verify_sdk(first),verify_sdk(selected))
                    self.assertEqual(distribution['stock'],receipt['stock'])
                    self.assertEqual(state.read_bytes(),b'existing chat')

    def test_unsupported_profiles_fail_before_work_or_downloads(self):
        with tempfile.TemporaryDirectory() as temp,patch.object(factory,'download') as download:
            root=Path(temp)
            for value in (True,False,0,8,10,'3',None):
                with self.subTest(value=value),self.assertRaisesRegex(ValueError,'compression'):
                    factory.build(root,root/'work',compression_preset=value)
            self.assertFalse((root/'work').exists());download.assert_not_called()


if __name__=='__main__':unittest.main()
