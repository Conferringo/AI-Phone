"""Offline QEMU, SSH, firmware and library profile for the owned Linux host."""
import argparse
from functools import partial
import json
from pathlib import Path
import linux_dependencies as dependencies
from linux_dependencies import DependencyProfile
from host_platform import host_key

PREFIX = 'runtime/container-host/native/'
PROGRAMS = ('qemu-system-x86_64', 'qemu-img', 'ssh', 'ssh-keygen', 'scp')
SOURCE = ('config/linux-host-runtime.json', 'scripts/linux_host_runtime.py', 'scripts/linux_dependencies.py',
          'scripts/linux_dependency_factory.py', 'scripts/linux-host/Dockerfile', 'scripts/linux-host/collect.py',
          'scripts/linux-host/launch.sh', 'scripts/linux-host/lima.sh')
REQUIRED = frozenset({'bin/' + name for name in PROGRAMS} | {'lib/ld-linux-x86-64.so.2',
    'share/qemu/edk2-x86_64-code.fd', 'share/qemu/bios-256k.bin', 'source-records.json', 'licenses/qemu-system-x86.copyright'})
PROFILE = DependencyProfile('config/linux-host-runtime.json', SOURCE,
    frozenset({'bin', 'lib', 'modules', 'share', 'licenses', 'source-records.json'}), REQUIRED,
    tuple(('launchers/' + name, 'scripts/linux-host/launch.sh', 0o755) for name in PROGRAMS))
build = partial(dependencies.build, profile=PROFILE)
prepare = partial(dependencies.prepare, profile=PROFILE, prefix=PREFIX)


def verify(root, component, *, host=None):
    return dependencies.verify(root, component, PROFILE, host=host or host_key())


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--destination', type=Path, required=True)
    args = parser.parse_args()
    value = build(args.root, args.archive, args.destination)
    print(json.dumps({'host': value['host'], 'files': len(value['files']), 'bytes': sum(row['size'] for row in value['files'])}))
