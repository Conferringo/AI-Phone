/** Disposable backend qualification through the same protobuf API as the workbench. */
import {create,fromBinary,toBinary,type DescMessage,type MessageInitShape,type MessageShape} from '@bufbuild/protobuf';
import {readFile,writeFile,lstat} from 'node:fs/promises';
import {join} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
import {RegistryRequestSchema,RegistryStateSchema,DeviceSaveRequestSchema} from '../packages/protocol/src/gen/ai_phone/v1/registry_pb';
import {DesktopProvisionRequestSchema,DesktopProvisionJobSchema,DesktopRequestSchema,DeviceLinkStateSchema,DesktopObservationSchema} from '../packages/protocol/src/gen/ai_phone/v1/desktop_pb';

export type Qualification={root:string;nonce:string;phase:1|2};
type Send=(url:string,init:BunFetchRequestInit)=>Promise<Response>;
function qualificationIdentity(root:string,nonce:string,phase:string|undefined):Qualification {
 if(!/^[0-9a-f]{32}$/.test(nonce)||!/^\/var\/lib\/aiphone\/installation\/releases\/[0-9a-f]{64}$/.test(root)||!['1','2'].includes(phase||''))throw Error('qualification_identity');
 return {root,nonce,phase:phase==='1'?1:2};
}
export function qualificationEnvironment(env:Record<string,string|undefined>,platform:string=process.platform):Qualification {
 if(platform!=='linux'||env.container!=='docker'||env.DOCKER_HOST!=='unix:///run/aiphone/docker.sock')throw Error('qualification_environment');
 return qualificationIdentity(env.AI_PHONE_PROJECT_ROOT||'',env.AI_PHONE_WSL_QUALIFICATION_NONCE||'',env.AI_PHONE_WSL_QUALIFICATION_PHASE);
}

function transport(token:string,send:Send){
 return async<I extends DescMessage,O extends DescMessage>(operation:string,input:I,value:MessageInitShape<I>,output:O):Promise<MessageShape<O>>=>{
  const response=await send('http://127.0.0.1:17863/v1/'+operation,{method:'POST',headers:{authorization:'Bearer '+token,'content-type':'application/x-protobuf'},body:toBinary(input,create(input,value)),signal:AbortSignal.timeout(120000),proxy:false});
  if(!response.ok||response.headers.get('content-type')!=='application/x-protobuf')throw Error('qualification_rpc_'+operation+'_'+response.status);
  return fromBinary(output,new Uint8Array(await response.arrayBuffer()));
 };
}
type Rpc=ReturnType<typeof transport>;
async function markerExists(path:string){
 try{await lstat(path);return true;}catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return false;throw error;}
}
async function recordedDevice(path:string,nonce:string){
 const info=await lstat(path);
 if(!info.isFile()||info.size>4096)throw Error('qualification_marker_invalid');
 let value:unknown;try{value=JSON.parse(await readFile(path,'utf8'));}catch{throw Error('qualification_marker_invalid');}
 const marker=value as {nonce:string;device_id:string};
 if(!marker||Object.keys(marker).sort().join(',')!=='device_id,nonce'||marker.nonce!==nonce||!/^[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/.test(marker.device_id))throw Error('qualification_marker_owner');
 return marker.device_id;
}
async function provision(root:string,device:string,rpc:Rpc){
 const profile=JSON.parse(await readFile(join(root,'data/deployment/profile.json'),'utf8'));
 const job=await rpc('desktop/provision',DesktopProvisionRequestSchema,{deviceId:device,requestId:crypto.randomUUID(),cpus:profile.desktop.cpus,memoryMb:profile.desktop.memory_mb,templateId:'debian13-openbox'},DesktopProvisionJobSchema);
 if(!job.id||job.deviceId!==device)throw Error('qualification_provision_identity');
 await waitProvisioned(device,job.id,rpc);
}
async function waitProvisioned(device:string,jobId:string,rpc:Rpc){
 const until=Date.now()+180000;
 for(;;){
  const state=await rpc('desktop/state',RegistryRequestSchema,{},DeviceLinkStateSchema),current=state.jobs.find(item=>item.id===jobId&&item.deviceId===device);
  if(current?.status==='completed')return;
  if(current?.status==='failed'||Date.now()>=until)throw Error('qualification_provision_failed');
  await delay(1000);
 }
}
async function createDesktop(value:Qualification,marker:string,rpc:Rpc){
 const state=await rpc('registry/state',RegistryRequestSchema,{},RegistryStateSchema);
 if(state.devices.some(device=>device.kind==='desktop'))throw Error('qualification_desktop_exists');
 const agent=state.agents[0];if(!agent)throw Error('qualification_agent_missing');
 const device=crypto.randomUUID();
 await writeFile(marker,JSON.stringify({nonce:value.nonce,device_id:device}),{flag:'wx',mode:0o600});
 await rpc('registry/device-save',DeviceSaveRequestSchema,{id:device,name:'Qualification desktop',kind:'desktop',defaultAgentId:agent.id},RegistryStateSchema);
 await provision(value.root,device,rpc);
 return device;
}
async function observeDesktop(device:string,rpc:Rpc){
 const state=await rpc('registry/state',RegistryRequestSchema,{},RegistryStateSchema);
 if(!state.devices.some(row=>row.id===device&&row.kind==='desktop'&&row.runtimeState==='managed'))throw Error('qualification_desktop_unmanaged');
 const frame=await rpc('desktop/observe',DesktopRequestSchema,{deviceId:device},DesktopObservationSchema);
 if(frame.deviceId!==device||frame.width!==1280||frame.height!==800||!Buffer.from(frame.png).subarray(0,8).equals(Buffer.from([137,80,78,71,13,10,26,10])))throw Error('qualification_observation');
}
export async function qualifyDesktop(value:Qualification,send:Send=fetch){
 const marker=join(value.root,'data/deployment/wsl-desktop-qualification.json');
 if(value.phase===1&&await markerExists(marker))throw Error('qualification_marker_exists');
 const previous=value.phase===2?await recordedDevice(marker,value.nonce):undefined;
 const token=(await readFile(join(value.root,'data/assistant/token'),'utf8')).trim(),rpc=transport(token,send);
 const device=previous??await createDesktop(value,marker,rpc);
 await observeDesktop(device,rpc);
 return {device_id:device,desktop_registered:true,desktop_observation:true};
}
if(import.meta.main){
 try{console.log(JSON.stringify(await qualifyDesktop(qualificationEnvironment(process.env))));}
 catch(error){const text=error instanceof Error?error.message:'';console.error(/^qualification_[a-z0-9_/]+$/.test(text)?text:'qualification_failed');process.exitCode=1;}
}
