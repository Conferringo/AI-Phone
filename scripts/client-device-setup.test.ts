import {expect, test} from 'bun:test';
import {create, fromBinary, toBinary} from '@bufbuild/protobuf';
import {mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {DeviceRequestSchema} from '../packages/protocol/src/gen/ai_phone/v1/phone_runtime_pb';

const script=join(import.meta.dir,'client-device-setup.ts');
const protobuf=import.meta.resolve('@bufbuild/protobuf');
const schema=(name:string)=>new URL('../packages/protocol/src/gen/ai_phone/v1/'+name+'_pb.ts',import.meta.url).href;
const privateBody='fixture-private-response-must-not-appear';

function runSetup(mode:string, failure?:{stage:string;error:string}){
 const root=mkdtempSync(join(tmpdir(),'aiphone-startup-observation-'));
 for(const service of ['assistant','phone-runner']){
  mkdirSync(join(root,'data',service),{recursive:true});
  writeFileSync(join(root,'data',service,'token'),'fixture-'+service,{mode:0o600});
 }
 mkdirSync(join(root,'data/deployment'),{recursive:true});
 writeFileSync(join(root,'data/deployment/profile.json'),JSON.stringify({desktop:{cpus:2,memory_mb:2048}}));
 const preload=join(root,'runner-fixture.ts'),trace=join(root,'trace.json');
 writeFileSync(preload,`
import {create,fromBinary,toBinary} from ${JSON.stringify(protobuf)};
import {writeFileSync} from 'node:fs';
import {mock} from 'bun:test';
import {RegistryRequestSchema,RegistryStateSchema} from ${JSON.stringify(schema('registry'))};
import {DeviceRequestSchema,DeviceObservationSchema} from ${JSON.stringify(schema('phone_runtime'))};
import {DesktopRequestSchema,DesktopObservationSchema,DesktopProvisionRequestSchema,DesktopProvisionJobSchema,DeviceLinkStateSchema} from ${JSON.stringify(schema('desktop'))};
const calls=[],requests=[],provisionRequests=[],mode=${JSON.stringify(mode)},failure=${JSON.stringify(failure)};
const provisioning=mode.startsWith('desktop-');
let stateReads=0,clock=0;
if(mode.startsWith('desktop-timeout'))Date.now=()=>clock;
const denyProcess=()=>{throw Error('fixture_unexpected_process')};
const processModule={spawn:denyProcess,spawnSync:denyProcess,exec:denyProcess,execSync:denyProcess,execFile:denyProcess,execFileSync:denyProcess,fork:denyProcess};
mock.module('node:child_process',()=>processModule);
mock.module('child_process',()=>processModule);
Bun.spawn=denyProcess;Bun.spawnSync=denyProcess;
const privateBody=${JSON.stringify(privateBody)};
const png=Uint8Array.from(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aObcAAAAASUVORK5CYII=','base64'));
const reply=(schema,value)=>new Response(toBinary(schema,create(schema,value)),{headers:{'content-type':'application/x-protobuf'}});
process.on('exit',()=>writeFileSync(${JSON.stringify(trace)},JSON.stringify({calls,requests,provisionRequests})));
// No original fetch is retained: every request is decoded locally or rejected.
globalThis.fetch=async(url,init)=>{
 const address=new URL(url),path=address.pathname;calls.push(path);
 if(init.method!=='POST'||init.headers['content-type']!=='application/x-protobuf')throw Error('fixture_request_contract');
 const body=new Uint8Array(init.body);
 if(address.origin==='http://127.0.0.1:17863'&&init.headers.authorization==='Bearer fixture-assistant'){
  if(path==='/v1/registry/state'){
   fromBinary(RegistryRequestSchema,body);
   return reply(RegistryStateSchema,{devices:[{id:'fixture-desktop',kind:'desktop',runtimeState:provisioning?'registered':'managed'}]});
  }
  if(path==='/v1/desktop/provision'&&provisioning){
   const request=fromBinary(DesktopProvisionRequestSchema,body);
   provisionRequests.push({deviceId:request.deviceId,requestId:request.requestId,cpus:request.cpus,memoryMb:request.memoryMb,templateId:request.templateId});
   if(request.deviceId!=='fixture-desktop'||!request.requestId||request.cpus!==2||request.memoryMb!==2048||request.templateId!=='debian13-openbox')throw Error('fixture_provision_scope');
   return reply(DesktopProvisionJobSchema,{id:'fixture-job',deviceId:request.deviceId,status:'queued'});
  }
  if(path==='/v1/desktop/state'&&provisioning){
   fromBinary(RegistryRequestSchema,body);
   if(mode.startsWith('desktop-timeout'))clock=stateReads++===0?180000:180001;
   const current={id:'fixture-job',deviceId:'fixture-desktop',status:mode.startsWith('desktop-timeout')?'running':'failed',stage:failure?.stage||'',error:failure?.error||''};
   return reply(DeviceLinkStateSchema,{jobs:[{id:'unrelated-job',status:'completed'},...(mode==='desktop-timeout-no-job'?[]:[current])]});
  }
  if(path==='/v1/desktop/observe'){
   if(fromBinary(DesktopRequestSchema,body).deviceId!=='fixture-desktop')throw Error('fixture_desktop_scope');
   return reply(DesktopObservationSchema,{deviceId:'fixture-desktop',width:1,height:1,png});
  }
 }
 if(address.origin==='http://127.0.0.1:17862'&&path==='/v1/observe'&&init.headers.authorization==='Bearer fixture-phone-runner'){
  const request=fromBinary(DeviceRequestSchema,body);
  requests.push({phoneId:request.phoneId,ownerId:request.ownerId,metadataOnly:request.metadataOnly,includeUiTree:request.includeUiTree,includeElements:request.includeElements});
  if(request.phoneId!=='phone-local-01'||request.ownerId!=='local-user')throw Error('fixture_phone_scope');
  if(mode==='unauthorized')return new Response(toBinary(DeviceObservationSchema,create(DeviceObservationSchema,{phoneId:request.phoneId,width:1,height:1,png,rootIdentity:'0'})),{status:401,headers:{'content-type':'application/x-protobuf'}});
  if(mode==='unavailable')return new Response(privateBody,{status:503});
  if(mode==='unknown')throw Error('fixture_observation_outcome_unknown');
  if(mode==='malformed')return new Response(Uint8Array.from([255]),{headers:{'content-type':'application/x-protobuf'}});
  if(mode==='empty-body')return new Response(new Uint8Array(),{headers:{'content-type':'application/x-protobuf'}});
  // Matches runner main.rs: metadata_only suppresses PNG, root_ready emits "0" or empty.
  const rootIdentity=mode==='no-root'?'':mode==='non-root'?'2000':mode==='id-command-text'?'uid=0(root) gid=0(root)':'0';
  return reply(DeviceObservationSchema,{phoneId:request.phoneId,width:mode==='no-width'?0:1,height:mode==='no-height'?0:1,png:request.metadataOnly||mode==='empty-png'?new Uint8Array():png,rootIdentity});
 }
 throw Error('fixture_unexpected_request');
};
`);
 try{
  const env:Record<string,string>={AI_PHONE_PROJECT_ROOT:root};
  for(const name of ['PATH','SYSTEMROOT','WINDIR','TMPDIR','TEMP','TMP'])if(process.env[name])env[name]=process.env[name]!;
  const result=Bun.spawnSync([process.execPath,'--no-env-file','--preload',preload,script],{env,timeout:10000});
  const stdout=result.stdout.toString(),stderr=result.stderr.toString();
  expect(stdout+stderr).not.toContain(privateBody);
  expect(stdout+stderr).not.toContain('Bearer fixture-');
  return {exitCode:result.exitCode,stdout,stderr,trace:JSON.parse(readFileSync(trace,'utf8'))};
 }finally{rmSync(root,{recursive:true,force:true});}
}

test('omitted proto3 observation flags request a frame without UI or OCR work',()=>{
 const request=fromBinary(DeviceRequestSchema,toBinary(DeviceRequestSchema,create(DeviceRequestSchema,{phoneId:'phone-local-01',ownerId:'local-user'})));
 expect(request.metadataOnly).toBe(false);
 expect(request.includeUiTree).toBe(false);
 expect(request.includeElements).toBe(false);
});

test('startup accepts the runner wire root identity and observes the existing desktop once',()=>{
 const result=runSetup('healthy');
 expect(result.exitCode).toBe(0);
 expect(result.stdout).toContain('Owned phone frame/root and desktop frame verified');
 expect(result.trace.calls).toEqual(['/v1/registry/state','/v1/observe','/v1/desktop/observe']);
 expect(result.trace.requests).toEqual([{phoneId:'phone-local-01',ownerId:'local-user',metadataOnly:false,includeUiTree:false,includeElements:false}]);
});

test.each(['unauthorized','unavailable','unknown','malformed','empty-body','empty-png','no-width','no-height','no-root','non-root','id-command-text'])('startup rejects %s without retrying or provisioning a desktop',(mode)=>{
 const result=runSetup(mode);
 expect(result.exitCode).not.toBe(0);
 expect(result.stdout).not.toContain('verified; user conversations unchanged');
 expect(result.trace.calls).toEqual(['/v1/registry/state','/v1/observe']);
 expect(result.trace.requests).toHaveLength(1);
});

const provisionFailures=[
 {stage:'准备Clash客户端',error:'permission denied',expectedStage:'prepare_network_app',flag:'permission'},
 {stage:'准备基础镜像',error:'invalid mount config for type "bind": bind source path does not exist',expectedStage:'prepare_base_image',flag:'bind_mount'},
 {stage:'检查资源',error:'安装至少需要6GiB可用空间',expectedStage:'check_resources',flag:'disk_budget'},
 {stage:'检查资源',error:'Bundled desktop image is missing; run client initialization again',expectedStage:'check_resources',flag:'image_missing'},
 {stage:'启动桌面',error:'OCI runtime create failed',expectedStage:'start_desktop',flag:'runtime'},
 {stage:'验证画面与控制',error:'桌面没有返回可验证画面',expectedStage:'verify_frame_control',flag:'frame_missing'},
 {stage:'__proto__',error:'unrecognized upstream result',expectedStage:'unknown',flag:'unknown'},
];

test.each(provisionFailures)('registered desktop failure reports only stage $expectedStage and flag $flag',(failure)=>{
 const secret='fixture-secret-key',path='/private/fixture-sensitive-home',argv='--fixture-private-argv';
 const result=runSetup('desktop-failed',{stage:failure.stage,error:failure.error+' '+privateBody+' '+secret+' '+path+' '+argv});
 expect(result.exitCode).not.toBe(0);
 expect(result.stderr).toContain('Desktop provisioning failed (stage='+failure.expectedStage+'; flags='+failure.flag+')');
 expect(result.stdout+result.stderr).not.toContain(failure.error);
 for(const privateValue of [secret,path,argv])expect(result.stdout+result.stderr).not.toContain(privateValue);
 expect(result.trace.calls).toEqual(['/v1/registry/state','/v1/observe','/v1/desktop/provision','/v1/desktop/state']);
 expect(result.trace.provisionRequests).toEqual([{deviceId:'fixture-desktop',requestId:expect.any(String),cpus:2,memoryMb:2048,templateId:'debian13-openbox'}]);
 expect(result.stdout).not.toContain('verified; user conversations unchanged');
});

test.each(['desktop-timeout','desktop-timeout-no-job'])('registered desktop %s keeps the deadline boundary and never repeats provision',(mode)=>{
 const result=runSetup(mode,{stage:'启动桌面',error:privateBody});
 expect(result.exitCode).not.toBe(0);
 expect(result.stderr).toContain('Desktop provisioning timed out (stage='+(mode==='desktop-timeout'?'start_desktop':'unknown')+'; flags=unknown)');
 expect(result.trace.calls).toEqual(['/v1/registry/state','/v1/observe','/v1/desktop/provision','/v1/desktop/state','/v1/desktop/state']);
 expect(result.trace.provisionRequests).toHaveLength(1);
 expect(result.stdout).not.toContain('verified; user conversations unchanged');
});
