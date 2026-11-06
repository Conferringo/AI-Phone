"""Temporary installer adapter; native Rust owns managed-engine policy and state."""
import json
from pathlib import Path
import subprocess


def invoke(root, operation, *arguments):
    from prebuilt_services import managed_engine_command
    result = subprocess.run([*managed_engine_command(root), '--managed-engine', str(root),
                             str(Path.home()), operation, *map(str, arguments)],
                            cwd=root, capture_output=True, text=True)
    if result.returncode:
        raise ValueError(result.stderr.strip())
    return json.loads(result.stdout)


def state_directory(root, home):
    return Path(invoke(root, 'state', home))


def claim_state(root, path):
    invoke(root, 'claim', path)


def save_selection(root, home, state, command, wrapper):
    invoke(root, 'save', home, state, command, wrapper)


def selected(root, operation, absent):
    record = Path(root) / 'data/deployment/container-host.json'
    if not record.exists() and not record.is_symlink():
        return absent
    return invoke(root, operation)


def selection(root):
    return selected(root, 'selection', None)


def environment(root):
    return selected(root, 'environment', {})


def ensure_running(root):
    return selected(root, 'start', False)
