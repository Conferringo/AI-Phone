"""Small Docker boundary for the owned browser; never pulls or publishes ports."""
import csv
import io
import json
import os
from pathlib import Path
import re
import subprocess
from browser_lease import HEX


def mount_argument(profile,*,readonly=False):
    fields=['type=bind','source='+str(profile),'target=/profile']
    if readonly:fields.append('readonly')
    output=io.StringIO();csv.writer(output,lineterminator='').writerow(fields)
    return output.getvalue()


def inspect_selector(name,identity):
    if (name is None)==(identity is None):raise ValueError('Select exactly one browser container')
    if identity is not None:
        if HEX.fullmatch(identity) is None:raise ValueError('Invalid browser container identity')
        return 'id='+identity
    if re.fullmatch('ai-phone-browser-[0-9a-f]{24}',name) is None:raise ValueError('Invalid browser container name')
    return 'name=^/'+name+'$'


def checked_container(value,identity,name):
    if not isinstance(value,list) or len(value)!=1 or not isinstance(value[0],dict):raise ValueError('Invalid browser container response')
    row=value[0]
    if row.get('Id')!=identity or not isinstance(row.get('Config'),dict):raise ValueError('Browser container response differs from selection')
    if name is not None and row.get('Name')!='/'+name:raise ValueError('Browser container name differs from selection')
    return row


class DockerEngine:
    def __init__(self,executable=None,*,runner=None,network='bridge'):
        if network not in ('bridge','none'):raise ValueError('Invalid browser network')
        self.executable=executable or os.environ.get('AI_PHONE_DOCKER','docker')
        self.command=runner or self.execute;self.network=network

    def execute(self,args):
        result=subprocess.run([self.executable,*args],capture_output=True,encoding='utf-8',timeout=15)
        if result.returncode:raise RuntimeError('Browser container command failed')
        if len(result.stdout)>1024**2:raise ValueError('Browser container response exceeds its budget')
        return result.stdout.strip()

    def inspect(self,*,name=None,identity=None):
        selection=inspect_selector(name,identity)
        output=self.command(['container','ls','--all','--no-trunc','--filter',selection,'--format','{{.ID}}'])
        if not output:return None
        if HEX.fullmatch(output) is None:raise ValueError('Ambiguous browser container selection')
        value=json.loads(self.command(['inspect',output]))
        return checked_container(value,output,name)

    def create(self,context,record,args):
        user=self.profile_user(context)
        labels={'dev.aiphone.component':'channel-browser','dev.aiphone.profile':context.key,'dev.aiphone.launch':record['token']}
        command=['create','--rm','--init','--interactive','--name',context.name,'--hostname',context.name,'--pull','never','--network',self.network,
                 '--memory','768m','--cpus','1','--shm-size','128m','--cap-drop','ALL','--security-opt','no-new-privileges',
                 '--mount',mount_argument(context.profile),'--user',user,
                 '--env','XDG_CONFIG_HOME=/profile/.config','--env','XDG_CACHE_HOME=/profile/.cache','--env','XDG_DATA_HOME=/profile/.local/share',
                 '--entrypoint','/usr/bin/python3']
        for key,value in labels.items():command+=['--label',key+'='+value]
        script=Path(__file__).with_name('browser_pipe.py').read_text(encoding='utf-8')
        identity=self.command([*command,context.image_id,'-c',script,'--guest',*args])
        if HEX.fullmatch(identity) is None:raise ValueError('Browser creation outcome is unknown')
        return identity

    def profile_user(self,context):
        # Ask the selected engine's namespace, including rootless/desktop mappings.
        # Do not make account files root-owned or change permissions to share them.
        script='import json,os; s=os.stat("/profile"); print(json.dumps([s.st_uid,s.st_gid]))'
        output=self.command(['run','--rm','--network','none','--pull','never','--cap-drop','ALL',
            '--mount',mount_argument(context.profile,readonly=True),'--entrypoint','/usr/bin/python3',context.image_id,'-c',script])
        ids=json.loads(output)
        if not isinstance(ids,list) or len(ids)!=2 or any(type(value) is not int or not 0<=value<2**32-1 for value in ids):raise ValueError('Unknown browser profile user mapping')
        return ':'.join(map(str,ids))

    def attach(self,identity):
        if HEX.fullmatch(identity) is None:raise ValueError('Invalid browser container identity')
        return subprocess.Popen([self.executable,'start','--attach','--interactive',identity],stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,bufsize=0)

    def remove(self,identity):
        if HEX.fullmatch(identity) is None:raise ValueError('Invalid browser container identity')
        current=self.inspect(identity=identity)
        if current is None:return
        state=current.get('State',{})
        if type(state.get('Running')) is not bool:raise ValueError('Unknown browser container state')
        if state['Running']:self.command(['stop','--time','5',identity])
        try:self.command(['rm','--force',identity])
        except RuntimeError:
            if self.inspect(identity=identity) is not None:raise

    def check(self,image_id):
        if re.fullmatch('sha256:[0-9a-f]{64}',image_id) is None:raise ValueError('Invalid browser image')
        output=self.command(['run','--rm','--network','none','--pull','never','--cap-drop','ALL','--entrypoint','/usr/bin/chromium',image_id,'--version'])
        if not re.fullmatch(r'Chromium \d+\.\d+\.\d+\.\d+.*',output):raise ValueError('Browser image has no qualified Chromium')
        return output
