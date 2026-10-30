"""Supported host policy and guest ABI are selected together, never guessed."""
import json
from pathlib import Path
import platform


def host_key(system=None, machine=None):
    system=system or platform.system();machine=machine or platform.machine()
    os_name={'Darwin':'macos','Linux':'linux','Windows':'windows'}.get(system)
    arch={'arm64':'arm64','aarch64':'arm64','AMD64':'x64','x86_64':'x64'}.get(machine)
    if not os_name or not arch:raise ValueError('Unsupported host platform')
    return os_name+'-'+arch


def host_policy(root, system=None, machine=None):
    config=json.loads((Path(root)/'config/host-bootstrap.json').read_text())
    key=host_key(system,machine)
    if key not in config['hosts']:raise ValueError('Independent deployment is not implemented for '+key)
    return config,config['hosts'][key]
