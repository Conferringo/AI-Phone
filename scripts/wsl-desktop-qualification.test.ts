import {expect,test} from 'bun:test';
import {create,fromBinary,toBinary} from '@bufbuild/protobuf';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {RegistryRequestSchema,RegistryStateSchema,DeviceSaveRequestSchema} from '../packages/protocol/src/gen/ai_phone/v1/registry_pb';
import {DesktopProvisionRequestSchema,DesktopProvisionJobSchema,DesktopRequestSchema,DeviceLinkStateSchema,DesktopObservationSchema} from '../packages/protocol/src/gen/ai_phone/v1/desktop_pb';
import {qualificationEnvironment,qualifyDesktop} from './wsl-desktop-qualification';
const nonce='a'.repeat(32),agent='11111111-1111-4111-8111-111111111111',savedDevice='22222222-2222-4222-8222-222222222222';
const image=Uint8Array.from([137,80,78,71,13,10,26,10]);
const validEnv={AI_PHONE_WSL_QUALIFICATION_NONCE:nonce,AI_PHONE_WSL_QUALIFICATION_PHASE:'1',AI_PHONE_PROJECT_ROOT:'/var/lib/aiphone/installation/releases/'+'1'.repeat(64),DOCKER_HOST:'unix:///run/aiphone/docker.sock',container:'docker'};
function response(schema:any,value:any){return new Response(toBinary(schema,create(schema,value)),{headers:{'content-type':'application/x-protobuf'}});}
function fixture(){
 const root=mkdtempSync(join(tmpdir(),'aiphone-desktop-q-'));mkdirSync(join(root,'data/assistant'),{recursive:true});mkdirSync(join(root,'data/deployment'),{recursive:true});
 writeFileSync(join(root,'data/assistant/token'),'fixture-private-token',{mode:0o600});writeFileSync(join(root,'data/deployment/profile.json'),JSON.stringify({desktop:{cpus:2,memory_mb:2048}}));
 return {root,marker:join(root,'data/deployment/wsl-desktop-qualification.json'),remove(){rmSync(root,{recursive:true,force:true});}};
}
test('qualification CLI refuses production roots, missing nonce and foreign daemon',()=>{
 expect(qualificationEnvironment(validEnv,'linux')).toEqual({root:validEnv.AI_PHONE_PROJECT_ROOT,nonce,phase:1});
 for(const changed of [{AI_PHONE_WSL_QUALIFICATION_NONCE:''},{AI_PHONE_PROJECT_ROOT:'/tmp/production'},{DOCKER_HOST:'unix:///var/run/docker.sock'},{container:''},{AI_PHONE_WSL_QUALIFICATION_PHASE:'3'}])expect(()=>qualificationEnvironment({...validEnv,...changed},'linux')).toThrow();
 expect(()=>qualificationEnvironment(validEnv,'darwin')).toThrow();
});
test('first phase creates and provisions only its desktop, then observes through protobuf',async()=>{
 const f=fixture(),calls:string[]=[];let device='',job='';
 const request=async(url:string,init:any)=>{
  const op=new URL(url).pathname;calls.push(op);expect(init.headers.authorization).toBe('Bearer fixture-private-token');expect(init.headers['content-type']).toBe('application/x-protobuf');expect(init.proxy).toBe(false);
  const body=new Uint8Array(init.body);
  if(op==='/v1/registry/state')return response(RegistryStateSchema,{agents:[{id:agent}],devices:device?[{id:device,kind:'desktop',runtimeState:'managed'}]:[]});
  if(op==='/v1/registry/device-save'){const value=fromBinary(DeviceSaveRequestSchema,body);expect(value.kind).toBe('desktop');expect(value.defaultAgentId).toBe(agent);device=value.id;return response(RegistryStateSchema,{});}
  if(op==='/v1/desktop/provision'){const value=fromBinary(DesktopProvisionRequestSchema,body);expect(value.deviceId).toBe(device);expect(value.cpus).toBe(2);expect(value.memoryMb).toBe(2048);job=value.requestId;return response(DesktopProvisionJobSchema,{id:job,deviceId:device,status:'queued'});}
  if(op==='/v1/desktop/state'){fromBinary(RegistryRequestSchema,body);return response(DeviceLinkStateSchema,{jobs:[{id:job,deviceId:device,status:'completed'}]});}
  if(op==='/v1/desktop/observe'){expect(fromBinary(DesktopRequestSchema,body).deviceId).toBe(device);return response(DesktopObservationSchema,{deviceId:device,width:1280,height:800,png:image});}
  throw Error('Unexpected operation');
 };
 try{const result=await qualifyDesktop({root:f.root,nonce,phase:1},request);expect(result).toEqual({device_id:device,desktop_registered:true,desktop_observation:true});expect(JSON.parse(readFileSync(f.marker,'utf8'))).toEqual({nonce,device_id:device});expect(calls).toEqual(['/v1/registry/state','/v1/registry/device-save','/v1/desktop/provision','/v1/desktop/state','/v1/registry/state','/v1/desktop/observe']);}finally{f.remove();}
});
test('restart phase observes the same managed device without creating or provisioning',async()=>{
 const f=fixture();writeFileSync(f.marker,JSON.stringify({nonce,device_id:savedDevice}));const calls:string[]=[];
 try{const result=await qualifyDesktop({root:f.root,nonce,phase:2},async(url,init)=>{
  const op=new URL(url).pathname;calls.push(op);
  if(op==='/v1/registry/state')return response(RegistryStateSchema,{devices:[{id:savedDevice,kind:'desktop',runtimeState:'managed'}]});
  if(op==='/v1/desktop/observe')return response(DesktopObservationSchema,{deviceId:savedDevice,width:1280,height:800,png:image});
  throw Error('Restart must not mutate registry or provision');
 });expect(result.device_id).toBe(savedDevice);expect(calls).toEqual(['/v1/registry/state','/v1/desktop/observe']);}finally{f.remove();}
});
test('a preexisting desktop or stale marker cannot be adopted by phase one',async()=>{
 const f=fixture();try{
  await expect(qualifyDesktop({root:f.root,nonce,phase:1},async()=>response(RegistryStateSchema,{agents:[{id:agent}],devices:[{id:savedDevice,kind:'desktop'}]}))).rejects.toThrow('qualification_desktop_exists');expect(existsSync(f.marker)).toBe(false);
  writeFileSync(f.marker,JSON.stringify({nonce,device_id:savedDevice}));let called=false;
  await expect(qualifyDesktop({root:f.root,nonce,phase:1},async()=>{called=true;throw Error('No API expected');})).rejects.toThrow('qualification_marker_exists');expect(called).toBe(false);
 }finally{f.remove();}
});
test('restart rejects a foreign marker, unmanaged device or invalid observation',async()=>{
 const f=fixture();try{
  writeFileSync(f.marker,JSON.stringify({nonce:'b'.repeat(32),device_id:savedDevice}));let called=false;
  await expect(qualifyDesktop({root:f.root,nonce,phase:2},async()=>{called=true;throw Error('No API expected');})).rejects.toThrow('qualification_marker_owner');expect(called).toBe(false);
  writeFileSync(f.marker,JSON.stringify({nonce,device_id:savedDevice}));
  await expect(qualifyDesktop({root:f.root,nonce,phase:2},async()=>response(RegistryStateSchema,{devices:[{id:savedDevice,kind:'desktop',runtimeState:'registered'}]}))).rejects.toThrow('qualification_desktop_unmanaged');
  await expect(qualifyDesktop({root:f.root,nonce,phase:2},async(url)=>new URL(url).pathname.endsWith('observe')?response(DesktopObservationSchema,{deviceId:savedDevice,width:1280,height:800,png:Uint8Array.from([1])}):response(RegistryStateSchema,{devices:[{id:savedDevice,kind:'desktop',runtimeState:'managed'}]}))).rejects.toThrow('qualification_observation');
 }finally{f.remove();}
});
test('HTTP errors return a safe operation/status error without server content',async()=>{
 const f=fixture();try{await expect(qualifyDesktop({root:f.root,nonce,phase:1},async()=>new Response('fixture-private-token',{status:503}))).rejects.toThrow('qualification_rpc_registry/state_503');}finally{f.remove();}
});

test('a failed provision is reported and the owned marker is retained without claiming observation',async()=>{
 const f=fixture();let device='',job='';const calls:string[]=[];
 try{
  await expect(qualifyDesktop({root:f.root,nonce,phase:1},async(url,init)=>{
   const op=new URL(url).pathname;calls.push(op);
   if(op==='/v1/registry/state')return response(RegistryStateSchema,{agents:[{id:agent}]});
   if(op==='/v1/registry/device-save'){device=fromBinary(DeviceSaveRequestSchema,new Uint8Array(init.body as Uint8Array)).id;return response(RegistryStateSchema,{});}
   if(op==='/v1/desktop/provision'){job=fromBinary(DesktopProvisionRequestSchema,new Uint8Array(init.body as Uint8Array)).requestId;return response(DesktopProvisionJobSchema,{id:job,deviceId:device,status:'queued'});}
   if(op==='/v1/desktop/state')return response(DeviceLinkStateSchema,{jobs:[{id:job,deviceId:device,status:'failed',error:'private fixture text'}]});
   throw Error('Unexpected observation after failure');
  })).rejects.toThrow('qualification_provision_failed');
  expect(JSON.parse(readFileSync(f.marker,'utf8'))).toEqual({nonce,device_id:device});expect(calls.at(-1)).toBe('/v1/desktop/state');
 }finally{f.remove();}
});
