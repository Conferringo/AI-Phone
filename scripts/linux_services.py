"""Transitional imports for existing installers; service control is owned by Rust."""
import json
from pathlib import Path
import subprocess
import sys
from prebuilt_services import supervisor_command


def _control(root, name, operation, spec=None):
    root=Path(root).resolve()
    if operation!='start' and not (root/'data/services').is_dir():
        return None if operation=='status' else False
    result=subprocess.run([*supervisor_command(root),'--service-control',str(root),name,operation],
                          input=json.dumps(spec) if spec is not None else '',capture_output=True,text=True,cwd=root)
    if result.returncode:
        sys.stderr.write(result.stderr)
        if 'service_configuration_changed' in result.stderr:
            raise ValueError('A running task has a different service configuration; stop the owned installation before upgrading')
        raise ValueError('Native service control failed; inspect the private service log')
    return json.loads(result.stdout)


def start_unit(root, name, command, environment):
    return _control(root,name,'start',{'command':list(map(str,command)),'environment':environment})


def unit_state(root, name):
    return _control(root,name,'status')


def stop_unit(root, name):
    return _control(root,name,'stop')
