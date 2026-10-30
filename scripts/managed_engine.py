"""Private managed-engine identity, short Lima state and restart environment."""
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess

NAME = 'ai-phone'
RECORD = 'data/deployment/container-host.json'


def data_identity(root):
    return str((Path(root) / 'data').resolve())


def installation_home(root):
    data = Path(data_identity(root))
    if data.name != 'data' or data.parent.name != 'user':
        raise ValueError('Managed engine requires the private installation data layout')
    return data.parent.parent


def socket_budget(path):
    if len(os.fsencode(str(path / NAME / 'ssh.sock.1234567890123456'))) >= 104:
        raise ValueError('Container state path is too long for Lima sockets; existing files were preserved')


def state_directory(root, home):
    home = Path(home).resolve()
    if home != installation_home(root):
        raise ValueError('Container installation owner differs')
    legacy = home / 'lima'
    if legacy.exists() or legacy.is_symlink():
        result = legacy
    else:
        identity = hashlib.sha256(data_identity(root).encode()).hexdigest()[:16]
        result = Path.home() / '.aiphone-vms' / identity
    socket_budget(result.resolve())
    return result


def private_directory(path):
    if path.is_symlink():
        raise ValueError('Managed container state cannot be a symlink')
    path.mkdir(mode=0o700, exist_ok=True)
    validate_directory(path)


def validate_directory(path):
    if path.is_symlink() or not path.is_dir():
        raise ValueError('Managed container state is missing or replaced')
    info = path.stat()
    if info.st_uid != os.getuid() or info.st_mode & 0o777 != 0o700:
        raise ValueError('Managed container state has a different owner')


def claim_state(root, path):
    path = Path(path)
    private_directory(path.parent)
    private_directory(path)
    marker = path / '.aiphone-owner.json'
    expected = {'format': 1, 'data_root': data_identity(root)}
    if marker.exists() or marker.is_symlink():
        verify_state(root, path)
        return
    if any(path.iterdir()):
        raise ValueError('Unowned container state was preserved; review migration before starting')
    from database import atomic_json
    atomic_json(marker, expected)


def verify_state(root, path):
    validate_directory(path.parent)
    validate_directory(path)
    marker = path / '.aiphone-owner.json'
    if marker.is_symlink() or not marker.is_file() or marker.stat().st_size > 16384:
        raise ValueError('Managed container ownership is missing')
    value = json.loads(marker.read_text())
    if value != {'format': 1, 'data_root': data_identity(root)} or type(value['format']) is not int:
        raise ValueError('Another installation owns this container state')


def digest(path):
    with Path(path).open('rb') as body:
        return hashlib.file_digest(body, 'sha256').hexdigest()


def validate_file(path, home, expected=None):
    path = Path(path)
    if path.is_symlink() or not path.resolve().is_relative_to(home) or not path.is_file():
        raise ValueError('Managed container executable is outside its installation')
    if not os.access(path, os.X_OK):
        raise ValueError('Managed container executable is unavailable')
    identity = digest(path)
    if expected is not None and identity != expected:
        raise ValueError('Managed container executable changed')
    return identity


def save_selection(root, home, state, command, wrapper):
    root, home, state = Path(root), Path(home).resolve(), Path(state)
    if home != installation_home(root) or state_directory(root, home) != state:
        raise ValueError('Managed container selection differs from its owner')
    claim_state(root, state)
    if Path(wrapper).resolve() != home / 'bin':
        raise ValueError('Managed Docker wrapper is outside its installation')
    value = {'format': 1, 'data_root': data_identity(root), 'home': str(home),
             'state': str(state), 'command': str(command),
             'command_sha256': validate_file(command, home),
             'wrapper_sha256': validate_file(Path(wrapper) / 'docker', home)}
    from database import atomic_json
    atomic_json(root / RECORD, value)


def selection(root):
    root = Path(root)
    path = root / RECORD
    if not path.exists() and not path.is_symlink():
        return None
    if path.is_symlink() or path.stat().st_size > 16384:
        raise ValueError('Invalid managed container selection')
    value = json.loads(path.read_text())
    validate_selection(root, value)
    return value


def validate_selection(root, value):
    fields = {'format', 'data_root', 'home', 'state', 'command', 'command_sha256', 'wrapper_sha256'}
    if not isinstance(value, dict) or set(value) != fields or type(value['format']) is not int or value['format'] != 1:
        raise ValueError('Invalid managed container selection')
    validate_strings(value)
    home = installation_home(root)
    if value['home'] != str(home) or value['data_root'] != data_identity(root):
        raise ValueError('Managed container data owner changed')
    state = state_directory(root, home)
    if value['state'] != str(state):
        raise ValueError('Managed container state owner changed')
    verify_state(root, state)
    validate_file(value['command'], home, value['command_sha256'])
    validate_file(home / 'bin/docker', home, value['wrapper_sha256'])


def validate_strings(value):
    if any(not isinstance(text,str) or not text for key,text in value.items() if key!='format'):
        raise ValueError('Invalid managed container selection fields')
    for key in ('command_sha256','wrapper_sha256'):
        if re.fullmatch('[0-9a-f]{64}',value[key]) is None:
            raise ValueError('Invalid managed container executable identity')


def environment(root):
    value = selection(root)
    if value is None:
        return {}
    return {'AI_PHONE_MANAGED_BIN': str(Path(value['home']) / 'bin'),
            'LIMA_HOME': value['state']}


def ensure_running(root):
    value = selection(root)
    if value is None:
        return False
    env = {**os.environ, 'LIMA_HOME': value['state']}
    subprocess.run([value['command'], 'start', '--tty=false', NAME], cwd=root,
                   env=env, check=True)
    return True
