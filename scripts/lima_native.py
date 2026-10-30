"""Build Lima from its pinned source with the client's real macOS link target."""
import argparse
import hashlib
import json
import os
import re
from pathlib import Path
import signal
import subprocess
import tempfile
import time
import shutil
from artifact_io import digest
from client_host_entry import checked_record, verify_file
from host_platform import host_key
from macos_target import minimum_version, verify_targets, version_parts

ROOT = Path(__file__).resolve().parent.parent


def build_policy(root):
    value = json.loads((Path(root) / 'config/container-host-runtime.json').read_text())
    if host_key() != 'macos-arm64':
        raise ValueError('Native Lima factory currently targets macOS ARM64')
    profile = value['hosts']['macos-arm64']
    return {**profile['lima_build'], 'version': profile['lima'], 'minimum': minimum_version(root)}


def fingerprint(root):
    value = {'policy': build_policy(root), 'producer': digest(Path(__file__))}
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def cached_build(root):
    folder = Path(root) / '.tools/lima-native'
    try:
        path = folder / 'build.json'
        if path.is_symlink() or path.stat().st_size > 65536:
            return None
        value = json.loads(path.read_text())
        if not valid_receipt(root, value):
            return None
        row = checked_record(value['file'])
        if row['path'] != 'limactl' or row['mode'] != 0o755:
            return None
        verify_file(folder, row)
        if (folder / 'limactl').stat().st_mode & 0o777 != row['mode']:
            return None
        return value
    except (OSError, ValueError, TypeError, KeyError):
        return None


def valid_receipt(root, value):
    if not isinstance(value, dict) or type(value.get('format')) is not int or value['format'] != 1:
        return False
    policy = build_policy(root)
    if value.get('fingerprint') != fingerprint(root) or value.get('source_commit') != policy['source_commit']:
        return False
    if value.get('go_version') != policy['go_version']:
        return False
    target=value.get('minimum_macos')
    return isinstance(target,str) and re.fullmatch(r'\d+\.\d+(?:\.\d+)?',target) is not None and version_parts(target) <= version_parts(policy['minimum'])


def build_environment(root):
    policy = build_policy(root)
    cache = Path(root) / '.tools/container-host-factory'
    target = policy['minimum']
    return {**os.environ, 'GOOS': 'darwin', 'GOARCH': 'arm64', 'CGO_ENABLED': '1',
            'GOMAXPROCS': '2', 'GOTOOLCHAIN': 'local', 'GOWORK': 'off', 'GOENV': 'off',
            'GOCACHE': str(cache / 'go-cache'), 'GOMODCACHE': str(cache / 'go-modules'),
            'GOTMPDIR': str(cache / 'go-tmp'), 'GOSUMDB': 'sum.golang.org',
            'GOPROXY': 'https://proxy.golang.org', 'GOPRIVATE': '', 'GONOSUMDB': '', 'GONOPROXY': '', 'GOINSECURE': '', 'GOFLAGS': '',
            'MACOSX_DEPLOYMENT_TARGET': target, 'CGO_CFLAGS': '-O2 -g0 -mmacosx-version-min=' + target,
            'CGO_LDFLAGS': '-mmacosx-version-min=' + target}


def compile_command(root, go, output):
    policy = build_policy(root)
    flags = '-s -w -X github.com/lima-vm/lima/v2/pkg/version.Version=' + policy['version']
    flags += ' -linkmode=external -extldflags=-mmacosx-version-min=' + policy['minimum']
    return [str(go), 'build', '-mod=readonly', '-trimpath', '-p', '2', '-ldflags', flags,
            '-o', str(output), './cmd/limactl']


def source_identity(source, expected):
    source = Path(source)
    commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip()
    if commit != expected:
        raise ValueError('Lima source differs from the pinned commit')
    for args in (['status', '--porcelain'], ['ls-files', '--others', '--ignored', '--exclude-standard']):
        if subprocess.check_output(['git', *args], cwd=source, text=True).strip():
            raise ValueError('Lima source contains unreviewed changes')
    return commit


def stop_compiler(child):
    if child.poll() is not None:
        return
    signal_compiler(child, signal.SIGTERM)
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        signal_compiler(child, signal.SIGKILL)
        child.wait()


def signal_compiler(child, signum):
    try:
        os.killpg(child.pid, signum)
    except ProcessLookupError:
        pass


def cancelled(signum, frame):
    raise KeyboardInterrupt('Lima factory cancelled')


def compile_lima(root, source, go, output, log):
    env = build_environment(root)
    for name in ('GOCACHE', 'GOMODCACHE', 'GOTMPDIR'):
        Path(env[name]).mkdir(parents=True, exist_ok=True)
    proxy = os.environ.get('AI_PHONE_BOOTSTRAP_PROXY')
    if proxy:
        env['HTTPS_PROXY'] = proxy
    with Path(log).open('w') as sink:
        child = subprocess.Popen(compile_command(root, go, output), cwd=source, env=env,
                                 stdout=sink, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            while child.poll() is None:
                if shutil.disk_usage(root).free < 4 * 1024**3:
                    raise ValueError('Compilation stopped at the 4 GiB recovery reserve')
                time.sleep(.25)
            if child.returncode:
                raise ValueError('Lima compilation failed; inspect the private factory log')
        finally:
            stop_compiler(child)


def build(root, source, go):
    root, source, go = Path(root).resolve(), Path(source).resolve(), Path(go).resolve()
    policy = build_policy(root)
    source_identity(source, policy['source_commit'])
    compiler = subprocess.check_output([str(go), 'version'], text=True).strip()
    if compiler != 'go version ' + policy['go_version'] + ' darwin/arm64':
        raise ValueError('Use the pinned Go compiler for the Lima factory')
    existing = cached_build(root)
    if existing is not None:
        return existing
    output = root / '.tools/lima-native'
    if output.exists():
        raise ValueError('Stale Lima build was preserved; review its replacement')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.lima-native-', dir=output.parent) as temporary:
        work = Path(temporary) / 'runtime'
        work.mkdir()
        binary = work / 'limactl'
        compile_lima(root, source, go, binary, root / '.tools/container-host-factory/lima-native-build.log')
        source_identity(source, policy['source_commit'])
        subprocess.run(['codesign', '-f', '--entitlements', str(source / 'vz.entitlements'), '-s', '-', str(binary)], check=True)
        subprocess.run(['codesign', '--verify', '--strict', str(binary)], check=True)
        target = verify_targets(root, {'lima': binary})['lima']
        version = subprocess.check_output([str(binary), '--version'], text=True).strip()
        if version != 'limactl version ' + policy['version']:
            raise ValueError('Lima version differs after compilation')
        value = {'format': 1, 'fingerprint': fingerprint(root), 'source_commit': policy['source_commit'],
                 'go_version': policy['go_version'], 'minimum_macos': target,
                 'file': {'path': 'limactl', 'size': binary.stat().st_size, 'sha256': digest(binary), 'mode': 0o755}}
        (work / 'build.json').write_text(json.dumps(value, sort_keys=True) + '\n')
        work.rename(output)
    return value


if __name__ == '__main__':
    signal.signal(signal.SIGTERM, cancelled)
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=ROOT)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--go', type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(build(args.root, args.source, args.go)))
