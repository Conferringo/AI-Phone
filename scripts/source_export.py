"""Export a new source-only review tree. No git history, user data or remote writes."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile
from check_source import check
from release_package import select_files

ROOT=Path(__file__).resolve().parent.parent

def export(root,destination,*,draft=False,git=False):
    root=Path(root).resolve();destination=Path(destination).resolve()
    if destination.exists():raise ValueError('Source export destination must not exist')
    # Prevent an export inside source code from recursively exporting itself next time.
    if destination.is_relative_to(root) and not any(destination.is_relative_to(root/p) for p in ['dist','data/backups']):raise ValueError('Use dist or a directory outside the development source tree')
    report=check(root,require_license=not draft);files=select_files(root)
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.source-export-',dir=destination.parent) as folder:
        work=Path(folder)/'source';work.mkdir();manifest=[]
        for name,content,mode in files:
            target=work/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(content);target.chmod(mode)
            manifest.append({'path':name,'size':len(content),'sha256':hashlib.sha256(content).hexdigest()})
        agent_guide=root/'docs/public/contributor-agents.md'
        if agent_guide.is_file():
            content=agent_guide.read_bytes();(work/'AGENTS.md').write_bytes(content);manifest.append({'path':'AGENTS.md','size':len(content),'sha256':hashlib.sha256(content).hexdigest()})
        policy=json.loads((root/'config/release-policy.json').read_text())
        remote_review=policy.get('source_preview',{}).get('remote_review_authorized') is True
        marker='REVIEW-ONLY.txt' if remote_review else 'LOCAL-REVIEW-ONLY.txt'
        (work/'SOURCE-MANIFEST.json').write_text(json.dumps({'format':1,'version':policy.get('version'),'license':policy.get('source_preview',{}).get('license'),'remote_publication_authorized':not draft and policy.get('source_preview',{}).get('remote_publication_authorized') is True,'remote_review_authorized':remote_review,'kind':('private-source-review-draft' if remote_review else 'local-review-draft') if draft else 'source-preview','product_ready':all(v is True for v in policy['publication_gates'].values()),'product_gates':policy['publication_gates'],'files':manifest},indent=2)+'\n')
        if draft:(work/marker).write_text('Review candidate. Not a public product release; see SOURCE-MANIFEST.json for incomplete acceptance. Never upload user state.\n')
        if git:
            subprocess.run(['git','init','--initial-branch=main'],cwd=work,check=True,stdout=subprocess.DEVNULL)
            subprocess.run(['git','config','user.name','AI Phone contributors'],cwd=work,check=True)
            subprocess.run(['git','config','user.email','contributors@aiphone.invalid'],cwd=work,check=True)
            # This is a newly created allowlisted export, never the live checkout.
            paths=[item['path'] for item in manifest]+['SOURCE-MANIFEST.json']+([marker] if draft else [])
            subprocess.run(['git','add','--',*paths],cwd=work,check=True)
            subprocess.run(['git','commit','-m','Prepare AI Phone source preview'],cwd=work,check=True,stdout=subprocess.DEVNULL)
        work.rename(destination)
    return {**report,'destination':str(destination),'git_initialized':git,'remote_created':False,'draft':draft}

if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--destination',required=True,type=Path);parser.add_argument('--draft',action='store_true');parser.add_argument('--git',action='store_true');args=parser.parse_args()
    try:print(json.dumps(export(ROOT,args.destination,draft=args.draft,git=args.git)))
    except (ValueError,OSError,subprocess.SubprocessError) as error:raise SystemExit(str(error))
