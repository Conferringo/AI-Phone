"""Canonical public-source manifest generation; no runtime or user-state changes."""
import argparse
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parent.parent
FORMAT_VERSION=1


def file_records(files):
    return [{'path':name,'size':len(content),'sha256':hashlib.sha256(content).hexdigest()}
            for name,content,_ in files]


def build(root,files,*,draft=False):
    policy=json.loads((Path(root)/'config/release-policy.json').read_text())
    preview=policy.get('source_preview',{})
    review=preview.get('remote_review_authorized') is True
    kind=('private-source-review-draft' if review else 'local-review-draft') if draft else 'source-preview'
    return {'format':FORMAT_VERSION,'version':policy.get('version'),'license':preview.get('license'),
            'remote_publication_authorized':not draft and preview.get('remote_publication_authorized') is True,
            'remote_review_authorized':review,'kind':kind,
            'product_ready':all(value is True for value in policy['publication_gates'].values()),
            'product_gates':policy['publication_gates'],'files':file_records(files)}


def refresh(root=ROOT):
    from check_source import check
    from release_package import select_files
    root=Path(root)
    check(root)
    path=root/'SOURCE-MANIFEST.json'
    if path.is_symlink():raise ValueError('Source manifest must be a regular file')
    files=select_files(root)
    path.write_text(json.dumps(build(root,files),indent=2)+'\n')
    return {'updated':'SOURCE-MANIFEST.json','source_files':len(files)}


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--refresh',action='store_true',required=True,help='Regenerate the manifest from the source allowlist')
    parser.parse_args()
    try:print(json.dumps(refresh()))
    except (ValueError,OSError) as error:raise SystemExit(str(error))
