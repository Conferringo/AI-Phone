"""Read-only admission and atomic publication of the owned WSL backend rootfs."""
import hashlib
import io
import json
import os
import posixpath
from pathlib import Path, PurePosixPath
import tarfile
import tempfile
from artifact_io import digest

MANIFEST='WSL-BACKEND-MANIFEST.json'
BODY='rootfs.tar.gz'
MAX_BYTES=16*1024**3
REQUIRED=frozenset(('etc/os-release','etc/wsl.conf','opt/aiphone/bundle/source.payload',
    'opt/aiphone/bundle/images.payload','opt/aiphone/tools/python/bin/python3',
    'opt/aiphone/tools/node/bin/node','opt/aiphone/bootstrap/wsl_backend_entry.py',
    'usr/local/bin/docker','usr/local/bin/dockerd'))


def member_name(value):
    name=value.removeprefix('./').rstrip('/')
    path=PurePosixPath(name)
    if not name or path.is_absolute() or '..' in path.parts or '\\' in name or any(ord(c)<32 for c in name):
        raise ValueError('Invalid WSL rootfs member')
    if path.as_posix()!=name:raise ValueError('Noncanonical WSL rootfs member')
    return name


def public_shell_config(parts):
    return parts[-1] in ('.bashrc','.profile','.bash_logout') and ((parts[0]=='root' and len(parts)==2) or (parts[0]=='home' and len(parts)==3))


def permitted(name):
    parts=PurePosixPath(name).parts
    if any(part in ('.git','.ssh','.env','authorized_keys','id_rsa','limactl','emulator') or part.startswith('qemu-system-') for part in parts):
        raise ValueError('Foreign runtime or private state in WSL rootfs')
    if name.startswith('var/lib/aiphone/') or (name.startswith(('home/','root/')) and not public_shell_config(parts)):
        raise ValueError('User state cannot be bundled in WSL rootfs')


def checked_link(entry,name):
    target=entry.linkname
    if not target or any(ord(c)<32 for c in target):raise ValueError('Invalid WSL link')
    base=str(PurePosixPath(name).parent) if entry.issym() else ''
    resolved=posixpath.normpath(posixpath.join(base,target))
    if resolved=='..' or resolved.startswith('../'):raise ValueError('WSL link escapes archive root')


def archive_member(entry, names, links):
    name=member_name(entry.name)
    if name in names:raise ValueError('Duplicate WSL rootfs member')
    if entry.type not in (tarfile.REGTYPE,tarfile.AREGTYPE,tarfile.DIRTYPE,tarfile.SYMTYPE,tarfile.LNKTYPE):
        raise ValueError('Unsupported WSL rootfs member')
    if not entry.isdir():permitted(name)
    names.add(name)
    if entry.issym() or entry.islnk():
        checked_link(entry,name)
        links.add(name)
    return name


def collect_inventory(source):
    names=set();links=set();files={};total=0;licenses=[]
    for index,entry in enumerate(source):
        if entry.name in ('.','./') and entry.isdir():continue
        if index>=100000 or entry.size<0:raise ValueError('WSL rootfs exceeds entry budget')
        name=archive_member(entry,names,links);total+=entry.size
        if total>MAX_BYTES:raise ValueError('WSL rootfs exceeds byte budget')
        record_file(source,entry,name,files,licenses)
    return names,links,files,total,licenses


def record_file(source,entry,name,files,licenses):
    if not entry.isfile():return
    if name in REQUIRED:files[name]=hashlib.file_digest(source.extractfile(entry),'sha256').hexdigest()
    if name.startswith('usr/share/doc/') and name.endswith('/copyright'):licenses.append(name)


def inspect_rootfs(path=None, *, fileobj=None):
    try:
        with tarfile.open(path,mode='r:gz',fileobj=fileobj) as source:names,links,files,total,licenses=collect_inventory(source)
    except (tarfile.TarError,EOFError,OSError) as error:raise ValueError('Invalid WSL rootfs archive') from error
    for name in names:
        if any(str(parent) in links for parent in PurePosixPath(name).parents):
            raise ValueError('WSL archive writes through a link parent')
    if set(files)!=REQUIRED or not licenses:
        missing=','.join(sorted(REQUIRED-set(files))) or 'none'
        raise ValueError(f'Incomplete WSL backend rootfs or license inventory: missing={missing}; license_count={len(licenses)}')
    return {'entries':len(names),'uncompressed_bytes':total,'required_files':files,'base_license_files':sorted(licenses)}


def metadata(rootfs, provenance):
    return {'format':1,'host':'windows-x64','backend_host':'linux-x64',
            'runtime':'wsl2-backend','rootfs':{'path':BODY,'size':rootfs.stat().st_size,'sha256':digest(rootfs)},
            'inventory':inspect_rootfs(rootfs),'provenance':provenance,'redistribution':'review-pending',
            'windows_boot_verified':False,'complete_device_bundle':False}


def add_bytes(target,name,body):
    entry=tarfile.TarInfo(name);entry.size=len(body);entry.mode=0o644;target.addfile(entry,io.BytesIO(body))


def publish_capsule(rootfs,output,provenance):
    rootfs,output=Path(rootfs),Path(output)
    if output.exists() or output.is_symlink():raise FileExistsError('Existing WSL component preserved')
    value=metadata(rootfs,provenance)
    with tempfile.NamedTemporaryFile(prefix='.wsl-',dir=output.parent,delete=False) as temporary:
        work=Path(temporary.name)
    try:
        with tarfile.open(work,'w:') as target:
            entry=tarfile.TarInfo(BODY);entry.size=value['rootfs']['size'];entry.mode=0o644
            with rootfs.open('rb') as body:target.addfile(entry,body)
            add_bytes(target,MANIFEST,json.dumps(value,sort_keys=True).encode())
        verify_capsule(work)
        # Hard-link publication cannot overwrite a component created concurrently.
        with work.open('rb') as body:os.fsync(body.fileno())
        os.link(work,output)
    finally:work.unlink(missing_ok=True)
    return value


def capsule_entry(header,offset,entries):
    entry=tarfile.TarInfo.frombuf(header,'utf-8','strict')
    limits={BODY:MAX_BYTES,MANIFEST:4*1024**2}
    if entry.name not in limits or entry.name in entries or entry.type not in (tarfile.REGTYPE,tarfile.AREGTYPE):
        raise ValueError('Invalid WSL capsule members')
    if not 0<entry.size<=limits[entry.name]:raise ValueError('Invalid WSL capsule budget')
    entry.offset=offset;entry.offset_data=offset+tarfile.BLOCKSIZE
    return entry


def capsule_entries(raw):
    entries={};offset=0
    # Parse at most three physical headers. TarFile.next() itself may recursively
    # enumerate PAX/GNU headers before returning one logical entry.
    for number in range(3):
        raw.seek(offset);header=raw.read(tarfile.BLOCKSIZE)
        if len(header)!=tarfile.BLOCKSIZE:raise ValueError('Incomplete WSL capsule header')
        if header==tarfile.NUL*tarfile.BLOCKSIZE:break
        if number==2:raise ValueError('Invalid WSL capsule members')
        entry=capsule_entry(header,offset,entries);entries[entry.name]=entry
        offset=entry.offset_data+(entry.size+511)//512*512
    if set(entries)!={BODY,MANIFEST}:raise ValueError('Incomplete WSL capsule members')
    return entries


def verify_stream(raw):
    entries=capsule_entries(raw);raw.seek(0)
    with tarfile.open(fileobj=raw,mode='r:') as source:
        value=json.load(source.extractfile(entries[MANIFEST]))
        checked_manifest(value)
        if entries[BODY].size!=value['rootfs']['size']:raise ValueError('WSL rootfs size differs')
        with source.extractfile(entries[BODY]) as body:actual=hashlib.file_digest(body,'sha256').hexdigest()
        if actual!=value['rootfs']['sha256']:raise ValueError('WSL rootfs integrity differs')
        with source.extractfile(entries[BODY]) as body:inventory=inspect_rootfs(fileobj=body)
        if inventory!=value['inventory']:raise ValueError('WSL rootfs inventory differs')
        return value


def verify_capsule(payload):
    try:
        with Path(payload).open('rb') as raw:return verify_stream(raw)
    except (tarfile.TarError,EOFError,UnicodeError,KeyError,TypeError) as error:
        raise ValueError('Invalid WSL capsule') from error


def checked_manifest(value):
    expected={'format':1,'host':'windows-x64','backend_host':'linux-x64','runtime':'wsl2-backend',
              'redistribution':'review-pending','windows_boot_verified':False,'complete_device_bundle':False}
    if not isinstance(value,dict) or any(value.get(key)!=item for key,item in expected.items()):
        raise ValueError('Unsupported WSL backend manifest')
    if type(value['format']) is not int or value['rootfs']['path']!=BODY or set(value['inventory']['required_files'])!=REQUIRED:
        raise ValueError('Incomplete WSL backend manifest')
