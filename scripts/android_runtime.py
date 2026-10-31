"""Stream a sealed Android SDK baseline separately from the small source payload.

Development components remain private until their redistribution/source review is
complete. This module never packages an AVD or reuses an existing guest data disk.
"""
import gzip
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import tarfile
import tempfile
import zipfile
from host_platform import host_key
from artifact_io import digest

MANIFEST='ANDROID-MANIFEST.json'
ABIS={'macos-arm64':'arm64-v8a','linux-x64':'x86_64','windows-x64':'x86_64'}
MAX_FILES=20000
MAX_FILE=4*1024**3
MAX_TOTAL=8*1024**3
RESERVE=4*1024**3


def checked_path_text(name):
    if not isinstance(name,str) or not name or '\\' in name or any(ord(c)<32 for c in name):
        raise ValueError('Invalid Android runtime path')
    return name


def windows_path(path):
    for part in path.parts:
        if part.endswith((' ','.')) or set(part).intersection('<>:"|?*'):
            raise ValueError('Invalid Windows Android runtime path')
        if re.fullmatch(r'(con|prn|aux|nul|com[1-9¹²³]|lpt[1-9¹²³])',part.split('.')[0],re.IGNORECASE):
            raise ValueError('Reserved Windows Android runtime path')


def checked_path(name, abi):
    path=PurePosixPath(checked_path_text(name))
    prefixes=('sdk/emulator/','sdk/platform-tools/',f'sdk/system-images/android-35/default/{abi}/')
    if path.as_posix()!=name or '..' in path.parts or not name.startswith(prefixes):
        raise ValueError('Invalid Android runtime path')
    if host_key()=='windows-x64':windows_path(path)
    if any(part in ('.env','snapshots','userdata-qemu.img','userdata-qemu.img.qcow2') or part.startswith('._') for part in path.parts):
        raise ValueError('Private Android runtime path')
    return path


def checked_file(row, abi):
    if not isinstance(row,dict) or set(row)!={'path','size','sha256','mode'}:
        raise ValueError('Invalid Android runtime file')
    name=checked_path(row['path'],abi).as_posix()
    if type(row['size']) is not int or not 0<=row['size']<=MAX_FILE:
        raise ValueError('Invalid Android runtime size')
    if type(row['mode']) is not int or row['mode'] not in (0o644,0o755):
        raise ValueError('Invalid Android runtime mode')
    if not isinstance(row['sha256'],str) or re.fullmatch('[0-9a-f]{64}',row['sha256']) is None:
        raise ValueError('Invalid Android runtime digest')
    return name


def checked_files(rows, abi):
    if not isinstance(rows,list) or not 1<=len(rows)<=MAX_FILES:
        raise ValueError('Invalid Android runtime inventory')
    records={};total=0;names=set();windows=host_key()=='windows-x64'
    for row in rows:
        name=checked_file(row,abi)
        identity=name.casefold() if windows else name
        if identity in names:
            raise ValueError('Duplicate Android runtime file')
        names.add(identity)
        total+=row['size']
        if total>MAX_TOTAL:
            raise ValueError('Android runtime exceeds size budget')
        records[name]=row
    suffix='.exe' if windows else ''
    required=['sdk/emulator/emulator'+suffix,'sdk/platform-tools/adb'+suffix,*[f'sdk/system-images/android-35/default/{abi}/{name}' for name in ('system.img','vendor.img','kernel-ranchu','ramdisk.img','NOTICE.txt')]]
    if not all(name in records for name in required):
        raise ValueError('Incomplete Android runtime')
    for name in required[:2]:
        if records[name]['mode']!=0o755:
            raise ValueError('Android runtime program is not executable')
    return records


def checked_manifest(value):
    if not isinstance(value,dict) or type(value.get('format')) is not int or value['format']!=1:
        raise ValueError('Invalid Android runtime manifest')
    if value.get('host')!=host_key() or value.get('abi')!=ABIS.get(host_key()) or type(value.get('api')) is not int or value['api']!=35:
        raise ValueError('Android runtime host mismatch')
    if value.get('redistribution')!='review-pending':
        raise ValueError('Android runtime review status is not supported')
    checked_files(value.get('files'),value['abi'])
    return value


def is_alias(path):
    return path.is_symlink() or path.is_junction()


def permissions_match(path,mode):
    actual=path.stat().st_mode
    if os.name=='nt':return bool(actual&0o200)==bool(mode&0o200)
    return actual&0o777==mode


def verify_sdk(sdk):
    sdk=Path(sdk);folder=sdk.parent
    if is_alias(sdk) or is_alias(folder):raise ValueError('Android runtime integrity: directory alias')
    manifest=checked_manifest(json.loads((folder/MANIFEST).read_text()))
    expected={row['path'] for row in manifest['files']}|{MANIFEST}
    actual=set()
    for path in folder.rglob('*'):
        if is_alias(path):
            raise ValueError('Android runtime integrity: symlink')
        if path.is_file():actual.add(path.relative_to(folder).as_posix())
    if expected!=actual:
        raise ValueError('Android runtime integrity: inventory')
    for row in manifest['files']:
        path=folder/row['path']
        if not permissions_match(path,row['mode']) or path.stat().st_size!=row['size'] or digest(path)!=row['sha256']:
            raise ValueError('Android runtime integrity: file')
    return manifest


def bundled_sdk(sdk):
    marker=Path(sdk).parent/MANIFEST
    return verify_sdk(sdk) if marker.exists() or marker.is_symlink() else None


def owned_components(home):
    home=Path(home)
    # Resolve OS aliases such as macOS /var, but never adopt a linked app root.
    if is_alias(home):
        raise ValueError('Android installation directory is a symlink')
    home=home.resolve()
    folder=home
    for name in ('components','android'):
        folder=folder/name
        if is_alias(folder):
            raise ValueError('Android component directory is a symlink')
    folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    return folder


def manifest_entry(source, entries):
    entry=next(entries,None)
    if entry is None or entry.name!=MANIFEST or not entry.isfile() or not 0<entry.size<=8*1024**2:
        raise ValueError('Invalid Android manifest header')
    return checked_manifest(json.load(source.extractfile(entry)))


def write_member(source, entry, work, row):
    if not entry.isfile() or entry.size!=row['size']:
        raise ValueError('Invalid Android runtime header')
    target=work/row['path'];target.parent.mkdir(parents=True,exist_ok=True)
    with source.extractfile(entry) as incoming,target.open('xb') as outgoing:shutil.copyfileobj(incoming,outgoing,1024**2)
    if digest(target)!=row['sha256']:
        raise ValueError('Android runtime integrity: archive body')
    target.chmod(row['mode'])


def stage(payload, home):
    folder=owned_components(home);destination=folder/digest(payload)
    if is_alias(destination):
        raise ValueError('Android component directory is a symlink')
    if destination.exists():verify_sdk(destination/'sdk');return destination/'sdk'
    with tarfile.open(payload,'r|gz') as source:
        entries=iter(source);manifest=manifest_entry(source,entries);records=checked_files(manifest['files'],manifest['abi'])
        if shutil.disk_usage(folder).free<RESERVE+sum(row['size'] for row in records.values()):
            raise ValueError('Insufficient free space for Android runtime and recovery reserve')
        with tempfile.TemporaryDirectory(prefix='.android-',dir=folder) as temp:
            work=Path(temp)/'runtime';work.mkdir();remaining=dict(records)
            for entry in entries:
                row=remaining.pop(entry.name,None)
                if row is None:
                    raise ValueError('Unlisted or duplicate Android runtime member')
                write_member(source,entry,work,row)
            if remaining:
                raise ValueError('Missing Android runtime members')
            (work/MANIFEST).write_text(json.dumps(manifest,sort_keys=True)+'\n')
            work.rename(destination)
    return destination/'sdk'


def inventory_file(name, path):
    executable=path.suffix.lower() in ('.exe','.bat','.cmd') if host_key()=='windows-x64' else bool(path.stat().st_mode&0o111)
    return {'path':name,'size':path.stat().st_size,'sha256':digest(path),'mode':0o755 if executable else 0o644}


def tools_inventory(sdk, abi):
    files=[];sources={}
    for component in ('emulator','platform-tools'):
        if is_alias(sdk/component):raise ValueError('SDK runtime source contains a link')
        if not (sdk/component/'source.properties').is_file():
            raise ValueError('Missing SDK component metadata')
        for path in sorted((sdk/component).rglob('*')):
            if path.name.startswith('._') or path.name=='.DS_Store':continue
            if is_alias(path):
                raise ValueError('SDK runtime source contains a link')
            if path.is_file():
                name='sdk/'+path.relative_to(sdk).as_posix();checked_path(name,abi)
                files.append(inventory_file(name,path));sources[name]=path
    return files,sources


def image_inventory(source, abi):
    files=[];sources={}
    for entry in source.infolist():
        if entry.is_dir():continue
        prefix=abi+'/'
        if not entry.filename.startswith(prefix) or entry.external_attr>>16&0o170000==0o120000:
            raise ValueError('Invalid factory image archive')
        name=f'sdk/system-images/android-35/default/{abi}/'+entry.filename[len(prefix):]
        checked_path(name,abi)
        if name in sources:
            raise ValueError('Duplicate factory image member')
        with source.open(entry) as incoming:sha=hashlib.file_digest(incoming,'sha256').hexdigest()
        files.append({'path':name,'size':entry.file_size,'sha256':sha,'mode':0o755 if entry.external_attr>>16&0o111 else 0o644});sources[name]=entry
    return files,sources


def build(sdk, image_archive, output, expected_image_sha256):
    sdk=Path(sdk);output=Path(output);abi=ABIS.get(host_key())
    if not abi:
        raise ValueError('Android runtime host not implemented')
    if digest(image_archive)!=expected_image_sha256:
        raise ValueError('Factory image archive checksum mismatch')
    files,tools=tools_inventory(sdk,abi)
    with zipfile.ZipFile(image_archive) as image:
        image_files,images=image_inventory(image,abi);files+=image_files
        manifest=checked_manifest({'format':1,'host':host_key(),'api':35,'abi':abi,'redistribution':'review-pending','image_archive_sha256':expected_image_sha256,'files':files})
        output.parent.mkdir(parents=True,exist_ok=True)
        with tempfile.TemporaryDirectory(prefix='.android-build-',dir=output.parent) as temp:
            archive=Path(temp)/'android.payload'
            with archive.open('wb') as raw,gzip.GzipFile(fileobj=raw,mode='wb',mtime=0,filename='',compresslevel=3) as compressed,tarfile.open(fileobj=compressed,mode='w|') as target:
                data=json.dumps(manifest,sort_keys=True).encode();header=tarfile.TarInfo(MANIFEST);header.size=len(data);target.addfile(header,io.BytesIO(data))
                for row in files:
                    header=tarfile.TarInfo(row['path']);header.size=row['size'];header.mode=row['mode']
                    incoming=tools[row['path']].open('rb') if row['path'] in tools else image.open(images[row['path']])
                    with incoming:target.addfile(header,incoming)
            archive.replace(output)
    return {'host':manifest['host'],'abi':abi,'files':len(files),'bytes':sum(row['size'] for row in files),'archive_sha256':digest(output),'redistribution':manifest['redistribution']}


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__);sub=parser.add_subparsers(dest='operation',required=True)
    command=sub.add_parser('build');command.add_argument('--sdk',required=True,type=Path);command.add_argument('--image-archive',required=True,type=Path);command.add_argument('--image-sha256',required=True);command.add_argument('--output',required=True,type=Path)
    command=sub.add_parser('stage');command.add_argument('--payload',required=True,type=Path);command.add_argument('--home',required=True,type=Path)
    args=parser.parse_args()
    if args.operation=='build':print(json.dumps(build(args.sdk,args.image_archive,args.output,args.image_sha256)))
    else:print(stage(args.payload,args.home))
