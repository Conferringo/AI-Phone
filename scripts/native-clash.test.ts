import {test,expect} from 'bun:test';
import {mkdtempSync,mkdirSync,writeFileSync,readFileSync,rmSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createHash} from 'node:crypto';

test('native app downloads verify pins, reuse intact files and reject altered caches',async()=>{
 const root=mkdtempSync(join(tmpdir(),'aiphone-clash-')),bytes=Buffer.from('vendor fixture, no user state');let requests=0;
 const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch(){requests++;return new Response(bytes);}});
 const pin={file:'fixture.deb',size_bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex'),url:`http://127.0.0.1:${server.port}/app`,version:'fixture'};
 const run=async()=>{const proc=Bun.spawn([process.execPath,resolve('apps/device-network/native-apps.ts')],{env:{...process.env,AI_PHONE_PROJECT_ROOT:root,AI_PHONE_BOOTSTRAP_PROXY:''},stdin:new TextEncoder().encode('{"operation":"download"}'),stdout:'pipe',stderr:'pipe'});const output=JSON.parse(await new Response(proc.stdout).text());return {code:await proc.exited,output};};
 try{
  mkdirSync(join(root,'config'));writeFileSync(join(root,'config/clash-apps.json'),JSON.stringify({android:pin,linux:pin}));
  expect((await run()).code).toBe(0);expect(requests).toBe(2);
  expect(readFileSync(join(root,'.tools/clash-apps/android/fixture.deb'))).toEqual(bytes);
  expect((await run()).code).toBe(0);expect(requests).toBe(2);
  writeFileSync(join(root,'.tools/clash-apps/android/fixture.deb'),'tampered');
  const altered=await run();expect(altered.code).toBe(1);expect(altered.output.error).toContain('integrity');expect(requests).toBe(2);
 }finally{server.stop(true);rmSync(root,{recursive:true,force:true});}
});

test('a changed response never becomes an installed app cache',async()=>{
 const root=mkdtempSync(join(tmpdir(),'aiphone-clash-'));
 const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch:()=>new Response('untrusted')});
 try{
  mkdirSync(join(root,'config'));const pin={file:'fixture.apk',size_bytes:9,sha256:'0'.repeat(64),url:`http://127.0.0.1:${server.port}/app`,version:'fixture'};
  writeFileSync(join(root,'config/clash-apps.json'),JSON.stringify({android:pin,linux:pin}));
  const proc=Bun.spawn([process.execPath,resolve('apps/device-network/native-apps.ts')],{env:{...process.env,AI_PHONE_PROJECT_ROOT:root,AI_PHONE_BOOTSTRAP_PROXY:''},stdin:new TextEncoder().encode('{"operation":"download"}'),stdout:'pipe',stderr:'pipe'});
  expect(await proc.exited).toBe(1);expect(existsSync(join(root,'.tools/clash-apps/android/fixture.apk'))).toBe(false);expect(existsSync(join(root,'.tools/clash-apps/linux/fixture.apk'))).toBe(false);
 }finally{server.stop(true);rmSync(root,{recursive:true,force:true});}
});

test('bundled APKs use each guest ABI without HTTP or source caches; corruption never downloads a substitute',async()=>{
 const root=mkdtempSync(join(tmpdir(),'aiphone-bundled-clash-'));let requests=0;
 const server=Bun.serve({hostname:'127.0.0.1',port:0,fetch(){requests++;return new Response('must not download');}});
 const pin=(abi:string)=>{const bytes=Buffer.from('verified fixture '+abi);return {file:'vendor-'+abi+'.apk',size_bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex'),url:`http://127.0.0.1:${server.port}/app`,version:'fixture'};};
 const run=async(abi:string)=>{
  const proc=Bun.spawn([process.execPath,resolve('apps/device-network/native-apps.ts')],{env:{...process.env,AI_PHONE_PROJECT_ROOT:root,AI_PHONE_ANDROID_ABI:abi,AI_PHONE_CLASH_ANDROID_APK:join(root,'runtime/guest-apps/clash-'+abi+'.apk'),AI_PHONE_IMAGE_PULL_POLICY:'never',AI_PHONE_DESKTOP_IMAGE:'ai-phone-bundle/desktop:sha-'+'a'.repeat(64)},stdin:new TextEncoder().encode('{"operation":"download"}'),stdout:'pipe',stderr:'pipe'});
  const output=JSON.parse(await new Response(proc.stdout).text());return {code:await proc.exited,output};
 };
 try{
  mkdirSync(join(root,'config'));mkdirSync(join(root,'runtime/guest-apps'),{recursive:true});
  writeFileSync(join(root,'config/clash-apps.json'),JSON.stringify({android:pin('arm64-v8a'),linux:pin('linux'),alternatives:{'android-x86_64':pin('x86_64'),'linux-amd64':pin('linux')}}));
  for(const abi of ['arm64-v8a','x86_64']){
   writeFileSync(join(root,'runtime/guest-apps/clash-'+abi+'.apk'),'verified fixture '+abi);
   expect((await run(abi)).code).toBe(0);
  }
  expect(requests).toBe(0);expect(existsSync(join(root,'.tools'))).toBe(false);
  writeFileSync(join(root,'runtime/guest-apps/clash-arm64-v8a.apk'),'changed');
  expect((await run('arm64-v8a')).output.error).toContain('integrity');
  rmSync(join(root,'runtime/guest-apps/clash-x86_64.apk'));
  expect((await run('x86_64')).code).toBe(1);
  expect(requests).toBe(0);expect(existsSync(join(root,'.tools'))).toBe(false);
 }finally{server.stop(true);rmSync(root,{recursive:true,force:true});}
});
