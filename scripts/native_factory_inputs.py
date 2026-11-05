"""Maintainer-only pinned build inputs. Never invoked by an installed client."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
from client_host_entry import verify_file
from linux_dependencies import pin
from linux_host_runtime import PROFILE as HOST
from linux_ocr_runtime import PROFILE as OCR

ROOT = Path(__file__).resolve().parent.parent
PROFILES = {'linux-host': (HOST, 'AI_PHONE_NATIVE_HOST_ASSET'),
            'linux-ocr': (OCR, 'AI_PHONE_NATIVE_OCR_ASSET')}


def record(root, role):
    archive = pin(root, PROFILES[role][0])
    return {'path': archive['file'], 'size': archive['size_bytes'],
            'sha256': archive['sha256'], 'mode': 0o644}


def archive_directory(root, role):
    directory = os.environ.get('AI_PHONE_FACTORY_ARCHIVES', '')
    if not directory:
        return None
    folder = Path(directory)
    verify_file(folder, record(root, role))
    return folder


def acquire(root, assets, repository):
    if not any(assets.values()):
        return None
    if (not all(re.fullmatch(r'[1-9][0-9]*', assets[role]) for role in PROFILES)
            or re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9-]*/[A-Za-z0-9_.-]+', repository) is None
            or repository.rsplit('/', 1)[-1] in ('.', '..')):
        raise ValueError('Native build inputs require both asset IDs and their repository')
    folder = Path(root)/'.tools/factory-native-inputs'
    folder.mkdir(parents=True, exist_ok=True, mode=0o700)
    for role in PROFILES:
        expected = record(root, role)
        destination = folder/expected['path']
        if destination.exists() or destination.is_symlink():
            verify_file(folder, expected)
            continue
        endpoint = 'repos/'+repository+'/releases/assets/'+assets[role]
        metadata = json.loads(subprocess.check_output(['gh', 'api', endpoint], text=True))
        if (metadata.get('name') != expected['path'] or metadata.get('size') != expected['size']
                or metadata.get('digest') != 'sha256:'+expected['sha256']):
            raise ValueError('Native build asset does not match the fixed dependency pin')
        descriptor, temporary = tempfile.mkstemp(prefix='.'+destination.name+'.', dir=folder)
        temporary = Path(temporary)
        try:
            with os.fdopen(descriptor, 'wb') as output:
                subprocess.run(['gh', 'api', endpoint, '--header', 'Accept: application/octet-stream'],
                               stdout=output, stderr=subprocess.PIPE, check=True)
            # Use the existing file admission before publishing the cache entry.
            verify_file(folder, {**expected, 'path': temporary.name})
            temporary.chmod(0o644)
            temporary.replace(destination)
        finally:
            temporary.unlink(missing_ok=True)
    return folder


def main():
    assets = {role: os.environ.get(variable, '') for role, (_profile, variable) in PROFILES.items()}
    folder = acquire(ROOT, assets, os.environ.get('GITHUB_REPOSITORY', ''))
    if folder is not None and os.environ.get('GITHUB_ENV'):
        with Path(os.environ['GITHUB_ENV']).open('a') as output:
            output.write('AI_PHONE_FACTORY_ARCHIVES='+str(folder)+'\n')
    print(json.dumps({'native_factory_inputs': 'pinned' if folder is not None else 'source'}))


if __name__ == '__main__':
    main()
