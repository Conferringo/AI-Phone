"""Owned small-file fixtures only; never imports or executes upstream modules."""
import contextlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


def deny_external(event, arguments):
    if event.startswith("subprocess.") or event in {"socket.connect", "socket.connect_ex", "os.system", "os.kill"}:
        raise AssertionError("Unmocked external operation: " + event)


sys.addaudithook(deny_external)
HERE = Path(__file__).resolve().parent
EVIDENCE = HERE / "fixtures"
spec = importlib.util.spec_from_file_location("private_aosp_emulator_producer", HERE / "aosp_emulator_producer.py")
producer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(producer)


class RecipeContractTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="aiphone-producer-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.qemu = self.root / "external/qemu"
        self.qemu.mkdir(parents=True)

    def test_fixed_official_target_table_gives_all_native_distributions(self):
        source = EVIDENCE / "android/build/python/aemu/platform/toolchains.py"
        for target, expected in [("darwin_aarch64", "darwin_aarch64"), ("linux_x86_64", "linux"), ("windows_x86_64", "windows")]:
            with self.subTest(target=target):
                self.assertEqual(producer.checked_distribution(source, target), expected)

    def test_unknown_target_is_rejected(self):
        source = EVIDENCE / "android/build/python/aemu/platform/toolchains.py"
        with self.assertRaisesRegex(ValueError, "does not declare this native target"):
            producer.checked_distribution(source, "android_aarch64")

    def test_changed_source_still_fails_with_bounded_status_evidence(self):
        record={'path':'external/qemu','revision':'a'*40}
        status='?? generated-file.cc\n M source.cc\n'+'z'*7000
        with patch.object(producer,'git_read',side_effect=[record['revision'],status]):
            with self.assertRaisesRegex(ValueError,'not clean') as failure:producer.verify_project(self.root,record)
        self.assertIn('generated-file.cc',str(failure.exception))
        self.assertLess(len(str(failure.exception)),4300)

    def test_missing_distribution_is_rejected_without_importing_source(self):
        source = self.root / "table.py"
        source.write_text("raise AssertionError('must not execute')\nclass Toolchain:\n AVAILABLE = {'linux-x86_64': 'toolchain.cmake'}\n DISTRIBUTION = {}\n", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "does not declare this distribution"):
            producer.checked_distribution(source, "linux_x86_64")

    def test_unreviewed_cli_cannot_reach_build(self):
        owner = self.qemu / "android/build/python/aemu/cmake.py"
        owner.parent.mkdir(parents=True)
        owner.write_text("raise AssertionError('must not execute')\n", encoding="utf-8")
        with patch.object(producer, "recipe_inputs", return_value=[]):
            with self.assertRaisesRegex(ValueError, "unreviewed build CLI"):
                producer.checked_entry(self.root, "linux_x86_64")

    def test_official_source_bytes_match_reviewed_git_blob(self):
        source = self.qemu / "source.properties"
        source.write_bytes((EVIDENCE / "source.properties").read_bytes())
        with patch.object(producer, "RECIPE_BLOBS", {"source.properties": "387fc735929597d06c517a241dce5967781c8b45"}):
            records = producer.recipe_inputs(self.root)
        self.assertEqual(records[0]["git_blob_sha1"], "387fc735929597d06c517a241dce5967781c8b45")
        self.assertEqual(records[0]["sha256"], "5afaac6da7eaf8c6087b49a45e247f90fa4e4e93673848f0402be1cda5e9a919")

    def test_different_recipe_bytes_are_rejected(self):
        (self.qemu / "source.properties").write_text("Pkg.Revision=999\n", encoding="utf-8")
        with patch.object(producer, "RECIPE_BLOBS", {"source.properties": "387fc735929597d06c517a241dce5967781c8b45"}):
            with self.assertRaisesRegex(ValueError, "differs from its reviewed official blob"):
                producer.recipe_inputs(self.root)

    def test_recipe_alias_is_rejected(self):
        target = self.root / "aliased.properties"
        target.write_bytes((EVIDENCE / "source.properties").read_bytes())
        (self.qemu / "source.properties").symlink_to(target)
        with patch.object(producer, "RECIPE_BLOBS", {"source.properties": "387fc735929597d06c517a241dce5967781c8b45"}):
            with self.assertRaisesRegex(ValueError, "Missing or aliased"):
                producer.recipe_inputs(self.root)

    def test_recipe_byte_budget_is_bounded(self):
        (self.qemu / "source.properties").write_bytes(b"x" * 262145)
        with patch.object(producer, "RECIPE_BLOBS", {"source.properties": "387fc735929597d06c517a241dce5967781c8b45"}):
            with self.assertRaisesRegex(ValueError, "small-file budget"):
                producer.recipe_inputs(self.root)


class ProducerLifecycleTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="aiphone-producer-lifecycle-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.checkout = self.root / "checkout"
        (self.checkout / ".repo").mkdir(parents=True)
        self.qemu = self.checkout / "external/qemu"
        self.qemu.mkdir(parents=True)
        self.work = self.root / "new-work"
        self.lock = self.root / "fixture-lock.xml"
        self.lock.write_text("<manifest/>\n", encoding="utf-8")
        self.entry = self.qemu / "android/build/python/cmake.py"
        self.sdk = self.checkout / "prebuilts/android-emulator-build/system-images/darwin"
        self.sdk.mkdir(parents=True)
        (self.sdk / ".knownPackages").write_text("pinned metadata")
        self.context = contextlib.ExitStack()
        self.addCleanup(self.context.close)
        self.context.enter_context(patch.object(producer.platform, "system", return_value="Darwin"))
        self.context.enter_context(patch.object(producer.platform, "machine", return_value="arm64"))
        self.context.enter_context(patch.object(producer, "project_inputs", return_value=([], self.lock)))
        self.context.enter_context(patch.object(producer, "checked_entry", return_value=(self.entry, [], "darwin_aarch64")))
        self.context.enter_context(contextlib.redirect_stdout(io.StringIO()))

    def run_main(self, build=False):
        argv = ["producer", "--checkout", str(self.checkout), "--work", str(self.work)]
        if build:
            argv.append("--build")
        with patch.object(sys, "argv", argv):
            producer.main()

    def upstream_distribution(self,*_args,**_kwargs):
        folder=self.work/'dist';folder.mkdir()
        (folder/('sdk-repo-darwin_aarch64-emulator-'+producer.BUILD_ID+'.zip')).write_bytes(b'owned mock artifact')
        return SimpleNamespace(returncode=0)

    def test_upstream_exit_is_separate_from_a_missing_distribution(self):
        with patch.object(producer.subprocess,'run',return_value=SimpleNamespace(returncode=0)):
            with self.assertRaisesRegex(ValueError,'distribution was not produced'):self.run_main(build=True)
        result=json.loads((self.work/'build-result.json').read_text())
        self.assertEqual(result['build_status'],'unknown')
        self.assertEqual(result['returncode'],0)
        self.assertEqual(result['verification_stage'],'distribution')
        self.assertNotIn('artifact',result)

    def test_source_postcheck_failure_is_not_misreported_as_a_failed_compiler(self):
        with patch.object(producer.subprocess,'run',side_effect=self.upstream_distribution),\
             patch.object(producer,'project_inputs',side_effect=[([],self.lock),ValueError('Source checkout is not clean: external/qemu')]):
            with self.assertRaisesRegex(ValueError,'not clean'):self.run_main(build=True)
        result=json.loads((self.work/'build-result.json').read_text())
        self.assertEqual(result['build_status'],'unknown')
        self.assertEqual(result['returncode'],0)
        self.assertEqual(result['verification_stage'],'source_inputs')
        self.assertNotIn('artifact',result)

    def test_digest_failure_preserves_the_verified_stage_without_qualifying_the_artifact(self):
        with patch.object(producer.subprocess,'run',side_effect=self.upstream_distribution),\
             patch.object(producer,'sha256',side_effect=OSError('owned read failed')):
            with self.assertRaises(OSError):self.run_main(build=True)
        result=json.loads((self.work/'build-result.json').read_text())
        self.assertEqual(result['build_status'],'unknown')
        self.assertEqual(result['returncode'],0)
        self.assertEqual(result['verification_stage'],'artifact_digest')
        self.assertNotIn('artifact',result)

    def test_failed_upstream_preserves_the_actual_exit_and_has_no_verification_stage(self):
        with patch.object(producer.subprocess,'run',return_value=SimpleNamespace(returncode=7)):
            with self.assertRaisesRegex(ValueError,'Upstream build failed'):self.run_main(build=True)
        result=json.loads((self.work/'build-result.json').read_text())
        self.assertEqual(result['build_status'],'failed')
        self.assertEqual(result['returncode'],7)
        self.assertNotIn('verification_stage',result)

    def test_plan_cannot_launch_upstream_or_create_work(self):
        with patch.object(producer.subprocess, "run", side_effect=AssertionError("Unexpected build")) as run:
            self.run_main()
        run.assert_not_called()
        self.assertFalse(self.work.exists())

    def test_build_uses_verified_distribution_and_preserves_unverified_gates(self):
        def build(command, **kwargs):
            self.assertEqual(command[command.index("--target") + 1], "darwin_aarch64")
            self.assertEqual(command[command.index("--crash") + 1], "none")
            self.assertEqual(command[command.index("--ccache") + 1], "none")
            self.assertIn("--qtwebengine", command)
            self.assertNotIn("--aosp", command)
            self.assertIn("--verbose", command)
            self.assertNotIn("--task-disable", command)
            self.assertNotIn("PYTHONPATH", kwargs["env"])
            self.assertEqual(kwargs["cwd"], self.qemu)
            distribution = self.work / "dist"
            distribution.mkdir()
            (distribution / ("sdk-repo-darwin_aarch64-emulator-" + producer.BUILD_ID + ".zip")).write_bytes(b"owned mock artifact")
            return SimpleNamespace(returncode=0)
        with patch.object(producer.subprocess, "run", side_effect=build):
            self.run_main(build=True)
        result = json.loads((self.work / "build-result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["build_status"], "built")
        self.assertEqual(result["distribution"], "darwin_aarch64")
        self.assertEqual(result["source_contract"]["emulator_revision"], "35.3.8")
        self.assertEqual(result["redistribution"], "review-pending")
        self.assertEqual(result["source_dependency_closure"], "unverified")
        self.assertEqual(result["android35_root_boot"], "not-executed")

    def test_interrupted_build_keeps_unknown_receipt(self):
        with patch.object(producer.subprocess, "run", side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                self.run_main(build=True)
        result = json.loads((self.work / "build-result.json").read_text(encoding="utf-8"))
        self.assertEqual(result["build_status"], "unknown")
        self.assertNotIn("artifact", result)

    def test_nested_fixed_sdk_path_writes_are_isolated_before_source_postcheck(self):
        original = self.sdk.stat().st_ino

        def build(*args, **kwargs):
            # The pinned upstream runner recomputes this path instead of honoring env.
            (self.sdk / ".knownPackages").write_text("generated metadata")
            (self.sdk / "package.xml").write_text("generated package")
            return self.upstream_distribution()

        def check(*args):
            self.assertEqual((self.sdk / ".knownPackages").read_text(), "pinned metadata")
            self.assertFalse((self.sdk / "package.xml").exists())
            self.assertFalse(self.sdk.is_symlink())
            self.assertEqual(self.sdk.stat().st_ino, original)
            return [], self.lock

        with patch.object(producer.subprocess, "run", side_effect=build), \
             patch.object(producer, "project_inputs", side_effect=check):
            self.run_main(build=True)
        self.assertEqual((self.work / "sdk/package.xml").read_text(), "generated package")
        self.assertEqual(json.loads((self.work / "build-result.json").read_text())["build_status"], "built")


class SdkWorkspaceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="aiphone-sdk-workspace-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.checkout = self.root / "checkout"
        self.source = self.checkout / "prebuilts/android-emulator-build/system-images/linux"
        self.source.mkdir(parents=True)
        (self.source / ".knownPackages").write_bytes(b"pinned metadata")
        generic = self.source.parent / "generic/system-images/android-30"
        generic.mkdir(parents=True)
        (generic / "source.properties").write_bytes(b"pinned image")
        (self.source / "system-images").symlink_to("../generic/system-images", target_is_directory=True)
        self.work = self.root / "work"
        self.work.mkdir()
        self.inode = self.source.stat().st_ino

    def assert_original(self):
        self.assertFalse(self.source.is_symlink())
        self.assertEqual(self.source.stat().st_ino, self.inode)
        self.assertEqual((self.source / ".knownPackages").read_bytes(), b"pinned metadata")
        self.assertTrue((self.source / "system-images").is_symlink())
        self.assertEqual((self.source / "system-images/android-30/source.properties").read_bytes(), b"pinned image")
        self.assertFalse((self.source / "system-images/android-30/package.xml").exists())

    def test_sdk_aliases_are_dereferenced_before_the_original_directory_moves(self):
        with producer.isolated_sdk(self.checkout, self.work, "linux") as sdk:
            self.assertEqual(sdk, self.work / "sdk")
            self.assertTrue(self.source.is_symlink())
            self.assertFalse((sdk / "system-images").is_symlink())
            (self.source / ".knownPackages").write_text("generated metadata")
            (self.source / "system-images/android-30/source.properties").write_text("generated metadata")
            (self.source / "system-images/android-30/package.xml").write_text("generated package")
        self.assert_original()
        self.assertEqual((sdk / "system-images/android-30/package.xml").read_text(), "generated package")
        self.assertFalse((self.work / "source-sdk").exists())

    def test_upstream_interruption_restores_the_original_directory(self):
        with self.assertRaises(KeyboardInterrupt):
            with producer.isolated_sdk(self.checkout, self.work, "linux"):
                (self.source / ".knownPackages").write_text("generated metadata")
                raise KeyboardInterrupt
        self.assert_original()

    def test_link_creation_failure_restores_the_original_directory(self):
        with patch.object(Path, "symlink_to", side_effect=PermissionError("directory links unavailable")):
            with self.assertRaises(PermissionError):
                with producer.isolated_sdk(self.checkout, self.work, "linux"):
                    self.fail("upstream must not execute")
        self.assert_original()

    def test_preexisting_sdk_alias_is_rejected_without_moving_its_target(self):
        original = self.work / "fixture-original"
        self.source.rename(original)
        self.source.symlink_to(original, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, "pinned SDK directory"):
            with producer.isolated_sdk(self.checkout, self.work, "linux"):
                self.fail("aliased input must not reach upstream")
        self.assertEqual(self.source.readlink(), original)
        self.assertEqual(original.stat().st_ino, self.inode)
        self.assertFalse((self.work / "source-sdk").exists())

    def test_unexpected_sdk_replacement_is_preserved_with_the_original_for_recovery(self):
        with self.assertRaisesRegex(ValueError, "SDK path changed"):
            with producer.isolated_sdk(self.checkout, self.work, "linux"):
                self.source.unlink()
                self.source.mkdir()
                (self.source / "unexpected").write_text("preserve")
        self.assertEqual((self.source / "unexpected").read_text(), "preserve")
        self.assertEqual((self.work / "source-sdk/.knownPackages").read_bytes(), b"pinned metadata")


if __name__ == "__main__":
    unittest.main()
