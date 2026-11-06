"""Maintainer build tool: signed flat APT metadata for existing DEB assets."""
import argparse
import gzip
import json
from pathlib import Path
import subprocess
import tempfile


def build(packages, output, base_url, key, gnupg_home):
    packages=[Path(path).resolve() for path in packages]
    output=Path(output).resolve();output.mkdir(parents=True)
    archive=['apt-ftparchive','--md5=no','--sha1=no','--sha512=no']
    # Index the original bytes without copying or repacking the full installers.
    with tempfile.TemporaryDirectory(prefix='aiphone-apt-') as temporary:
        stage=Path(temporary)
        for package in packages:(stage/package.name).symlink_to(package)
        index=subprocess.check_output([*archive,'packages','.'],cwd=stage)
    (output/'Packages').write_bytes(index)
    (output/'Packages.gz').write_bytes(gzip.compress(index,mtime=0))
    release=subprocess.check_output([*archive,'-o','APT::FTPArchive::Release::Origin=AI Phone','-o','APT::FTPArchive::Release::Label=AI Phone','release','.'],cwd=output)
    (output/'Release').write_bytes(release)
    gpg=['gpg','--homedir',str(gnupg_home),'--batch','--yes','--local-user',key]
    subprocess.run([*gpg,'--output',str(output/'InRelease'),'--clearsign',str(output/'Release')],check=True)
    subprocess.run([*gpg,'--armor','--output',str(output/'Release.gpg'),'--detach-sign',str(output/'Release')],check=True)
    public_key=subprocess.check_output([*gpg,'--export-options','export-minimal','--armor','--export',key],text=True)
    (output/'ai-phone-archive-key.asc').write_text(public_key)
    architectures=sorted({subprocess.check_output(['dpkg-deb','--field',str(path),'Architecture'],text=True).strip() for path in packages})
    embedded='\n'.join(' '+(line or '.') for line in public_key.splitlines())
    (output/'ai-phone.sources').write_text('Types: deb\nURIs: '+base_url.rstrip('/')+'/\nSuites: ./\nArchitectures: '+' '.join(architectures)+'\nSigned-By:\n'+embedded+'\n')
    return {'packages':[path.name for path in packages],'metadata':[path.name for path in sorted(output.iterdir())]}


if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('packages',type=Path,nargs='+')
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--base-url',required=True)
    parser.add_argument('--key',required=True,help='Maintainer signing-key fingerprint')
    parser.add_argument('--gnupg-home',type=Path,required=True)
    args=parser.parse_args()
    print(json.dumps(build(args.packages,args.output,args.base_url,args.key,args.gnupg_home)))
