"""Bounded SDK ZIP extraction shared by native platform resource factories."""
from pathlib import Path
import shutil
import tempfile
import zipfile
from client_host_entry import canonical_name
from host_platform import host_key
from android_runtime import windows_path

SDK_MAX_BYTES=8*1024**3
SDK_MAX_FILES=20000


def sdk_name(entry,component):
    name=canonical_name(entry.filename.rstrip('/')).as_posix()
    if name!=component and not name.startswith(component+'/'):raise ValueError('Invalid SDK archive component')
    if host_key()=='windows-x64':windows_path(Path(name))
    kind=entry.external_attr>>16&0o170000
    if kind not in (0,0o100000,0o040000):raise ValueError('Invalid SDK archive member type')
    if kind==0o040000 and not entry.is_dir():raise ValueError('Invalid SDK archive directory')
    return name


def sdk_members(source,component):
    total=0;seen=set();windows=host_key()=='windows-x64'
    for number,entry in enumerate(source.infolist()):
        name=sdk_name(entry,component);total+=entry.file_size
        if number>=SDK_MAX_FILES or total>SDK_MAX_BYTES:raise ValueError('SDK archive budget exceeded')
        identity=name.casefold() if windows else name
        if identity in seen:raise ValueError('Invalid duplicate SDK archive member')
        seen.add(identity)
        yield name,entry


def unpack_sdk(archive,destination,component):
    destination=Path(destination);destination.parent.mkdir(parents=True,exist_ok=True)
    if destination.exists():raise ValueError('SDK destination must be new')
    with zipfile.ZipFile(archive) as source,tempfile.TemporaryDirectory(prefix='.sdk-',dir=destination.parent) as temporary:
        folder=Path(temporary)
        for name,entry in sdk_members(source,component):
            target=folder/name
            if entry.is_dir():target.mkdir(parents=True,exist_ok=True);continue
            target.parent.mkdir(parents=True,exist_ok=True)
            with source.open(entry) as incoming,target.open('xb') as output:shutil.copyfileobj(incoming,output,1024**2)
            target.chmod(0o755 if entry.external_attr>>16&0o111 else 0o644)
        folder.rename(destination)


