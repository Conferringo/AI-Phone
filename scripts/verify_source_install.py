"""Build an actual source envelope with independent dependencies and an empty test DB.

This verifies installation preparation on this host, not a clean physical machine.
It never starts launchd jobs, devices, channels or model requests.
"""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid
from database import prepare
from release import configure,default_profile
from release_package import create_package
from release_stage import stage_bundle
from state_backup import psql,digest,private_json

ROOT=Path(__file__).resolve().parent.parent

def verify(output,container,port):
    output=Path(output).resolve()
    if output.exists():raise ValueError('Verification output must be a new private directory')
    label=subprocess.check_output(['docker','inspect','-f','{{index .Config.Labels "dev.aiphone.test"}}',container],text=True).strip()
    if label!='permissions' or not container.startswith('aiphone-'):raise ValueError('An explicitly labelled disposable PostgreSQL container is required')
    output.mkdir(parents=True,mode=0o700);name='aiphone_install_'+uuid.uuid4().hex
    psql(container,'ai_phone','ai_phone','CREATE DATABASE "'+name+'";')
    try:
        archive=create_package(ROOT,output/'bundle',public=False)
        stage=output/'staged';source=stage_bundle(archive,stage)
        profile=output/'profile.json';private_json(profile,default_profile(source));configure(source,profile)
        # Fixture-only credentials, never a production password or existing account.
        (source/'.env').write_text(f'POSTGRES_DB={name}\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=isolated-fixture-password\n');(source/'.env').chmod(0o600)
        env={**os.environ,'CARGO_BUILD_JOBS':'2','NEXT_TELEMETRY_DISABLED':'1','AI_PHONE_PROJECT_ROOT':str(source),'AI_PHONE_PG_PORT':str(port)}
        steps=[('bootstrap',['./scripts/bootstrap.sh']),('contracts-types-workspace',['./scripts/bun','run','check']),('rust-build',['cargo','build','--workspace','--locked']),('production-web',['./scripts/bun','run','build'])]
        completed=[]
        for phase,args in steps:
            print('Verifying '+phase,flush=True)
            with (output/(phase+'.log')).open('w') as log:
                result=subprocess.run(args,cwd=source,env=env,stdout=log,stderr=subprocess.STDOUT)
            if result.returncode:raise ValueError('Fresh source verification failed: '+phase+'; see private verification logs')
            completed.append(phase)
        prepare(source,container)
        for flag in ['--migrate','--database-check','--sync-builtin-skills']:
            with (output/(flag[2:]+'.log')).open('w') as log:
                result=subprocess.run([source/'target/debug/ai-phone-assistant',flag],env=env,stdout=log,stderr=subprocess.STDOUT)
            if result.returncode:raise ValueError('Fresh database initialization failed: '+flag)
        state=json.loads(psql(container,'ai_phone',name,"SELECT json_build_object('migrations',(SELECT count(*) FROM _sqlx_migrations),'messages',(SELECT count(*) FROM messages),'accounts',(SELECT count(*) FROM admin_account),'builtin_skills',(SELECT count(*) FROM runtime_skills WHERE builtin));"))
        if state['messages']!=0 or state['accounts']!=0 or state['builtin_skills']<1:raise ValueError('Fresh database unexpectedly contains user state or lacks built-in Skills')
        report={'scope':'same-host-isolated-source-install-preparation','archive_sha256':digest(archive),'independent_node_modules_and_target':True,'checks':completed+['empty-database-migrations','unprivileged-runtime','builtin-skills'],'database':state,'existing_guests_started_or_changed':False,'clean_physical_host_verified':False,'device_creation_and_account_setup_verified':False}
        private_json(output/'report.json',report);print(json.dumps(report),flush=True)
        return report
    finally:
        psql(container,'ai_phone','ai_phone','DROP DATABASE "'+name+'" WITH (FORCE);')
        # Only this script's isolated staging tree; retain bundle/reports/logs for review.
        if (output/'staged').is_dir():shutil.rmtree(output/'staged')

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--pg-container',required=True);parser.add_argument('--pg-port',required=True,type=int)
    args=parser.parse_args();verify(args.output,args.pg_container,args.pg_port)

if __name__=='__main__':
    try:main()
    except (ValueError,OSError,subprocess.SubprocessError) as error:raise SystemExit(str(error))
