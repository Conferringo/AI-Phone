import {expect,test} from 'bun:test';
import {createServer} from 'node:net';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';

async function freePort(){
 const listener=createServer();await new Promise<void>(resolve=>listener.listen(0,'127.0.0.1',resolve));
 const address=listener.address();if(!address||typeof address==='string')throw Error('No fixture port');
 await new Promise<void>(resolve=>listener.close(()=>resolve()));return address.port;
}

test('media HTTP service starts without Android SDK and rejects unavailable phone capture',async()=>{
 const folder=mkdtempSync(join(tmpdir(),'aiphone-media-lazy-')),runner=join(folder,'runner');mkdirSync(runner);
 writeFileSync(join(runner,'token'),'qualification-fixture-token',{mode:0o600});
 const port=await freePort(),origin=`http://127.0.0.1:${port}`;
 const child=Bun.spawn([process.execPath,join(import.meta.dir,'server.ts')],{
  env:{...process.env,AI_PHONE_PROJECT_ROOT:folder,AI_PHONE_RUNNER_DIR:runner,AI_PHONE_SDK_ROOT:join(folder,'missing-sdk'),AI_PHONE_MEDIA_PORT:String(port)},
  stdout:'ignore',stderr:'ignore',stdin:'ignore',
 });
 try{
  let responding=false;const until=Date.now()+3000;
  while(Date.now()<until&&child.exitCode===null){
   try{responding=(await fetch(origin+'/cursor',{signal:AbortSignal.timeout(100)})).status===401;if(responding)break;}catch{}
   await Bun.sleep(25);
  }
  expect(responding).toBe(true);
  const response=await fetch(origin+'/snapshot',{headers:{authorization:'Bearer qualification-fixture-token'},signal:AbortSignal.timeout(2000)});
  expect(response.status).toBe(503);
  expect(child.exitCode).toBe(null);
 }finally{
  child.kill('SIGTERM');await child.exited;rmSync(folder,{recursive:true,force:true});
 }
});
