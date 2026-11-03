"""Retain source-emulator build output for private review, never public approval."""
import argparse
import errno
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

import package_delivery as delivery
from artifact_io import digest

ROOT = Path(__file__).resolve().parent.parent
sys.path.append(str(ROOT / 'scripts/emulator-source-build'))
import aosp_emulator_producer as producer

KIND = 'source-emulator-review'
HOST = 'linux-x64'
TARGET = 'linux_x86_64'


def read_json(path):
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 1024**2:
        raise ValueError('Invalid source candidate metadata')
    value = json.loads(path.read_bytes())
    if not isinstance(value, dict):
        raise ValueError('Invalid source candidate metadata')
    return value


def checked_build(root, work, source):
    receipt = read_json(work / 'public-receipt.json')
    expected = {'format': 1, 'host': HOST, 'stage': 'complete', 'status': 'built',
                'source_commit': source, 'source_manifest_sha256': digest(root / 'SOURCE-MANIFEST.json'),
                'source_lock_sha256': producer.LOCK_SHA256, 'selected_projects': 66}
    if any(receipt.get(key) != value for key, value in expected.items()):
        raise ValueError('Source build receipt differs from reviewed source')
    result = read_json(work / 'build/build-result.json')
    identity = {'producer': 'aosp-source-main-emulator', 'host': HOST, 'target': TARGET,
                'build_status': 'built', 'build_id': producer.BUILD_ID,
                'source_lock_sha256': producer.LOCK_SHA256}
    if any(result.get(key) != value for key, value in identity.items()):
        raise ValueError('Source producer identity differs')
    artifact = result['artifact']
    name = 'sdk-repo-linux-emulator-' + producer.BUILD_ID + '.zip'
    if artifact.get('path') != name:
        raise ValueError('Unexpected source-emulator filename')
    path = work / 'build/dist' / name
    if path.is_symlink() or not path.is_file() or path.stat().st_size != artifact['bytes']:
        raise ValueError('Source-emulator file differs from the built result')
    if (receipt.get('artifact_bytes'), receipt.get('artifact_sha256')) != (artifact['bytes'], artifact['sha256']):
        raise ValueError('Source build receipts disagree')
    if not zipfile.is_zipfile(path):
        raise ValueError('Source-emulator archive is not a ZIP')
    return path, {'file': name, 'size': artifact['bytes'], 'sha256': artifact['sha256']}


def link_or_copy(source, destination):
    try:
        os.link(source, destination)
    except OSError as error:
        if error.errno != errno.EXDEV:
            raise
        shutil.copyfile(source, destination)


def stage(root, work, source):
    root, work = Path(root), Path(work)
    original, artifact = checked_build(root, work, source)
    dist = root / 'dist'
    dist.mkdir(exist_ok=True)
    folder = Path(tempfile.mkdtemp(prefix='source-emulator-' + source[:12] + '-', dir=dist))
    try:
        archive = folder / artifact['file']
        link_or_copy(original, archive)
        volumes = folder / (archive.name + '.volumes')
        sealed = delivery.native(root, 'split', archive, volumes, artifact['sha256'])
        manifest = volumes / sealed['manifest']
        package = {'format': 'volumes', 'manifest': manifest.relative_to(dist).as_posix(),
                   'manifest_sha256': sealed['manifest_sha256']}
        delivery.volume_records(root, package, artifact)
        archive.unlink()
        metadata = {'format': 1, 'kind': KIND, 'source_commit': source,
                    'source_manifest_sha256': digest(root / 'SOURCE-MANIFEST.json'),
                    'host': HOST, 'build_id': producer.BUILD_ID, 'source_lock_sha256': producer.LOCK_SHA256,
                    'binary_publication': False, 'redistribution': 'review-pending',
                    'source_dependency_closure': 'unverified', 'android35_root_boot': 'not-executed',
                    'user_state_bundled': False, 'artifact': artifact, 'package_delivery': package}
        path = folder / 'source-emulator-review.json'
        path.write_text(json.dumps(metadata, indent=2) + '\n')
        return path
    except BaseException as original_error:
        try:
            shutil.rmtree(folder)
        except BaseException as cleanup:
            raise BaseExceptionGroup('Source candidate and owned cleanup failed', [original_error, cleanup])
        raise


def upload_plan(root, path, source):
    root, path = Path(root), Path(path)
    record = delivery.file_record(root, path)
    value = read_json(path)
    expected = {'format': 1, 'kind': KIND, 'source_commit': source, 'host': HOST,
                'source_manifest_sha256': digest(root / 'SOURCE-MANIFEST.json'),
                'build_id': producer.BUILD_ID, 'source_lock_sha256': producer.LOCK_SHA256,
                'binary_publication': False, 'user_state_bundled': False,
                'redistribution': 'review-pending', 'source_dependency_closure': 'unverified',
                'android35_root_boot': 'not-executed'}
    if any(value.get(key) != expected_value for key, expected_value in expected.items()):
        raise ValueError('Source review candidate identity differs')
    if value.get('binary_publication') is not False or value.get('user_state_bundled') is not False:
        raise ValueError('Source review cannot approve publication or user state')
    files = delivery.volume_records(root, value['package_delivery'], value['artifact']) + [record]
    if len({row['name'] for row in files}) != len(files):
        raise ValueError('Duplicate source candidate asset name')
    return {'format': 1, 'kind': KIND, 'source_commit': source, 'host': HOST,
            'binary_publication': False, 'files': files}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation', choices=['stage', 'plan'])
    parser.add_argument('input', type=Path)
    args = parser.parse_args()
    source = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    if args.operation == 'stage':
        result = {'report': stage(ROOT, args.input, source).relative_to(ROOT).as_posix()}
    else:
        result = upload_plan(ROOT, args.input, source)
    print(json.dumps(result))
