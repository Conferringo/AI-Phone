"""Source-bound, unpublished image handoff between native maintainer build hosts."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

from artifact_io import digest
from client_delivery import ASSET_LIMIT
import container_images
import package_delivery as delivery

ROOT = Path(__file__).resolve().parent.parent
BODY = 'factory-images-arm64.payload'
METADATA = 'factory-images-arm64.json'
KIND = 'factory-container-images'


def read_json(path):
    if path.is_symlink() or not path.is_file() or not 0 < path.stat().st_size <= 1024**2:
        raise ValueError('Invalid factory image metadata')
    value = json.loads(path.read_bytes())
    if not isinstance(value, dict):
        raise ValueError('Invalid factory image metadata')
    return value


def header(root, source):
    if re.fullmatch('[0-9a-f]{40}', source) is None:
        raise ValueError('Invalid factory source identity')
    return {'format': 1, 'kind': KIND, 'source_commit': source,
            'source_manifest_sha256': digest(Path(root)/'SOURCE-MANIFEST.json'),
            'architecture': 'arm64', 'redistribution': 'review-pending',
            'binary_publication': False, 'user_state_bundled': False}


def checked_artifact(payload, record):
    if not isinstance(record, dict) or set(record) != {'file', 'size', 'sha256'}:
        raise ValueError('Invalid factory image artifact record')
    if (record['file'], type(record['size'])) != (BODY, int) or not 0 < record['size'] < ASSET_LIMIT:
        raise ValueError('Invalid factory image artifact size or name')
    if payload.is_symlink() or not payload.is_file():
        raise ValueError('Factory image payload is missing or aliased')
    if (payload.stat().st_size, digest(payload)) != (record['size'], record['sha256']):
        raise ValueError('Factory image payload changed')


def checked_component(root, folder, identity, source):
    folder = Path(folder)
    metadata, payload = folder/METADATA, folder/BODY
    value = read_json(metadata)
    expected = header(root, source)
    if (digest(metadata) != identity or set(value) != {*expected, 'artifact'}
            or type(value.get('format')) is not int
            or any(type(value.get(key)) is not type(item) or value[key] != item for key, item in expected.items())):
        raise ValueError('Factory image source or metadata identity differs')
    checked_artifact(payload, value['artifact'])
    return payload, value


def admit(root, folder, identity, source):
    payload, _value = checked_component(root, folder, identity, source)
    prepared = container_images.stage(payload)
    if prepared.manifest['architecture'] != 'arm64':
        raise ValueError('Mac factory images must be ARM64')
    return prepared


def stage(root, payload, source):
    root, payload = Path(root), Path(payload)
    value = header(root, source)
    prepared = container_images.stage(payload)
    if prepared.manifest['architecture'] != 'arm64' or not 0 < payload.stat().st_size < ASSET_LIMIT:
        raise ValueError('Unsupported native factory image handoff')
    dist = root/'dist'
    dist.mkdir(exist_ok=True)
    folder = Path(tempfile.mkdtemp(prefix='factory-images-', dir=dist))
    try:
        target = folder/BODY
        shutil.copyfile(payload, target)
        if digest(target) != prepared.identity:
            raise ValueError('Factory images changed during staging')
        value['artifact'] = {'file': BODY, 'size': target.stat().st_size, 'sha256': prepared.identity}
        metadata = folder/METADATA
        metadata.write_text(json.dumps(value, indent=2)+'\n')
        return metadata
    except BaseException:
        shutil.rmtree(folder)
        raise


def upload_plan(root, report, source):
    report = Path(report)
    admit(root, report.parent, digest(report), source)
    files = [delivery.file_record(root, path) for path in [report, report.parent/BODY]]
    return {'format': 1, 'kind': KIND, 'source_commit': source,
            'host': 'linux-arm64', 'binary_publication': False, 'files': files}


def asset_ready(row):
    return (row.get('state') == 'uploaded' and type(row.get('size')) is int
            and 0 < row['size'] < ASSET_LIMIT
            and re.fullmatch(r'sha256:[0-9a-f]{64}', row.get('digest', '')) is not None)


def checked_assets(rows, identity):
    assets = {row['name']: row for row in rows}
    if (len(rows) != 2 or set(assets) != {BODY, METADATA}
            or any(not asset_ready(row) for row in rows)
            or assets[METADATA]['size'] > 1024**2 or assets[METADATA]['digest'] != 'sha256:'+identity):
        raise ValueError('Factory handoff asset inventory or metadata digest differs')
    return assets


def checked_remote(value, tag, identity, source):
    pattern = 'candidate-'+re.escape(source[:12])+r'-[0-9]+-[0-9a-f]{32}'
    if (re.fullmatch('[0-9a-f]{40}', source) is None or re.fullmatch(pattern, tag) is None
            or value.get('draft') is not True or value.get('published_at') is not None
            or value.get('target_commitish') != source or value.get('tag_name') != tag):
        raise ValueError('Factory handoff is not an owned unpublished source-matched draft')
    return checked_assets(value.get('assets', []), identity)


def github(*arguments):
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    if re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository) is None:
        raise ValueError('Explicit GitHub factory repository required')
    command = ['gh', *arguments]
    if arguments[0] == 'api':
        command[2] = 'repos/'+repository+'/'+arguments[1]
    else:
        command += ['--repo', repository]
    return subprocess.check_output(command, text=True)


def fetch(root, tag, identity, source, folder):
    remote = json.loads(github('api', 'releases/tags/'+tag))
    assets = checked_remote(remote, tag, identity, source)
    folder = Path(folder)
    folder.mkdir(parents=True)
    github('release', 'download', tag, '--dir', str(folder), '--pattern', BODY, '--pattern', METADATA)
    _payload, value = checked_component(root, folder, identity, source)
    if not delivery.matching_asset(value['artifact'], assets[BODY]):
        raise ValueError('Downloaded factory image differs from its remote asset')
    return folder


def discard(tag, identity, source):
    remote = json.loads(github('api', 'releases/tags/'+tag))
    checked_remote(remote, tag, identity, source)
    github('release', 'delete', tag, '--cleanup-tag', '--yes')


def build(root, work, source):
    from host_platform import host_key
    from linux_client_factory import check_docker, prepare_images
    from check_source import check
    root, work = Path(root).resolve(), Path(work)
    if (host_key() != 'linux-arm64' or os.environ.get('GITHUB_ACTIONS') != 'true'
            or os.environ.get('RUNNER_ENVIRONMENT') != 'github-hosted'
            or Path(os.environ.get('GITHUB_WORKSPACE', '')).resolve() != root):
        raise ValueError('Native images require a disposable ARM64 Linux factory')
    if shutil.disk_usage(root).free < 35*1024**3:
        raise ValueError('Keep 35 GiB free before native image assembly')
    check(root, repository=True, manifest=True)
    check_docker()
    work.mkdir(parents=True)
    identity = digest(root/'SOURCE-MANIFEST.json')
    payload, _result = prepare_images(root, work, identity, architecture='arm64')
    if digest(root/'SOURCE-MANIFEST.json') != identity:
        raise ValueError('Source inventory changed during image assembly')
    return stage(root, payload, source)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['build', 'plan', 'fetch', 'discard'])
    parser.add_argument('input')
    parser.add_argument('--identity')
    parser.add_argument('--work', type=Path)
    args = parser.parse_args()
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if args.operation == 'build':
        path = build(ROOT, Path(args.input), source)
        result = {'report': path.relative_to(ROOT).as_posix(), 'metadata_sha256': digest(path)}
    elif args.operation == 'plan':
        result = upload_plan(ROOT, Path(args.input), source)
    elif args.operation == 'fetch':
        if args.work is None or args.identity is None: parser.error('--work and --identity required')
        result = {'directory': str(fetch(ROOT, args.input, args.identity, source, args.work))}
    else:
        if args.identity is None: parser.error('--identity required')
        discard(args.input, args.identity, source)
        result = {'discarded': True}
    print(json.dumps(result))
