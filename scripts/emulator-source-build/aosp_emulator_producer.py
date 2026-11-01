"""Maintainer-only source-emulator producer; not a redistribution approval or installer."""
import argparse
import ast
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import subprocess
import sys
import xml.etree.ElementTree as ET

LOCK_SHA256 = "be4339c025820be0bdf15e6d21eb9a0c83e23c8d8b8e520bc3de5bd6d8b74373"
QEMU_REVISION = "d1bfd6eb0ea9f697dc4cc1d258887c8b4220b155"
BUILD_ID = "aiphone-source-d1bfd6eb0ea9-r1"
RECIPE_BLOBS = {
    "android/build/python/cmake.py": "63e691c78f5056b27a0d986de59ceda57db3a5f9",
    "android/build/python/aemu/cmake.py": "b3272257e32868c2f76846d134af2a3a2326b281",
    "android/build/python/aemu/platform/toolchains.py": "3a1039312e709819e4b27029fec3ef9c1c045f10",
    "android/build/python/aemu/tasks/configure.py": "aa0217962b88916a858e9eda35bceacc4cdf20d6",
    "android/build/python/aemu/tasks/compile.py": "c283cd475f0b50145faca89520265358eb107468",
    "android/build/python/aemu/tasks/distribution.py": "ab4db3986f5f83a71330e091f93a5fd938567ea3",
    "android/build/python/aemu/process/environment.py": "c915bb702089c8d02738c1663fd5e02f709d2c92",
    "android/build/toolchains.json": "93be9b4e3c562f0791611ed3952200fe3ccd2be4",
    "source.properties": "387fc735929597d06c517a241dce5967781c8b45",
    "android/build/cmake/config/emu-qt5-config.cmake": "89006df7b90d1a0438a8be5e0710c9932c601ea6",
    "android/build/cmake/toolchain.cmake": "1fa863f9f813d1750039cc1952df8d02ec735b07",
    "android/build/cmake/toolchain-windows_msvc-x86_64.cmake": "56a53fabbd3da178e89887bea93ad5846853e52b",
    "android/build/cmake/toolchain-darwin-aarch64.cmake": "2ee5ecbd5f3654750ca372e41692a21e28e78396",
    "android/build/cmake/toolchain-linux-x86_64.cmake": "e8491c7ff1c8d74621db21f07be9b7ea57e6ab98",
}
SOURCE_CONTRACT = {"emulator_revision": "35.3.8", "clang": "clang-r487747c", "rust": "1.73.0", "qt": "6.5.3"}
HOSTS = {
    ("Darwin", "arm64"): ("macos-arm64", "darwin_aarch64", "darwin"),
    ("Darwin", "aarch64"): ("macos-arm64", "darwin_aarch64", "darwin"),
    ("Linux", "x86_64"): ("linux-x64", "linux_x86_64", "linux"),
    ("Windows", "amd64"): ("windows-x64", "windows_x86_64", "windows"),
    ("Windows", "x86_64"): ("windows-x64", "windows_x86_64", "windows"),
}


def sha256(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def git_read(folder, *arguments):
    result = subprocess.run(
        ["git", "--no-optional-locks", "-c", "core.fsmonitor=false", "-C", str(folder), *arguments],
        stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=30, check=False,
    )
    if result.returncode:
        raise ValueError("Cannot inspect pinned source checkout")
    return result.stdout.strip()


def locked_projects():
    lock = Path(__file__).with_name("source-lock.xml")
    if sha256(lock) != LOCK_SHA256:
        raise ValueError("Source-lock bytes differ from this producer")
    tree = ET.fromstring(lock.read_bytes())
    if any(node.tag in {"include", "extend-project", "remove-project", "superproject", "repo-hooks"} for node in tree):
        raise ValueError("Implicit source manifest input is not permitted")
    remotes = tree.findall("remote")
    if len(remotes) != 1 or remotes[0].get("fetch") != "https://android.googlesource.com/":
        raise ValueError("Unexpected source remote")
    projects = tree.findall("project")
    if len(projects) != 75:
        raise ValueError("Incomplete source-lock project inventory")
    return projects, lock


def locked_project(node, seen):
    name, revision = node.get("path", ""), node.get("revision", "")
    relative = PurePosixPath(name)
    if not name or relative.is_absolute() or ".." in relative.parts or name in seen or not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Invalid pinned project")
    seen.add(name)
    return {"path": name, "name": node.get("name"), "revision": revision}


def verify_project(checkout, record):
    name, revision = record["path"], record["revision"]
    folder = checkout.joinpath(*PurePosixPath(name).parts)
    if not folder.is_dir() or folder.resolve().is_relative_to(checkout) is False:
        raise ValueError("Missing or aliased source project")
    if git_read(folder, "rev-parse", "HEAD") != revision:
        raise ValueError("Source project revision mismatch: " + name)
    if git_read(folder, "status", "--porcelain", "--untracked-files=all"):
        raise ValueError("Source checkout is not clean: " + name)


def project_inputs(checkout, group):
    projects, lock = locked_projects()
    records = []
    seen = set()
    for node in projects:
        record = locked_project(node, seen)
        groups = set(node.get("groups", "").split(","))
        if "notdefault" in groups and "platform-" + group not in groups:
            continue
        verify_project(checkout, record)
        records.append(record)
    qemu = next(record for record in records if record["path"] == "external/qemu")
    if qemu["revision"] != QEMU_REVISION:
        raise ValueError("QEMU source revision mismatch")
    return records, lock


def recipe_inputs(checkout):
    records = []
    qemu = checkout / "external/qemu"
    for name, expected in RECIPE_BLOBS.items():
        path = qemu / name
        if path.is_symlink() or not path.is_file():
            raise ValueError("Missing or aliased reviewed recipe: " + name)
        with path.open("rb") as source:
            data = source.read(262145)
        if len(data) > 262144:
            raise ValueError("Reviewed recipe exceeds the small-file budget: " + name)
        blob = hashlib.sha1(b"blob " + str(len(data)).encode("ascii") + b"\0" + data, usedforsecurity=False).hexdigest()
        if blob != expected:
            raise ValueError("Pinned recipe differs from its reviewed official blob: " + name)
        records.append({"path": str(path.relative_to(checkout)), "git_blob_sha1": blob,
                        "sha256": hashlib.sha256(data).hexdigest()})
    return records


def declared_flags(owner):
    syntax = ast.parse(owner.read_text(encoding="utf-8"))
    return {argument.value for call in ast.walk(syntax) if isinstance(call, ast.Call)
             and isinstance(call.func, ast.Attribute) and call.func.attr == "add_argument"
             for argument in call.args if isinstance(argument, ast.Constant) and isinstance(argument.value, str)}


def toolchain_tables(toolchains):
    syntax = ast.parse(toolchains.read_text(encoding="utf-8"))
    owner = next((node for node in syntax.body if isinstance(node, ast.ClassDef) and node.name == "Toolchain"), None)
    if owner is None:
        raise ValueError("Pinned checkout has no reviewed Toolchain owner")
    tables = {}
    for node in owner.body:
        if isinstance(node, ast.Assign):
            for name in node.targets:
                if isinstance(name, ast.Name) and name.id in {"AVAILABLE", "DISTRIBUTION"}:
                    tables[name.id] = ast.literal_eval(node.value)
    return tables


def checked_distribution(toolchains, target):
    tables = toolchain_tables(toolchains)
    normalized = target.replace("_", "-", 1)
    if normalized not in tables.get("AVAILABLE", {}):
        raise ValueError("Pinned checkout does not declare this native target")
    distribution = tables.get("DISTRIBUTION", {}).get(normalized)
    if not isinstance(distribution, str) or not re.fullmatch(r"[a-z0-9_]+", distribution):
        raise ValueError("Pinned checkout does not declare this distribution")
    return distribution


def checked_entry(checkout, target):
    records = recipe_inputs(checkout)
    qemu = checkout / "external/qemu"
    owner = qemu / "android/build/python/aemu/cmake.py"
    required = {"--aosp", "--target", "--out", "--dist", "--sdk_build_number", "--crash", "--config", "--ccache", "--qtwebengine", "--verbose"}
    if not required <= declared_flags(owner):
        raise ValueError("Pinned checkout has an unreviewed build CLI")
    distribution = checked_distribution(qemu / "android/build/python/aemu/platform/toolchains.py", target)
    return qemu / "android/build/python/cmake.py", records, distribution


def write_once(path, value):
    with path.open("x", encoding="utf-8") as output:
        json.dump(value, output, indent=2)
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())


def checked_build_request(args):
    host = HOSTS.get((platform.system(), platform.machine().lower()))
    if host is None or sys.maxsize <= 2**32:
        raise ValueError("Use a native Mac ARM64, Linux x64 or Windows x64 build host")
    if not args.checkout.is_absolute() or not args.work.is_absolute():
        raise ValueError("Explicit absolute paths are required")
    checkout = args.checkout.resolve(strict=True)
    work = args.work.resolve()
    if work.exists() or work.is_relative_to(checkout) or checkout.is_relative_to(work) or not (checkout / ".repo").is_dir():
        raise ValueError("Use a fresh work directory outside the pinned repo checkout")
    return host, checkout, work


def build_once(checkout, work, lock, command, receipt, group, distribution):
    work.mkdir()
    (work / "source-lock.xml").write_bytes(lock.read_bytes())
    write_once(work / "build-intent.json", {**receipt, "build_status": "started"})
    environment = dict(os.environ)
    environment.pop("PYTHONPATH", None)
    environment["PYTHONNOUSERSITE"] = "1"
    environment["PYTHONPYCACHEPREFIX"] = str(work / "pycache")
    environment["ANDROID_USER_HOME"] = str(work / "android-user")
    environment["ANDROID_AVD_HOME"] = str(work / "avd")
    environment["ANDROID_EMULATOR_HOME"] = str(work / "emulator-home")
    result = {**receipt, "build_status": "unknown"}
    try:
        with (work / "build.log").open("xb") as log:
            completed = subprocess.run(command, cwd=checkout / "external/qemu", env=environment,
                stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT,
                check=False)
        if completed.returncode:
            result.update(build_status="failed", returncode=completed.returncode)
            raise ValueError("Upstream build failed; preserve the work directory and inspect its log")
        artifact = work / "dist" / ("sdk-repo-" + distribution + "-emulator-" + BUILD_ID + ".zip")
        if not artifact.is_file() or artifact.is_symlink():
            raise ValueError("Expected upstream distribution was not produced")
        project_inputs(checkout, group)
        result.update(build_status="built", artifact={"path": artifact.name, "bytes": artifact.stat().st_size, "sha256": sha256(artifact)})
    finally:
        write_once(work / "build-result.json", result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--checkout", required=True, type=Path)
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--build", action="store_true")
    args = parser.parse_args()
    host, checkout, work = checked_build_request(args)
    records, lock = project_inputs(checkout, host[2])
    entry, recipe_files, distribution = checked_entry(checkout, host[1])
    command = [sys.executable, str(entry), "--aosp", str(checkout), "--target", host[1],
               "--out", str(work / "objs"), "--dist", str(work / "dist"), "--config", "release",
               "--sdk_build_number", BUILD_ID, "--crash", "none", "--ccache", "none", "--qtwebengine", "--verbose"]
    receipt = {"format": 1, "producer": "aosp-source-main-emulator", "host": host[0], "target": host[1],
               "build_id": BUILD_ID, "source_lock_sha256": LOCK_SHA256, "projects": records,
               "recipe_files": recipe_files, "source_contract": SOURCE_CONTRACT,
               "distribution": distribution, "command": command, "python_version": platform.python_version(),
               "redistribution": "review-pending", "source_dependency_closure": "unverified",
               "android35_root_boot": "not-executed", "factory_layout": "not-verified"}
    if not args.build:
        print(json.dumps({**receipt, "build_status": "planned"}, indent=2))
        return
    result = build_once(checkout, work, lock, command, receipt, host[2], distribution)
    print(json.dumps({"build_status": result["build_status"], "artifact": result["artifact"], "redistribution": "review-pending"}))


if __name__ == "__main__":
    main()
