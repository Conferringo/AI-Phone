"""Reviewed portable Python/Node resources, independent of user-installed tools."""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import tarfile
import tempfile
import zipfile
from artifact_io import digest
from client_host_entry import canonical_name,checked_record,verify_file
from host_platform import host_key
from host_filesystem import windows_path,is_alias,permissions_match,file_mode

ROLES=('python','node')
MAX_FILES=12000
MAX_BYTES=512*1024**2
RESERVE=4*1024**3


def tool_path(name,host=None):
    path=canonical_name(name)
    if not 2<=len(path.parts)<=64 or path.parts[0] not in ROLES:
        raise ValueError('Invalid installer tool path')
    if (host or host_key())=='windows-x64':windows_path(path)
    return path


def checked_manifest(value, host):
    if not isinstance(value,dict) or type(value.get('format')) is not int or value['format']!=1 or value.get('host')!=host:
        raise ValueError('Installer tools do not match this host')
    records=inventory_records(value.get('files'),host);tools=value.get('tools')
    if not isinstance(tools,dict) or set(tools)!=set(ROLES):raise ValueError('Incomplete installer tools')
    for role in ROLES:checked_tool(role,tools[role],records)
    return value


def inventory_records(rows,host=None):
    if not isinstance(rows,list) or not 1<=len(rows)<=MAX_FILES:raise ValueError('Invalid installer file inventory')
    records={};total=0;names=set();windows=(host or host_key())=='windows-x64'
    for row in map(checked_record,rows):
        name=tool_path(row['path'],host).as_posix();total+=row['size'];identity=name.casefold() if windows else name
        if identity in names or total>MAX_BYTES:
            raise ValueError('Installer tools exceed their inventory budget')
        names.add(identity)
        records[name]=row
    return records


def checked_tool(role,tool,records):
    if not isinstance(tool,dict) or not isinstance(tool.get('version'),str) or re.fullmatch(r'\d+\.\d+\.\d+',tool['version']) is None:
        raise ValueError('Invalid installer tool version')
    entry=tool.get('executable')
    if not isinstance(entry,str) or entry not in records or tool_path(entry).parts[0]!=role or records[entry]['mode']!=0o755:
        raise ValueError('Installer tool executable is missing')


def read_manifest(root,host=None):
    path=Path(root)/'manifest.json'
    if is_alias(path) or path.stat().st_size>4*1024**2:
        raise ValueError('Invalid installer tool manifest')
    return checked_manifest(json.loads(path.read_text()),host or host_key())


def verify(root,host=None):
    root=Path(root)
    if is_alias(root):
        raise ValueError('Installer tools cannot be a symlink')
    manifest=read_manifest(root,host);expected={row['path'] for row in manifest['files']}|{'manifest.json'}
    if tree_inventory(root)!=expected:
        raise ValueError('Installer tools contain unlisted or missing files')
    for row in manifest['files']:
        verify_file(root,row)
        if not permissions_match(root/row['path'],row['mode']):
            raise ValueError('Installer tool permissions changed')
    return manifest


def tree_inventory(root):
    actual=set()
    for count,path in enumerate(root.rglob('*')):
        if count>=MAX_FILES*2 or is_alias(path):
            raise ValueError('Invalid installer tool tree')
        if path.is_file():actual.add(path.relative_to(root).as_posix())
        elif not path.is_dir():raise ValueError('Invalid installer tool member')
    return actual


def environment(component,manifest,base_path=None):
    root=Path(component).resolve();tools=manifest['tools'];paths=[str((root/tools[role]['executable']).parent) for role in ROLES]
    return {'AI_PHONE_TOOL_RUNTIME':str(root),'AI_PHONE_PYTHON':str(root/tools['python']['executable']),
            'PYTHONDONTWRITEBYTECODE':'1','PATH':os.pathsep.join([*paths,os.environ.get('PATH','') if base_path is None else base_path])}


def select(root,component):
    from database import atomic_json
    component=Path(component).resolve();manifest=verify(component)
    atomic_json(Path(root)/'data/deployment/tool-runtime.json',{'format':1,'path':str(component),'sha256':digest(component/'manifest.json')})
    return environment(component,manifest)


def selected_environment(root,base_path=None):
    path=Path(root)/'data/deployment/tool-runtime.json'
    if not path.exists():return {}
    value=json.loads(path.read_text())
    if not isinstance(value,dict) or type(value.get('format')) is not int or value['format']!=1 or not isinstance(value.get('path'),str) or not Path(value['path']).is_absolute():
        raise ValueError('Invalid installer tool selection')
    component=Path(value['path']);manifest=verify(component)
    if digest(component/'manifest.json')!=value.get('sha256'):
        raise ValueError('Installer tool selection changed')
    return environment(component,manifest,base_path)


def prepare_resources(folder,component):
    if component is None:return {}
    component=Path(component).resolve();manifest=verify(component);target=Path(folder)/'tools';identity=digest(component/'manifest.json')
    if target.exists():
        verify(target)
        if digest(target/'manifest.json')!=identity:raise ValueError('Generated installer tools differ; regenerate the owned tools directory')
    else:
        size=sum(row['size'] for row in manifest['files'])
        if shutil.disk_usage(folder).free<RESERVE+size:raise ValueError('Keep 4 GiB free while preparing installer tools')
        target.mkdir()
        try:shutil.copytree(component,target,dirs_exist_ok=True);verify(target)
        except BaseException:
            if target.exists():shutil.rmtree(target)
            raise
    return {'tools/manifest.json':identity}


def archive_inputs(archives,pins):
    if set(pins)!=set(ROLES):raise ValueError('Both Python and Node archives are required')
    result={}
    for role in ROLES:
        record=pins[role];path=Path(archives)/record['file']
        if Path(record['file']).name!=record['file'] or path.is_symlink() or path.stat().st_size!=record['size_bytes'] or digest(path)!=record['sha256']:
            raise ValueError('Installer archive checksum or size mismatch')
        result[role]=path
    return result


def entry_name(entry,role,prefix,host=None):
    path=canonical_name(entry.name.rstrip('/'))
    if path.parts[0]!=prefix or (role=='python' and prefix!='python'):
        raise ValueError('Unexpected installer archive root')
    name=Path(role,*path.parts[1:])
    if len(name.parts)>1:tool_path(name.as_posix(),host)
    elif not entry.isdir():raise ValueError('Installer archive root must be a directory')
    return name


def bounded_entries(source,role,host=None):
    seen=set();total=0;prefix=None;windows=(host or host_key())=='windows-x64'
    for count,entry in enumerate(source):
        if count>=MAX_FILES or entry.size<0 or entry.size>MAX_BYTES:
            raise ValueError('Installer archive budget exceeded')
        prefix=prefix or entry.name.split('/')[0];name=entry_name(entry,role,prefix,host);total+=entry.size
        identity=name.as_posix().casefold() if windows else name.as_posix()
        if identity in seen or total>MAX_BYTES:raise ValueError('Duplicate or oversized installer archive')
        seen.add(identity);yield entry,name


def unpack(archive,destination,role,*,host=None,version=None):
    host=host or host_key()
    if role=='node' and host=='windows-x64':return unpack_windows_node(archive,destination,version)
    aliases={}
    with tarfile.open(archive,'r:gz') as source:
        for entry,name in bounded_entries(source,role,host):
            if entry.isdir():continue
            if role=='node' and name.as_posix() not in ('node/bin/node','node/LICENSE','node/README.md'):continue
            target=destination/name;target.parent.mkdir(parents=True,exist_ok=True)
            if entry.issym():aliases[name]=entry.linkname
            elif entry.isfile():
                with source.extractfile(entry) as body,target.open('xb') as output:shutil.copyfileobj(body,output,1024**2)
                target.chmod(file_mode(target,host,entry.mode))
            else:raise ValueError('Invalid installer archive member')
    materialize_aliases(destination,role,aliases)


def unpack_windows_node(archive,destination,version):
    prefix='node-v'+version+'-win-x64/';total=0
    with zipfile.ZipFile(archive) as source:
        names=source.namelist()
        for name in ('node.exe','LICENSE','README.md'):
            if names.count(prefix+name)!=1:raise ValueError('Missing or duplicate Windows Node resource')
            entry=source.getinfo(prefix+name);total+=entry.file_size
            if not 0<entry.file_size<=MAX_BYTES or total>MAX_BYTES or entry.external_attr>>16&0o170000 not in (0,0o100000):
                raise ValueError('Invalid Windows Node resource')
            target=destination/'node'/name;target.parent.mkdir(parents=True,exist_ok=True)
            with source.open(entry) as incoming,target.open('xb') as output:shutil.copyfileobj(incoming,output,1024**2)
            target.chmod(file_mode(target,'windows-x64'))


def materialize_aliases(root,role,aliases):
    pending=dict(aliases)
    while pending:
        progress=False
        for name,value in list(pending.items()):
            target=(root/name.parent/value).resolve()
            if not target.is_relative_to((root/role).resolve()):raise ValueError('Installer alias escapes its component')
            if not target.is_file():continue
            shutil.copyfile(target,root/name);(root/name).chmod(target.stat().st_mode&0o777);del pending[name];progress=True
        if not progress:raise ValueError('Installer alias is missing or cyclic')


def file_records(folder,host=None):
    return [{'path':p.relative_to(folder).as_posix(),'size':p.stat().st_size,'sha256':digest(p),'mode':file_mode(p,host or host_key())}
            for p in sorted(folder.rglob('*')) if p.is_file()]


def build(archives,output,*,pins=None,host=None):
    host=host or host_key()
    if pins is None:
        config=Path(__file__).resolve().parent.parent/'config/installer-tools.json';pins=json.loads(config.read_text())['hosts'][host]
    inputs=archive_inputs(archives,pins);output=Path(output)
    if output.exists():raise ValueError('Installer tool destination must be new')
    output.parent.mkdir(parents=True,exist_ok=True)
    if shutil.disk_usage(output.parent).free<RESERVE+MAX_BYTES:raise ValueError('Keep 4 GiB free while building installer tools')
    with tempfile.TemporaryDirectory(prefix='.tool-build-',dir=output.parent) as temporary:
        work=Path(temporary)/'tools';work.mkdir()
        for role,path in inputs.items():unpack(path,work,role,host=host,version=pins[role]['version'])
        programs={'python':'python/python.exe','node':'node/node.exe'} if host=='windows-x64' else {'python':'python/bin/python3','node':'node/bin/node'}
        tools={role:{'version':pins[role]['version'],'executable':programs[role],
                     'source':{'url':pins[role]['url'],'sha256':pins[role]['sha256']}} for role in ROLES}
        manifest={'format':1,'host':host,'tools':tools,'files':file_records(work,host)}
        (work/'manifest.json').write_text(json.dumps(manifest,sort_keys=True)+'\n');verify(work,host);work.rename(output)
    return {'host':host,'manifest_sha256':digest(output/'manifest.json'),'files':len(manifest['files']),'bytes':sum(row['size'] for row in manifest['files'])}


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--archives',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args()
    print(json.dumps(build(args.archives,args.output)))
