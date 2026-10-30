"""Linux/macOS CI integration: disposable PostgreSQL only, no devices or real models."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

ROOT=Path(__file__).resolve().parent.parent

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--container',required=True);parser.add_argument('--port',required=True,type=int);args=parser.parse_args()
    if not args.container.startswith('aiphone-') or args.port==55482:raise ValueError('Disposable container and non-production port required')
    label=subprocess.check_output(['docker','inspect','-f','{{index .Config.Labels "dev.aiphone.test"}}',args.container],text=True).strip()
    if label!='permissions':raise ValueError('Disposable test container label required')
    bindings=json.loads(subprocess.check_output(['docker','inspect','-f','{{json .HostConfig.PortBindings}}',args.container],text=True))
    if not any(binding.get('HostIp')=='127.0.0.1' and binding.get('HostPort')==str(args.port) for binding in bindings.get('5432/tcp',[])):
        raise ValueError('A fixed loopback PostgreSQL port matching --port is required; random ports change during recovery restart tests')
    for attempt in range(40):
        # The official image briefly starts a socket-only bootstrap server. Wait
        # for TCP so role creation cannot race its shutdown during initialization.
        if subprocess.run(['docker','exec',args.container,'pg_isready','-h','127.0.0.1','-U','ai_phone','-d','ai_phone'],capture_output=True).returncode==0:break
        time.sleep(.5)
    else:raise ValueError('Disposable PostgreSQL did not become ready')
    # The persistence regressions deliberately reject the production-role username.
    # Provision their separate fixture login only inside this verified test container.
    subprocess.run(['docker','exec','-i',args.container,'psql','-X','-v','ON_ERROR_STOP=1','-U','ai_phone','-d','ai_phone'],
        input="""DO $fixture$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='aiphone_test') THEN CREATE ROLE aiphone_test LOGIN; END IF;
 ALTER ROLE aiphone_test WITH LOGIN NOSUPERUSER CREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD 'isolated-persistence-fixture';
END $fixture$;""",text=True,stdout=subprocess.DEVNULL,check=True)
    env={**os.environ,'AI_PHONE_TEST_DATABASE_URL':f'postgres://ai_phone:isolated-fixture-password@127.0.0.1:{args.port}/ai_phone',
         'AI_PHONE_TEST_PERSISTENCE_DATABASE_URL':f'postgres://aiphone_test:isolated-persistence-fixture@127.0.0.1:{args.port}/ai_phone',
         'AI_PHONE_TEST_PG_CONTAINER':args.container,'AI_PHONE_TEST_PG_PORT':str(args.port)}
    subprocess.run(['cargo','build','-p','ai-phone-assistant','--locked'],cwd=ROOT,env=env,check=True)
    result=subprocess.run(['cargo','test','-p','ai-phone-assistant','--no-run','--locked','--message-format=json'],cwd=ROOT,env=env,capture_output=True,text=True,check=True)
    binaries=[item['executable'] for line in result.stdout.splitlines() if (item:=json.loads(line)).get('reason')=='compiler-artifact' and item.get('executable') and item['target']['name']=='ai-phone-assistant']
    if len(binaries)!=1:raise ValueError('Expected exactly one assistant test executable')
    binary=binaries[0]
    # Integration tests and the fault driver deliberately share a fixture cluster:
    # keep them sequential, because the latter restarts it.
    subprocess.run([binary,'--ignored','--skip','model_adapter::tests::live_model_capabilities','--skip','worker::process_recovery_tests::worker_child','--test-threads=1'],cwd=ROOT,env=env,check=True)
    for name in ['test_database.py','test_state_backup.py']:
        subprocess.run([sys.executable,'-m','unittest','discover','-s','scripts','-p',name],cwd=ROOT,env=env,check=True)
    with tempfile.TemporaryDirectory(prefix='aiphone-ci-recovery-') as folder:
        subprocess.run([sys.executable,'scripts/verify_task_recovery.py','--pg-container',args.container,'--pg-port',str(args.port),'--test-binary',binary,'--output',str(Path(folder)/'run')],cwd=ROOT,env=env,check=True)
    print('PASS: isolated database, permissions, restore and worker-process recovery; no production device or provider calls')

if __name__=='__main__':main()
