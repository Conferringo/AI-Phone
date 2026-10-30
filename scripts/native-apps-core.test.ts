import {test,expect,spyOn} from 'bun:test';
import * as fs from 'node:fs';
import {createHash} from 'node:crypto';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,existsSync,readFileSync} from 'node:fs';
import {join,dirname,isAbsolute} from 'node:path';
import {tmpdir} from 'node:os';
import {NativeApps} from '../apps/device-network/native-apps';

function fixture(bundled:boolean,url='http://127.0.0.1:1/unavailable',bytes=Buffer.from('pinned vendor fixture')){
 const root=mkdtempSync(join(tmpdir(),'aiphone-native-apps-'));
 const pin={file:'vendor.apk',size_bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex'),url,version:'fixture'};
 mkdirSync(join(root,'config'));writeFileSync(join(root,'config/clash-apps.json'),JSON.stringify({android:pin,linux:pin,alternatives:{'android-x86_64':pin,'linux-amd64':pin}}));
 const path=join(root,'runtime/guest-apps/clash-arm64-v8a.apk');
 if(bundled){mkdirSync(join(root,'runtime/guest-apps'),{recursive:true});writeFileSync(path,bytes);}
 const env:Record<string,string>={AI_PHONE_ANDROID_ABI:'arm64-v8a',AI_PHONE_DEVICE_ID:'phone-fixture',AI_PHONE_BOOTSTRAP_PROXY:''};
 if(bundled)Object.assign(env,{AI_PHONE_CLASH_ANDROID_APK:path,AI_PHONE_IMAGE_PULL_POLICY:'never',AI_PHONE_DESKTOP_IMAGE:'ai-phone-bundle/desktop:sha-'+'a'.repeat(64)});
 return {root,bytes,path,env};
}

test('owned offline installation context verifies both ABIs without a download or cache write',async()=>{
 const f=fixture(true);
 try{
  const apps=new NativeApps(f.root,f.env);
  expect(apps.verify()).toEqual({verified:true});
  expect((await apps.download()).verified).toBe(true);
  f.env.AI_PHONE_CLASH_ANDROID_APK='changed after creation';
  expect(apps.verify().verified).toBe(true);
  const x64=join(f.root,'runtime/guest-apps/clash-x86_64.apk');writeFileSync(x64,f.bytes);
  expect(new NativeApps(f.root,{...f.env,AI_PHONE_ANDROID_ABI:'x86_64',AI_PHONE_CLASH_ANDROID_APK:x64}).verify().verified).toBe(true);
  expect(existsSync(join(f.root,'.tools'))).toBe(false);
 }finally{rmSync(f.root,{recursive:true,force:true});}
});

test('source acquisition publishes only verified bytes and preserves the same cache on reuse',async()=>{
 let calls=0;const bytes=Buffer.alloc(2048,7);
 const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch(){calls++;return new Response(bytes);}});
 const f=fixture(false,`http://127.0.0.1:${server.port}/app`,bytes);
 try{
  const apps=new NativeApps(f.root,f.env);
  expect(()=>apps.verify(['android'])).toThrow('missing');
  const writes:string[]=[],original=fs.writeFileSync;
  const spy=spyOn(fs,'writeFileSync').mockImplementation((path,data,options)=>{writes.push(String(path));return original(path,data,options);});
  try{await apps.download(['android']);await apps.download(['android']);}finally{spy.mockRestore();}
  expect(writes.length).toBeGreaterThan(0);
  expect(writes.every(path=>isAbsolute(path)&&dirname(path)===join(f.root,'.tools/clash-apps/android'))).toBe(true);
  expect(calls).toBe(1);expect(readFileSync(join(f.root,'.tools/clash-apps/android/vendor.apk'))).toEqual(bytes);
  expect(apps.verify(['android']).verified).toBe(true);
 }finally{server.stop(true);rmSync(f.root,{recursive:true,force:true});}
});

test('bad status, changed hash and oversized responses never publish a cache',async()=>{
 let mode='status';
 const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch(){return mode==='empty-status'?new Response(null,{status:304}):mode==='status'?new Response('failure',{status:503}):new Response(mode==='large'?'x'.repeat(200):'x'.repeat(21));}});
 const f=fixture(false,`http://127.0.0.1:${server.port}/app`);
 try{
  const apps=new NativeApps(f.root,f.env);
  for(const value of ['status','empty-status','hash','large']){mode=value;await expect(apps.download(['android'])).rejects.toThrow(value==='large'?'exceeds pinned':value.includes('status')?'download failed':'integrity');}
  expect(existsSync(join(f.root,'.tools/clash-apps/android/vendor.apk'))).toBe(false);
 }finally{server.stop(true);rmSync(f.root,{recursive:true,force:true});}
});

test('an existing guest application and enabled source configuration are preserved',async()=>{
 const f=fixture(true),calls:string[][]=[];
 try{
  const folder=join(f.root,'data/device-network/phone-fixture');mkdirSync(folder,{recursive:true});writeFileSync(join(folder,'source.json'),'{"enabled":true,"keep":"fixture"}');
  const apps=new NativeApps(f.root,f.env,async args=>{calls.push(args);return 'package:/existing/base.apk';});
  expect((await apps.installPhone()).installed).toBe(true);
  expect(calls).toHaveLength(1);expect(calls[0][0]).toBe('shell');
  expect(readFileSync(join(folder,'source.json'),'utf8')).toBe('{"enabled":true,"keep":"fixture"}');
  expect(existsSync(join(folder,'app-managed.json'))).toBe(false);
 }finally{rmSync(f.root,{recursive:true,force:true});}
});

test('a new guest receives the selected bundled APK; a failed command cannot publish the handoff marker',async()=>{
 const f=fixture(true),calls:string[][]=[];
 try{
  const apps=new NativeApps(f.root,f.env,async args=>{calls.push(args);return '';});
  await apps.installPhone();
  expect(calls[1]).toEqual(['install','-r',f.path]);
  expect(existsSync(join(f.root,'data/device-network/phone-fixture/app-managed.json'))).toBe(true);
  const marker=join(f.root,'data/device-network/phone-fixture/app-managed.json');writeFileSync(marker,'keep-marker');await apps.installPhone();expect(readFileSync(marker,'utf8')).toBe('keep-marker');
  const failed=new NativeApps(f.root,{...f.env,AI_PHONE_DEVICE_ID:'failed'},async args=>{if(args[0]==='install')throw Error('device offline');return '';});
  await expect(failed.installPhone()).rejects.toThrow('offline');
  expect(existsSync(join(f.root,'data/device-network/failed/app-managed.json'))).toBe(false);
 }finally{rmSync(f.root,{recursive:true,force:true});}
});

test('source acquisition honors an explicit local bootstrap proxy',async()=>{
 const bytes=Buffer.from('pinned vendor fixture');let received=0;
 const proxy=Bun.serve({hostname:'127.0.0.1',port:0,fetch(){received++;return new Response(bytes);}});
 const f=fixture(false,'http://aiphone-fixture.invalid/vendor.apk');
 try{
  const apps=new NativeApps(f.root,{...f.env,AI_PHONE_BOOTSTRAP_PROXY:`http://127.0.0.1:${proxy.port}`});
  expect((await apps.download(['android'])).verified).toBe(true);expect(received).toBe(1);
 }finally{proxy.stop(true);rmSync(f.root,{recursive:true,force:true});}
});
