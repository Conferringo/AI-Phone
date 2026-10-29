import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from native_backports import verified_backports


class BackportTests(unittest.TestCase):
    def fixture(self, root):
        vendor = root/'apps/client/src-tauri/vendor/library'
        vendor.mkdir(parents=True)
        (root/'config').mkdir()
        (root/'apps/client/src-tauri/Cargo.toml').write_text('[patch.crates-io]\nlibrary={path="vendor/library"}\n')
        (vendor/'Cargo.toml').write_text('[package]\nname="library"\nversion="1.0.0"\n')
        (vendor/'lib.rs').write_text('reviewed source')
        record = {'package':'library','version':'1.0.0','directory':vendor.relative_to(root).as_posix(),
                  'advisory':'RUSTSEC-fixture','upstream_patch':'https://example.invalid/commit/fixture',
                  'files':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in vendor.iterdir()}}
        (root/'config/native-backports.json').write_text(json.dumps({'backports':[record]}))
        return vendor

    def test_backport_must_match_every_file_and_selected_cargo_path(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);vendor=self.fixture(root)
            self.assertIn(('library@1.0.0','RUSTSEC-fixture'),verified_backports(root))
            (vendor/'lib.rs').write_text('unreviewed source')
            with self.assertRaisesRegex(ValueError,'differs'):verified_backports(root)
            (vendor/'lib.rs').write_text('reviewed source')
            (vendor/'extra.rs').write_text('unexpected')
            with self.assertRaisesRegex(ValueError,'differs'):verified_backports(root)
            (vendor/'extra.rs').unlink()
            (root/'apps/client/src-tauri/Cargo.toml').write_text('[patch.crates-io]\n')
            with self.assertRaisesRegex(ValueError,'not selected'):verified_backports(root)

    def test_backport_rejects_symlinks_and_missing_licence_or_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);vendor=self.fixture(root)
            (vendor/'lib.rs').unlink()
            with self.assertRaisesRegex(ValueError,'differs'):verified_backports(root)
            (vendor/'lib.rs').symlink_to(vendor/'Cargo.toml')
            with self.assertRaisesRegex(ValueError,'differs'):verified_backports(root)


if __name__ == '__main__':unittest.main()
