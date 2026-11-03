"""Retire only disposable native compiler intermediates after payload sealing."""
import json
import os
from pathlib import Path
import shutil
import tarfile
from artifact_io import digest
from build_native_services import read_build_receipt,cached_artifact
from client_host_entry import archive_records
from prebuilt_services import binary_path
from wsl_runtime_fixture import require_disposable

PROFILES=('release','debug')
CACHES=('deps','build','.fingerprint','incremental')


def cache_directories(root):
    profiles=[root/'target'/name for name in PROFILES]
    caches=[profile/name for profile in profiles for name in CACHES]
    if any(path.is_symlink() or path.exists() and not path.is_dir() for path in [root/'target',*profiles,*caches]):
        raise ValueError('Compiler cache must belong to this disposable checkout')
    return [path for path in caches if path.is_dir()]


def sealed_native_artifacts(root,payload):
    value=read_build_receipt(root/'target/release/native-service-build.json')
    if value is None or not all(cached_artifact(root,role,row) for role,row in value['artifacts'].items()):
        raise ValueError('Native service artifacts differ before cache retirement')
    expected=json.loads(payload.with_name('integrity.json').read_text()).get('source.payload')
    if digest(payload)!=expected:raise ValueError('Sealed payload changed before cache retirement')
    with tarfile.open(payload,'r:*') as archive:
        _,records,_=archive_records(archive)
    for role,row in value['artifacts'].items():
        stored=records.get(binary_path(role),{})
        if stored.get('sha256')!=row['sha256'] or stored.get('size')!=row['size']:
            raise ValueError('Sealed source does not bind the native service artifacts')
    return value['artifacts']


def retire_native_cache(root,payload):
    require_disposable();root=Path(root).resolve();payload=Path(payload)
    workspace=os.environ.get('GITHUB_WORKSPACE')
    if not workspace or Path(workspace).resolve()!=root:
        raise ValueError('Only this disposable GitHub workspace may retire its build cache')
    directories=cache_directories(root)
    artifacts=sealed_native_artifacts(root,payload)
    before=shutil.disk_usage(root).free
    retired=[]
    for directory in directories:
        shutil.rmtree(directory);retired.append(directory.relative_to(root).as_posix())
    if not all(cached_artifact(root,role,row) for role,row in artifacts.items()):
        raise ValueError('Native programs changed during cache retirement')
    return {'scope':'disposable-native-build-intermediates','retired':retired,
            'free_before':before,'free_after':shutil.disk_usage(root).free,
            'native_programs_preserved':True,'source_payload_preserved':True,
            'docker_images_removed':False,'capacity_guard_changed':False}
