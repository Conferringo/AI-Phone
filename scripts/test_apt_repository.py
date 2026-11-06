"""Native APT install/upgrade of a signed repository, isolated from host state."""
import functools
import getpass
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest


@unittest.skipUnless(sys.platform == 'linux', 'APT packaging runs on Linux')
class AptRepositoryTests(unittest.TestCase):
    def test_signed_install_upgrade_preserves_user_data_and_rejects_tampering(self):
        from apt_repository import build
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);home=root/'keys';home.mkdir(mode=0o700)
            def run(*args,**kwargs):
                return subprocess.run(list(map(str,args)),check=True,capture_output=True,text=True,**kwargs)
            run('gpg','--homedir',home,'--batch','--pinentry-mode','loopback','--passphrase','','--quick-generate-key','AI Phone fixture','ed25519','sign','1d')
            listing=run('gpg','--homedir',home,'--with-colons','--list-secret-keys').stdout
            key=next(line.split(':')[9] for line in listing.splitlines() if line.startswith('fpr:'))
            architecture=run('dpkg','--print-architecture').stdout.strip()
            repos=root/'repos';repos.mkdir();current=repos/'current'
            server=http.server.ThreadingHTTPServer(('127.0.0.1',0),functools.partial(http.server.SimpleHTTPRequestHandler,directory=str(repos)))
            thread=threading.Thread(target=server.serve_forever,daemon=True);thread.start()
            self.addCleanup(server.server_close);self.addCleanup(thread.join);self.addCleanup(server.shutdown)
            base=f'http://127.0.0.1:{server.server_port}/current/'
            target=root/'installed';status=target/'var/lib/dpkg/status';status.parent.mkdir(parents=True);status.touch()
            state=root/'state';(state/'lists/partial').mkdir(parents=True);cache=root/'cache';(cache/'archives/partial').mkdir(parents=True)
            sources=root/'ai-phone.sources';logs=root/'logs';logs.mkdir()
            apt=['apt-get','-o',f'Dir::Etc::sourcelist={sources}','-o','Dir::Etc::sourceparts=-','-o',f'Dir::State={state}','-o',f'Dir::State::status={status}','-o',f'Dir::Cache={cache}','-o',f'Dir::Log={logs}','-o','Debug::NoLocking=true','-o',f'APT::Sandbox::User={getpass.getuser()}','-o',f'DPkg::Options::=--root={target}','-o','DPkg::Options::=--force-not-root','-o','DPkg::Use-Pty=0','--assume-yes']
            sentinel=target/'home/user/work.txt';sentinel.parent.mkdir(parents=True);sentinel.write_text('keep')
            for version in ('1.0','2.0'):
                package=root/version;control=package/'DEBIAN/control';control.parent.mkdir(parents=True)
                control.write_text(f'Package: aiphone-apt-fixture\nVersion: {version}\nArchitecture: {architecture}\nMaintainer: AI Phone fixture\nDescription: Isolated package\n')
                payload=package/'usr/share/aiphone-apt-fixture/version';payload.parent.mkdir(parents=True);payload.write_text(version)
                deb=root/f'aiphone-apt-fixture_{version}_{architecture}.deb';run('dpkg-deb','--build','--root-owner-group',package,deb)
                repository=repos/version
                if version=='1.0':result=build([deb],repository,base,key,home)
                else:
                    tool=Path(__file__).with_name('apt_repository.py')
                    result=json.loads(run(sys.executable,tool,'index',deb,'--output',repository).stdout)
                    self.assertFalse((repository/'InRelease').exists())
                    run(sys.executable,tool,'sign','--output',repository,'--base-url',base,'--key',key,'--gnupg-home',home)
                self.assertEqual(result['packages'],[deb.name])
                # This is the same DEB, not a repackaged APT binary.
                (repository/deb.name).symlink_to(deb)
                sources.write_text((repository/'ai-phone.sources').read_text())
                if version=='1.0':
                    for path in repository.iterdir():
                        if not path.is_symlink():os.utime(path,(1,1))
                replacement=repos/'next';replacement.symlink_to(repository.name);replacement.replace(current)
                run(*apt,'update');run(*apt,'install','aiphone-apt-fixture')
                self.assertEqual((target/'usr/share/aiphone-apt-fixture/version').read_text(),version)
                self.assertEqual(sentinel.read_text(),'keep')
            # The real APT client refuses altered archive bytes under the signed index.
            body=deb.read_bytes();deb.write_bytes(bytes([body[0]^1])+body[1:])
            downloads=root/'downloads';downloads.mkdir()
            with self.assertRaises(subprocess.CalledProcessError) as failed:run(*apt,'download','aiphone-apt-fixture',cwd=downloads)
            self.assertIn('hash sum mismatch',failed.exception.stderr.lower())
            (repository/'InRelease').write_text((repository/'InRelease').read_text().replace('Origin: AI Phone','Origin: Changed'))
            changed=repository/'InRelease';mtime=changed.stat().st_mtime+2;os.utime(changed,(mtime,mtime))
            with self.assertRaises(subprocess.CalledProcessError) as failed:run(*apt,'-o','APT::Update::Error-Mode=any','update')
            self.assertIn('signature',failed.exception.stderr.lower())


if __name__ == '__main__':unittest.main()
