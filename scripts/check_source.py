"""Scan the release allowlist, not private user state. Never print matched values."""
import argparse
import json
import hashlib
from pathlib import Path
import re
from release_package import select_files
from native_backports import verified_backports

ROOT=Path(__file__).resolve().parent.parent

def check(root=ROOT,require_license=True):
    root=Path(root);files=select_files(root);issues=[]
    verified_backports(root)
    patterns={'developer-home':rb'/Users/[A-Za-z0-9._-]+/','private-lan-origin':rb'https?://(?:172\.(?:1[6-9]|2[0-9]|3[01])\.|192\.168\.)'}
    for name,content,_ in files:
        # This scanner's own pattern definitions are intentional test rules.
        if name=='scripts/check_source.py':continue
        for label,pattern in patterns.items():
            if re.search(pattern,content):issues.append({'file':name,'rule':label})
    names={name for name,_,_ in files}
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
    supplements=root/'config/native-notice-supplements.json'
    if supplements.is_file():
        for package in json.loads(supplements.read_text()).get('packages',[]):
            for item in package['files']:
                name=item['file']
                if name not in names or hashlib.sha256((root/name).read_bytes()).hexdigest()!=item['sha256']:
                    issues.append({'file':name,'rule':'upstream-notice-hash'})
    if issues:raise ValueError('Source checks failed: '+json.dumps(issues,ensure_ascii=False))
    return {'files':len(files),'private_state_included':False,'license_present':'LICENSE' in names,'source_scan_passed':True}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--draft',action='store_true',help='Prepare local review before the project licence is selected');args=parser.parse_args()
    try:print(json.dumps(check(require_license=not args.draft)))
    except (ValueError,OSError) as error:raise SystemExit(str(error))
