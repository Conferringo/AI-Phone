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

function runSetup(mode:string){
 const root=mkdtempSync(join(tmpdir(),'aiphone-startup-observation-'));
 for(const service of ['assistant','phone-runner']){
  mkdirSync(join(root,'data',service),{recursive:true});
  writeFileSync(join(root,'data',service,'token'),'fixture-'+service,{mode:0o600});
 }
 const preload=join(root,'runner-fixture.ts'),trace=join(root,'trace.json');
 writeFileSync(preload,`
import {create,fromBinary,toBinary} from ${JSON.stringify(protobuf)};
import {writeFileSync} from 'node:fs';
import {RegistryRequestSchema,RegistryStateSchema} from ${JSON.stringify(schema('registry'))};
import {DeviceRequestSchema,DeviceObservationSchema} from ${JSON.stringify(schema('phone_runtime'))};
import {DesktopRequestSchema,DesktopObservationSchema} from ${JSON.stringify(schema('desktop'))};
const calls=[],requests=[],mode=${JSON.stringify(mode)};
const privateBody=${JSON.stringify(privateBody)};
const png=Uint8Array.from(Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aObcAAAAASUVORK5CYII=','base64'));
const reply=(schema,value)=>new Response(toBinary(schema,create(schema,value)),{headers:{'content-type':'application/x-protobuf'}});
process.on('exit',()=>writeFileSync(${JSON.stringify(trace)},JSON.stringify({calls,requests})));
// No original fetch is retained: every request is decoded locally or rejected.
globalThis.fetch=async(url,init)=>{
 const address=new URL(url),path=address.pathname;calls.push(path);
 if(init.method!=='POST'||init.headers['content-type']!=='application/x-protobuf')throw Error('fixture_request_contract');
 const body=new Uint8Array(init.body);
 if(address.origin==='http://127.0.0.1:17863'&&init.headers.authorization==='Bearer fixture-assistant'){
  if(path==='/v1/registry/state'){
   fromBinary(RegistryRequestSchema,body);
   return reply(RegistryStateSchema,{devices:[{id:'fixture-desktop',kind:'desktop',runtimeState:'managed'}]});
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
