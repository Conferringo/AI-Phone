import {test,expect} from 'bun:test';
import {create,fromBinary,toBinary} from '@bufbuild/protobuf';
import {ChannelBindRequestSchema,ChannelListRequestSchema,ChannelListResponseSchema,ChannelUnbindRequestSchema} from '@ai-phone/protocol/ai_phone/v1/channels_pb';
import {mkdtempSync,mkdirSync,writeFileSync,existsSync,readFileSync,rmSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';

test('real protobuf channel server isolates QR sessions, preserves IDs and rejects overlapping mutations',async()=>{
 const root=resolve(import.meta.dir,'../..'),fixture=mkdtempSync(join(tmpdir(),'aiphone-channel-rpc-'));
 mkdirSync(join(fixture,'data/assistant'),{recursive:true});writeFileSync(join(fixture,'data/assistant/token'),'fixture-only-token');
 const preload=join(fixture,'fixture.ts');
 writeFileSync(preload,`
 import {mock} from 'bun:test';
 import {writeFileSync} from 'node:fs';
 import {create,toBinary} from ${JSON.stringify(import.meta.resolve('@bufbuild/protobuf'))};
 import {RegistryStateSchema} from ${JSON.stringify(resolve(root,'packages/protocol/src/gen/ai_phone/v1/registry_pb.ts'))};
 const serve=Bun.serve;Bun.serve=(options)=>serve({...options,port:0});
 globalThis.fetch=async(url)=>{if(String(url)!=='http://127.0.0.1:17863/v1/registry/state')throw Error('No outbound calls allowed in fixture');return new Response(toBinary(RegistryStateSchema,create(RegistryStateSchema,{agents:[{id:'one'},{id:'two'}]})));};
 mock.module(${JSON.stringify(resolve(root,'apps/channels/whatsapp.ts'))},()=>({startWhatsApp:()=>({state:'waiting_qr',error:'',qrPng:new Uint8Array([137,80,78,71]),async stop(){},async logout(){writeFileSync(${JSON.stringify(join(fixture,'logout-started'))},'yes');await new Promise(r=>setTimeout(r,120));return false;},async send(){throw Error('No messages permitted');},async sendImage(){throw Error('No images permitted');}})}));
 `);
 const child=Bun.spawn([process.execPath,'--preload',preload,resolve(root,'apps/channels/server.ts')],{cwd:root,env:{...process.env,AI_PHONE_PROJECT_ROOT:fixture,AI_PHONE_WHATSAPP_CHROMIUM:process.execPath},stdout:'pipe',stderr:'pipe'});
 try{
  const reader=child.stdout.getReader();let output='',port=0;const end=Date.now()+5000;
  while(Date.now()<end&&!port){const result=await Promise.race([reader.read(),new Promise<never>((_,reject)=>setTimeout(()=>reject(Error('fixture startup timeout')),5000))]);if(result.done)throw Error('fixture exited: '+await new Response(child.stderr).text());output+=new TextDecoder().decode(result.value);port=Number(output.match(/127\.0\.0\.1:(\d+)/)?.[1]);}
  reader.releaseLock();expect(port).toBeGreaterThan(0);
  async function rpc(operation:string,schema:any,value:any,token='fixture-only-token'){
   return fetch(`http://127.0.0.1:${port}/v1/${operation}`,{method:'POST',headers:{authorization:'Bearer '+token,'content-type':'application/x-protobuf'},body:toBinary(schema,create(schema,value))});
  }
  const list=async(agentId:string)=>{const r=await rpc('list',ChannelListRequestSchema,{agentId});expect(r.status).toBe(200);return fromBinary(ChannelListResponseSchema,new Uint8Array(await r.arrayBuffer()));};
  expect((await rpc('list',ChannelListRequestSchema,{agentId:'one'},'wrong')).status).toBe(401);
  const bind=await rpc('bind',ChannelBindRequestSchema,{agentId:'one',provider:'whatsapp'});expect(bind.status).toBe(200);
  let first=(await list('one')).bindings.find(v=>v.provider==='whatsapp')!;expect(first.configured).toBe(true);expect(first.qrPng).toEqual(new Uint8Array([137,80,78,71]));const id=first.appId;
  expect((await list('two')).bindings.find(v=>v.provider==='whatsapp')!.configured).toBe(false);
  expect((await rpc('bind',ChannelBindRequestSchema,{agentId:'one',provider:'whatsapp'})).status).toBe(200);
  first=(await list('one')).bindings.find(v=>v.provider==='whatsapp')!;expect(first.appId).toBe(id);
  const unbind=rpc('unbind',ChannelUnbindRequestSchema,{agentId:'one',provider:'whatsapp'});
  for(let i=0;i<50&&!existsSync(join(fixture,'logout-started'));i++)await new Promise(r=>setTimeout(r,5));
  expect(existsSync(join(fixture,'logout-started'))).toBe(true);
  expect((await rpc('bind',ChannelBindRequestSchema,{agentId:'one',provider:'whatsapp'})).status).toBe(409);
  expect((await unbind).status).toBe(200);
  first=(await list('one')).bindings.find(v=>v.provider==='whatsapp')!;expect(first.configured).toBe(false);expect(first.error).toContain('未确认');
  expect(readFileSync(join(fixture,'data/channels/vault.bin')).subarray(0,1).toString()).not.toBe('{');
 }finally{child.kill('SIGTERM');await child.exited;rmSync(fixture,{recursive:true,force:true});}
},15000);
