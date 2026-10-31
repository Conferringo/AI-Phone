"""Build and execute the client-owned Python/Node installer environment."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
from artifact_io import digest
from client_host import download
from host_platform import host_key
from installer_tools import build,verify,environment
from runtime_verification import isolated_environment


def qualify(component):
    component=Path(component).resolve();manifest=verify(component);checks={}
    with tempfile.TemporaryDirectory(prefix='installer-tools-check-') as temporary:
        root=Path(temporary);env={**isolated_environment(root),**environment(component,manifest,base_path=os.defpath),
              'HOME':str(root),'USERPROFILE':str(root),'LOCALAPPDATA':str(root/'local'),'APPDATA':str(root/'roaming')}
        python=component/manifest['tools']['python']['executable']
        source='import ctypes,json,pathlib,sqlite3,ssl,sys;assert pathlib.Path(sys.executable).resolve()==pathlib.Path(sys.argv[1]).resolve();assert sqlite3.connect(":memory:").execute("select 7319").fetchone()[0]==7319;print(json.dumps({"version":".".join(map(str,sys.version_info[:3])),"sqlite":sqlite3.sqlite_version,"ssl":ssl.OPENSSL_VERSION}))'
        result=subprocess.run([python,'-I','-S','-B','-c',source,str(python)],env=env,cwd=root,capture_output=True,text=True,timeout=15,check=True)
        checks['python']=json.loads(result.stdout)
        node=component/manifest['tools']['node']['executable']
        result=subprocess.run([node,'--version'],env=env,cwd=root,capture_output=True,text=True,timeout=15,check=True)
        checks['node']={'version':result.stdout.strip().removeprefix('v')}
    for role in checks:
        if checks[role]['version']!=manifest['tools'][role]['version']:raise ValueError('Installer tool version differs from its pin')
    verify(component)
    return checks


def factory(root,work):
    root=Path(root).resolve();pins=json.loads((root/'config/installer-tools.json').read_text())['hosts'][host_key()]
    work=Path(work);work.mkdir(parents=True);inputs=work/'inputs'
    for role,pin in pins.items():download(pin,inputs/pin['file'])
    component=work/'component';receipt=build(inputs,component,pins=pins)
    receipt.update(qualification=qualify(component),source_commit=os.environ.get('GITHUB_SHA',''),factory_policy_sha256=digest(root/'config/installer-tools.json'))
    return receipt


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--root',type=Path,default=Path(__file__).resolve().parent.parent)
    parser.add_argument('--work',type=Path,required=True);parser.add_argument('--receipt',type=Path,required=True);args=parser.parse_args()
    result=factory(args.root,args.work);args.receipt.parent.mkdir(parents=True,exist_ok=True);args.receipt.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result))
