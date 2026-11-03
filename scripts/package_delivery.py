"""Maintainer-only handoff from an inspected installer to native delivery volumes."""
import json
from pathlib import Path
import re
import subprocess

from client_delivery import ASSET_LIMIT


def native(root,*arguments):
    result=subprocess.run(['cargo','run','--release','--locked','-p','ai-phone-host-runtime','--bin','package-volumes','--',
                           *map(str,arguments)],cwd=root,check=True,stdout=subprocess.PIPE,text=True)
    return json.loads(result.stdout)


def verify_volumes(root,manifest,identity,artifact):
    verified=native(root,'verify',manifest,identity)
    if verified.get('verified') is not True or verified['sha256']!=artifact['sha256'] or verified['size']!=artifact['size']:
        raise ValueError('Volume verification differs from the inspected installer')
    return verified


def prepare(root,artifact):
    root=Path(root)
    name=artifact['file']
    if Path(name).name!=name or not artifact['delivery']['complete_device_bundle']:
        raise ValueError('Delivery requires a complete inspected installer')
    source=root/'dist'/name
    if source.stat().st_size!=artifact['size']:
        raise ValueError('Installer size changed before delivery')
    if artifact['size']<ASSET_LIMIT:
        return {'format':'single','file':name,'size':artifact['size'],'sha256':artifact['sha256']}
    folder=root/'dist'/(name+'.volumes')
    sealed=native(root,'split',source,folder,artifact['sha256'])
    manifest=folder/sealed['manifest']
    verified=verify_volumes(root,manifest,sealed['manifest_sha256'],artifact)
    return {'format':'volumes','manifest':manifest.relative_to(root/'dist').as_posix(),
            'manifest_sha256':sealed['manifest_sha256'],'size':verified['size'],'sha256':verified['sha256'],
            'parts':json.loads(manifest.read_text())['parts']}


def file_record(root,path):
    from artifact_io import digest
    dist=Path(root)/'dist';path=Path(path)
    if path.is_symlink() or not path.is_file() or not path.resolve().is_relative_to(dist.resolve()):
        raise ValueError('Candidate file is outside the completed distribution')
    relative=path.relative_to(dist)
    if any(re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{0,190}',part) is None for part in relative.parts):
        raise ValueError('Invalid candidate asset path')
    size=path.stat().st_size
    if not 0<size<ASSET_LIMIT:raise ValueError('Candidate asset exceeds the release limit')
    return {'path':relative.as_posix(),'name':path.name,'size':size,'sha256':digest(path)}


def checked_artifact(root,report,source):
    from artifact_io import digest
    if report['format']!=1 or report['source_commit']!=source or report['binary_publication'] is not False:
        raise ValueError('Candidate source or publication state differs')
    if report['source_manifest_sha256']!=digest(Path(root)/'SOURCE-MANIFEST.json'):
        raise ValueError('Candidate source inventory changed')
    artifact=report['artifact'];name=artifact['file']
    if Path(name).name!=name or artifact['delivery']['complete_device_bundle'] is not True:
        raise ValueError('Candidate requires a complete inspected installer')
    return artifact


def metadata_path(root,artifact,source):
    path=Path(root)/'dist'/(artifact['file']+'.manifest.json')
    file_record(root,path)
    value=json.loads(path.read_text())
    if value['sha256']!=artifact['sha256'] or value['source_commit']!=source or value['complete_device_bundle'] is not True:
        raise ValueError('Candidate installer metadata differs')
    if value.get('local_bootstrap',{}).get('user_state_bundled') is not False:
        raise ValueError('Candidate may not contain user state')
    return path


def volume_records(root,package,artifact):
    manifest=Path(root)/'dist'/package['manifest']
    record=file_record(root,manifest)
    if record['sha256']!=package['manifest_sha256']:
        raise ValueError('Candidate volume manifest changed')
    verify_volumes(root,manifest,package['manifest_sha256'],artifact)
    value=json.loads(manifest.read_text())
    records=[record]
    for part in value['parts']:
        record=file_record(root,manifest.parent/part['name'])
        if record['size']!=part['size'] or record['sha256']!=part['sha256']:
            raise ValueError('Candidate volume changed after verification')
        records.append(record)
    return records


def installer_records(root,package,artifact):
    if package['format']=='volumes':return volume_records(root,package,artifact)
    if package['format']!='single':raise ValueError('Unsupported candidate package format')
    path=Path(root)/'dist'/artifact['file'];record=file_record(root,path)
    if record['sha256']!=artifact['sha256'] or record['size']!=artifact['size']:
        raise ValueError('Candidate installer changed')
    return [record]


def upload_plan(root,report,source):
    artifact=checked_artifact(root,report,source)
    metadata=metadata_path(root,artifact,source)
    records=installer_records(root,report['package_delivery'],artifact)
    records.extend(file_record(root,path) for path in [metadata,Path(root)/'dist'/(artifact['file']+'.sha256')])
    if len({row['name'] for row in records})!=len(records):raise ValueError('Duplicate candidate asset name')
    return {'format':1,'source_commit':source,'host':report['host'],'binary_publication':False,'files':records}


def matching_asset(record,asset):
    return asset.get('state')=='uploaded' and asset.get('size')==record['size'] and asset.get('digest')=='sha256:'+record['sha256']


def verify_uploaded(plan,release):
    if release.get('draft') is not True or release.get('published_at') is not None or release.get('target_commitish')!=plan['source_commit']:
        raise ValueError('Candidate is not an unpublished source-matched draft')
    assets={row['name']:row for row in release['assets']}
    if len(assets)!=len(release['assets']) or set(assets)!={row['name'] for row in plan['files']}:
        raise ValueError('Candidate remote asset inventory differs')
    for row in plan['files']:
        if not matching_asset(row,assets[row['name']]):raise ValueError('Candidate remote asset digest or size differs')
    return True


if __name__=='__main__':
    import argparse
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('operation',choices=['plan','verify-upload'])
    parser.add_argument('input',type=Path)
    parser.add_argument('--release',type=Path)
    args=parser.parse_args();root=Path(__file__).resolve().parent.parent
    value=json.loads(args.input.read_text())
    if args.operation=='plan':
        source=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
        result=upload_plan(root,value,source)
    else:
        if args.release is None:parser.error('--release is required for remote verification')
        result={'verified':verify_uploaded(value,json.loads(args.release.read_text()))}
    print(json.dumps(result))
