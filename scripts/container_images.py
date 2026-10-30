"""Sealed, architecture-specific Linux images; imports never reuse user aliases.

The capsule holds one compressed Docker archive and a trailing manifest. It is
validated and streamed straight into Docker, without a second image-sized copy.
No running container or data volume is ever exported, stopped or replaced.
"""
from contextlib import contextmanager,nullcontext
from dataclasses import dataclass
import hashlib
import json
import os
from pathlib import Path,PurePosixPath
import re
import shutil
import subprocess
import tarfile
import tempfile
from artifact_io import digest

MANIFEST='IMAGES-MANIFEST.json'
ARCHIVE='images.tar.gz'
ROLES=('workspace','desktop','postgres','redis')
MAX_ARCHIVE=8*1024**3
MAX_METADATA=8*1024**2
RESERVE=4*1024**3


def sha(value):
    if not isinstance(value,str) or re.fullmatch('sha256:[0-9a-f]{64}',value) is None:
        raise ValueError('Invalid image content identity')
    return value


def image_ref(role,identity):
    return 'ai-phone-bundle/'+role+':sha-'+sha(identity)[7:]


def checked_image(role,record):
    if not isinstance(record,dict):
        raise ValueError('Invalid image record')
    for key in ('source_digest','export_digest','config_digest'):sha(record.get(key))
    if record.get('ref')!=image_ref(role,record['export_digest']):
        raise ValueError('Foreign image reference')
    if type(record.get('size')) is not int or not 0<record['size']<=32*1024**3:
        raise ValueError('Invalid image size')
    return record


def checked_archive(archive):
    if not isinstance(archive,dict) or archive.get('path')!=ARCHIVE:
        raise ValueError('Invalid image archive')
    if type(archive.get('size')) is not int or not 0<archive['size']<=MAX_ARCHIVE:
        raise ValueError('Invalid image archive size')
    sha('sha256:'+str(archive.get('sha256')))


def checked_images(images):
    if not isinstance(images,dict) or set(images)!=set(ROLES):
        raise ValueError('Incomplete image roles')
    for role in ROLES:checked_image(role,images[role])


def checked_manifest(value):
    if not isinstance(value,dict) or type(value.get('format')) is not int or value['format']!=1:
        raise ValueError('Invalid image manifest')
    if value.get('os')!='linux' or value.get('architecture') not in ('arm64','amd64') or value.get('redistribution')!='review-pending':
        raise ValueError('Unsupported image runtime')
    checked_images(value.get('images'))
    checked_archive(value.get('archive'))
    return value


@dataclass(frozen=True)
class PreparedImages:
    payload: Path
    manifest: dict
    identity: str

    @contextmanager
    def archive(self, payload_file=None):
        with (self.payload.open('rb') if payload_file is None else nullcontext(payload_file)) as file:
            file.seek(0)
            with tarfile.open(fileobj=file,mode='r:') as source:
                with source.extractfile(ARCHIVE) as body:yield body


def outer_manifest(source):
    entries={}
    for entry in source:
        if entry.name not in (ARCHIVE,MANIFEST) or entry.name in entries or not entry.isfile():
            raise ValueError('Invalid image capsule members')
        limit=MAX_METADATA if entry.name==MANIFEST else MAX_ARCHIVE
        if not 0<entry.size<=limit:
            raise ValueError('Image capsule exceeds budget')
        entries[entry.name]=entry
    if set(entries)!={ARCHIVE,MANIFEST}:
        raise ValueError('Incomplete image capsule')
    value=checked_manifest(json.load(source.extractfile(MANIFEST)))
    if entries[ARCHIVE].size!=value['archive']['size']:
        raise ValueError('Image archive integrity: size')
    return value


def stage(payload):
    payload=Path(payload).resolve()
    try:
        with tarfile.open(payload,'r:') as source:
            manifest=outer_manifest(source)
            with source.extractfile(ARCHIVE) as body:actual=hashlib.file_digest(body,'sha256').hexdigest()
            if actual!=manifest['archive']['sha256']:
                raise ValueError('Image archive integrity: digest')
            with source.extractfile(ARCHIVE) as body:inspect_archive(body,manifest)
    except tarfile.TarError as error:raise ValueError('Invalid image capsule integrity') from error
    return PreparedImages(payload,manifest,digest(payload))


def archive_path(name):
    if not isinstance(name,str) or any(ord(c)<32 for c in name):
        raise ValueError('Invalid Docker archive path')
    path=PurePosixPath(name)
    if path.is_absolute() or path.as_posix()!=name or '..' in path.parts or '\\' in name:
        raise ValueError('Invalid Docker archive path')
    return path


def read_object(archive, entry):
    with archive.extractfile(entry) as body:
        if entry.size<=MAX_METADATA:
            data=body.read();identity=hashlib.sha256(data).hexdigest()
            try:value=json.loads(data)
            except (UnicodeDecodeError,ValueError):value=None
        else:identity=hashlib.file_digest(body,'sha256').hexdigest();value=None
    if entry.name.startswith('blobs/sha256/') or re.fullmatch(r'[0-9a-f]{64}\.json',entry.name):
        if Path(entry.name).name.removesuffix('.json')!=identity:
            raise ValueError('Docker object integrity mismatch')
    return value


def checked_docker_file(entry, files):
    if not entry.isfile() or entry.name in files or entry.size<0:
        raise ValueError('Invalid Docker archive member')
    if entry.size>MAX_ARCHIVE:
        raise ValueError('Docker archive exceeds budget')
    if entry.name=='repositories':
        raise ValueError('Legacy Docker aliases are not accepted')


def archive_metadata(source):
    objects={};files=set();total=0;metadata=0
    with tarfile.open(fileobj=source,mode='r|gz') as archive:
        for position,entry in enumerate(archive):
            if position>=20000:
                raise ValueError('Docker archive exceeds entry budget')
            archive_path(entry.name)
            if entry.isdir():continue
            checked_docker_file(entry,files);total+=entry.size
            if total>MAX_ARCHIVE:
                raise ValueError('Docker archive exceeds budget')
            files.add(entry.name)
            value=read_object(archive,entry)
            if value is not None:
                metadata+=entry.size
                if metadata>MAX_METADATA:
                    raise ValueError('Docker archive exceeds metadata budget')
                objects[entry.name]=value
    return objects,files


def config_entry(row, objects, files):
    if not isinstance(row,dict):
        raise ValueError('Invalid Docker image row')
    config_path=archive_path(row.get('Config')).as_posix();config=objects.get(config_path)
    if not isinstance(config,dict) or config.get('os')!='linux':
        raise ValueError('Invalid Docker image config')
    identity=sha('sha256:'+Path(config_path).name.removesuffix('.json'))
    layers=row.get('Layers')
    if not isinstance(layers,list) or not all(archive_path(name).as_posix() in files for name in layers):
        raise ValueError('Missing Docker layers')
    refs=row.get('RepoTags')
    if not isinstance(refs,list) or not all(isinstance(ref,str) for ref in refs):
        raise ValueError('Invalid Docker image aliases')
    return {'config_digest':identity,'architecture':config.get('architecture'),'export_digest':identity},refs


def named_descriptor(descriptor, objects, found):
    if not isinstance(descriptor,dict):
        raise ValueError('Invalid OCI descriptor')
    annotations=descriptor.get('annotations',{})
    ref=annotations.get('io.containerd.image.name','').removeprefix('docker.io/')
    if not ref:return annotations
    if ref not in found:
        raise ValueError('Unlisted OCI image reference')
    identity=sha(descriptor.get('digest'));graph=objects.get('blobs/sha256/'+identity[7:])
    if not isinstance(graph,dict) or graph.get('config',{}).get('digest')!=found[ref]['config_digest']:
        raise ValueError('OCI manifest/config binding mismatch')
    found[ref]['export_digest']=identity
    return None


def bind_indexes(indexes, objects, found):
    if not isinstance(indexes,dict) or indexes.get('schemaVersion')!=2 or not isinstance(indexes.get('manifests'),list):
        raise ValueError('Invalid OCI index')
    attestations=[]
    for descriptor in indexes['manifests']:
        item=named_descriptor(descriptor,objects,found)
        if item is not None:attestations.append(item)
    exported={record['export_digest'] for record in found.values()}
    for annotations in attestations:
        if annotations.get('io.containerd.manifest.subject') not in exported or annotations.get('org.opencontainers.image.ref.name'):
            raise ValueError('Unlisted OCI artifact')


def inventory(source):
    objects,files=archive_metadata(source);rows=objects.get('manifest.json')
    if not isinstance(rows,list) or not rows or len(rows)>len(ROLES):
        raise ValueError('Invalid Docker image inventory')
    found={}
    for row in rows:
        config,refs=config_entry(row,objects,files)
        for ref in refs:
            if ref in found:
                raise ValueError('Duplicate Docker image reference')
            found[ref]=dict(config)
    if 'index.json' in objects:bind_indexes(objects['index.json'],objects,found)
    return found


def inspect_archive(source, manifest):
    actual=inventory(source);expected=manifest['images']
    if set(actual)!={row['ref'] for row in expected.values()}:
        raise ValueError('Docker archive references differ from the manifest')
    for record in expected.values():
        entry=actual[record['ref']]
        if entry['config_digest']!=record['config_digest'] or entry['export_digest']!=record['export_digest'] or entry['architecture']!=manifest['architecture']:
            raise ValueError('Docker archive image identity mismatch')
    return actual


def stop_child(child):
    if child.poll() is not None:return
    child.terminate()
    try:child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        child.kill();child.wait()


def docker_command(args, source=None):
    if source is None:
        result=subprocess.run(['docker',*args],capture_output=True,text=True,check=False)
        return result.stdout if result.returncode==0 else None
    with tempfile.TemporaryFile() as log:
        child=subprocess.Popen(['docker',*args],stdin=subprocess.PIPE,stdout=log,stderr=log)
        try:
            shutil.copyfileobj(source,child.stdin,1024**2);child.stdin.close()
            if child.wait()!=0:
                raise ValueError('Bundled image import did not complete; retry initialization')
        finally:
            stop_child(child)
            if not child.stdin.closed:child.stdin.close()
    return ''


def daemon_architecture(docker):
    value=docker(['info','--format','{{.Architecture}}'])
    aliases={'arm64':'arm64','aarch64':'arm64','amd64':'amd64','x86_64':'amd64'}
    if value is None or value.strip() not in aliases:
        raise ValueError('Supported Docker engine unavailable')
    return aliases[value.strip()]


def present(record,architecture,docker):
    result=docker(['image','inspect',record['ref']])
    if result is None:return False
    rows=json.loads(result)
    if not isinstance(rows,list) or len(rows)!=1:
        raise ValueError('Invalid Docker image response')
    row=rows[0]
    if row.get('Descriptor',{}).get('mediaType')=='application/vnd.oci.image.index.v1+json':
        selected=docker(['image','inspect',record['ref'],'--platform','linux/'+architecture])
        if selected is None:
            raise ValueError('Cannot verify the selected image platform')
        row=json.loads(selected)[0]
    if row.get('Id') not in {record['export_digest'],record['config_digest']} or row.get('Architecture')!=architecture or row.get('Os')!='linux':
        raise ValueError('Existing image reference conflict; no image was overwritten')
    return True


def atomic_selection(root, manifest):
    folder=Path(root)/'data/deployment';folder.mkdir(parents=True,exist_ok=True,mode=0o700)
    with tempfile.NamedTemporaryFile(mode='w',dir=folder,delete=False) as target:
        path=Path(target.name);json.dump(manifest,target);target.flush();os.fsync(target.fileno())
    try:
        os.replace(path,folder/'image-runtime.json')
        directory=os.open(folder,os.O_RDONLY)
        try:os.fsync(directory)
        finally:os.close(directory)
    finally:path.unlink(missing_ok=True)


def activate(root, prepared, docker=docker_command):
    with prepared.payload.open('rb') as file:
        before=os.fstat(file.fileno())
        if hashlib.file_digest(file,'sha256').hexdigest()!=prepared.identity:
            raise ValueError('Image capsule changed after validation')
        manifest=prepared.manifest;arch=daemon_architecture(docker)
        if arch!=manifest['architecture']:
            raise ValueError('Docker engine architecture differs from bundled images')
        ready=[present(row,arch,docker) for row in manifest['images'].values()]
        if not all(ready):
            if shutil.disk_usage(root).free<RESERVE:
                raise ValueError('Keep 4 GiB free before importing images')
            after=os.fstat(file.fileno())
            if (before.st_size,before.st_mtime_ns)!=(after.st_size,after.st_mtime_ns):
                raise ValueError('Image capsule changed during initialization')
            with prepared.archive(file) as source:docker(['image','load','--quiet'],source)
        if not all(present(row,arch,docker) for row in manifest['images'].values()):
            raise ValueError('Bundled images are not ready; initialization can be retried')
    atomic_selection(root,manifest)
    return image_environment(root)


def image_environment(root):
    record=Path(root)/'data/deployment/image-runtime.json'
    if not record.exists():return {}
    manifest=checked_manifest(json.loads(record.read_text()))
    return {**{'AI_PHONE_'+role.upper()+'_IMAGE':manifest['images'][role]['ref'] for role in ROLES},'AI_PHONE_IMAGE_PULL_POLICY':'never'}
