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
    def test_fast_and_distribution_capsules_have_identical_raw_tar_and_sdk(self):
        with tempfile.TemporaryDirectory() as temp,patch('android_runtime.host_key',return_value='macos-arm64'),patch('android_component_factory.host_key',return_value='macos-arm64'):
            root=Path(temp);fixtures.fixture(root)
            normal,distribution=factory.build(root,root/'distribution')
            fast,preview=factory.build(root,root/'preview',compression_preset=3)
            self.assertEqual(distribution['compression_preset'],6)
            self.assertEqual(preview['compression_preset'],3)
            self.assertEqual(lzma.decompress(normal.read_bytes()),lzma.decompress(fast.read_bytes()))
            first=stage(normal,root/'installation');second=stage(fast,root/'installation')
            self.assertEqual(verify_sdk(first),verify_sdk(second))
            self.assertEqual(distribution['stock'],preview['stock'])

    def test_unsupported_profiles_fail_before_work_or_downloads(self):
        with tempfile.TemporaryDirectory() as temp,patch.object(factory,'download') as download:
            root=Path(temp)
            for value in (True,False,0,9,'3',None):
                with self.subTest(value=value),self.assertRaisesRegex(ValueError,'compression'):
                    factory.build(root,root/'work',compression_preset=value)
            self.assertFalse((root/'work').exists());download.assert_not_called()


if __name__=='__main__':unittest.main()
