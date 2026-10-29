"""Private, consistent state backups; restore only into empty destinations/databases."""
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import plistlib
import re
import shutil
import subprocess
import tarfile
import tempfile
import time
import uuid
from contextlib import AbstractContextManager, contextmanager
from datetime import datetime, timezone
from database import env_values, preparation_sql, credentials, identifier, verify_restore_roles
from runtime_config import runtime_env

CHUNK=1024*1024
ZERO=bytes(CHUNK)
SERVICES=[('dev.aiphone.lan','data/phone-runner/lan.plist'),('dev.aiphone.channels','data/channels/service.plist'),('dev.aiphone.web.p2','data/phone-runner/web.plist'),('dev.aiphone.assistant','data/assistant/assistant.plist'),('dev.aiphone.runner.p2','data/phone-runner/runner.plist'),('dev.aiphone.device-stream','data/device-stream/service.plist'),('dev.aiphone.emulator.p2','data/android/run/emulator.plist')]

def digest(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as file:
        while block:=file.read(CHUNK):h.update(block)
    return h.hexdigest()

def copy_with_budget(source,destination,disk):
    while block:=source.read(CHUNK):
        if shutil.disk_usage(disk).free<4*1024**3+len(block):raise ValueError('Operation stopped to preserve 4 GiB of host free space')
        destination.write(block)

def private_json(path,value):
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    with os.fdopen(os.open(path,os.O_WRONLY|os.O_CREAT|os.O_TRUNC,0o600),'w') as file:
        os.fchmod(file.fileno(),0o600)
        json.dump(value,file,indent=2);file.write('\n');file.flush();os.fsync(file.fileno())

def checked(args,**kwargs):
    result=subprocess.run([str(x) for x in args],capture_output=True,**kwargs)
    if result.returncode:raise ValueError('Backup/recovery command failed: '+str(args[0]))
    return result.stdout

def inspect_container(name):
    return json.loads(checked(['docker','inspect',name]))[0]

class Freeze(AbstractContextManager):
    def __init__(self,root,desktop,enabled):self.root=root;self.desktop=desktop;self.enabled=enabled;self.jobs=[];self.desktop_running=False
    def __enter__(self):
        if not self.enabled:return self
        domain=f'gui/{os.getuid()}'
        # Validate every loaded job before stopping any of them.
        for label,relative in SERVICES:
            active=subprocess.run(['launchctl','print',domain+'/'+label],capture_output=True)
            if active.returncode:continue
            path=self.root/relative
            if not path.is_file():
                path=Path.home()/'Library/LaunchAgents'/(label+'.plist')
            config=plistlib.loads(path.read_bytes())
            if config.get('WorkingDirectory') not in [str(self.root),str(self.root/'apps/web')] or not any(str(self.root) in str(x) for x in config.get('ProgramArguments',[])):
                raise ValueError('A running service belongs to another checkout; no services stopped')
            self.jobs.append((label,path))
        if self.desktop:
            info=inspect_container(self.desktop)
            homes=[Path(m['Source']).resolve() for m in info['Mounts'] if m.get('Destination')=='/home/ai']
            if homes!=[(self.root/'data/desktop/home').resolve()]:raise ValueError('Desktop is not owned by this installation')
            self.desktop_running=info['State']['Running']
        try:
            # Flush Android while its original runner is still available.
            if any(label.endswith('emulator.p2') for label,_ in self.jobs):
                checked([self.root/'scripts/android-adb','shell','sync'])
            for label,_ in self.jobs:
                if label.endswith('emulator.p2'):continue
                checked(['launchctl','bootout',domain+'/'+label])
            if any(label.endswith('emulator.p2') for label,_ in self.jobs):checked([self.root/'scripts/android-stop.sh'],cwd=self.root)
            if self.desktop_running:checked(['docker','stop','--time','20',self.desktop])
        except BaseException:
            self.__exit__(None,None,None);raise
        return self
    def __exit__(self,*_):
        if not self.enabled:return False
        failures=[]
        if self.desktop_running:
            try:checked(['docker','start',self.desktop])
            except Exception:failures.append('desktop')
        domain=f'gui/{os.getuid()}'
        for label,path in reversed(self.jobs):
            active=subprocess.run(['launchctl','print',domain+'/'+label],capture_output=True)
            if active.returncode:
                try:checked(['launchctl','bootstrap',domain,path])
                except Exception:failures.append(label)
        if failures:raise ValueError('Backup finished but some owned services need restart: '+', '.join(failures))
        return False

def psql(container,admin,database,sql):
    return checked(['docker','exec','-i',container,'psql','-X','-v','ON_ERROR_STOP=1','-U',admin,'-d',database,'-At'],input=sql.encode()).decode().strip()

def fingerprints(container,admin,database):
    tables=psql(container,admin,database,"SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename;").splitlines()
    result={}
    for table in tables:
        # Keep image-bearing rows out of aggregate/sort contexts on small PG instances.
        # Only 32-byte hashes cross the pipe; comparison stays independent of row order.
        query=f"SELECT md5(row_to_json(t)::text) FROM public.{identifier(table)} t;"
        hashes=psql(container,admin,database,query).splitlines()
        result[table]={'rows':len(hashes),'digest':hashlib.md5(''.join(sorted(hashes)).encode()).hexdigest()}
    return result

def disk_dependencies(root):
    sdk=Path(runtime_env(root)['AI_PHONE_SDK_ROOT']);tool=sdk/'emulator/qemu-img';dependencies={};chains=[]
    data=(root/'data').resolve()
    images=list((data/'android/avd').glob('*.avd/*.qcow2'))
    if images and not tool.is_file():raise ValueError('qemu-img required to verify phone disk backing chains')
    for image in images:
        chain=json.loads(checked([tool,'info','--backing-chain','--output=json',image]))
        for entry in chain:
            entry_image=Path(entry['filename'])
            if not entry_image.is_absolute():entry_image=image.parent/entry_image
            entry_image=entry_image.resolve()
            backing=entry.get('full-backing-filename') or entry.get('backing-filename')
            if not backing:continue
            backing=Path(backing);backing=(entry_image.parent/backing).resolve() if not backing.is_absolute() else backing.resolve()
            if backing.is_relative_to((root/'data').resolve()):
                dependencies[str(backing.relative_to((root/'data').resolve()))]=True
            elif backing.is_relative_to(sdk.resolve()):
                rel=str(backing.relative_to(sdk.resolve()));dependencies['sdk:'+rel]={'size':backing.stat().st_size,'sha256':digest(backing)}
            else:raise ValueError('Phone image depends on a file outside this installation and its SDK')
            if not entry_image.is_relative_to((root/'data').resolve()):raise ValueError('Writable backing image must belong to user state')
            record={'image':str(entry_image.relative_to(data)),'backing':str(backing),'format':entry.get('backing-filename-format','raw')}
            if record not in chains:chains.append(record)
    return dependencies,chains

class HashReader:
    def __init__(self,file):self.file=file;self.hash=hashlib.sha256()
    def read(self,size=-1):
        data=self.file.read(size);self.hash.update(data);return data

def write_state(root,path,dependencies):
    records={};excluded=[];data=(root/'data').resolve()
    def excluded_path(relative):
        text=relative.as_posix()
        prefixes=('backups','android/backups','android/downloads')
        for prefix in prefixes:
            if (text==prefix or text.startswith(prefix+'/')) and not any(k==text or k.startswith(text+'/') for k in dependencies if not k.startswith('sdk:')):return True
        # Do not discard a user's work file just because it ends in .log or .lock.
        runtime_log=(relative.name.endswith('.log') and (len(relative.parts)==2 and relative.parts[0] in ('assistant','channels','device-stream','phone-runner') or relative.parts[:2]==('android','logs')))
        avd_lock=relative.parts[:2]==('android','avd') and relative.name.endswith('.lock')
        return runtime_log or avd_lock or relative.name.startswith('Singleton') or relative.name=='.DS_Store'
    with tarfile.open(path,'w:gz',compresslevel=1) as tar:
        env=root/'.env';info=tarfile.TarInfo('.env');info.size=env.stat().st_size;info.mode=0o600
        with env.open('rb') as file:r=HashReader(file);tar.addfile(info,r)
        records['.env']={'type':'file','size':info.size,'sha256':r.hash.hexdigest()}
        for base,dirs,files in os.walk(data,followlinks=False):
            folder=Path(base)
            for name in list(dirs)+files:
                source=folder/name;relative=source.relative_to(data)
                if excluded_path(relative):
                    excluded.append(str(relative))
                    if name in dirs:dirs.remove(name)
                    continue
                # Store hardlinked regular files independently; no unverified hardlink targets.
                tar.inodes.clear()
                archive='data/'+relative.as_posix();info=tar.gettarinfo(str(source),arcname=archive)
                if info is None:
                    excluded.append(str(relative));continue
                if info.issym():tar.addfile(info);records[archive]={'type':'symlink','target':info.linkname}
                elif info.isdir():tar.addfile(info)
                elif info.isfile():
                    with source.open('rb') as file:r=HashReader(file);tar.addfile(info,r)
                    records[archive]={'type':'file','size':info.size,'allocated_bytes':source.stat().st_blocks*512,'sha256':r.hash.hexdigest()}
                else:excluded.append(str(relative))
    path.chmod(0o600)
    return records,excluded

def create_backup(root,output,*,pg_container='ai-phone-postgres-1',desktop_container='ai-phone-desktop',quiesce=True,progress=print):
    root=Path(root).resolve();output=Path(output).resolve();env=env_values(root)
    if output.exists():raise ValueError('Backup destination already exists')
    output.parent.mkdir(parents=True,exist_ok=True,mode=0o700)
    data=(root/'data').resolve()
    if output.is_relative_to(data) and not output.is_relative_to(data/'backups'):raise ValueError('Backups inside user state must be under data/backups')
    backup_id=uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix='.aiphone-backup-',dir=output.parent) as name:
        work=Path(name);work.chmod(0o700)
        with Freeze(root,desktop_container,quiesce):
            progress('Services quiesced; checking disk dependencies',flush=True)
            dependencies,chains=disk_dependencies(root)
            progress('Backing up PostgreSQL',flush=True)
            with (work/'database.dump').open('wb') as file:
                result=subprocess.run(['docker','exec',pg_container,'pg_dump','-U',env['POSTGRES_USER'],'-d',env['POSTGRES_DB'],'-Fc','--no-owner','--no-acl'],stdout=file,stderr=subprocess.PIPE)
                if result.returncode:raise ValueError('Database backup failed')
            rows=fingerprints(pg_container,env['POSTGRES_USER'],env['POSTGRES_DB'])
            desktop=None
            if desktop_container:
                info=inspect_container(desktop_container)
                if info['State']['Running']:raise ValueError('Desktop must be stopped for a consistent backup')
                desktop={'config':info['Config'],'memory':info['HostConfig']['Memory'],'cpus':info['HostConfig']['NanoCpus'],'shm':info['HostConfig']['ShmSize']}
                progress('Exporting Linux root filesystem',flush=True)
                import gzip
                with gzip.open(work/'desktop-rootfs.tar.gz','wb',compresslevel=1) as file:
                    producer=subprocess.Popen(['docker','export',desktop_container],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL)
                    with producer.stdout:shutil.copyfileobj(producer.stdout,file,CHUNK)
                    if producer.wait()!=0:raise ValueError('Linux root filesystem export failed')
            progress('Archiving phone disks, files, accounts and action records',flush=True)
            records,excluded=write_state(root,work/'state.tar.gz',dependencies)
            manifest={'format':1,'id':backup_id,'created_at':datetime.now(timezone.utc).isoformat(),'consistency':'quiesced' if quiesce else 'offline-fixture','source_root':str(root),'source_data':str((root/'data').resolve()),'records':records,'excluded':excluded,'database':rows,'desktop':desktop,'disk_dependencies':dependencies,'disk_chains':chains,'files':[]}
        progress('Original services resumed; sealing backup',flush=True)
        for file in work.iterdir():
            if file.is_file():file.chmod(0o600);manifest['files'].append({'path':file.name,'size':file.stat().st_size,'sha256':digest(file)})
        private_json(work/'manifest.json',manifest)
        if output.suffix=='.backup':
            # Directory envelopes avoid a second full-size copy of encrypted phone disks.
            work.rename(output)
        else:
            total=sum(p.stat().st_size for p in work.iterdir())
            if shutil.disk_usage(output.parent).free<total+4*1024**3:raise ValueError('Not enough temporary space to seal a tar; use a .backup directory envelope')
            temp=output.with_name(output.name+'.partial')
            try:
                with os.fdopen(os.open(temp,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600),'wb') as output_file:
                    with tarfile.open(fileobj=output_file,mode='w') as tar:
                        for file in sorted(work.iterdir()):tar.add(file,arcname=file.name,recursive=False)
                temp.chmod(0o600);os.replace(temp,output)
            finally:temp.unlink(missing_ok=True)
    size=sum(p.stat().st_size for p in output.iterdir()) if output.is_dir() else output.stat().st_size
    private_json(output.with_name(output.name+'.sha256.json'),{'sha256':digest(output/'manifest.json' if output.is_dir() else output),'size':size,'format':1,'envelope':'directory' if output.is_dir() else 'tar'})
    return {'id':backup_id,'path':str(output),'size':size,'records':len(records),'consistent':quiesce}

@contextmanager
def payload_file(archive,name):
    archive=Path(archive)
    if archive.is_dir():
        path=archive/name
        if path.is_symlink() or not path.is_file():raise ValueError('Unsafe backup payload')
        with path.open('rb') as file:yield file
    else:
        with tarfile.open(archive,'r:') as tar:
            with tar.extractfile(name) as file:yield file

def read_backup(archive,work,*,copy=True):
    archive=Path(archive)
    if archive.is_dir():
        paths=list(archive.iterdir());names={p.name for p in paths}
        if not all(p.is_file() and not p.is_symlink() for p in paths) or not names<={'manifest.json','database.dump','state.tar.gz','desktop-rootfs.tar.gz'} or not {'manifest.json','database.dump','state.tar.gz'}<=names:raise ValueError('Unsafe backup directory')
        if (archive/'manifest.json').stat().st_size>64*1024*1024:raise ValueError('Backup manifest too large')
        manifest=json.loads((archive/'manifest.json').read_text())
        if manifest.get('format')!=1 or not re.fullmatch('[a-f0-9]{32}',manifest.get('id','')):raise ValueError('Unsupported backup format')
        expected={x['path']:x for x in manifest['files']}
        if len(expected)!=len(manifest['files']) or set(expected)!=names-{'manifest.json'} or bool(manifest.get('desktop'))!=('desktop-rootfs.tar.gz' in names):raise ValueError('Backup manifest differs from payload')
        for name,record in expected.items():
            path=archive/name
            if path.stat().st_size!=record['size'] or digest(path)!=record['sha256']:raise ValueError('Backup payload checksum mismatch')
            if copy:shutil.copyfile(path,work/name);(work/name).chmod(0o600)
        return manifest
    with tarfile.open(archive,'r:') as tar:
        members=tar.getmembers();names=[m.name for m in members]
        if len(names)!=len(set(names)) or not set(names)<= {'manifest.json','database.dump','state.tar.gz','desktop-rootfs.tar.gz'} or not all(m.isfile() for m in members):raise ValueError('Unsafe backup envelope')
        if not {'manifest.json','database.dump','state.tar.gz'}<=set(names):raise ValueError('Backup payload missing')
        if tar.getmember('manifest.json').size>64*1024*1024:raise ValueError('Backup manifest too large')
        manifest=json.load(tar.extractfile('manifest.json'))
        if manifest.get('format')!=1 or not re.fullmatch('[a-f0-9]{32}',manifest.get('id','')):raise ValueError('Unsupported backup format')
        expected={x['path']:x for x in manifest['files']}
        if len(expected)!=len(manifest['files']) or set(expected)!=set(names)-{'manifest.json'} or bool(manifest.get('desktop'))!=('desktop-rootfs.tar.gz' in names):raise ValueError('Backup manifest differs from payload')
        for member in members:
            if member.name=='manifest.json':continue
            record=expected[member.name]
            if member.size!=record['size']:raise ValueError('Backup payload size mismatch')
            h=hashlib.sha256();path=work/member.name
            with tar.extractfile(member) as source:
                if copy:
                    with path.open('wb') as file:
                        while block:=source.read(CHUNK):h.update(block);file.write(block)
                    path.chmod(0o600)
                else:
                    while block:=source.read(CHUNK):h.update(block)
            if h.hexdigest()!=record['sha256']:raise ValueError('Backup payload checksum mismatch')
    return manifest

def restore_state(payload,destination,manifest):
    seen=set();links=[];records=manifest['records'];total=0
    options={'fileobj':payload} if hasattr(payload,'read') else {'name':payload}
    with tarfile.open(mode='r|gz',**options) as tar:
        for member in tar:
            path=PurePosixPath(member.name)
            if member.name in seen or path.is_absolute() or '..' in path.parts or path.as_posix()!=member.name or not(path.parts[0]=='data' or member.name=='.env') or not(member.isfile() or member.isdir() or member.issym()):raise ValueError('Unsafe user-state path')
            seen.add(member.name);total+=member.size
            if len(seen)>200000 or member.size>64*1024**3 or total>128*1024**3:raise ValueError('Backup state exceeds restore limits')
            target=destination/member.name
            if member.isdir():target.mkdir(parents=True,exist_ok=True,mode=0o700);continue
            expected=records.get(member.name)
            if not expected:raise ValueError('Unlisted state entry')
            if member.issym():
                if expected!={'type':'symlink','target':member.linkname}:raise ValueError('State link differs from manifest')
                links.append((target,member.linkname));continue
            if expected.get('type')!='file' or expected['size']!=member.size:raise ValueError('State file differs from manifest')
            target.parent.mkdir(parents=True,exist_ok=True,mode=0o700);source=tar.extractfile(member);h=hashlib.sha256();written=0
            with target.open('wb') as file:
                while block:=source.read(CHUNK):
                    h.update(block);written+=len(block)
                    if block==ZERO[:len(block)]:file.seek(len(block),1)
                    else:file.write(block)
                file.truncate(written)
            target.chmod(member.mode&0o777)
            if h.hexdigest()!=expected['sha256']:raise ValueError('State file checksum mismatch')
            if shutil.disk_usage(destination).free<4*1024**3:raise ValueError('Restore stopped to preserve host free space')
    if not set(records)<=seen:raise ValueError('Missing state entries')
    # Links are created last and cannot be parents of any other extracted entry.
    for target,link in links:
        if target.exists() or target.is_symlink():raise ValueError('State link conflicts with a directory/file')
        target.parent.mkdir(parents=True,exist_ok=True,mode=0o700);target.symlink_to(link)

def restore_database(container,admin,database,dump,env,config):
    count=psql(container,admin,database,"SELECT count(*) FROM pg_tables WHERE schemaname='public';")
    if count!='0':raise ValueError('Restore database must be empty; existing records were not changed')
    existing_roles=verify_restore_roles(container,admin,database,config)
    # Dump schema/data, ownership and grants commit in the same transaction.
    with Path(dump).open('rb') as file, tempfile.TemporaryFile() as errors:
        producer=subprocess.Popen(['docker','exec','-i',container,'pg_restore','--no-owner','--no-acl','--file','-'],stdin=file,stdout=subprocess.PIPE,stderr=errors)
        consumer=subprocess.Popen(['docker','exec','-i',container,'psql','-X','-v','ON_ERROR_STOP=1','-U',admin,'-d',database],stdin=subprocess.PIPE,stdout=subprocess.DEVNULL,stderr=errors)
        try:
            consumer.stdin.write(b'BEGIN;\n');shutil.copyfileobj(producer.stdout,consumer.stdin,CHUNK)
            if producer.wait()!=0:raise ValueError('Database backup cannot be decoded')
            role_sql=preparation_sql(env,config,verified_existing_roles=existing_roles).removeprefix('BEGIN;').removesuffix('COMMIT;')
            consumer.stdin.write(role_sql.encode());consumer.stdin.write(b'\nCOMMIT;\n');consumer.stdin.close()
            if consumer.wait()!=0:raise ValueError('Database recovery transaction failed; target rolled back')
        except BaseException:
            if consumer.stdin and not consumer.stdin.closed:consumer.stdin.close()
            producer.kill() if producer.poll() is None else None
            consumer.wait(timeout=30)
            raise
        finally:
            producer.stdout.close()

def relocate_text(text,old_root,old_data,final_root):
    # One pass, longest path first; a recovery directory can be inside the old root.
    # Protect already-relocated paths too, so maintenance retries are idempotent.
    mapping={str(old_root):str(final_root),str(old_data):str(final_root/'data'),str(final_root):str(final_root),str(final_root/'data'):str(final_root/'data')}
    pattern='|'.join(re.escape(path) for path in sorted(mapping,key=len,reverse=True))
    return re.sub(pattern,lambda match:mapping[match.group()],text)

def relocate(root,manifest,final_root,sdk_root=None):
    changes=[];old_root=manifest['source_root'];old_data=manifest['source_data']
    for path in (root/'data/android/avd').glob('**/*.ini'):
        source=path.read_text();updated=relocate_text(source,old_root,old_data,final_root)
        if source!=updated:path.write_text(updated);changes.append(str(path.relative_to(root)))
    for path in (root/'data/device-network').glob('*'):
        if path.is_symlink():
            link=os.readlink(path);updated=relocate_text(link,old_root,old_data,final_root)
            if updated!=link:path.unlink();path.symlink_to(updated);changes.append(str(path.relative_to(root)))
    sdk=Path(sdk_root or runtime_env(root)['AI_PHONE_SDK_ROOT'])
    for name,record in manifest['disk_dependencies'].items():
        if name.startswith('sdk:'):
            path=sdk/name[4:]
            if not path.is_file() or path.stat().st_size!=record['size'] or digest(path)!=record['sha256']:raise ValueError('Matching Android system image required before restoring this phone')
    for chain in manifest['disk_chains']:
        image=root/'data'/chain['image'];backing=Path(chain['backing'])
        if str(backing).startswith(old_data+'/'):backing=root/'data'/str(backing)[len(old_data)+1:]
        elif str(backing).startswith(old_root+'/'):backing=root/str(backing)[len(old_root)+1:]
        # SDK backing paths also relocate through the pinned relative SDK file.
        for name in manifest['disk_dependencies']:
            if name.startswith('sdk:') and str(backing).endswith('/'+name[4:]):backing=sdk/name[4:]
        if backing.is_relative_to(root):
            final=final_root/backing.relative_to(root)
            checked([sdk/'emulator/qemu-img','rebase','-u','-f','qcow2','-F',chain['format'],'-b',final,image]);changes.append(str(image.relative_to(root)))
        elif backing.is_relative_to(sdk):
            checked([sdk/'emulator/qemu-img','rebase','-u','-f','qcow2','-F',chain['format'],'-b',backing,image])
    return changes

def restore_backup(archive,destination,*,pg_container,pg_admin=None,database=None,sdk_root=None):
    destination=Path(destination).resolve()
    if destination.exists() and any(p.name!='data' or not p.is_dir() or any(p.iterdir()) for p in destination.iterdir()):raise ValueError('Restore destination must be empty; current user files were not changed')
    destination.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='.aiphone-restore-',dir=destination.parent) as name:
        work=Path(name);work.chmod(0o700);manifest=read_backup(archive,work,copy=False);state=work/'restored';state.mkdir(mode=0o700)
        with payload_file(archive,'state.tar.gz') as file:restore_state(file,state,manifest)
        with payload_file(archive,'database.dump') as source, (work/'database.dump').open('wb') as file:copy_with_budget(source,file,work)
        env=env_values(state)
        if database and database!=env['POSTGRES_DB']:
            text=(state/'.env').read_text();text=re.sub(r'^POSTGRES_DB=.*$',lambda _: 'POSTGRES_DB='+database,text,flags=re.M);(state/'.env').write_text(text)
            config=json.loads((state/'data/database/credentials.json').read_text());config['database']=database;private_json(state/'data/database/credentials.json',config);env=env_values(state)
        config=credentials(state);admin=pg_admin or env['POSTGRES_USER']
        changes=relocate(state,manifest,destination,sdk_root)
        restore_database(pg_container,admin,env['POSTGRES_DB'],work/'database.dump',env,config)
        restored=fingerprints(pg_container,admin,env['POSTGRES_DB'])
        if restored!=manifest['database']:raise ValueError('Restored database fingerprints differ; destination not activated')
        # Never automatically resume old queued work or duplicate remote account activity.
        psql(pg_container,admin,env['POSTGRES_DB'],"UPDATE runs SET status='paused',next_wake_at=NULL WHERE status IN ('queued','running'); UPDATE proactive_state SET exploring=false,notifications_enabled=false; UPDATE desktop_exploration SET enabled=false;" )
        private_json(state/'data/restore-quarantine.json',{'backup_id':manifest['id'],'reason':'Restore verified; original instance must be offline before activation'})
        if manifest['desktop']:
            recovery=state/'data/desktop/recovery';recovery.mkdir(parents=True,exist_ok=True,mode=0o700)
            with payload_file(archive,'desktop-rootfs.tar.gz') as source, (recovery/'rootfs.tar.gz').open('wb') as file:copy_with_budget(source,file,work)
            (recovery/'rootfs.tar.gz').chmod(0o600);private_json(recovery/'container.json',manifest['desktop'])
        report={'backup_id':manifest['id'],'database_verified':True,'tables':len(restored),'state_entries':len(manifest['records']),'relocations':changes,'quarantined':True,'desktop_rootfs_saved':bool(manifest['desktop'])}
        if manifest['desktop']:report['linux_rootfs_sha256']=next(x['sha256'] for x in manifest['files'] if x['path']=='desktop-rootfs.tar.gz')
        private_json(state/'data/restore-report.json',report)
        if destination.exists():
            (destination/'data').rmdir() if (destination/'data').exists() else None
            destination.rmdir()
        state.rename(destination)
    return report

def recover_desktop(destination,name):
    """Boot the exported system in an offline container, never replace the original."""
    root=Path(destination).resolve();recovery=root/'data/desktop/recovery'
    if not re.fullmatch(r'aiphone-recovery-[a-z0-9-]{1,40}',name):raise ValueError('Recovery container name must start with aiphone-recovery-')
    if not (root/'data/restore-quarantine.json').is_file():raise ValueError('Only quarantined recovered state can use the offline recovery boot')
    report=json.loads((root/'data/restore-report.json').read_text());metadata=json.loads((recovery/'container.json').read_text());config=metadata['config']
    if digest(recovery/'rootfs.tar.gz')!=report.get('linux_rootfs_sha256'):raise ValueError('Recovered Linux rootfs checksum mismatch')
    exists=subprocess.run(['docker','inspect',name],capture_output=True)
    if exists.returncode==0:raise ValueError('Recovery container already exists; it was not changed')
    image=name+':verified'
    if subprocess.run(['docker','image','inspect',image],capture_output=True).returncode==0:raise ValueError('Recovery image already exists; it was not changed')
    with tarfile.open(recovery/'rootfs.tar.gz','r|gz') as tar:expanded=sum(member.size for member in tar if member.isfile())
    if shutil.disk_usage(root).free<expanded+4*1024**3:raise ValueError('Not enough host space to import the recovered Linux system while preserving 4 GiB free')
    changes=[]
    for key in ('Entrypoint','Cmd'):
        value=config.get(key)
        if value:
            if not isinstance(value,list) or not all(isinstance(x,str) for x in value):raise ValueError('Invalid Linux startup metadata')
            changes+=['--change',('ENTRYPOINT' if key=='Entrypoint' else 'CMD')+' '+json.dumps(value)]
    with (recovery/'rootfs.tar.gz').open('rb') as file:
        checked(['docker','import','--platform','linux/arm64',*changes,'-',image],stdin=file)
    created=False
    try:
        with tempfile.TemporaryDirectory(prefix='.linux-env-',dir=recovery) as folder:
            env_file=Path(folder)/'environment';values=config.get('Env') or []
            if not all(isinstance(x,str) and '=' in x and '\n' not in x and '\r' not in x and '\x00' not in x for x in values):raise ValueError('Invalid Linux environment metadata')
            env_file.write_text('\n'.join(values)+'\n');env_file.chmod(0o600)
            args=['docker','create','--name',name,'--label','dev.aiphone.recovery='+report['backup_id'],'--init','--network','none','--cap-add','NET_ADMIN','--memory',str(metadata['memory'] or 2560*1024**2),'--cpus',str((metadata['cpus'] or 2000000000)/1000000000),'--shm-size',str(metadata['shm'] or 256*1024**2),'--env-file',env_file]
            for source,target in [(root/'data/desktop/home','/home/ai'),(root/'data/device-network/desktop','/etc/ai-phone-network')]:
                source=source.resolve()
                if not source.is_dir() or not source.is_relative_to(root/'data'):raise ValueError('Recovered Linux mount leaves restored user state')
                args+=['--mount',f'type=bind,source={source},target={target}']
            checked([*args,image]);created=True
        checked(['docker','start',name])
    except BaseException:
        if created:subprocess.run(['docker','rm','-f',name],capture_output=True)
        subprocess.run(['docker','image','rm',image],capture_output=True);raise
    return {'container':name,'image':image,'network':'none','host_ports':False,'original_preserved':True}
