"""Scan the release allowlist, not private user state. Never print matched values."""
import argparse
import json
import hashlib
from pathlib import Path
import re
import subprocess
from release_package import private_metadata, select_files
from native_backports import verified_backports
from source_manifest import FORMAT_VERSION, file_records

ROOT=Path(__file__).resolve().parent.parent

def manifest_issues(root, files):
    name='SOURCE-MANIFEST.json';path=root/name
    if path.is_symlink():return [{'file':name,'rule':'source-manifest-symlink'}]
    try:value=json.loads(path.read_text(encoding='utf-8'))
    except OSError:return [{'file':name,'rule':'source-manifest-unreadable'}]
    except ValueError:return [{'file':name,'rule':'source-manifest-invalid-json'}]
    expected={'format':FORMAT_VERSION,'files':file_records(files)}
    actual={key:value.get(key) for key in expected} if isinstance(value,dict) else None
    if json.dumps(actual,sort_keys=True)!=json.dumps(expected,sort_keys=True):
        return [{'file':name,'rule':'source-manifest-mismatch'}]
    return []

def repository_issues(root, names):
    try:
        result=subprocess.run(['git','--no-optional-locks','-c','core.fsmonitor=false','-C',str(root),'ls-files','-z'],stdin=subprocess.DEVNULL,capture_output=True,timeout=10,check=True)
    except (OSError,subprocess.SubprocessError) as error:
        raise ValueError('Cannot inspect the source repository index') from error
    allowed=names|{'SOURCE-MANIFEST.json'}
    return [{'file':name,'rule':'private-development-metadata' if private_metadata(name) else 'outside-source-allowlist'}
            for name in result.stdout.decode('utf-8').split('\0') if name and name not in allowed]


def check(root=ROOT,require_license=True,*,repository=False,manifest=False):
    root=Path(root);files=select_files(root);issues=[]
    verified_backports(root)
    patterns={'developer-home':rb'/Users/[A-Za-z0-9._-]+/','private-lan-origin':rb'https?://(?:172\.(?:1[6-9]|2[0-9]|3[01])\.|192\.168\.)'}
    for name,content,_ in files:
        # This scanner's own pattern definitions are intentional test rules.
        if name=='scripts/check_source.py':continue
        for label,pattern in patterns.items():
            if re.search(pattern,content):issues.append({'file':name,'rule':label})
    names={name for name,_,_ in files}
    if repository:issues.extend(repository_issues(root,names))
    path=root/'SOURCE-MANIFEST.json'
    if manifest or repository and (path.exists() or path.is_symlink()):issues.extend(manifest_issues(root,files))
    for required in ['README.md','README.en.md','SECURITY.md','DISCLAIMER.md','PRIVACY.md','THIRD_PARTY_NOTICES.md','.github/workflows/source.yml']+(['LICENSE'] if require_license else []):
        if required not in names:issues.append({'file':required,'rule':'required-public-file'})
    for name in names:
        if name.startswith(('data/','user/','docs/evidence/','.git/')) or name=='.env':issues.append({'file':name,'rule':'private-state'})
    provenance=root/'config/source-assets.json'
    if provenance.is_file():
        for asset in json.loads(provenance.read_text()).get('assets',[]):
            entries=list(asset.get('sha256',{}).items())+[(ref['license_file'],ref['license_sha256']) for ref in asset.get('references',[])]
            for name,expected in entries:
                if name not in names or hashlib.sha256((root/name).read_bytes()).hexdigest()!=expected:
                    issues.append({'file':name,'rule':'asset-provenance-hash'})
    for supplement_name in ['native-notice-supplements.json','rust-runtime-notices.json']:
        supplements=root/'config'/supplement_name
        if not supplements.is_file():continue
        for package in json.loads(supplements.read_text()).get('packages',[]):
            for item in package['files']:
                name=item['file']
                if name not in names or hashlib.sha256((root/name).read_bytes()).hexdigest()!=item['sha256']:
                    issues.append({'file':name,'rule':'upstream-notice-hash'})
    if issues:raise ValueError('Source checks failed: '+json.dumps(issues,ensure_ascii=False))
    return {'files':len(files),'private_state_included':False,'license_present':'LICENSE' in names,'source_scan_passed':True}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--draft',action='store_true',help='Prepare local review before the project licence is selected');parser.add_argument('--repository',action='store_true',help='Require the Git index to contain only public source files');parser.add_argument('--manifest',action='store_true',help='Require a valid source manifest with current file sizes and hashes');args=parser.parse_args()
    try:print(json.dumps(check(require_license=not args.draft,repository=args.repository,manifest=args.manifest)))
    except (ValueError,OSError) as error:raise SystemExit(str(error))
