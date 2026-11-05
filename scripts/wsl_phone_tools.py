"""Maintainer-only offline Linux ADB/schema resource producer; no emulator VM."""
import json
from pathlib import Path
import shutil
import tempfile

from android_component_factory import checked_revision
from android_sdk_archive import unpack_sdk
from artifact_io import digest
from wsl_backend import PHONE_MANIFEST as MANIFEST, PHONE_SCHEMA as SCHEMA, PHONE_ROOT as ROOTFS, PHONE_REQUIRED as REQUIRED

VENDOR='packages/protocol/vendor/android-emulator'


def platform_tools_pin(root):
    return json.loads((Path(root)/'config/android-factory.json').read_text())['hosts']['linux-x64']['archives']['platform-tools']


def schema_input(root):
    vendor=Path(root)/VENDOR
    pin=json.loads((vendor/'provenance.json').read_text());source=vendor/'emulator_controller.proto'
    if source.stat().st_size!=pin['bytes'] or digest(source)!=pin['sha256']:
        raise ValueError('Emulator protocol integrity differs')
    return source,pin


def linux_adb(sdk):
    with (Path(sdk)/'platform-tools/adb').open('rb') as source:header=source.read(20)
    if len(header)!=20 or header[:7]!=b'\x7fELF\x02\x01\x01' or header[18:20]!=b'\x3e\0':
        raise ValueError('Phone tools require Linux x64 ADB')
    if not (Path(sdk)/'platform-tools/NOTICE.txt').is_file():raise ValueError('Missing ADB third-party notices')


def files(folder):
    result={}
    for path in sorted(Path(folder).rglob('*')):
        if path.is_symlink() or not (path.is_dir() or path.is_file()):raise ValueError('Invalid phone tools member')
        if path.is_file() and path.name!=MANIFEST:
            result[path.relative_to(folder).as_posix()]={'bytes':path.stat().st_size,'sha256':digest(path),'mode':path.stat().st_mode&0o777}
    return result


def verify(root,folder):
    folder=Path(folder);value=json.loads((folder/MANIFEST).read_text());pin=platform_tools_pin(root)
    _,schema=schema_input(root)
    if value.get('format')!=1 or value.get('host')!='linux-x64' or value.get('platform_tools_sha256')!=pin['sha256'] or value.get('schema_sha256')!=schema['sha256'] or value.get('files')!=files(folder):
        raise ValueError('Phone tools integrity differs')
    if not {name.removeprefix(ROOTFS) for name in REQUIRED}-{MANIFEST} <= value['files'].keys():
        raise ValueError('Incomplete phone tools')
    checked_revision(folder,'platform-tools',pin);linux_adb(folder)
    return value


def build(root,archive,destination):
    root,archive,destination=Path(root),Path(archive),Path(destination)
    if destination.exists() or destination.is_symlink():raise FileExistsError('Existing phone tools preserved')
    pin=platform_tools_pin(root);schema,schema_pin=schema_input(root)
    if archive.stat().st_size!=pin['size_bytes'] or digest(archive)!=pin['sha256']:
        raise ValueError('Linux platform tools archive integrity differs')
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.phone-tools-',dir=destination.parent) as temporary:
        staged=Path(temporary)/'sdk';unpack_sdk(archive,staged,'platform-tools')
        checked_revision(staged,'platform-tools',pin);linux_adb(staged)
        (staged/SCHEMA).parent.mkdir(parents=True);shutil.copyfile(schema,staged/SCHEMA)
        shutil.copyfile(root/VENDOR/'provenance.json',staged/'provenance.json')
        shutil.copyfile(root/'LICENSE',staged/'LICENSE')
        value={'format':1,'host':'linux-x64','platform_tools_sha256':pin['sha256'],
               'schema_sha256':schema_pin['sha256'],'redistribution':'review-pending','files':files(staged)}
        (staged/MANIFEST).write_text(json.dumps(value,sort_keys=True)+'\n')
        verify(root,staged);staged.rename(destination)
    return value


def prepare(root,work):
    from linux_client_factory import fetch
    work=Path(work);archive=fetch(work,'platform-tools.zip',platform_tools_pin(root))
    output=work/'sdk';build(root,archive,output)
    return output
