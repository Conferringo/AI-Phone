"""Mac stock SDK admission and reproducible producer identity, without user SDKs."""
import hashlib
import importlib
import json
from pathlib import Path
import shutil
import struct
import zipfile
from artifact_io import digest

RESERVE=4*1024**3+32*1024**2
ARM64=0x0100000c
PRODUCERS=('android_component_factory','android_factory_stock','android_runtime','android_sdk_archive','artifact_io','client_host','client_host_entry','host_filesystem','host_platform')


def macos_policy(archives,image):
    emulator,tools=archives['emulator'],archives['platform-tools']
    if (emulator.get('host_os'),emulator.get('host_arch'))!=('macosx','aarch64'):
        raise ValueError('Mac factory requires a native ARM64 emulator archive')
    if (tools.get('host_os'),tools.get('host_arch'))!=('macosx',None):
        raise ValueError('Mac factory requires the universal Mac platform-tools archive')
    if (image.get('api'),image.get('abi'),image.get('channel'))!=(35,'arm64-v8a','stable'):
        raise ValueError('Mac factory requires Android35 default ARM64 image metadata')


def stock_identity(pins):
    sources={name+'.py':digest(Path(importlib.import_module(name).__file__)) for name in PRODUCERS}
    encoded=lambda value:hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':')).encode()).hexdigest()
    return {'policy_sha256':encoded(pins),'producer_sha256':encoded(sources),'producer_files':sources,
            'archives':{role:pin['sha256'] for role,pin in pins.items()},'redistribution':'review-pending'}


def reserve(folder,needed):
    folder=Path(folder)
    while not folder.exists():folder=folder.parent
    if shutil.disk_usage(folder).free<RESERVE+needed:
        raise ValueError('Insufficient free space for Mac SDK factory and recovery reserve')


def extraction_reserve(archives,folder):
    sizes={}
    for role,path in archives.items():
        with zipfile.ZipFile(path) as archive:sizes[role]=sum(entry.file_size for entry in archive.infolist())
    # Extracted SDK plus an uncompressed upper bound for the completed capsule.
    reserve(folder,sum(sizes.values())+sizes['emulator']+sizes['platform-tools']+32*1024**2)


def checked_properties(raw,expected):
    if len(raw)>32768:raise ValueError('Mac stock image metadata exceeds budget')
    rows={}
    for line in raw.decode('utf-8').splitlines():
        if not line.strip() or line.lstrip().startswith('#'):continue
        key,separator,value=line.partition('=');key=key.strip()
        if not separator or key in rows:raise ValueError('Mac stock image metadata is invalid')
        rows[key]=value.strip()
    if any(rows.get(key)!=value for key,value in expected.items()):
        raise ValueError('Mac stock image metadata differs from its stable pin')


def image_properties(raw,pin):
    checked_properties(raw,{'Pkg.Revision':pin['version'],'AndroidVersion.ApiLevel':'35',
                            'SystemImage.Abi':'arm64-v8a','SystemImage.TagId':'default'})


def archive_image(archive,pin):
    with zipfile.ZipFile(archive) as source:
        entry=source.getinfo('arm64-v8a/source.properties')
        if entry.file_size>32768:raise ValueError('Mac stock image metadata exceeds budget')
        image_properties(source.read(entry),pin)


def thin_arm64(header):
    endian={b'\xcf\xfa\xed\xfe':'<',b'\xfe\xed\xfa\xcf':'>'}.get(header[:4])
    return endian is not None and len(header)>=8 and struct.unpack(endian+'I',header[4:8])[0]==ARM64


def fat_arm64(source,header,size):
    layout={b'\xca\xfe\xba\xbe':('>',20),b'\xbe\xba\xfe\xca':('<',20),
            b'\xca\xfe\xba\xbf':('>',32),b'\xbf\xba\xfe\xca':('<',32)}.get(header[:4])
    if layout is None or len(header)!=8:return False
    endian,width=layout;count=struct.unpack(endian+'I',header[4:8])[0]
    if not 1<=count<=64:return False
    table=source.read(count*width)
    if len(table)!=count*width:return False
    return arm64_slice(source,table,endian,width,size)


def arm64_slice(source,table,endian,width,size):
    for start in range(0,len(table),width):
        cpu=struct.unpack_from(endian+'I',table,start)[0]
        if cpu!=ARM64:continue
        offset,length=struct.unpack_from(endian+('II' if width==20 else 'QQ'),table,start+8)
        if offset<8+len(table) or length<8 or offset+length>size:return False
        source.seek(offset);return thin_arm64(source.read(8))
    return False


def native_arm64(program):
    with Path(program).open('rb') as source:
        header=source.read(8)
        if not (thin_arm64(header) or fat_arm64(source,header,Path(program).stat().st_size)):
            raise ValueError('Mac stock program does not contain a native ARM64 Mach-O slice')


def sdk_programs(sdk):
    for role,program in [('emulator','emulator'),('platform-tools','adb')]:native_arm64(Path(sdk)/role/program)


def built_receipt(payload,pins,built):
    if not isinstance(built,dict) or built.get('stock')!=stock_identity(pins) or built.get('archive_sha256')!=digest(payload):
        raise ValueError('Mac SDK qualification requires its matching stock factory receipt')
