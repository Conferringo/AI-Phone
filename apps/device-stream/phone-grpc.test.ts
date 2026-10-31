import {expect,test,spyOn} from 'bun:test';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {createPhoneGrpc,PhoneRuntimeUnavailable} from './phone-grpc';
const proto='syntax="proto3"; package android.emulation.control; message Empty {} service EmulatorController { rpc getScreenshot(Empty) returns (Empty); }';
function fixture(){
 const sdk=mkdtempSync(join(tmpdir(),'aiphone-grpc-'));
 return {sdk,install(){const folder=join(sdk,'emulator/lib');mkdirSync(folder,{recursive:true});writeFileSync(join(folder,'emulator_controller.proto'),proto);},remove(){rmSync(sdk,{recursive:true,force:true});}};
}

test('constructing and closing an unavailable phone never discovers or loads its SDK',()=>{
 let discovered=0;const phone=createPhoneGrpc({sdk:'/missing-fixture-sdk',port:1,serial:'fixture'},()=>{discovered++;throw Error('not ready');});
 expect(discovered).toBe(0);phone.close();expect(discovered).toBe(0);
 expect(()=>phone.connection()).toThrow(PhoneRuntimeUnavailable);
});
test('missing SDK fails only on phone use and can recover after SDK installation',()=>{
 const f=fixture(),phone=createPhoneGrpc({sdk:f.sdk,port:1,serial:'fixture'},()=>({path:'fixture-one',token:'one'}));
 try{
  expect(()=>phone.connection()).toThrow('phone_runtime_unavailable');f.install();
  expect(phone.connection().metadata.get('authorization')).toEqual(['Bearer one']);
 }finally{phone.close();f.remove();}
});
test('a new phone identity replaces the client, while token rotation keeps the same client',()=>{
 const f=fixture();f.install();let identity={path:'one',token:'first'};
 const phone=createPhoneGrpc({sdk:f.sdk,port:1,serial:'fixture'},()=>identity);
 try{
  const first=phone.connection(),closed=spyOn(first.client,'close');identity={path:'one',token:'rotated'};
  expect(phone.connection().client).toBe(first.client);expect(phone.connection().metadata.get('authorization')).toEqual(['Bearer rotated']);expect(closed).toHaveBeenCalledTimes(0);
  identity={path:'two',token:'second'};expect(phone.connection().client).not.toBe(first.client);expect(closed).toHaveBeenCalledTimes(1);
  phone.close();expect(()=>phone.connection()).toThrow('phone_runtime_unavailable');expect(closed).toHaveBeenCalledTimes(1);
 }finally{phone.close();f.remove();}
});
test('missing discovery and malformed SDK report an unavailable runtime without paths or tokens',()=>{
 const f=fixture();f.install();
 const phone=createPhoneGrpc({sdk:f.sdk,port:1,serial:'fixture'},()=>{throw Error('private-fixture-token');});
 try{
  expect(()=>phone.connection()).toThrow('phone_runtime_unavailable');
  writeFileSync(join(f.sdk,'emulator/lib/emulator_controller.proto'),'not a proto');
  const invalid=createPhoneGrpc({sdk:f.sdk,port:1,serial:'fixture'});
  expect(()=>invalid.connection()).toThrow('phone_runtime_unavailable');invalid.close();
 }finally{phone.close();f.remove();}
});
