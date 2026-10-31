"""Native path, alias and executable metadata for sealed runtime components."""
import os
import re


def windows_path(path):
    for part in path.parts:
        if part.endswith((' ','.')) or set(part).intersection('<>:"|?*'):
            raise ValueError('Invalid Windows runtime path')
        if re.fullmatch(r'(con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])',part.split('.')[0],re.IGNORECASE):
            raise ValueError('Reserved Windows runtime path')


def is_alias(path):
    return path.is_symlink() or path.is_junction()


def permissions_match(path,mode):
    actual=path.stat().st_mode
    if os.name=='nt':return bool(actual&0o200)==bool(mode&0o200)
    return actual&0o777==mode


def file_mode(path,host,mode=None):
    if host=='windows-x64':return 0o755 if path.suffix.lower() in ('.exe','.bat','.cmd') else 0o644
    actual=path.stat().st_mode if mode is None else mode
    return 0o755 if actual&0o111 else 0o644
