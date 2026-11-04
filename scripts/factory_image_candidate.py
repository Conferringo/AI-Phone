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
    root = Path(root).resolve()
    report = Path(report)
    if not report.is_absolute():
        report = root/report
    admit(root, report.parent, digest(report), source)
    files = [delivery.file_record(root, path) for path in [report, report.parent/BODY]]
    return {'format': 1, 'kind': KIND, 'source_commit': source,
            'host': 'linux-arm64', 'binary_publication': False, 'files': files}


def asset_ready(row):
    return (row.get('state') == 'uploaded' and type(row.get('size')) is int
            and type(row.get('id')) is int and row['id'] > 0
            and 0 < row['size'] < ASSET_LIMIT
            and re.fullmatch(r'sha256:[0-9a-f]{64}', row.get('digest', '')) is not None)


def checked_assets(rows, identity):
    assets = {row['name']: row for row in rows}
    if (len(rows) != 2 or set(assets) != {BODY, METADATA}
            or any(not asset_ready(row) for row in rows)
            or assets[METADATA]['size'] > 1024**2 or assets[METADATA]['digest'] != 'sha256:'+identity):
        raise ValueError('Factory handoff asset inventory or metadata digest differs')
    return assets


def checked_remote(value, tag, identity, source, release_id):
    pattern = 'candidate-'+re.escape(source[:12])+r'-[0-9]+-[0-9a-f]{32}'
    if (type(release_id) is not int or release_id <= 0 or re.fullmatch('[0-9a-f]{40}', source) is None
            or re.fullmatch(pattern, tag) is None):
        raise ValueError('Factory handoff is not an owned unpublished source-matched draft')
    expected = {'id': release_id, 'draft': True, 'published_at': None,
                'target_commitish': source, 'tag_name': tag}
    if any(type(value.get(key)) is not type(item) or value.get(key) != item for key, item in expected.items()):
        raise ValueError('Factory handoff is not an owned unpublished source-matched draft')
    return checked_assets(value.get('assets', []), identity)


def github(*arguments, output=None):
    repository = os.environ.get('GITHUB_REPOSITORY', '')
    if re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository) is None:
        raise ValueError('Explicit GitHub factory repository required')
    command = ['gh', *arguments]
    if arguments[0] == 'api':
        command[2] = 'repos/'+repository+'/'+arguments[1]
    else:
        command += ['--repo', repository]
    if output is None:
        return subprocess.check_output(command, text=True)
    with Path(output).open('xb') as stream:
        subprocess.run(command, stdout=stream, check=True)
    return ''


def fetch(root, tag, identity, source, folder, *, release_id):
    remote = json.loads(github('api', 'releases/'+str(release_id)))
    assets = checked_remote(remote, tag, identity, source, release_id)
    folder = Path(folder)
    folder.mkdir(parents=True)
    for name in [METADATA, BODY]:
        github('api', 'releases/assets/'+str(assets[name]['id']), '-H', 'Accept: application/octet-stream', output=folder/name)
    _payload, value = checked_component(root, folder, identity, source)
    if not delivery.matching_asset(value['artifact'], assets[BODY]):
        raise ValueError('Downloaded factory image differs from its remote asset')
    return folder


def discard(tag, identity, source, *, release_id):
    remote = json.loads(github('api', 'releases/'+str(release_id)))
    checked_remote(remote, tag, identity, source, release_id)
    refs = json.loads(github('api', 'git/matching-refs/tags/'+tag))
    exact = [row for row in refs if row.get('ref') == 'refs/tags/'+tag]
    if len(exact) > 1 or any(row.get('object',{}).get('type') != 'commit' or row.get('object',{}).get('sha') != source for row in exact):
        raise ValueError('Factory candidate tag does not name its owned source commit')
    if exact:
        github('api', 'git/refs/tags/'+tag, '--method', 'DELETE')
    github('api', 'releases/'+str(release_id), '--method', 'DELETE')


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


def parse_arguments(arguments):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['build', 'plan', 'fetch', 'discard'])
    parser.add_argument('input')
    parser.add_argument('--identity')
    parser.add_argument('--release-id', type=int)
    parser.add_argument('--work', type=Path)
    parser.add_argument('--result', type=Path, help='Write the machine-readable result separately from build logs')
    args = parser.parse_args(arguments)
    if args.operation == 'build' and args.result is None:
        parser.error('build requires --result')
    if args.operation in ('fetch','discard'):
        if args.release_id is None or args.release_id <= 0:
            parser.error('fetch/discard require a positive --release-id from the producer')
        if args.identity is None:
            parser.error('--identity required')
    if args.operation == 'fetch' and args.work is None:
        parser.error('--work required')
    return args


def main(arguments=None):
    args = parse_arguments(arguments)
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if args.operation == 'build':
        path = build(ROOT, Path(args.input), source)
        result = {'report': path.relative_to(ROOT).as_posix(), 'metadata_sha256': digest(path)}
    elif args.operation == 'plan':
        result = upload_plan(ROOT, Path(args.input), source)
    elif args.operation == 'fetch':
        result = {'directory': str(fetch(ROOT, args.input, args.identity, source, args.work, release_id=args.release_id))}
    else:
        discard(args.input, args.identity, source, release_id=args.release_id)
        result = {'discarded': True}
    if args.result is not None:
        args.result.write_text(json.dumps(result)+'\n')
    else:
        print(json.dumps(result))


if __name__ == '__main__':
    main()
