/** Local installer only: use the existing authenticated protobuf provisioning API. */
import {create, fromBinary, toBinary, type DescMessage, type MessageShape, type MessageInitShape} from '@bufbuild/protobuf';
import {readFile} from 'node:fs/promises';
import {join} from 'node:path';
import {setTimeout as delay} from 'node:timers/promises';
import {RegistryRequestSchema, RegistryStateSchema, DeviceSaveRequestSchema} from '../packages/protocol/src/gen/ai_phone/v1/registry_pb';
import {DesktopProvisionRequestSchema, DesktopProvisionJobSchema, DesktopRequestSchema, DeviceLinkStateSchema, DesktopObservationSchema} from '../packages/protocol/src/gen/ai_phone/v1/desktop_pb';

const root=process.env.AI_PHONE_PROJECT_ROOT;
if(!root)throw Error('Missing owned installation root');
const token=(await readFile(join(root,'data/assistant/token'),'utf8')).trim();
async function rpc<I extends DescMessage,O extends DescMessage>(path:string,input:I,value:MessageInitShape<I>,output:O):Promise<MessageShape<O>>{
 const response=await fetch('http://127.0.0.1:17863/v1/'+path,{method:'POST',headers:{authorization:'Bearer '+token,'content-type':'application/x-protobuf'},body:toBinary(input,create(input,value)),signal:AbortSignal.timeout(120000),proxy:null});
 if(!response.ok||response.headers.get('content-type')!=='application/x-protobuf')throw Error('Device setup failed: '+path+' HTTP '+response.status);
 return fromBinary(output,new Uint8Array(await response.arrayBuffer()));
}
let state=await rpc('registry/state',RegistryRequestSchema,{},RegistryStateSchema);
let desktop=state.devices.find(device=>device.kind==='desktop');
if(desktop?.runtimeState==='managed'){
 // Never recreate a device on retry. Read-only observation verifies the live runtime.
 const frame=await rpc('desktop/observe',DesktopRequestSchema,{deviceId:desktop.id},DesktopObservationSchema);
 if(!frame.png.length)throw Error('Desktop did not return a frame');
}else{
 const id=desktop?.id||crypto.randomUUID();
 if(!desktop){
  await rpc('registry/device-save',DeviceSaveRequestSchema,{id,name:'Linux',kind:'desktop',defaultAgentId:state.agents[0]?.id||''},RegistryStateSchema);
  state=await rpc('registry/state',RegistryRequestSchema,{},RegistryStateSchema);
  desktop=state.devices.find(device=>device.id===id);
 }
 if(!desktop)throw Error('Desktop registration unavailable');
 const profile=JSON.parse(await readFile(join(root,'data/deployment/profile.json'),'utf8'));
 const job=await rpc('desktop/provision',DesktopProvisionRequestSchema,{deviceId:id,requestId:crypto.randomUUID(),cpus:profile.desktop.cpus,memoryMb:profile.desktop.memory_mb,templateId:'debian13-openbox'},DesktopProvisionJobSchema);
 const deadline=Date.now()+180000;
 for(;;){
  const status=await rpc('desktop/state',RegistryRequestSchema,{},DeviceLinkStateSchema);
  const current=status.jobs.find(item=>item.id===job.id);
  if(current?.status==='completed')break;
  if(current?.status==='failed'||Date.now()>deadline)throw Error('Desktop provisioning did not complete');
  await delay(1000);
 }
 const frame=await rpc('desktop/observe',DesktopRequestSchema,{deviceId:id},DesktopObservationSchema);
 if(!frame.png.length)throw Error('Desktop did not return a frame');
}
console.log('Owned desktop verified; user conversations unchanged');
