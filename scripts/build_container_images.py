"""Export reviewed images into one streamable capsule without exporting containers."""
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
from artifact_io import digest
from build_capsule_codec import write_xz, read_tar, decoded
from container_images import MANIFEST,XZ_ARCHIVE,ROLES,RESERVE,MAX_ARCHIVE,MAX_RAW,checked_image,daemon_architecture,docker_command,image_ref,inventory,stage,image_pipe
from oci_layer_transform import transform


class ArchiveWriter:
    def __init__(self, file, folder):
        self.file=file;self.folder=folder;self.since_check=0

    def write(self, value):
        self.since_check+=len(value)
        if self.since_check>=32*1024**2:
            self.since_check=0
            if shutil.disk_usage(self.folder).free<RESERVE:raise ValueError('Keep 4 GiB free while exporting images')
        return self.file.write(value)


class RawArchiveWriter:
    def __init__(self, target):
        self.target=target;self.written=0

    def write(self, value):
        self.written+=len(value)
        if self.written>MAX_RAW:raise ValueError('Raw image archive exceeds budget')
        return self.target.write(value)


def save_images(refs, raw, architecture):
    with tempfile.TemporaryFile() as log:
        child=subprocess.Popen(['docker','image','save','--platform','linux/'+architecture,*refs],stdout=subprocess.PIPE,stderr=log)
        with image_pipe(child,child.stdout,close_first=True):
            with write_xz(ArchiveWriter(raw,Path(raw.name).parent)) as target:
                shutil.copyfileobj(child.stdout,target,1024**2)
            if child.wait()!=0:raise ValueError('Docker image export failed')


def bound_records(records, actual, architecture):
    if set(actual)!={row['ref'] for row in records.values()}:
        raise ValueError('Exported image references differ from inspected images')
    bound={}
    for role,row in records.items():
        entry=actual[row['ref']]
        if entry['architecture']!=architecture or entry['export_digest']!=row['export_digest'] or row.get('config_digest',entry['config_digest'])!=entry['config_digest']:
            raise ValueError('Exported image identity differs from inspected images')
        bound[role]=checked_image(role,{**row,'config_digest':entry['config_digest']})
    return bound


def transformed_images(records, raw, architecture):
    folder=Path(raw.name).parent
    with (folder/'docker-export.tar.xz').open('w+b') as source:
        save_images([row['ref'] for row in records.values()],source,architecture)
        source.flush();source.seek(0)
        records=bound_records(records,inventory(source,'xz'),architecture)
        source.seek(0)
        with read_tar(source,MAX_RAW,stream=True) as exported:
            oci=any(entry.name=='oci-layout' for entry in exported)
        source.seek(0)
        with write_xz(ArchiveWriter(raw,folder)) as target:
            writer=RawArchiveWriter(target)
            if oci:
                result=transform(source,writer,records)
            else:
                with decoded(source,MAX_RAW,expected='xz') as original:
                    shutil.copyfileobj(original,writer,1024**2)
                result={'architecture':architecture,'images':records}
    if result['architecture']!=architecture:raise ValueError('Converted image architecture mismatch')
    return result['images']


def source_records(sources, architecture):
    records={}
    for role in ROLES:
        result=docker_command(['image','inspect',sources[role]])
        if result is None:
            raise ValueError('Missing source image: '+role)
        image=json.loads(result)[0]
        if image['Architecture']!=architecture or image['Os']!='linux':
            raise ValueError('Source image architecture mismatch')
        source=image['Id'];selected=docker_command(['image','inspect',source,'--platform','linux/'+architecture])
        if selected is None:
            raise ValueError('Image export requires platform-aware Docker inspection')
        primary=json.loads(selected)[0]['Id'];ref=image_ref(role,primary);found=docker_command(['image','inspect',ref,'--platform','linux/'+architecture])
        if found is not None and json.loads(found)[0]['Id']!=primary:
            raise ValueError('Bundle image tag conflict')
        if found is None:
            subprocess.run(['docker','tag',source,ref],check=True)
        records[role]={'ref':ref,'source_digest':source,'export_digest':primary,'size':image['Size']}
    return records


def header(name, size):
    entry=tarfile.TarInfo(name);entry.size=size;entry.mode=0o644;entry.mtime=0
    return entry.tobuf()


def finish_metadata(path, records, architecture, size):
    with tarfile.open(path,'r:') as source,source.extractfile(XZ_ARCHIVE) as body:
        import hashlib
        identity=hashlib.file_digest(body,'sha256').hexdigest()
    with tarfile.open(path,'r:') as source,source.extractfile(XZ_ARCHIVE) as body:actual=inventory(body)
    records=bound_records(records,actual,architecture)
    return {'format':2,'os':'linux','architecture':architecture,'redistribution':'review-pending','images':records,'archive':{'path':XZ_ARCHIVE,'codec':'xz','size':size,'sha256':identity}}


def build(sources, output):
    if not isinstance(sources,dict) or set(sources)!=set(ROLES):raise ValueError('Provide the four image roles')
    output=Path(output);output.parent.mkdir(parents=True,exist_ok=True)
    if shutil.disk_usage(output.parent).free<RESERVE:raise ValueError('Keep 4 GiB free before exporting images')
    arch=daemon_architecture(docker_command);records=source_records(sources,arch)
    with tempfile.TemporaryDirectory(prefix='.images-build-',dir=output.parent) as temp:
        archive=Path(temp)/'images.payload'
        with archive.open('w+b') as raw:
            raw.write(b'\0'*512);records=transformed_images(records,raw,arch);size=raw.tell()-512
            if size>MAX_ARCHIVE:raise ValueError('Image capsule exceeds budget')
            raw.seek(0);raw.write(header(XZ_ARCHIVE,size));raw.seek(0,2);raw.write(b'\0'*(-size%512));raw.flush()
            manifest=finish_metadata(archive,records,arch,size);data=json.dumps(manifest,sort_keys=True).encode()
            raw.write(header(MANIFEST,len(data)));raw.write(data);raw.write(b'\0'*(-len(data)%512));raw.write(b'\0'*1024)
        stage(archive);archive.replace(output)
    return {'path':str(output),'sha256':digest(output),'architecture':arch,'bytes':output.stat().st_size,'images':records,'redistribution':'review-pending'}


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--sources',required=True,type=Path);parser.add_argument('--output',required=True,type=Path);args=parser.parse_args()
    print(json.dumps(build(json.loads(args.sources.read_text()),args.output)))
