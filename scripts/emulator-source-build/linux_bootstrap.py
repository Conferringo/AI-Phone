"""One isolated Linux source-build attempt; no installer or redistribution approval."""
import argparse
import ast
import contextlib
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import time

import aosp_emulator_producer as producer

HERE = Path(__file__).resolve().parent
REPO_URL = "https://gerrit.googlesource.com/git-repo"
REPO_TAG = "v2.54"
REPO_TAG_OBJECT = "4a375aa5f16bba000a26dd18641ebf7a99655460"
REPO_COMMIT = "97dc5c1bd9527c2abe2183b16a4b7ef037dc34a7"
REPO_LAUNCHER_BLOB = "2526ab03fdc8eb364e9fa2e50441c49690a721d5"
GIB = 1024**3
DISK_FLOOR = 2 * GIB
FAILURE_REASONS = {"process_owner", "command_failed", "metadata_budget", "fresh_work_required",
                   "work_outside_source_required", "linux_x64_required", "python_311_64bit_required",
                   "clean_builder_home_required", "required_tool_missing", "attempt_headroom",
                   "kvm_access_required", "repo_launcher_budget", "repo_launcher_identity",
                   "repo_key_declaration", "repo_tag_identity", "repo_commit_identity", "repo_checkout_identity",
                   "manifest_identity", "synced_manifest_identity", "manifest_checkout_identity",
                   "installed_repo_identity", "installed_repo_dirty", "producer_receipt"}


class BuildFailure(RuntimeError):
    def __init__(self, reason):
        super().__init__(reason if reason in FAILURE_REASONS else "internal_failure")


class BuildInterrupted(RuntimeError):
    pass


def observed_exit(process):
    """Prove this unreaped child is ours without releasing its PID reservation."""
    try:
        info = os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
    except OSError as error:
        raise BuildFailure("process_owner") from error
    if info is not None and (info.si_pid != process.pid or info.si_code not in {os.CLD_EXITED, os.CLD_KILLED, os.CLD_DUMPED}):
        raise BuildFailure("process_owner")
    return info


def signal_session(process, requested):
    observed_exit(process)
    try:
        if os.getpgid(process.pid) != process.pid or os.getsid(process.pid) != process.pid:
            raise BuildFailure("process_owner")
    except ProcessLookupError as error:
        raise BuildFailure("process_owner") from error
    try:
        os.killpg(process.pid, requested)
    except ProcessLookupError:
        # The still-unreaped leader reserves this identity; an empty group is done.
        pass


class SourceCommand:
    def __init__(self, root, environment, log):
        self.root = root
        self.environment = environment
        self.log = log

    def wait(self, process, timeout):
        deadline = time.monotonic() + timeout
        while True:
            info = observed_exit(process)
            if info is not None:
                return info.si_status if info.si_code == os.CLD_EXITED else -info.si_status
            if shutil.disk_usage(self.root).free < DISK_FLOOR:
                raise BuildInterrupted("disk_floor")
            if time.monotonic() >= deadline:
                raise BuildInterrupted("command_timeout")
            time.sleep(.05)

    def stop(self, process):
        if observed_exit(process) is None:
            signal_session(process, signal.SIGTERM)
            deadline = time.monotonic() + 2
            while observed_exit(process) is None and time.monotonic() < deadline:
                time.sleep(.02)
        # An exited leader stays unreaped until its remaining group has been stopped.
        # Short, successful Git commands therefore need no fixed grace delay.
        signal_session(process, signal.SIGKILL)
        process.wait(timeout=10)

    def run(self, arguments, cwd, timeout=300, capture=False, manifest_file=False):
        output = tempfile.TemporaryFile(dir=self.root / "tmp") if capture else contextlib.nullcontext(self.log)
        environment = {**self.environment, "GIT_ALLOW_PROTOCOL": "file:https" if manifest_file else "https"}
        with output as stream:
            process = subprocess.Popen(arguments, cwd=cwd, env=environment, stdin=subprocess.DEVNULL,
                                       stdout=stream, stderr=self.log, start_new_session=True)
            try:
                code = self.wait(process, timeout)
                if code:
                    raise BuildFailure("command_failed")
                value = ""
                if capture:
                    stream.seek(0)
                    data = stream.read(4097)
                    if len(data) > 4096:
                        raise BuildFailure("metadata_budget")
                    value = data.decode("utf-8").strip()
            except BaseException as original:
                try:
                    self.stop(process)
                except BaseException as cleanup:
                    raise BaseExceptionGroup("Source command and owned-session stop failed", [original, cleanup])
                raise
            self.stop(process)
            return value


def fresh_root(path):
    source = HERE.parents[1]
    if not path.is_absolute() or path.exists() or path.is_symlink():
        raise BuildFailure("fresh_work_required")
    root = path.parent.resolve(strict=True) / path.name
    if root.is_relative_to(source) or source.is_relative_to(root):
        raise BuildFailure("work_outside_source_required")
    root.mkdir(mode=0o700)
    return root


def isolated_environment(root):
    for name in ("tmp", "gnupg", "repo-config", "xdg/git", "cache", "pycache"):
        (root / name).mkdir(mode=0o700, parents=True)
    config = root / "xdg/git/config"
    config.write_text("[user]\n\tname = AI Phone Source Builder\n\temail = source-build@localhost\n[core]\n\thooksPath = /dev/null\n[credential]\n\thelper =\n", encoding="utf-8")
    environment = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "C.UTF-8",
            "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": str(config), "GIT_TERMINAL_PROMPT": "0",
            "GIT_ASKPASS": "/bin/false", "SSH_ASKPASS": "/bin/false", "GIT_SSH_COMMAND": "/bin/false",
            "GNUPGHOME": str(root / "gnupg"), "REPO_CONFIG_DIR": str(root / "repo-config"),
            "XDG_CONFIG_HOME": str(root / "xdg"), "XDG_CACHE_HOME": str(root / "cache"),
            "TMPDIR": str(root / "tmp"), "PYTHONNOUSERSITE": "1", "PYTHONPYCACHEPREFIX": str(root / "pycache"),
            "CMAKE_BUILD_PARALLEL_LEVEL": "2"}
    # Keep existing identity and the CI process-cleanup owner; do not repurpose either.
    for name in ("HOME", "RUNNER_TRACKING_ID"):
        if name in os.environ:
            environment[name] = os.environ[name]
    return environment


def resource_snapshot(root):
    rows = Path("/proc/meminfo").read_text(encoding="ascii").splitlines()
    available = next((int(row.split()[1]) * 1024 for row in rows if row.startswith("MemAvailable:")), 0)
    return {"cpu_count": os.cpu_count() or 1, "memory_available_bytes": available,
            "disk_free_bytes": shutil.disk_usage(root).free}


def host_requirements():
    if platform.system() != "Linux" or platform.machine().lower() not in {"x86_64", "amd64"}:
        raise BuildFailure("linux_x64_required")
    if sys.version_info < (3, 11) or sys.maxsize <= 2**32:
        raise BuildFailure("python_311_64bit_required")
    if Path.home().joinpath(".netrc").exists():
        raise BuildFailure("clean_builder_home_required")
    for name in ("git", "gpg"):
        if shutil.which(name) is None:
            raise BuildFailure("required_tool_missing")


def admission(root):
    host_requirements()
    snapshot = resource_snapshot(root)
    if snapshot["disk_free_bytes"] < 8 * GIB or snapshot["memory_available_bytes"] < 2 * GIB:
        raise BuildFailure("attempt_headroom")
    return snapshot


def require_kvm():
    path = Path("/dev/kvm")
    if not stat.S_ISCHR(path.stat().st_mode) or not os.access(path, os.R_OK | os.W_OK):
        raise BuildFailure("kvm_access_required")


def git(command, folder, *arguments, capture=False):
    return command.run(["git", "-c", "core.hooksPath=/dev/null", "-C", str(folder), *arguments], folder, capture=capture)


def reviewed_launcher(path):
    data = path.read_bytes()
    if len(data) > 262144:
        raise BuildFailure("repo_launcher_budget")
    blob = hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data, usedforsecurity=False).hexdigest()
    if blob != REPO_LAUNCHER_BLOB:
        raise BuildFailure("repo_launcher_identity")
    return data


def launcher_keys(path):
    tree = ast.parse(reviewed_launcher(path).decode("utf-8"))
    assignments = []
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        names = [name.id for name in node.targets if isinstance(name, ast.Name)]
        if names == ["MAINTAINER_KEYS"]:
            assignments.append(node.value)
    if len(assignments) != 1 or not isinstance(assignments[0], ast.Constant) or not isinstance(assignments[0].value, str):
        raise BuildFailure("repo_key_declaration")
    return assignments[0].value


def verify_repo_identity(command, folder):
    if git(command, folder, "rev-parse", "refs/tags/" + REPO_TAG, capture=True) != REPO_TAG_OBJECT:
        raise BuildFailure("repo_tag_identity")
    if git(command, folder, "rev-parse", "refs/tags/" + REPO_TAG + "^{commit}", capture=True) != REPO_COMMIT:
        raise BuildFailure("repo_commit_identity")


def pinned_repo_tool(root, command):
    folder = root / "repo-tool"
    folder.mkdir()
    git(command, folder, "init", "--quiet")
    git(command, folder, "remote", "add", "origin", REPO_URL)
    git(command, folder, "fetch", "--no-tags", "--depth=1", "origin", "refs/tags/" + REPO_TAG + ":refs/tags/" + REPO_TAG)
    verify_repo_identity(command, folder)
    git(command, folder, "checkout", "--quiet", "--detach", REPO_COMMIT)
    launcher = folder / "repo"
    keys = root / "repo-maintainer-keys.asc"
    keys.write_text(launcher_keys(launcher), encoding="utf-8")
    command.run(["gpg", "--batch", "--no-autostart", "--import", str(keys)], root)
    git(command, folder, "-c", "gpg.program=gpg", "verify-tag", "--raw", "refs/tags/" + REPO_TAG)
    if git(command, folder, "rev-parse", "HEAD", capture=True) != REPO_COMMIT:
        raise BuildFailure("repo_checkout_identity")
    return launcher


def local_manifest(root, command):
    producer.locked_projects()
    folder = root / "manifest"
    folder.mkdir()
    (folder / "source-lock.xml").write_bytes(HERE.joinpath("source-lock.xml").read_bytes())
    git(command, folder, "init", "--quiet")
    git(command, folder, "add", "--", "source-lock.xml")
    git(command, folder, "commit", "--quiet", "--no-gpg-sign", "-m", "Locked emulator source inputs")
    revision = git(command, folder, "rev-parse", "HEAD", capture=True)
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise BuildFailure("manifest_identity")
    return folder, revision


def sync_sources(root, command, launcher, manifest, revision):
    checkout = root / "checkout"
    checkout.mkdir()
    command.run([sys.executable, str(launcher), "init", "-u", manifest.as_uri(), "-b", revision,
                 "-m", "source-lock.xml", "-g", "default,platform-linux", "--depth=1",
                 "--repo-url", REPO_URL, "--repo-rev", REPO_COMMIT,
                 "--no-clone-bundle", "--no-tags", "--no-use-superproject"], checkout, timeout=1800, manifest_file=True)
    command.run([sys.executable, str(launcher), "sync", "--jobs=2", "--detach", "--no-manifest-update",
                 "--no-clone-bundle", "--no-tags", "--no-use-superproject"], checkout, timeout=10800)
    return checkout


def verified_sources(command, checkout, revision):
    actual = checkout / ".repo/manifests/source-lock.xml"
    if actual.read_bytes() != HERE.joinpath("source-lock.xml").read_bytes():
        raise BuildFailure("synced_manifest_identity")
    if git(command, checkout / ".repo/manifests", "rev-parse", "HEAD", capture=True) != revision:
        raise BuildFailure("manifest_checkout_identity")
    if git(command, checkout / ".repo/repo", "rev-parse", "HEAD", capture=True) != REPO_COMMIT:
        raise BuildFailure("installed_repo_identity")
    if git(command, checkout / ".repo/repo", "status", "--porcelain", "--untracked-files=all", capture=True):
        raise BuildFailure("installed_repo_dirty")
    launcher_keys(checkout / ".repo/repo/repo")
    producer.recipe_inputs(checkout)


def built_receipt(root):
    value = json.loads((root / "build/build-result.json").read_text(encoding="utf-8"))
    if value.get("build_status") != "built" or value.get("source_lock_sha256") != producer.LOCK_SHA256:
        raise BuildFailure("producer_receipt")
    if value.get("build_id") != producer.BUILD_ID or value.get("target") != "linux_x86_64":
        raise BuildFailure("producer_receipt")
    artifact = value.get("artifact", {})
    if type(artifact.get("bytes")) is not int or artifact["bytes"] <= 0:
        raise BuildFailure("producer_receipt")
    if not isinstance(artifact.get("sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", artifact["sha256"]):
        raise BuildFailure("producer_receipt")
    return {"artifact_bytes": artifact["bytes"], "artifact_sha256": artifact["sha256"]}


def publish_receipt(root, value):
    temporary = root / "public-receipt.next"
    with temporary.open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    temporary.replace(root / "public-receipt.json")


def enter_stage(root, receipt, stage):
    receipt["stage"] = stage
    publish_receipt(root, receipt)


def controlled_build(root):
    environment = isolated_environment(root)
    receipt = {"format": 1, "host": "linux-x64", "stage": "admission", "status": "unknown",
               "source_lock_sha256": producer.LOCK_SHA256, "qemu_commit": producer.QEMU_REVISION,
               "repo_commit": REPO_COMMIT, "declared_projects": 75, "selected_projects": 66,
               "redistribution": "review-pending", "source_dependency_closure": "unverified",
               "android35_root_boot": "not-executed", "resources": {}}
    publish_receipt(root, receipt)
    try:
        receipt["resources"]["admission"] = admission(root)
        require_kvm()
        with (root / "private-build.log").open("xb") as log:
            command = SourceCommand(root, environment, log)
            enter_stage(root, receipt, "repo_tool")
            launcher = pinned_repo_tool(root, command)
            enter_stage(root, receipt, "manifest")
            manifest, revision = local_manifest(root, command)
            enter_stage(root, receipt, "source_sync")
            checkout = sync_sources(root, command, launcher, manifest, revision)
            receipt["resources"]["after_sync"] = resource_snapshot(root)
            enter_stage(root, receipt, "source_proof")
            verified_sources(command, checkout, revision)
            command.run([sys.executable, str(HERE / "aosp_emulator_producer.py"), "--checkout", str(checkout),
                         "--work", str(root / "build")], root)
            enter_stage(root, receipt, "source_build")
            command.run([sys.executable, str(HERE / "aosp_emulator_producer.py"), "--checkout", str(checkout),
                         "--work", str(root / "build"), "--build"], root, timeout=10800)
            receipt.update(built_receipt(root))
        receipt["resources"]["after_build"] = resource_snapshot(root)
        receipt.update(stage="complete", status="built")
    except BuildFailure as error:
        receipt.update(status="failed", reason=str(error))
        raise
    except BuildInterrupted as error:
        reason = str(error) if str(error) in {"disk_floor", "command_timeout"} else "interrupted_or_unknown"
        receipt.update(status="unknown", reason=reason)
        receipt["resources"]["interrupted"] = resource_snapshot(root)
        raise
    except BaseException:
        receipt.update(status="unknown", reason="interrupted_or_unknown")
        raise
    finally:
        publish_receipt(root, receipt)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work", required=True, type=Path)
    args = parser.parse_args()
    controlled_build(fresh_root(args.work))


if __name__ == "__main__":
    try:
        main()
    except SystemExit:
        raise
    except BaseException:
        print("Controlled source build did not complete; its receipt is retained.", file=sys.stderr)
        sys.exit(1)
