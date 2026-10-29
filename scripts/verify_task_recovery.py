"""Real worker subprocess fault tests; only a labelled disposable PG and local model.
No real devices, model APIs, channels or production state are used.
"""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import signal
import subprocess
import tempfile
import threading
import time
import uuid
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from database import prepare,literal
from state_backup import psql,private_json

ROOT=Path(__file__).resolve().parent.parent
CHILD='worker::process_recovery_tests::worker_child'

class Model:
    def __init__(self):
        self.counts={};self.seen={};self.hold=threading.Event();self.release=threading.Event();self.long_hold=threading.Event();self.long_release=threading.Event()
        model=self
        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):
                body=json.loads(self.rfile.read(int(self.headers['content-length'])))
                name=body['model'];index=model.counts.get(name,0)+1;model.counts[name]=index;model.seen.setdefault(name,[]).append(body)
                # A request interrupted before checkpointing has no tool result.
                # The mock must resume from persisted context, not its HTTP counter.
                calls={item.get('call_id') for item in body.get('input',[]) if isinstance(item,dict) and item.get('name')=='memory_save'}
                completed={item.get('call_id') for item in body.get('input',[]) if isinstance(item,dict) and item.get('type')=='function_call_output' and item.get('call_id') in calls and '已保存记忆' in json.dumps(item.get('output'),ensure_ascii=False)}
                # The Responses compatibility adapter archives tools as user input
                # records. Count the confirmed records in that representation too.
                for item in body.get('input',[]):
                    if not isinstance(item,dict):continue
                    parts=item.get('content',[])
                    if not isinstance(parts,list):continue
                    texts=[part.get('text','') for part in parts if isinstance(part,dict)]
                    if texts and 'tool: memory_save\n' in texts[0] and '已保存记忆' in texts[1:]:completed.add(texts[0])
                long_index=len(completed)+1
                if name=='long' and index==17:model.long_hold.set();model.long_release.wait(30)
                if name=='database-outage':model.hold.set();model.release.wait(30)
                if name=='transient' and index<3:
                    self.send_response(503 if index==1 else 429);self.end_headers();self.wfile.write(b'{"error":{"code":"rate_limit"}}');return
                if name=='quota':
                    self.send_response(429);self.end_headers();self.wfile.write(b'{"error":{"code":"insufficient_quota"}}');return
                def call(tool,args):return {'type':'function_call','name':tool,'arguments':json.dumps(args),'call_id':name+'-'+str(index)}
                if name=='wait':output=[call('task_wait',{'seconds':12,'reason':'等待本地恢复演练计时'})] if index==1 else [call('task_finish',{'status':'needs_input','summary':'计时验证已返回，等待维护者核对校验值。'})]
                elif name=='long' and long_index<=56:output=[call('memory_save',{'content':'isolated memory '+str(long_index)})]
                elif name=='long':output=[call('task_finish',{'status':'needs_input','summary':'56轮检查点已走完，等待维护者核对校验值。'})]
                elif name=='stream' and index==1:output=[call('memory_save',{'content':'FORBIDDEN_PARTIAL_STREAM'})]
                elif name=='database-outage':output=[call('memory_save',{'content':'must-not-run-during-checkpoint-failure'})]
                else:output=[{'type':'message','role':'assistant','content':[{'type':'output_text','text':'fixture reply'}]}]
                self.send_response(200);self.send_header('Content-Type','text/event-stream');self.end_headers()
                event={'type':'response.output_item.done','item':output[0]} if name=='stream' and index==1 else {'type':'response.completed','response':{'output':output}}
                try:self.wfile.write(('data: '+json.dumps(event)+'\n\n').encode());self.wfile.flush()
                except (BrokenPipeError,ConnectionResetError):pass
            def log_message(self,*args):pass
        self.server=ThreadingHTTPServer(('127.0.0.1',0),Handler);self.thread=threading.Thread(target=self.server.serve_forever,daemon=True);self.thread.start()
    def close(self):self.release.set();self.long_release.set();self.server.shutdown();self.server.server_close();self.thread.join()

class Driver:
    def __init__(self,args,folder,model):
        self.args=args;self.folder=Path(folder);self.model=model;self.processes=[];self.names=[];self.reports=[]
        label=subprocess.check_output(['docker','inspect','-f','{{index .Config.Labels "dev.aiphone.test"}}',args.pg_container],text=True).strip()
        if label!='permissions' or not args.pg_container.startswith('aiphone-') or args.pg_port==55482:raise ValueError('Explicit disposable test PostgreSQL required')
    def sql(self,db,sql):return psql(self.args.pg_container,'ai_phone',db,sql)
    def fixture(self,name):
        database='aiphone_recovery_'+uuid.uuid4().hex;self.names.append(database);self.sql('ai_phone','CREATE DATABASE "'+database+'";')
        root=self.folder/name;root.mkdir(mode=0o700);(root/'.isolated-recovery-test').touch();(root/'.env').write_text('POSTGRES_DB='+database+'\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=isolated-fixture-password\n');(root/'.env').chmod(0o600)
        # All fixture databases share the same server roles and stable fixture credentials.
        # Reuse this run's initial login file, avoiding rotation under other test workers.
        shared=self.folder/'role-credentials.json'
        if shared.exists():
            value=json.loads(shared.read_text());value['database']=database;private_json(root/'data/database/credentials.json',value)
        prepare(root,self.args.pg_container)
        if not shared.exists():shared.write_bytes((root/'data/database/credentials.json').read_bytes());shared.chmod(0o600)
        env=self.env(root);r=subprocess.run([ROOT/'target/debug/ai-phone-assistant','--migrate'],env=env,capture_output=True)
        if r.returncode:raise ValueError('Fixture migration failed')
        private_json(root/'data/model/credentials.json',{'api_key':'','base_url':f'http://127.0.0.1:{self.model.server.server_port}/v1','model':name,'provider':'compatible','protocol':'responses'})
        conversation=self.sql(database,'SELECT id FROM conversations LIMIT 1;');message=str(uuid.uuid4());run=str(uuid.uuid4())
        self.sql(database,f"INSERT INTO messages(id,conversation_id,role,text) VALUES('{message}','{conversation}','assistant',''); INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,model_revision,context) VALUES('{run}','{conversation}','{message}','{uuid.uuid4()}','fixture','queued',1,{literal(json.dumps([{'role':'user','content':[{'type':'input_text','text':'你好，这是一轮本地校验。'}]}]))}::jsonb);")
        return root,database,run,message
    def env(self,root):return {**os.environ,'AI_PHONE_RECOVERY_CHILD':'1','AI_PHONE_PROJECT_ROOT':str(root),'AI_PHONE_PG_PORT':str(self.args.pg_port),'RUST_TEST_THREADS':'1'}
    def start(self,root):
        log=(root/('worker-'+uuid.uuid4().hex[:8]+'.log')).open('wb');p=subprocess.Popen([self.args.test_binary,'--exact',CHILD,'--ignored','--nocapture'],env=self.env(root),stdout=log,stderr=subprocess.STDOUT);self.processes.append((p,log));return p
    def stop(self,p):
        if p.poll() is None:p.kill();p.wait(timeout=10)
    def row(self,db,run):return json.loads(self.sql(db,f"SELECT row_to_json(x) FROM (SELECT status,step,loop_state,pending_tools,extract(epoch FROM next_wake_at)*1000 wake FROM runs WHERE id='{run}') x;"))
    def wait(self,predicate,timeout=35):
        deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            try:
                value=predicate()
                if value:return value
            except (ValueError,json.JSONDecodeError):pass
            for p,_ in self.processes:
                if p.poll() is not None and p.returncode not in (-9,0):raise ValueError('Worker subprocess failed; inspect private logs')
            time.sleep(.15)
        raise ValueError('Recovery test timed out; inspect private logs')
    def test_wait_and_completed_replay(self):
        root,db,run,message=self.fixture('wait');action=str(uuid.uuid4());call={'type':'function_call','call_id':'cached-write','local_action_id':action,'name':'workspace_write','arguments':json.dumps({'path':'never-rewrite.txt','content':'WRONG_REPLAY'})}
        context=[{'role':'user','content':[{'type':'input_text','text':'你好，这是一轮本地校验。'}]},call]
        self.sql(db,f"UPDATE runs SET context={literal(json.dumps(context))}::jsonb,pending_tools={literal(json.dumps([call]))}::jsonb WHERE id='{run}'; INSERT INTO harness_events(id,run_id,message_id,kind,title,status,result) VALUES('{action}','{run}','{message}','tool','cached','completed','[{{\"type\":\"input_text\",\"text\":\"{{\\\"success\\\":true}}\"}}]'::jsonb);")
        file=root/'data/workspaces/admin/never-rewrite.txt';file.parent.mkdir(parents=True);file.write_text('ORIGINAL_COMPLETED_RESULT');before=file.stat().st_mtime_ns
        p=self.start(root);row=self.wait(lambda:self.row(db,run) if self.row(db,run)['wake'] else None);wake=row['wake'];self.stop(p)
        p=self.start(root);time.sleep(.7);assert self.row(db,run)['wake']==wake and self.model.counts['wait']==1
        # A real database restart happens while the persisted wait is pending.
        subprocess.run(['docker','restart',self.args.pg_container],check=True,stdout=subprocess.DEVNULL)
        self.wait(lambda:self.sql('ai_phone','SELECT 1;')=='1');assert self.row(db,run)['wake']==wake
        p.send_signal(signal.SIGSTOP);time.sleep(1);p.send_signal(signal.SIGCONT)
        self.wait(lambda:self.row(db,run)['status']=='paused',timeout=25);self.stop(p)
        assert file.read_text()=='ORIGINAL_COMPLETED_RESULT' and file.stat().st_mtime_ns==before
        assert self.sql(db,f"SELECT count(*) FROM harness_events WHERE run_id='{run}' AND title='等待结束';")=='1'
        assert self.model.counts['wait']==2
        self.reports.append('SIGKILL/restart + PostgreSQL restart + SIGSTOP/resume preserve wait deadline, append one wake event and do not repeat a completed write')
    def test_unknown_not_replayed(self):
        root,db,run,message=self.fixture('unknown');action=str(uuid.uuid4());call={'type':'function_call','call_id':'unknown-write','local_action_id':action,'name':'workspace_write','arguments':json.dumps({'path':'forbidden.txt','content':'DO_NOT_RUN'})}
        context=[{'role':'user','content':[{'type':'input_text','text':'你好'}]},call]
        self.sql(db,f"UPDATE runs SET status='running',context={literal(json.dumps(context))}::jsonb,pending_tools={literal(json.dumps([call]))}::jsonb WHERE id='{run}'; INSERT INTO harness_events(id,run_id,message_id,kind,title,status) VALUES('{action}','{run}','{message}','tool','unknown','running');")
        p=self.start(root);self.wait(lambda:self.row(db,run)['status']=='paused');self.stop(p)
        assert not (root/'data/workspaces/admin/forbidden.txt').exists() and self.model.counts.get('unknown',0)==0
        self.reports.append('Interrupted side effect with no recorded result pauses without execution or model call')
    def test_provider_retry_stream_and_quota(self):
        for name in ['transient','stream','quota']:
            root,db,run,message=self.fixture(name);p=self.start(root)
            if name=='transient':
                self.wait(lambda:self.row(db,run)['loop_state'].get('model_retries')==1);wake=self.row(db,run)['wake'];self.stop(p);p=self.start(root)
                assert self.row(db,run)['wake']==wake
            self.wait(lambda:self.row(db,run)['status'] in ('succeeded','paused'),timeout=35);row=self.row(db,run);self.stop(p)
            assert row['status']==('paused' if name=='quota' else 'succeeded')
            assert self.sql(db,'SELECT count(*) FROM memories;')=='0'
            assert self.model.counts[name]=={'transient':3,'stream':2,'quota':1}[name]
        self.reports.append('503/429 retry counters survive SIGKILL; partial SSE never executes a tool; explicit exhausted quota pauses without retry storm')
    def test_database_outage_result_persists(self):
        root,db,run,message=self.fixture('database-outage');p=self.start(root)
        assert self.model.hold.wait(15)
        subprocess.run(['docker','stop','--time','3',self.args.pg_container],check=True,stdout=subprocess.DEVNULL)
        self.model.release.set()
        self.wait(lambda:any('holding the checkpoint' in x.read_text(errors='ignore') for x in root.glob('worker-*.log')),timeout=20)
        assert p.poll() is None
        subprocess.run(['docker','start',self.args.pg_container],check=True,stdout=subprocess.DEVNULL)
        self.wait(lambda:self.sql('ai_phone','SELECT 1;')=='1')
        self.wait(lambda:self.row(db,run)['status'] in ('failed','paused'));self.stop(p)
        assert self.sql(db,'SELECT count(*) FROM memories;')=='0'
        self.reports.append('Actual PostgreSQL outage during worker completion waits for recovery and saves a terminal state; no stuck running task or replayed tools')
    def test_long_run(self):
        root,db,run,message=self.fixture('long');p=self.start(root)
        assert self.model.long_hold.wait(35), 'Model did not reach the interrupted request boundary'
        assert self.sql(db,'SELECT count(*) FROM memories;')=='16'
        self.stop(p);self.model.long_release.set();p=self.start(root)
        self.wait(lambda:self.row(db,run)['status']=='paused',timeout=55);row=self.row(db,run);self.stop(p)
        assert row['step']>=56 and self.sql(db,'SELECT count(*) FROM memories;')=='56'
        assert self.sql(db,'SELECT count(DISTINCT content) FROM memories;')=='56'
        self.reports.append('56 committed tool results cross slices and SIGKILL during a model request; interrupted response is retried from durable context, all unique memory effects retained once')
    def close(self):
        self.model.release.set();self.model.long_release.set()
        for p,log in self.processes:self.stop(p);log.close()
        subprocess.run(['docker','start',self.args.pg_container],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        self.wait(lambda:self.sql('ai_phone','SELECT 1;')=='1')
        for name in self.names:self.sql('ai_phone','DROP DATABASE "'+name+'" WITH (FORCE);')

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('--pg-container',required=True);parser.add_argument('--pg-port',required=True,type=int);parser.add_argument('--test-binary',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);args=parser.parse_args();args.test_binary=args.test_binary.resolve()
    args.output.mkdir(parents=True,exist_ok=False,mode=0o700);model=Model();driver=Driver(args,args.output,model)
    try:
        for test in [driver.test_wait_and_completed_replay,driver.test_unknown_not_replayed,driver.test_provider_retry_stream_and_quota,driver.test_database_outage_result_persists,driver.test_long_run]:
            print('RUN '+test.__name__,flush=True);test();print('PASS '+test.__name__,flush=True)
        report={'passed':driver.reports,'host_sleep_reboot_verified':False,'physical_power_loss_verified':False,'production_devices_or_conversations_changed':False}
        private_json(args.output/'report.json',report);print(json.dumps(report),flush=True)
    finally:driver.close();model.close()

if __name__=='__main__':main()
