"""The WSL bootstrap delegates source publication to the existing Rust owner."""
import hashlib
import io
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
import sys
from unittest.mock import patch

import wsl_backend_entry as entry
import wsl_backend_factory as factory
import test_wsl_backend as source_fixture


class WslNativeSourceTests(unittest.TestCase):
    def test_factory_wires_the_native_program_and_pin_into_its_offline_recipe(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary)
            source=source_fixture.WslSourceTests().payload(root,extra={'scripts/wsl_backend_entry.py':b'fixture bootstrap entry'})
            identity=factory.source_input(source)
            images=root/'images.payload';images.write_bytes(b'fixture sealed images')
            tools=root/'tools';tools.mkdir();(tools/'manifest.json').write_text('{"format":1}')
            packages=root/'packages';packages.mkdir()
            plan={'base_image':'fixture-base','engine_image':'fixture-engine','engine_packages':[]}
            with patch.object(factory,'validated_inputs',return_value=(plan,identity,{'tools':{}})), patch('wsl_engine_inputs.licenses',return_value=[]):
                factory.prepare_context(root,root/'context',source,images,tools,packages)
            context=root/'context'
            self.assertEqual((context/'source.sha256').read_text(),identity['sha256']+'\n')
            self.assertEqual((context/'bootstrap.program').read_bytes(),b'fixture-native-staging-component')
            self.assertEqual((context/'bootstrap.program').stat().st_mode&0o777,0o755)
            recipe=(context/'Dockerfile').read_text()
            self.assertIn('COPY bootstrap.program /usr/local/libexec/aiphone-bootstrap',recipe)
            self.assertIn('COPY source.sha256 /opt/aiphone/bundle/source.sha256',recipe)
            self.assertIn('--source-staging-contract',recipe)
            self.assertNotIn('cargo ',recipe)

    def test_staging_calls_only_the_fixed_component_and_checks_its_receipt(self):
        identity = 'a' * 64
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary) / 'owned installation'
            bundle = Path(temporary) / 'bundle'
            bundle.mkdir()
            (bundle / 'source.sha256').write_text(identity + '\n')
            expected = home.resolve() / 'releases' / identity
            response = subprocess.CompletedProcess([], 0, f'{identity} {expected}\n', '')
            with patch.object(entry, 'BUNDLE', bundle), patch.object(entry.subprocess, 'run', return_value=response) as run:
                try:
                    self.assertEqual(entry.staged(home), expected)
                finally:
                    if str(expected / 'scripts') in sys.path:
                        sys.path.remove(str(expected / 'scripts'))
                self.assertEqual(run.call_args.args[0], [str(entry.BOOTSTRAP), '--stage-source', str(bundle / 'source.payload'), str(home), identity])
                self.assertFalse(home.exists(), 'the legacy adapter does not publish source or state')
                for stdout in ('', f'{identity} /foreign\n', f'{identity} {expected}\nextra\n'):
                    response.stdout = stdout
                    with self.assertRaises(ValueError):
                        entry.staged(home)

    def test_invalid_pin_fails_before_spawning_or_materializing_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            bundle = Path(temporary)
            with patch.object(entry, 'BUNDLE', bundle), patch.object(entry.subprocess, 'run') as run:
                for text in ('', 'a' * 63, 'A' * 64, 'a' * 64 + '\nextra'):
                    (bundle / 'source.sha256').write_text(text)
                    with self.assertRaises(ValueError):
                        entry.staged(bundle / 'uncreated')
                run.assert_not_called()

    def test_factory_extracts_the_verified_native_program_without_user_state(self):
        program = b'fixture-native-component'
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / 'source.payload'
            name = 'runtime/services/bin/assistant'
            with tarfile.open(source, 'w') as archive:
                header = tarfile.TarInfo(name)
                header.size = len(program)
                archive.addfile(header, io.BytesIO(program))
            records = {name: {'path': name, 'mode': 0o755, 'size': len(program), 'sha256': hashlib.sha256(program).hexdigest()}}
            with patch.object(factory, 'archive_records', return_value=({}, records, True)):
                factory.bootstrap_program(source, root / 'bootstrap')
                self.assertEqual((root / 'bootstrap').read_bytes(), program)
                with self.assertRaises(FileExistsError):
                    factory.bootstrap_program(source, root / 'bootstrap')
                self.assertEqual((root / 'bootstrap').read_bytes(), program)
                images = root / 'images.payload'
                images.write_bytes(b'fixture images')
                total, budget = factory.input_budget(source, images, root / 'tools', root / 'packages')
                self.assertEqual(total, source.stat().st_size + images.stat().st_size + len(program) + 65)
                self.assertEqual(budget, total + 1536 * 1024 ** 2)
                records[name]['sha256'] = '0' * 64
                with self.assertRaises(ValueError):
                    factory.bootstrap_program(source, root / 'uncreated')
                self.assertFalse((root / 'uncreated').exists())


if __name__ == '__main__':
    unittest.main()
