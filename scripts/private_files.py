"""One private atomic JSON writer for credentials and installation metadata."""
import json
import os
from pathlib import Path
import tempfile


def protect(path, *, directory=False, descriptor=None):
    if Path(path).is_symlink():
        raise ValueError('Private files cannot replace or follow an unknown alias')
    if os.name == 'nt':
        from windows_private import protect as windows_protect
        windows_protect(path)
    elif descriptor is not None:
        os.fchmod(descriptor, 0o700 if directory else 0o600)
    else:
        Path(path).chmod(0o700 if directory else 0o600)


def is_private(path):
    if os.name == 'nt':
        from windows_private import is_private as windows_is_private
        return windows_is_private(path)
    return not Path(path).stat().st_mode & 0o077


def replace(source, destination):
    if os.name == 'nt':
        from windows_private import replace as windows_replace
        windows_replace(source, destination)
        return
    os.replace(source, destination)
    directory = os.open(Path(destination).parent, os.O_RDONLY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)


def atomic_json(path, value):
    path = Path(path)
    if path.is_symlink() or path.parent.is_symlink():
        raise ValueError('Private files cannot replace or follow an unknown alias')
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    protect(path.parent, directory=True)
    fd, temporary = tempfile.mkstemp(dir=path.parent, prefix='.credentials-')
    file = None
    try:
        file = os.fdopen(fd, 'w')
        with file:
            protect(temporary, descriptor=file.fileno())
            json.dump(value, file, indent=2)
            file.write('\n')
            file.flush()
            os.fsync(file.fileno())
        replace(temporary, path)
    finally:
        if file is None:
            os.close(fd)
        Path(temporary).unlink(missing_ok=True)
