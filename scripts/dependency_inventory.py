"""Reproducible upstream licence metadata; no host paths or user files in output."""
import hashlib
import json
from pathlib import Path
import subprocess
import os
import argparse
import tomllib

ROOT=Path(__file__).resolve().parent.parent

def checksum(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def notices(folder):
    items=[]
    for path in sorted(folder.iterdir()):
        if path.is_file() and path.name.upper().startswith(('LICENSE','LICENCE','COPYING','NOTICE','COPYRIGHT')):
            items.append({'file':path.name,'sha256':checksum(path)})
    return items

def supplemental_notices(root, package, folder):
    config = root/'config/native-notice-supplements.json'
    if not config.exists(): return []
    key = package['name']+'@'+package['version']
    match = next((row for row in json.loads(config.read_text())['packages'] if row['package']==key), None)
    if not match: return []
    if package.get('repository')!=match['repository']:
        raise ValueError('Supplemental notice repository mismatch: '+key)
    if match.get('commit'):
        vcs = json.loads((folder/'.cargo_vcs_info.json').read_text())
        if vcs['git']['sha1']!=match['commit']:
            raise ValueError('Supplemental notice source mismatch: '+key)
    else:
        locked=tomllib.loads((root/'apps/client/src-tauri/Cargo.lock').read_text())['package']
        archive=next((p.get('checksum') for p in locked if p['name']==package['name'] and p['version']==package['version']),None)
        if not archive or archive!=match.get('archive_sha256'):
            raise ValueError('Supplemental notice archive mismatch: '+key)
        for item in match['grant_files']:
            path=(folder/item['file']).resolve()
            if not path.is_relative_to(folder.resolve()) or checksum(path)!=item['sha256']:
                raise ValueError('Supplemental notice grant mismatch: '+key)
    paths=[]
    for item in match['files']:
        path=(root/item['file']).resolve()
        if not path.is_relative_to((root/'docs/public/licenses/native').resolve()) or checksum(path)!=item['sha256']:
            raise ValueError('Supplemental notice hash/path mismatch: '+key)
        paths.append((path,item['url']))
    return paths

def inventory(root=ROOT):
    rust_packages={};locks={'Cargo.lock':checksum(root/'Cargo.lock'),'bun.lock':checksum(root/'bun.lock')}
    manifests=['Cargo.toml','apps/client/src-tauri/Cargo.toml']
    for manifest in manifests:
        if not (root/manifest).exists():continue
        metadata=json.loads(subprocess.check_output(['cargo','metadata','--manifest-path',str(root/manifest),'--locked','--format-version','1'],cwd=root))
        workspace=set(metadata['workspace_members'])
        lock=Path(manifest).with_name('Cargo.lock');locks[str(lock)]=checksum(root/lock)
        for package in metadata['packages']:
            if package['id'] in workspace:continue
            key=(package['name'],package['version'])
            if key not in rust_packages:
                rust_packages[key]={'name':package['name'],'version':package['version'],'license':package['license'],'repository':package['repository'],'notices':notices(Path(package['manifest_path']).parent),'workspaces':[]}
            rust_packages[key]['workspaces'].append(manifest)
    rust=list(rust_packages.values())
    npm={}
    for base,dirs,files in os.walk(root/'node_modules',followlinks=False):
        if 'package.json' not in files:continue
        path=Path(base)/'package.json'
        try:value=json.loads(path.read_text())
        except (ValueError,OSError):continue
        if not value.get('name') or not value.get('version') or value.get('private'):continue
        key=(value['name'],value['version']);repo=value.get('repository');license=value.get('license')
        if isinstance(repo,dict):repo=repo.get('url')
        if isinstance(license,dict):license=license.get('type')
        npm[key]={'name':key[0],'version':key[1],'license':license,'repository':repo,'notices':notices(path.parent)}
    node=sorted(npm.values(),key=lambda x:(x['name'],x['version']));rust.sort(key=lambda x:(x['name'],x['version']))
    return {'format':1,'kind':'dependency-license-metadata','scope':'locked-source-build-dependencies; server and native client installed metadata, not a redistribution clearance','locks':locks,'rust':rust,'javascript':node,'missing_license_metadata':{'rust':[x['name']+'@'+x['version'] for x in rust if not x['license']],'javascript':[x['name']+'@'+x['version'] for x in node if not x['license']]}}

def client_notices(root=ROOT):
    manifest=root/'apps/client/src-tauri/Cargo.toml'
    metadata=json.loads(subprocess.check_output(['cargo','metadata','--manifest-path',str(manifest),'--locked','--format-version','1'],cwd=root))
    owned=set(metadata['workspace_members']);blocks=['AI Phone native client dependency notices\n\nExact locked build graph, including platform/build dependencies. Component licences remain independent. This collection is not a final redistribution clearance.\n'];missing=[]
    for package in sorted(metadata['packages'],key=lambda p:(p['name'],p['version'])):
        if package['id'] in owned:continue
        folder=Path(package['manifest_path']).parent
        paths=[folder/item['file'] for item in notices(folder)]
        if package.get('license_file'):
            path=Path(package['license_file']);path=path if path.is_absolute() else folder/path
            if path.is_file() and path not in paths:paths.append(path)
        supplement=supplemental_notices(root,package,folder)
        if not paths and not supplement:missing.append(package['name']+'@'+package['version'])
        blocks.append('\n'+'='*72+'\n'+package['name']+' '+package['version']+'\nSPDX: '+(package.get('license') or 'see licence file')+'\nSource: '+(package.get('repository') or 'https://crates.io/crates/'+package['name'])+'\nAttribution: '+(', '.join(package.get('authors',[])) or 'See retained upstream grant/source')+'\n')
        for path in paths:blocks.append('\n--- '+path.name+' ---\n'+path.read_text())
        for path,url in supplement:blocks.append('\n--- '+path.name+' (pinned grant or linked full licence text) ---\n'+url+'\n'+path.read_text())
    return '\n'.join(blocks),missing

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--client-notices-output',type=Path);args=parser.parse_args()
    result=inventory()
    if args.client_notices_output:
        text,missing=client_notices();args.client_notices_output.parent.mkdir(parents=True,exist_ok=True);args.client_notices_output.write_text(text)
        result['native_client_missing_notice_files']=missing
        print(json.dumps({'client_notice_files_missing':missing}))
    args.output.parent.mkdir(parents=True,exist_ok=True);args.output.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps({'rust':len(result['rust']),'javascript':len(result['javascript']),'missing_license_metadata':result['missing_license_metadata']}))
