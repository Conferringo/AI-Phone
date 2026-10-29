from pathlib import Path
import tempfile
import unittest
from prepare_android_client import patch_build_task

class AndroidBuildTaskTests(unittest.TestCase):
    def test_gradle_calls_actual_cli_file_and_preserves_build_flags(self):
        with tempfile.TemporaryDirectory(prefix='android project ') as d:
            root=Path(d);task=root/'BuildTask.kt';task.write_text('val executable = """node""";\nval args = listOf("tauri", "android", "android-studio-script");\nargs("--release")')
            cli=root/'node_modules/@tauri-apps/cli/tauri.js'
            patch_build_task(task,cli)
            text=task.read_text();self.assertIn(str(cli),text);self.assertNotIn('listOf("tauri",',text);self.assertIn('args("--release")',text)
            patch_build_task(task,cli);self.assertEqual(task.read_text(),text)
    def test_unknown_generator_template_fails_before_packaging(self):
        with tempfile.TemporaryDirectory() as d:
            task=Path(d)/'BuildTask.kt';task.write_text('unrecognized template')
            with self.assertRaisesRegex(ValueError,'template'):patch_build_task(task,Path(d)/'tauri.js')

if __name__=='__main__':unittest.main()
