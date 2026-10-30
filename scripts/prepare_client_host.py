"""Build deterministic client resources; no network, history or user state."""
import gzip
import hashlib
import io
import json
from pathlib import Path
import shutil
import tarfile
from release_package import select_files
ROOT = Path(__file__).resolve().parent.parent

def prepare(root=ROOT, *, with_web_runtime=False, with_service_runtime=False, android_runtime=None, image_runtime=None, installer_tools=None):
    folder = root / 'apps/client/src-tauri/gen/host'
    folder.mkdir(parents=True, exist_ok=True)
    files = select_files(root)
    if with_web_runtime:
        from bootstrap_bun import bun_path
        from prebuilt_web import prepare_web
        files.extend(prepare_web(root, bun_path(root)))
    if with_service_runtime:
        from prebuilt_services import prepare_services
        files.extend(prepare_services(root))
    components = (['web'] if with_web_runtime else []) + (['native-services'] if with_service_runtime else [])
    manifest = {'format': 2 if components else 1, 'components': components, 'files': [dict(path=name, size=len(data), sha256=hashlib.sha256(data).hexdigest(), mode=mode) for name, data, mode in files]}
    files.append(('HOST-MANIFEST.json', json.dumps(manifest, sort_keys=True).encode(), 420))
    archive = folder / 'source.payload'
    with archive.with_suffix('.tmp').open('wb') as raw, gzip.GzipFile(fileobj=raw, mode='wb', mtime=0, filename='') as compressed, tarfile.open(fileobj=compressed, mode='w') as target:
        for name, data, mode in files:
            entry = tarfile.TarInfo(name)
            entry.mode = mode
            entry.size = len(data)
            entry.mtime = 0
            target.addfile(entry, io.BytesIO(data))
    archive.with_suffix('.tmp').replace(archive)
    shutil.copyfile(root / 'scripts/client_host_entry.py', folder / 'entry.py')
    from artifact_io import digest
    digests = {name: digest(folder/name) for name in ['source.payload', 'entry.py']}
    digests.update(prepare_android(folder,android_runtime))
    digests.update(prepare_images(folder,image_runtime))
    from installer_tools import prepare_resources
    digests.update(prepare_resources(folder,installer_tools))
    (folder / 'integrity.json').write_text(json.dumps(digests, sort_keys=True))
    if (root/'apps/client/src-tauri/tauri.conf.json').exists():
        from client_resources import build_config
        build_config(root)
    return digests


def prepare_android(folder, payload):
    from artifact_io import digest
    from android_runtime import manifest_entry
    android=folder/'android.payload'
    if payload is None:
        android.unlink(missing_ok=True)
        return {}
    with tarfile.open(payload,'r|gz') as source:manifest_entry(source,iter(source))
    if Path(payload).resolve()!=android.resolve():shutil.copyfile(payload,android)
    return {'android.payload':digest(android)}

def prepare_images(folder, payload):
    from artifact_io import digest
    from container_images import stage
    target=folder/'images.payload'
    if payload is None:
        target.unlink(missing_ok=True)
        return {}
    stage(payload)
    if Path(payload).resolve()!=target.resolve():shutil.copyfile(payload,target)
    return {'images.payload':digest(target)}

def verify_prepared_web(root=ROOT):
    import tempfile
    from client_host_entry import stage_source
    from check_prebuilt_web import verify
    with tempfile.TemporaryDirectory(prefix='aiphone-bundled-web-') as folder:
        staged = stage_source(root / 'apps/client/src-tauri/gen/host/source.payload', Path(folder) / 'installation')
        return verify(staged)

def verify_prepared_services(root=ROOT, *, pg_container=None, pg_port=None):
    import tempfile
    from client_host_entry import stage_source
    from check_native_services import verify, database_receipt
    with tempfile.TemporaryDirectory(prefix='aiphone-bundled-services-') as folder:
        staged = stage_source(root / 'apps/client/src-tauri/gen/host/source.payload', Path(folder) / 'installation')
        result = verify(staged)
        if pg_container is not None:
            result['database'] = database_receipt(staged, pg_container, pg_port)
        return result

def payload_components(root=ROOT):
    archive = root / 'apps/client/src-tauri/gen/host/source.payload'
    if not archive.exists():
        return []
    with tarfile.open(archive, 'r:gz') as source:
        entry = source.getmember('HOST-MANIFEST.json')
        if entry.size > 4 * 1024 * 1024:
            raise ValueError('Invalid source manifest size')
        components=json.load(source.extractfile(entry)).get('components', [])
    sidecar=archive.parent/'integrity.json'
    integrity=json.loads(sidecar.read_text()) if sidecar.exists() else {}
    return components+(['android-runtime'] if 'android.payload' in integrity else [])+(['container-images'] if 'images.payload' in integrity else [])+(['installer-tools'] if 'tools/manifest.json' in integrity else [])
if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--with-web-runtime', action='store_true')
    parser.add_argument('--with-service-runtime', action='store_true')
    parser.add_argument('--android-runtime',type=Path)
    parser.add_argument('--images-runtime',type=Path)
    parser.add_argument('--installer-tools',type=Path)
    parser.add_argument('--verify-web-runtime', action='store_true')
    parser.add_argument('--verify-service-runtime', action='store_true')
    parser.add_argument('--pg-container')
    parser.add_argument('--pg-port', type=int)
    args = parser.parse_args()
    if args.verify_web_runtime and (not args.with_web_runtime):
        parser.error('Verification requires --with-web-runtime')
    if args.verify_service_runtime and not args.with_service_runtime:
        parser.error('Verification requires --with-service-runtime')
    if (args.pg_container is None) != (args.pg_port is None) or (args.pg_container is not None and not args.verify_service_runtime):
        parser.error('Database fixture requires container/port and service verification')
    result = prepare(with_web_runtime=args.with_web_runtime, with_service_runtime=args.with_service_runtime,android_runtime=args.android_runtime,image_runtime=args.images_runtime,installer_tools=args.installer_tools)
    if args.verify_web_runtime:
        result['web_runtime_verification'] = verify_prepared_web()
    if args.verify_service_runtime:
        result['service_runtime_verification'] = verify_prepared_services(pg_container=args.pg_container, pg_port=args.pg_port)
    print(json.dumps(result))
