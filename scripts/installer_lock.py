"""Nonblocking kernel locks; ownership ends when the descriptor/process closes."""
import errno
import os
from pathlib import Path
from private_files import protect


def exclusive(descriptor):
    if os.name != 'nt':
        import fcntl
        fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        return
    import msvcrt
    os.lseek(descriptor, 0, os.SEEK_SET)
    try:
        msvcrt.locking(descriptor, msvcrt.LK_NBLCK, 1)
    except OSError as error:
        if error.errno in (errno.EACCES, errno.EDEADLK):
            raise BlockingIOError('Installer lock is occupied') from error
        raise


def acquire(path):
    path = Path(path)
    if path.is_symlink():
        raise ValueError('Installer lock was replaced by an unknown alias')
    descriptor = os.open(path, os.O_RDWR | os.O_CREAT | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0), 0o600)
    file = None
    try:
        file = os.fdopen(descriptor, 'a+b')
        protect(path, descriptor=file.fileno())
        exclusive(file.fileno())
        return file
    except BaseException:
        if file is None:
            os.close(descriptor)
        else:
            file.close()
        raise
