"""Owned files and narrow process mocks; no Git, GPG, network, KVM or build execution."""
import contextlib
import hashlib
import io
import json
from pathlib import Path
import signal
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch


def deny_external(event, arguments):
    if event.startswith("subprocess.") or event in {"socket.connect", "socket.connect_ex", "os.system", "os.kill", "os.killpg"}:
        raise AssertionError("Unmocked external operation: " + event)


sys.addaudithook(deny_external)
import linux_bootstrap as bootstrap


SNAPSHOT = {"cpu_count": 4, "memory_available_bytes": 16 * bootstrap.GIB, "disk_free_bytes": 40 * bootstrap.GIB}
LAUNCHER = b"MAINTAINER_KEYS = 'owned public-key fixture'\nraise AssertionError('must never execute upstream')\n"
LAUNCHER_BLOB = hashlib.sha1(b"blob " + str(len(LAUNCHER)).encode("ascii") + b"\0" + LAUNCHER).hexdigest()


class OwnedFixture(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="emulator-bootstrap-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name).resolve()


class BootstrapInputTests(OwnedFixture):
    def test_resource_snapshot_reports_total_and_available_disk_separately(self):
        with patch.object(bootstrap.Path, 'read_text', return_value='MemAvailable: 2048 kB\n'), patch.object(bootstrap.os, 'cpu_count', return_value=4):
            with patch.object(bootstrap.shutil, 'disk_usage', return_value=SimpleNamespace(total=120*bootstrap.GIB,free=96*bootstrap.GIB)):
                self.assertEqual(bootstrap.resource_snapshot(self.root), {'cpu_count':4,'memory_available_bytes':2048*1024,
                    'disk_total_bytes':120*bootstrap.GIB,'disk_free_bytes':96*bootstrap.GIB})

    def linux_project_fixture(self):
        projects, unused_lock = bootstrap.producer.locked_projects()
        selected = [node for node in projects if "notdefault" not in node.get("groups", "").split(",")
                    or "platform-linux" in node.get("groups", "").split(",")]
        checkout = self.root / "source"
        expected = {node.get("path"): node.get("revision") for node in selected}
        for name in expected:
            checkout.joinpath(name).mkdir(parents=True)
        return checkout, expected

    def test_real_lock_selects_and_proves_only_linux_66(self):
        checkout, expected = self.linux_project_fixture()

        def read(folder, *arguments):
            if arguments == ("rev-parse", "HEAD"):
                return expected[folder.relative_to(checkout).as_posix()]
            self.assertEqual(arguments, ("status", "--porcelain", "--untracked-files=all"))
            return ""

        with patch.object(bootstrap.producer, "git_read", side_effect=read) as read_git:
            records, unused_lock = bootstrap.producer.project_inputs(checkout, "linux")
        self.assertEqual({record["path"]: record["revision"] for record in records}, expected)
        self.assertEqual(len(records), 66)
        self.assertEqual(read_git.call_count, 132)

    def test_real_lock_rejects_wrong_selected_commit(self):
        checkout, expected = self.linux_project_fixture()

        def read(folder, *arguments):
            if arguments == ("rev-parse", "HEAD"):
                return "0" * 40 if folder == checkout / "external/qemu" else expected[folder.relative_to(checkout).as_posix()]
            return ""

        with patch.object(bootstrap.producer, "git_read", side_effect=read):
            with self.assertRaisesRegex(ValueError, "Source project revision mismatch: external/qemu"):
                bootstrap.producer.project_inputs(checkout, "linux")

    def test_fresh_root_rejects_existing_files_without_modification(self):
        existing = self.root / "existing"
        existing.mkdir()
        marker = existing / "marker"
        marker.write_bytes(b"retained")
        with self.assertRaisesRegex(bootstrap.BuildFailure, "fresh_work_required"):
            bootstrap.fresh_root(existing)
        self.assertEqual(marker.read_bytes(), b"retained")

    def test_git_repo_gpg_config_are_nonce_owned_without_home_override(self):
        with patch.dict(bootstrap.os.environ, {"PRIVATE_SENTINEL": "never-forward"}):
            environment = bootstrap.isolated_environment(self.root)
        self.assertEqual(environment.get("HOME"), bootstrap.os.environ.get("HOME"))
        self.assertNotIn("PRIVATE_SENTINEL", environment)
        self.assertEqual(environment["GIT_CONFIG_GLOBAL"], environment["XDG_CONFIG_HOME"] + "/git/config")
        self.assertTrue(Path(environment["GIT_CONFIG_GLOBAL"]).is_file())
        self.assertTrue(Path(environment["GNUPGHOME"]).is_relative_to(self.root))
        self.assertTrue(Path(environment["REPO_CONFIG_DIR"]).is_relative_to(self.root))
        self.assertNotIn("include", Path(environment["GIT_CONFIG_GLOBAL"]).read_text())

    def test_existing_netrc_rejects_without_reading_its_contents(self):
        (self.root / ".netrc").write_bytes(b"private sentinel")
        with patch.object(bootstrap.platform, "system", return_value="Linux"), patch.object(bootstrap.platform, "machine", return_value="x86_64"):
            with patch.object(bootstrap.Path, "home", return_value=self.root):
                with self.assertRaisesRegex(bootstrap.BuildFailure, "clean_builder_home_required"):
                    bootstrap.host_requirements()

    def test_missing_gpg_is_not_a_permitted_signature_downgrade(self):
        with patch.object(bootstrap.platform, "system", return_value="Linux"), patch.object(bootstrap.platform, "machine", return_value="x86_64"):
            with patch.object(bootstrap.Path, "home", return_value=self.root):
                with patch.object(bootstrap.shutil, "which", side_effect=lambda name: "/usr/bin/git" if name == "git" else None):
                    with self.assertRaisesRegex(bootstrap.BuildFailure, "required_tool_missing"):
                        bootstrap.host_requirements()

    def test_launcher_blob_and_literal_keys_are_checked_without_import(self):
        path = self.root / "repo"
        path.write_bytes(LAUNCHER)
        with patch.object(bootstrap, "REPO_LAUNCHER_BLOB", LAUNCHER_BLOB):
            self.assertEqual(bootstrap.launcher_keys(path), "owned public-key fixture")
        path.write_bytes(LAUNCHER + b"# changed\n")
        with patch.object(bootstrap, "REPO_LAUNCHER_BLOB", LAUNCHER_BLOB):
            with self.assertRaisesRegex(bootstrap.BuildFailure, "repo_launcher_identity"):
                bootstrap.launcher_keys(path)

    def test_failure_reason_cannot_project_arbitrary_text(self):
        self.assertEqual(str(bootstrap.BuildFailure("private-key-sentinel")), "internal_failure")


class RepoBoundaryTests(OwnedFixture):
    def test_native_input_selection_keeps_linux_qt_and_the_pinned_compiler(self):
        checkout = self.root / "checkout"
        projects = ("prebuilts/clang/host/linux-x86", "prebuilts/android-emulator-build/qt")
        for project in projects:
            (checkout / ".repo/projects" / (project + ".git")).mkdir(parents=True)
        command = SimpleNamespace(run=Mock(side_effect=lambda *args, capture=False, **kwargs: "tree" if capture else ""))
        bootstrap.select_build_inputs(checkout, command)
        qt = checkout / ".repo/projects/prebuilts/android-emulator-build/qt.git/info/sparse-checkout"
        self.assertEqual(qt.read_text(), "/*\n!/*/\n/linux-x86_64/\n")
        clang = checkout / ".repo/projects/prebuilts/clang/host/linux-x86.git/info/sparse-checkout"
        self.assertEqual(clang.read_text(), "/*\n!/*/\n/clang-r487747c/\n")

    def test_installed_repo_mismatch_or_dirty_state_blocks_recipe(self):
        checkout = self.root / "checkout"
        manifest = checkout / ".repo/manifests"
        manifest.mkdir(parents=True)
        (manifest / "source-lock.xml").write_bytes((bootstrap.HERE / "source-lock.xml").read_bytes())
        for dirty in (False, True):
            with self.subTest(dirty=dirty):
                def read(command, folder, *arguments, **kwargs):
                    if folder == manifest:
                        return "1" * 40
                    if arguments[0] == "status":
                        return " M repo" if dirty else ""
                    return bootstrap.REPO_COMMIT if dirty else "0" * 40
                with patch.object(bootstrap, "git", side_effect=read), patch.object(bootstrap.producer, "recipe_inputs") as recipe:
                    with self.assertRaises(bootstrap.BuildFailure):
                        bootstrap.verified_sources(object(), checkout, "1" * 40)
                recipe.assert_not_called()

    def tool_commands(self, fail_signature=False, wrong_tag=False):
        calls = []

        def run(arguments, cwd, **kwargs):
            calls.append(arguments)
            if arguments[0] == "gpg":
                self.assertEqual(arguments[1:4], ["--batch", "--no-autostart", "--import"])
                return ""
            self.assertEqual(arguments[0], "git")
            operation = arguments[5:]
            if operation[0] == "rev-parse":
                if operation[1] == "refs/tags/" + bootstrap.REPO_TAG:
                    return "0" * 40 if wrong_tag else bootstrap.REPO_TAG_OBJECT
                return bootstrap.REPO_COMMIT
            if operation[0] == "checkout":
                (Path(cwd) / "repo").write_bytes(LAUNCHER)
                return ""
            if operation[:3] == ["-c", "gpg.program=gpg", "verify-tag"]:
                if fail_signature:
                    raise bootstrap.BuildFailure("command_failed")
                return ""
            if operation[0] in {"init", "remote", "fetch"}:
                return ""
            raise AssertionError("Unrecognized tool fixture command")

        return SimpleNamespace(run=Mock(side_effect=run)), calls

    def test_tool_signature_check_precedes_any_launcher_return(self):
        command, calls = self.tool_commands()
        with patch.object(bootstrap, "REPO_LAUNCHER_BLOB", LAUNCHER_BLOB):
            launcher = bootstrap.pinned_repo_tool(self.root, command)
        self.assertEqual(launcher, self.root / "repo-tool/repo")
        imported = next(i for i, args in enumerate(calls) if args[0] == "gpg")
        verified = next(i for i, args in enumerate(calls) if "verify-tag" in args)
        self.assertLess(imported, verified)
        self.assertTrue(all(args[0] in {"git", "gpg"} for args in calls))

    def test_invalid_signature_prevents_launcher_use(self):
        command, calls = self.tool_commands(fail_signature=True)
        with patch.object(bootstrap, "REPO_LAUNCHER_BLOB", LAUNCHER_BLOB):
            with self.assertRaisesRegex(bootstrap.BuildFailure, "command_failed"):
                bootstrap.pinned_repo_tool(self.root, command)
        self.assertFalse(any(args[0] == sys.executable for args in calls))

    def test_wrong_tag_object_is_rejected_before_checkout(self):
        command, calls = self.tool_commands(wrong_tag=True)
        with self.assertRaisesRegex(bootstrap.BuildFailure, "repo_tag_identity"):
            bootstrap.pinned_repo_tool(self.root, command)
        self.assertFalse(any("checkout" in args for args in calls))

    def test_sync_uses_exact_repo_pin_host_groups_and_owned_manifest(self):
        command = SimpleNamespace(run=Mock(return_value=""))
        manifest = self.root / "manifest"
        manifest.mkdir()
        phases = []
        boundaries = []
        command.run.side_effect = lambda *args, **kwargs: boundaries.append(phases[-1]) or ''
        with patch.object(bootstrap, 'select_build_inputs') as select:
            bootstrap.sync_sources(self.root, command, self.root / "pinned/repo", manifest, "1" * 40,on_phase=phases.append)
        initial, fetched, synchronized = command.run.call_args_list
        init_args = initial.args[0]
        self.assertEqual(init_args[init_args.index("-u") + 1], manifest.resolve().as_uri())
        self.assertEqual(init_args[init_args.index("--repo-rev") + 1], bootstrap.REPO_COMMIT)
        self.assertEqual(init_args[init_args.index("-g") + 1], "default,platform-linux")
        self.assertIn("--depth=1", init_args)
        self.assertIn("--partial-clone", init_args)
        self.assertIn("--clone-filter=blob:none", init_args)
        self.assertNotIn("--no-repo-verify", init_args)
        self.assertTrue(initial.kwargs["manifest_file"])
        self.assertNotIn("manifest_file", synchronized.kwargs)
        self.assertIn("--no-manifest-update", synchronized.args[0])
        self.assertIn('--network-only', fetched.args[0])
        self.assertNotIn('--detach', fetched.args[0])
        self.assertIn('--local-only', synchronized.args[0])
        self.assertIn('--detach', synchronized.args[0])
        select.assert_called_once_with(self.root/'checkout', command)
        self.assertEqual(phases,['initialize','fetch','select_inputs','checkout'])
        self.assertEqual(boundaries,['initialize','fetch','checkout'])

    def test_sparse_selection_requires_the_locked_directory_to_be_a_tree(self):
        checkout=self.root/'checkout';project='prebuilts/android-emulator-build/qt'
        metadata=checkout/'.repo/projects'/(project+'.git');metadata.mkdir(parents=True)
        with self.assertRaisesRegex(bootstrap.BuildFailure,'source_input_tree'):
            bootstrap.select_input_project(checkout,SimpleNamespace(run=Mock(return_value='blob')),project,('linux-x86_64',),'a'*40)
        self.assertFalse((metadata/'info/sparse-checkout').exists())

    def test_sparse_selection_keeps_root_files_and_the_pinned_compiler_only(self):
        checkout=self.root/'checkout';folder=checkout/'.repo/projects/prebuilts/clang/host/linux-x86.git'
        folder.mkdir(parents=True);command=SimpleNamespace(run=Mock(return_value='tree'))
        bootstrap.select_input_project(checkout,command,'prebuilts/clang/host/linux-x86',('clang-r487747c',),'1'*40)
        self.assertEqual((folder/'info/sparse-checkout').read_text(),'/*\n!/*/\n/clang-r487747c/\n')
        self.assertEqual(command.run.call_args_list[0].args[0][-3:],['cat-file','-t','1'*40+':clang-r487747c'])
        self.assertEqual([call.args[0][-3:] for call in command.run.call_args_list[1:]],[['config','core.sparseCheckout','true'],['config','core.sparseCheckoutCone','true']])
        with self.assertRaisesRegex(bootstrap.BuildFailure,'source_checkout_layout'):
            bootstrap.select_input_project(checkout,command,'prebuilts/clang/host/linux-x86',('clang-r487747c',),'1'*40)

    def test_network_and_checkout_share_the_existing_sync_time_budget(self):
        for elapsed,expired in ((10800.5,False),(10801,True),(10802,True)):
            with self.subTest(elapsed=elapsed):
                root=self.root/str(elapsed);root.mkdir();manifest=root/'manifest';manifest.mkdir()
                command=SimpleNamespace(run=Mock(return_value=''))
                with patch.object(bootstrap,'select_build_inputs'),patch.object(bootstrap.time,'monotonic',side_effect=[1,elapsed]):
                    if expired:
                        with self.assertRaisesRegex(bootstrap.BuildInterrupted,'command_timeout'):
                            bootstrap.sync_sources(root,command,root/'pinned/repo',manifest,'1'*40)
                    else:bootstrap.sync_sources(root,command,root/'pinned/repo',manifest,'1'*40)
                self.assertEqual(len(command.run.call_args_list),2 if expired else 3)
                if expired:self.assertIn('--network-only',command.run.call_args_list[-1].args[0])

    def test_sparse_selection_requires_existing_project_metadata(self):
        checkout=self.root/'checkout';checkout.mkdir()
        with self.assertRaisesRegex(bootstrap.BuildFailure,'source_checkout_layout'):
            bootstrap.select_input_project(checkout,SimpleNamespace(run=Mock(return_value='')),'prebuilts/clang/host/linux-x86',('clang-r487747c',),'1'*40)
        self.assertEqual(list(checkout.iterdir()),[])

    def test_sparse_selection_rejects_a_git_or_info_directory_outside_owned_metadata(self):
        for alias in ('project','info'):
            with self.subTest(alias=alias):
                checkout=self.root/alias;folder=checkout/'.repo/projects/prebuilts/clang/host/linux-x86.git'
                outside=self.root/(alias+'-outside');outside.mkdir()
                if alias=='project':
                    folder.parent.mkdir(parents=True);folder.symlink_to(outside,target_is_directory=True)
                else:
                    folder.mkdir(parents=True);(folder/'info').symlink_to(outside,target_is_directory=True)
                with self.assertRaisesRegex(bootstrap.BuildFailure,'source_checkout_layout'):
                    bootstrap.select_input_project(checkout,SimpleNamespace(run=Mock(return_value='')),'prebuilts/clang/host/linux-x86',('clang-r487747c',),'1'*40)
                self.assertEqual(list(outside.iterdir()),[])


class CommandOwnerTests(OwnedFixture):
    def setUp(self):
        super().setUp()
        self.environment = bootstrap.isolated_environment(self.root)
        self.log = (self.root / "owned.log").open("wb")
        self.addCleanup(self.log.close)
        self.command = bootstrap.SourceCommand(self.root, self.environment, self.log)

    def test_disk_floor_stops_only_the_new_owned_session(self):
        process = SimpleNamespace(pid=43210, returncode=None, poll=Mock(return_value=None), wait=Mock(return_value=-9))
        exited = SimpleNamespace(si_pid=43210, si_code=bootstrap.os.CLD_EXITED, si_status=0)
        with patch.object(bootstrap.os, "waitid", side_effect=[None, None, None, exited, exited]), patch.object(bootstrap.os, "getsid", return_value=43210):
            with patch.object(bootstrap.subprocess, "Popen", return_value=process) as start:
                with patch.object(bootstrap.shutil, "disk_usage", return_value=SimpleNamespace(free=0)):
                    with patch.object(bootstrap.os, "getpgid", return_value=43210), patch.object(bootstrap.os, "killpg") as kill:
                        with patch.object(bootstrap.time, "sleep"):
                            with self.assertRaisesRegex(bootstrap.BuildInterrupted, "disk_floor"):
                                self.command.run(["never-executed"], self.root)
            self.assertTrue(start.call_args.kwargs["start_new_session"])
            self.assertEqual([call.args for call in kill.call_args_list], [(43210, signal.SIGTERM), (43210, signal.SIGKILL)])
            process.wait.assert_called_once_with(timeout=10)

    def test_owner_mismatch_preserves_original_and_cleanup_failure(self):
        process = SimpleNamespace(pid=43210, returncode=None, wait=Mock())
        with patch.object(bootstrap.os, "waitid", return_value=None):
            with patch.object(bootstrap.subprocess, "Popen", return_value=process):
                with patch.object(self.command, "wait", side_effect=bootstrap.BuildInterrupted("command_timeout")):
                    with patch.object(bootstrap.os, "getpgid", return_value=43211), patch.object(bootstrap.os, "killpg") as kill:
                        with self.assertRaises(BaseExceptionGroup) as raised:
                            self.command.run(["never-executed"], self.root)
            self.assertIsInstance(raised.exception.exceptions[0], bootstrap.BuildInterrupted)
            self.assertIsInstance(raised.exception.exceptions[1], bootstrap.BuildFailure)
            kill.assert_not_called()

    def test_only_init_transport_capability_allows_local_manifest(self):
        process = SimpleNamespace(pid=43210, returncode=0, poll=Mock(return_value=0), wait=Mock(return_value=0))
        exited = SimpleNamespace(si_pid=43210, si_code=bootstrap.os.CLD_EXITED, si_status=0)
        with patch.object(bootstrap.os, "waitid", return_value=exited), patch.object(bootstrap.os, "getpgid", return_value=43210), patch.object(bootstrap.os, "getsid", return_value=43210), patch.object(bootstrap.os, "killpg"):
            with patch.object(bootstrap.subprocess, "Popen", return_value=process) as start:
                self.command.run(["never-executed"], self.root)
                self.assertEqual(start.call_args.kwargs["env"]["GIT_ALLOW_PROTOCOL"], "https")
                self.command.run(["never-executed"], self.root, manifest_file=True)
                self.assertEqual(start.call_args.kwargs["env"]["GIT_ALLOW_PROTOCOL"], "file:https")

    def test_cancel_signals_owned_session_and_reraises_original(self):
        process = SimpleNamespace(pid=43210, returncode=None, wait=Mock(return_value=-9))
        exited = SimpleNamespace(si_pid=43210, si_code=bootstrap.os.CLD_EXITED, si_status=0)
        with patch.object(bootstrap.os, "waitid", side_effect=[None, None, exited, exited]), patch.object(bootstrap.os, "getsid", return_value=43210):
            with patch.object(bootstrap.subprocess, "Popen", return_value=process):
                with patch.object(self.command, "wait", side_effect=KeyboardInterrupt):
                    with patch.object(bootstrap.os, "getpgid", return_value=43210), patch.object(bootstrap.os, "killpg") as kill:
                        with patch.object(bootstrap.time, "sleep"):
                            with self.assertRaises(KeyboardInterrupt):
                                self.command.run(["never-executed"], self.root)
            self.assertEqual([call.args for call in kill.call_args_list], [(43210, signal.SIGTERM), (43210, signal.SIGKILL)])


class LifecycleTests(OwnedFixture):
    def setUp(self):
        super().setUp()
        self.context = contextlib.ExitStack()
        self.addCleanup(self.context.close)
        self.context.enter_context(patch.object(bootstrap, "admission", return_value=SNAPSHOT))
        self.context.enter_context(patch.object(bootstrap, "resource_snapshot", return_value=SNAPSHOT))
        self.context.enter_context(patch.object(bootstrap, "require_kvm"))
        self.context.enter_context(patch.object(bootstrap, "pinned_repo_tool", return_value=self.root / "tool/repo"))
        self.context.enter_context(patch.object(bootstrap, "local_manifest", return_value=(self.root / "manifest", "1" * 40)))
        self.sync = self.context.enter_context(patch.object(bootstrap, "sync_sources", return_value=self.root / "checkout"))
        self.proof = self.context.enter_context(patch.object(bootstrap, "verified_sources"))

    def public_receipt(self):
        return json.loads((self.root / "public-receipt.json").read_text(encoding="utf-8"))

    def test_disk_interruption_retains_the_sync_phase_without_restarting_it(self):
        def stop(*args,on_phase):
            on_phase('checkout')
            args[1].log.write(('Checkout '+str(self.root/'checkout/input')+'\n').encode())
            args[1].log.flush()
            raise bootstrap.BuildInterrupted('disk_floor')
        self.sync.side_effect=stop
        output=io.StringIO()
        with contextlib.redirect_stderr(output),self.assertRaisesRegex(bootstrap.BuildInterrupted,'disk_floor'):
            bootstrap.controlled_build(self.root)
        receipt=self.public_receipt()
        self.assertEqual(receipt['sync_phase'],'checkout')
        self.assertEqual(receipt['resources']['sync_checkout'],SNAPSHOT)
        self.assertEqual(receipt['status'],'unknown')
        self.assertEqual(receipt['reason'],'disk_floor')
        self.assertIn('Checkout <build>/checkout/input',output.getvalue())
        self.assertNotIn(str(self.root),output.getvalue())
        self.sync.assert_called_once()

    def test_public_build_tail_keeps_errors_and_redacts_credentials_and_builder_paths(self):
        build = self.root / "build"
        build.mkdir()
        key = "sk-" + "a" * 32
        (build / "build.log").write_text(
            'omitted-head\n' + 'x' * 9000 + '\nCMake Error at ' + str(self.root / 'checkout/source.cmake') + ': compiler missing\n'
            'cache=' + str(Path.home() / '.cache') + '\nworker=owned-worker-fixture\n'
            'api_key=private-key-sentinel\n' + key + '\n')
        output = io.StringIO()
        with patch.dict(bootstrap.os.environ, {"RUNNER_TRACKING_ID": "owned-worker-fixture"}), contextlib.redirect_stderr(output):
            bootstrap.print_failure_log(self.root)
        text = output.getvalue()
        self.assertIn('CMake Error at <build>/checkout/source.cmake: compiler missing', text)
        self.assertIn('cache=<home>/.cache', text)
        self.assertIn('worker=<runner>', text)
        self.assertNotIn('omitted-head', text)
        self.assertNotIn(str(self.root), text)
        self.assertNotIn(str(Path.home()), text)
        self.assertNotIn('private-key-sentinel', text)
        self.assertNotIn(key, text)
        self.assertLess(len(text), 8400)

    def test_missing_log_or_closed_diagnostic_stream_cannot_replace_build_failure(self):
        output = io.StringIO()
        with contextlib.redirect_stderr(output):
            bootstrap.print_failure_log(self.root)
        self.assertEqual(output.getvalue(), '')
        build = self.root / "build"
        build.mkdir()
        (build / 'build.log').write_text('CMake Error: owned failure')
        output.close()
        with contextlib.redirect_stderr(output):
            bootstrap.print_failure_log(self.root)

    def test_failed_upstream_keeps_fixed_diagnostics_without_log_contents(self):
        calls = []

        def run(command, arguments, cwd, **kwargs):
            calls.append(arguments)
            if "--build" not in arguments:
                return ""
            build = self.root / "build"
            build.mkdir()
            result = {"build_status": "failed", "returncode": 2,
                      "source_lock_sha256": bootstrap.producer.LOCK_SHA256,
                      "build_id": bootstrap.producer.BUILD_ID, "target": "linux_x86_64",
                      "api_key": "private-key-sentinel"}
            (build / "build-result.json").write_text(json.dumps(result))
            recipe = self.root / "checkout/external/qemu/android/build/python/cmake.py"
            (build / "build.log").write_text(
                'private-key-sentinel\n  File "' + str(recipe) + '", line 91, in main\n'
                "ModuleNotFoundError: No module named 'private-key-sentinel'\n")
            raise bootstrap.BuildFailure("command_failed")

        with patch.object(bootstrap.SourceCommand, "run", autospec=True, side_effect=run):
            with self.assertRaisesRegex(bootstrap.BuildFailure, "command_failed"):
                bootstrap.controlled_build(self.root)
        receipt = self.public_receipt()
        self.assertEqual(len(calls), 2)
        self.assertEqual(receipt["status"], "failed")
        self.assertEqual(receipt["build_diagnostics"], {
            "producer_status": "failed", "upstream_returncode": 2,
            "upstream": {"exception": "ModuleNotFoundError", "recipe_frames": [
                {"file": "android/build/python/cmake.py", "line": 91}], "signals": []},
            "producer": {"recipe_frames": [], "signals": []}})
        self.assertNotIn("private-", json.dumps(receipt))
        self.assertNotIn(str(self.root), json.dumps(receipt))

    def test_invalid_or_absent_producer_result_does_not_mask_original_failure(self):
        for value in (None, "broken-json", json.dumps({"build_status": "failed", "returncode": True}),
                      json.dumps({"source_lock_sha256": "0" * 64, "returncode": "private-key-sentinel"})):
            with self.subTest(value=value), tempfile.TemporaryDirectory() as folder:
                root = Path(folder)
                build = root / "build"
                build.mkdir()
                if value is not None:
                    (build / "build-result.json").write_text(value)
                result = bootstrap.build_diagnostics(root)
                self.assertNotIn("upstream_returncode", result)
                self.assertNotIn("private-", json.dumps(result))

    def test_diagnostic_tail_is_bounded_and_ignores_unpinned_traceback_frames(self):
        log = self.root / "log"
        recipe = self.root / "checkout/external/qemu/android/build/python/cmake.py"
        log.write_text('ModuleNotFoundError: private-head\n' + 'x' * 70000 + '\n'
                       '  File "/private-key-sentinel.py", line 99, in private\n'
                       '  File "' + str(recipe) + '", line 12, in main\n'
                       'CMake Error: private-tail\nNo space left on device\n')
        result = bootstrap.log_diagnostics(log, self.root / "checkout")
        self.assertEqual(result, {"recipe_frames": [{"file": "android/build/python/cmake.py", "line": 12}],
                                  "signals": ["cmake_error", "disk_full"]})
        self.assertNotIn("private-", json.dumps(result))

    def test_short_log_keeps_its_first_byte_and_latest_pinned_error_location(self):
        log = self.root / "short.log"
        recipe = self.root / "checkout/external/qemu/android/build/python/cmake.py"
        log.write_text('CMake Error: private-message\nModuleNotFoundError: private-first\n'
                       '  File "' + str(recipe) + '", line 11, in main\n'
                       '  File "' + str(recipe) + '", line 82, in main\n'
                       'FileNotFoundError: private-last\n')
        self.assertEqual(bootstrap.log_diagnostics(log, self.root / "checkout"), {
            "exception": "FileNotFoundError", "recipe_frames": [{"file": "android/build/python/cmake.py", "line": 82}],
            "signals": ["cmake_error"]})

    def test_producer_identity_and_numeric_exit_contract(self):
        build = self.root / "build"
        build.mkdir()
        path = build / "build-result.json"
        identity = {"source_lock_sha256": bootstrap.producer.LOCK_SHA256,
                    "build_id": bootstrap.producer.BUILD_ID, "target": "linux_x86_64"}
        for status, code, expected in [("failed", -128, -128), ("failed", 255, 255), ("failed", True, None),
                                      ("failed", "private-key-sentinel", None), ("failed", -129, None),
                                      ("failed", 256, None), ("unknown", 2, None)]:
            with self.subTest(status=status, code=code):
                path.write_text(json.dumps({**identity, "build_status": status, "returncode": code}))
                result = bootstrap.producer_diagnostics(self.root)
                self.assertEqual(result["producer_status"], status)
                self.assertEqual(result.get("upstream_returncode"), expected)
        for value in ([], {**identity, "target": "private-key-sentinel"}):
            path.write_text(json.dumps(value))
            self.assertEqual(bootstrap.producer_diagnostics(self.root), {"producer_status": "invalid"})

    def test_receipt_byte_budget_accepts_the_limit_and_rejects_larger_input(self):
        build = self.root / "build"
        build.mkdir()
        path = build / "build-result.json"
        value = {"source_lock_sha256": bootstrap.producer.LOCK_SHA256,
                 "build_id": bootstrap.producer.BUILD_ID, "target": "linux_x86_64", "build_status": "unknown"}
        text = json.dumps(value)
        path.write_text(text + ' ' * (1024**2 - len(text)))
        self.assertEqual(bootstrap.producer_diagnostics(self.root), {"producer_status": "unknown"})
        with path.open('a') as output:
            output.write(' ')
        self.assertEqual(bootstrap.producer_diagnostics(self.root), {"producer_status": "invalid"})

    def test_plan_then_single_build_keep_only_sanitized_receipt(self):
        calls = []

        def run(command, arguments, cwd, **kwargs):
            calls.append(arguments)
            self.proof.assert_called_once()
            self.assertEqual(self.public_receipt()["stage"], "source_build" if "--build" in arguments else "source_proof")
            self.assertEqual(self.public_receipt()["status"], "unknown")
            if "--build" in arguments:
                build = self.root / "build"
                build.mkdir()
                result = {"build_status": "built", "source_lock_sha256": bootstrap.producer.LOCK_SHA256,
                          "build_id": bootstrap.producer.BUILD_ID, "target": "linux_x86_64",
                          "artifact": {"bytes": 1234, "sha256": "a" * 64, "path": "private-path-sentinel"},
                          "api_key": "private-key-sentinel"}
                (build / "build-result.json").write_text(json.dumps(result), encoding="utf-8")
            return ""

        with patch.object(bootstrap.SourceCommand, "run", autospec=True, side_effect=run):
            bootstrap.controlled_build(self.root)
        receipt = self.public_receipt()
        self.assertEqual(len(calls), 2)
        self.assertNotIn("--build", calls[0])
        self.assertIn("--build", calls[1])
        self.assertEqual(receipt["status"], "built")
        self.assertEqual(receipt["selected_projects"], 66)
        self.assertEqual(receipt["redistribution"], "review-pending")
        self.assertEqual(receipt["android35_root_boot"], "not-executed")
        self.assertNotIn("private-", json.dumps(receipt))

    def test_sync_failure_does_not_launch_producer(self):
        self.sync.side_effect = bootstrap.BuildFailure("command_failed")
        with patch.object(bootstrap.SourceCommand, "run") as run:
            with self.assertRaises(bootstrap.BuildFailure):
                bootstrap.controlled_build(self.root)
        run.assert_not_called()
        self.proof.assert_not_called()
        self.assertEqual(self.public_receipt()["stage"], "source_sync")
        self.assertEqual(self.public_receipt()["status"], "failed")

    def test_source_proof_failure_does_not_launch_producer(self):
        self.proof.side_effect = bootstrap.BuildFailure("installed_repo_identity")
        with patch.object(bootstrap.SourceCommand, "run") as run:
            with self.assertRaises(bootstrap.BuildFailure):
                bootstrap.controlled_build(self.root)
        run.assert_not_called()
        self.assertEqual(self.public_receipt()["stage"], "source_proof")
        self.assertEqual(self.public_receipt()["status"], "failed")

    def test_cancel_keeps_unknown_and_never_retries(self):
        self.sync.side_effect = KeyboardInterrupt
        with patch.object(bootstrap.SourceCommand, "run") as run:
            with self.assertRaises(KeyboardInterrupt):
                bootstrap.controlled_build(self.root)
        run.assert_not_called()
        self.sync.assert_called_once()
        self.assertEqual(self.public_receipt()["status"], "unknown")
        self.assertEqual(self.public_receipt()["reason"], "interrupted_or_unknown")


if __name__ == "__main__":
    unittest.main()
