"""One kernel owner for a browser profile and its recoverable container intent."""
from dataclasses import dataclass,field
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import shutil
from installer_lock import acquire
from private_files import atomic_json,is_private,protect

HEX=re.compile('[0-9a-f]{64}')
IMAGE=re.compile('ai-phone-bundle/desktop:sha-([0-9a-f]{64})')


class BrowserOwnerChanged(ValueError):pass


@dataclass(frozen=True)
class BrowserContext:
    root:Path
    session:str
    image:str
    profile:Path=field(init=False)
    key:str=field(init=False)
    image_id:str=field(init=False)

    def __post_init__(self):
        if re.fullmatch('[a-zA-Z0-9_-]{1,128}',self.session) is None:raise ValueError('Invalid browser session')
        match=IMAGE.fullmatch(self.image)
        if match is None:raise ValueError('Browser requires an immutable bundled image')
        base=(Path(self.root)/'data/channels/whatsapp').resolve()
        if any(ord(c)<32 for c in str(base)):raise ValueError('Invalid browser profile path')
        profile=base/('session-'+self.session)
        object.__setattr__(self,'profile',profile)
        object.__setattr__(self,'key',hashlib.sha256(str(profile).encode()).hexdigest())
        object.__setattr__(self,'image_id','sha256:'+match[1])

    @property
    def name(self):return 'ai-phone-browser-'+self.key[:24]
    @property
    def record(self):return self.profile.parent/'.browser-runtime'/(self.key+'.json')
    @property
    def lock(self):return self.record.with_suffix('.lock')
    @property
    def marker(self):return self.profile/'.aiphone-runtime.json'


def private_json(path):
    if path.is_symlink() or path.stat().st_size>4096 or not is_private(path):raise ValueError('Browser ownership record is not private')
    value=json.loads(path.read_text(encoding='utf-8'))
    if not isinstance(value,dict) or type(value.get('format')) is not int or value['format']!=1:raise ValueError('Invalid browser ownership format')
    return value


def checked_record(value,key):
    if set(value)!={'format','profile_hash','image_id','token','container_id'} or value['profile_hash']!=key:raise ValueError('Browser ownership record differs from this profile')
    if HEX.fullmatch(str(value['token'])) is None or re.fullmatch('sha256:[0-9a-f]{64}',str(value['image_id'])) is None:raise ValueError('Invalid browser ownership identity')
    identity=value['container_id']
    if identity!='' and HEX.fullmatch(str(identity)) is None:raise ValueError('Invalid browser container identity')
    return value


def owned_container(container,record,key):
    labels=container.get('Config',{}).get('Labels',{})
    expected={'dev.aiphone.component':'channel-browser','dev.aiphone.profile':key,'dev.aiphone.launch':record['token']}
    if not isinstance(labels,dict) or any(labels.get(name)!=value for name,value in expected.items()):raise ValueError('Container is not owned by this browser launch')
    if container.get('Image')!=record['image_id'] or HEX.fullmatch(str(container.get('Id'))) is None:raise ValueError('Browser container image or identity differs')
    if record['container_id'] and record['container_id']!=container['Id']:raise ValueError('Browser container was replaced')


def stale_profile_links(profile):
    """Only Chromium's ephemeral IPC links, never their external targets."""
    patterns={
        'SingletonLock':r'(?:[0-9a-f]{12}|ai-phone-browser-[0-9a-f]{24})-[1-9][0-9]*',
        'SingletonSocket':r'/tmp/\.?org\.chromium\.Chromium\.[^/]+/SingletonSocket',
        'SingletonCookie':r'[0-9]+',
    }
    links=[]
    for name,pattern in patterns.items():
        path=profile/name
        if not path.exists() and not path.is_symlink():continue
        if not path.is_symlink() or re.fullmatch(pattern,os.readlink(path)) is None:raise ValueError('Unknown browser profile IPC')
        links.append(path)
    return links


class BrowserLease:
    """The caller explicitly recovers/cleans while holding this lifetime's lock.
    Releasing a process lock never erases account files or adopts a container.
    """
    def __init__(self,context,engine,*,owner=None):
        self.context=context;self.engine=engine;self.file=None
        self.owner=secrets.token_hex(32) if owner is None else owner
        if HEX.fullmatch(str(self.owner)) is None:raise ValueError('Invalid browser launch owner')

    def __enter__(self):
        directory=self.context.record.parent
        if directory.is_symlink():raise ValueError('Browser ownership directory is an alias')
        directory.mkdir(parents=True,exist_ok=True,mode=0o700);protect(directory,directory=True)
        self.file=acquire(self.context.lock)
        return self

    def __exit__(self,*_):
        self.file.close();self.file=None

    def require_lock(self):
        if self.file is None or self.file.closed:raise RuntimeError('Browser profile lock is required')

    def marker_value(self):return {'format':1,'mode':'container','session_id':self.context.session,'owner_token':self.owner}

    def check_profile(self):
        if self.context.profile.is_symlink():raise ValueError('Browser profile is an unknown alias')
        if self.context.marker.exists() or self.context.marker.is_symlink():
            value=private_json(self.context.marker);owner=value.get('owner_token')
            if HEX.fullmatch(str(owner)) is None or value!={**self.marker_value(),'owner_token':owner}:raise ValueError('Browser profile belongs to another runtime')
        elif self.context.profile.exists() and any(self.context.profile.iterdir()):raise ValueError('Existing host browser profile requires its original runtime')

    def prepare_profile(self):
        self.require_lock()
        if self.record() is not None:raise ValueError('Recover the previous browser before preparing its profile')
        self.check_profile()
        self.context.profile.mkdir(exist_ok=True,mode=0o700)
        atomic_json(self.context.marker,self.marker_value())

    def record(self):
        path=self.context.record
        if not path.exists() and not path.is_symlink():return None
        return checked_record(private_json(path),self.context.key)

    def observed(self,record):
        named=self.engine.inspect(name=self.context.name)
        if record is None or not record['container_id']:return named
        if named is not None and named.get('Id')!=record['container_id']:raise ValueError('Browser name is occupied by another container')
        return self.engine.inspect(identity=record['container_id'])

    def require_owner(self,expected):
        if expected is None:return
        record=self.record()
        if record is not None and record['token']!=expected:raise BrowserOwnerChanged('Browser launch was superseded')
        self.check_profile()
        if self.context.marker.exists() and private_json(self.context.marker)['owner_token']!=expected:raise BrowserOwnerChanged('Browser profile owner changed')

    def recover(self,*,expected_owner=None):
        self.require_lock()
        self.require_owner(expected_owner)
        record=self.record();container=self.observed(record)
        if container is not None:
            if record is None:raise ValueError('Browser container has no owned launch record')
            owned_container(container,record,self.context.key)
            self.engine.remove(container['Id'])
            if self.engine.inspect(identity=container['Id']) is not None:raise ValueError('Browser container has not stopped')
        if record is not None:self.context.record.unlink()

    def create(self,args):
        self.require_lock()
        if self.record() is not None:raise ValueError('Observe the previous browser launch before creating another')
        if self.engine.inspect(name=self.context.name) is not None:raise ValueError('Browser container name is already occupied')
        self.prepare_profile()
        # The previous container is proven absent under the profile's kernel lock.
        # Validate the entire set before unlinking anything or creating a process.
        for link in stale_profile_links(self.context.profile):link.unlink()
        record={'format':1,'profile_hash':self.context.key,'image_id':self.context.image_id,'token':self.owner,'container_id':''}
        atomic_json(self.context.record,record)
        identity=self.engine.create(self.context,record,args)
        if HEX.fullmatch(str(identity)) is None:raise ValueError('Browser creation returned an unknown identity')
        record['container_id']=identity;atomic_json(self.context.record,record)
        return identity

    def erase_profile(self,*,expected_owner=None):
        self.recover(expected_owner=expected_owner);self.check_profile()
        if self.context.profile.exists():shutil.rmtree(self.context.profile)
