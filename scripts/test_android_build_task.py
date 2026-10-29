from pathlib import Path
import tempfile
import unittest
from prepare_android_client import patch_build_task, resource_task
import json

class AndroidBuildTaskTests(unittest.TestCase):
    def test_gradle_calls_actual_cli_file_and_preserves_build_flags(self):
        with tempfile.TemporaryDirectory(prefix='android project ') as d:
            root=Path(d);task=root/'BuildTask.kt';task.write_text('val executable = """node""";\nval args = listOf("tauri", "android", "android-studio-script");\nargs("--release")')
            cli=root/'node_modules/@tauri-apps/cli/tauri.js'
            patch_build_task(task,cli)
            text=task.read_text();self.assertIn(str(cli),text);self.assertNotIn('listOf("tauri",',text);self.assertIn('args("--release")',text)
            patch_build_task(task,cli);self.assertEqual(task.read_text(),text)
    def test_all_configured_resources_are_explicit_gradle_inputs_and_outputs(self):
        with tempfile.TemporaryDirectory(prefix='android resources ') as d:
            root=Path(d);native=root/'apps/client/src-tauri';native.mkdir(parents=True)
            payload=native/'host/source.tar.gz';payload.parent.mkdir();payload.write_bytes(b'reviewed source archive')
            (native/'tauri.conf.json').write_text(json.dumps({'bundle':{'resources':{'host/source.tar.gz':'host/source.tar.gz'}}}))
            task=resource_task(root)
            self.assertIn('Copy::class',task);self.assertIn('from(',task);self.assertIn('into("host")',task)
            self.assertIn('rename(".*", "source.tar.gz")',task);self.assertIn('dependsOn(syncAiPhoneResources)',task)
            payload.unlink()
            with self.assertRaisesRegex(ValueError,'Missing configured'):resource_task(root)

    def test_unknown_generator_template_fails_before_packaging(self):
        with tempfile.TemporaryDirectory() as d:
            task=Path(d)/'BuildTask.kt';task.write_text('unrecognized template')
            with self.assertRaisesRegex(ValueError,'template'):patch_build_task(task,Path(d)/'tauri.js')

if __name__=='__main__':unittest.main()
