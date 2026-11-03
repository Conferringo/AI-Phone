"""Materialize reviewed WSL capsules into native resources at maintainer build time."""
import hashlib
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
from artifact_io import digest
from wsl_backend import BODY,checked_manifest,verify_stream

RESERVE=4*1024**3


def manifest(folder,expected=None):
    folder=Path(folder);path=folder/'manifest.json'
    if folder.is_symlink() or path.is_symlink() or not path.is_file() or path.stat().st_size>4*1024**2:
        raise ValueError('Invalid prepared WSL manifest')
    if expected is not None and digest(path)!=expected:raise ValueError('Prepared WSL manifest changed')
    value=json.loads(path.read_text());checked_manifest(value)
    return value


def verify(folder,expected=None):
    folder=Path(folder);value=manifest(folder,expected)
    body=folder/BODY
    if {p.name for p in folder.iterdir()}!={'manifest.json',BODY} or body.is_symlink() or not body.is_file():
        raise ValueError('Unexpected prepared WSL resource')
    if body.stat().st_size!=value['rootfs']['size'] or digest(body)!=value['rootfs']['sha256']:
        raise ValueError('Prepared WSL rootfs changed')
    return value


def copy_rootfs(source,output,record):
    measured=hashlib.sha256();size=0
    with output.open('xb') as target:
        while chunk:=source.read(1024**2):
            size+=len(chunk)
            if size>record['size']:raise ValueError('WSL rootfs copy exceeded its inventory')
            measured.update(chunk);target.write(chunk)
    if size!=record['size'] or measured.hexdigest()!=record['sha256']:
        raise ValueError('WSL rootfs changed during preparation')


def prepare(folder,component):
    folder=Path(folder);target=folder/'wsl'
    if component is None:
        if target.exists() or target.is_symlink():
            verify(target);shutil.rmtree(target)
        return {}
    with Path(component).open('rb') as raw:
        value=verify_stream(raw)
        if target.exists() or target.is_symlink():
            if verify(target)!=value:raise ValueError('Generated WSL resources differ; regenerate the owned directory')
        else:
            if shutil.disk_usage(folder).free<RESERVE+value['rootfs']['size']:
                raise ValueError('Keep 4 GiB free while preparing WSL resources')
            raw.seek(0)
            with tempfile.TemporaryDirectory(prefix='.wsl-resources-',dir=folder) as temporary:
                staged=Path(temporary)/'wsl';staged.mkdir()
                with tarfile.open(fileobj=raw,mode='r:') as archive:
                    with archive.extractfile(BODY) as body:copy_rootfs(body,staged/BODY,value['rootfs'])
                (staged/'manifest.json').write_text(json.dumps(value,sort_keys=True)+'\n')
                staged.rename(target)
    return {'wsl/manifest.json':digest(target/'manifest.json')}
