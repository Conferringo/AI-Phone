"""Reuse only the reviewed x64 engine APKs from a sealed Linux host archive."""
import json
from pathlib import Path
import shutil
import tarfile
import tempfile
from container_host_runtime import package_inputs
from artifact_io import digest

RESERVE=4*1024**3


def records(root):
    return json.loads((Path(root)/'config/container-host-runtime.json').read_text())['hosts']['linux-x64']['packages']


def extract(archive,root,destination):
    destination=Path(destination);pins=records(root);expected={'packages/'+row['file']:row for row in pins}
    if destination.exists() or destination.is_symlink():raise FileExistsError('Existing engine inputs preserved')
    required=RESERVE+sum(row['size_bytes'] for row in pins)
    if shutil.disk_usage(destination.parent).free<required:
        raise ValueError(f'Engine extraction requires {required} bytes including 4 GiB reserve')
    with tempfile.TemporaryDirectory(prefix='.wsl-engine-',dir=destination.parent) as temporary:
        work=Path(temporary)/'packages';work.mkdir();seen=set()
        with tarfile.open(archive,'r:gz') as source:
            for entry in source:
                if entry.name not in expected:continue
                if entry.name in seen or not entry.isfile() or entry.size!=expected[entry.name]['size_bytes']:
                    raise ValueError('Invalid engine archive member')
                seen.add(entry.name)
                with source.extractfile(entry) as body,(work/expected[entry.name]['file']).open('xb') as output:
                    shutil.copyfileobj(body,output,1024**2)
        package_inputs(work,pins)
        work.rename(destination)
    return {'host':'linux-x64','packages':len(pins),'bytes':sum(row['size_bytes'] for row in pins),
            'source_archive_sha256':digest(archive),'packages_sha256':{row['file']:row['sha256'] for row in pins}}


def package_license(package,pin):
    entry=package.getmember('.PKGINFO')
    if not entry.isfile() or not 0<entry.size<65536:raise ValueError('Invalid engine package attribution')
    rows=package.extractfile(entry).read().decode().splitlines()
    values={key:value for line in rows if ' = ' in line for key,value in [line.split(' = ',1)]}
    if not values.get('license') or not values.get('pkgname'):raise ValueError('Missing engine package attribution')
    return {'package':values['pkgname'],'version':values['pkgver'],'license':values['license'],
            'source':values.get('url',''),'archive_url':pin['url'],'sha256':pin['sha256']}


def licenses(folder,pins):
    result=[]
    for pin in pins:
        with tarfile.open(Path(folder)/pin['file'],'r:gz') as source:result.append(package_license(source,pin))
    return result


def relocate_links(root):
    """Relocate engine-internal absolute aliases; keep kernel /proc links external."""
    import os
    root=Path(root)
    if root.is_symlink() or not root.is_dir():raise ValueError('Engine root must be an owned directory')
    changed=[]
    for path in root.rglob('*'):
        if not path.is_symlink():continue
        target=Path(os.readlink(path))
        if not target.is_absolute():continue
        destination=root/str(target).lstrip('/')
        if destination.exists() or destination.is_symlink():
            path.unlink();path.symlink_to(os.path.relpath(destination,path.parent));changed.append(path.relative_to(root).as_posix())
    return changed
