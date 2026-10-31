"""Collect the Linux QEMU/SSH closure and its BIOS/UEFI firmware."""
from pathlib import Path
import shutil
from factory import archive, native_closure, package_records

PROGRAMS = {name: '/usr/bin/' + name for name in ('qemu-system-x86_64', 'qemu-img', 'ssh', 'ssh-keygen', 'scp')}


def firmware(root):
    for name in ('qemu', 'seabios'):
        shutil.copytree('/usr/share/' + name, root / 'share' / name, symlinks=False)
    # QEMU's explicit -L selects one data directory; distro firmware lives in siblings.
    for folder in (Path('/usr/share/seabios'), Path('/usr/lib/ipxe/qemu')):
        for path in folder.iterdir():
            if path.is_file(): shutil.copyfile(path, root / 'share/qemu' / path.name)
    shutil.copyfile('/usr/share/OVMF/OVMF_CODE_4M.fd', root / 'share/qemu/edk2-x86_64-code.fd')


if __name__ == '__main__':
    root = Path('/out'); root.mkdir()
    modules = tuple(Path('/usr/lib/x86_64-linux-gnu/qemu').glob('*.so'))
    files = native_closure(root, PROGRAMS, modules); firmware(root); package_records(root, files)
    output = Path('/export'); output.mkdir(); archive(root, output / 'linux-x64-host.tar.gz')
