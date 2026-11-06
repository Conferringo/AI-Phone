"""Maintainer build tool: signed flat APT metadata for existing DEB assets."""
import argparse
import gzip
import json
from pathlib import Path
from artifact_io import digest
import subprocess
import tempfile


def index(packages, output):
    packages=[Path(path).resolve() for path in packages]
    output=Path(output).resolve();output.mkdir(parents=True,exist_ok=True)
    archive=['apt-ftparchive','--md5=no','--sha1=no','--sha512=no']
    # Index the original bytes without copying or repacking the full installers.
    with tempfile.TemporaryDirectory(prefix='aiphone-apt-') as temporary:
        stage=Path(temporary)
        for package in packages:(stage/package.name).symlink_to(package)
        index=subprocess.check_output([*archive,'packages','.'],cwd=stage)
    for name in ('InRelease','Release.gpg','ai-phone-archive-key.asc','ai-phone.sources'):
        (output/name).unlink(missing_ok=True)
    (output/'Packages').write_bytes(index)
    (output/'Packages.gz').write_bytes(gzip.compress(index,mtime=0))
    release=subprocess.check_output([*archive,'-o','APT::FTPArchive::Release::Origin=AI Phone','-o','APT::FTPArchive::Release::Label=AI Phone','release','.'],cwd=output)
    (output/'Release').write_bytes(release)
    return {'packages':[path.name for path in packages],'metadata':records([output/name for name in ('Packages','Packages.gz','Release')])}


def records(paths):
    return [{'file':path.name,'size':path.stat().st_size,'sha256':digest(path)} for path in paths]


def sign(output, base_url, key, gnupg_home):
    output=Path(output).resolve()
    gpg=['gpg','--homedir',str(gnupg_home),'--batch','--yes','--local-user',key]
    subprocess.run([*gpg,'--output',str(output/'InRelease'),'--clearsign',str(output/'Release')],check=True)
    subprocess.run([*gpg,'--armor','--output',str(output/'Release.gpg'),'--detach-sign',str(output/'Release')],check=True)
    public_key=subprocess.check_output([*gpg,'--export-options','export-minimal','--armor','--export',key],text=True)
    (output/'ai-phone-archive-key.asc').write_text(public_key)
    architectures=sorted({line.removeprefix('Architecture: ') for line in (output/'Packages').read_text().splitlines() if line.startswith('Architecture: ')})
    embedded='\n'.join(' '+(line or '.') for line in public_key.splitlines())
    (output/'ai-phone.sources').write_text('Types: deb\nURIs: '+base_url.rstrip('/')+'/\nSuites: ./\nArchitectures: '+' '.join(architectures)+'\nSigned-By:\n'+embedded+'\n')
    return {'metadata':records([output/name for name in ('InRelease','Release.gpg','ai-phone-archive-key.asc','ai-phone.sources')])}


def build(packages, output, base_url, key, gnupg_home):
    result=index(packages,output)
    result['metadata'].extend(sign(output,base_url,key,gnupg_home)['metadata'])
    return result


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    operations=parser.add_subparsers(dest='operation',required=True)
    for operation in ('index','sign','build'):
        command=operations.add_parser(operation)
        command.add_argument('--output',type=Path,required=True)
        if operation!='sign':command.add_argument('packages',type=Path,nargs='+')
        if operation!='index':
            command.add_argument('--base-url',required=True)
            command.add_argument('--key',required=True,help='Maintainer signing-key fingerprint')
            command.add_argument('--gnupg-home',type=Path,required=True)
    args=parser.parse_args()
    if args.operation=='index':result=index(args.packages,args.output)
    elif args.operation=='sign':result=sign(args.output,args.base_url,args.key,args.gnupg_home)
    else:result=build(args.packages,args.output,args.base_url,args.key,args.gnupg_home)
    print(json.dumps(result))
