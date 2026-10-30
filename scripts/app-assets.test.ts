import {test,expect} from 'bun:test';
import {createHash} from 'node:crypto';
import {mkdtempSync,mkdirSync,writeFileSync,rmSync,symlinkSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {tmpdir} from 'node:os';
import {appAsset,defaultAppKinds,verifyAppAsset} from '../apps/device-network/app-assets';

test('desktop app download is skipped only for a pinned offline image selection',()=>{
 const image='ai-phone-bundle/desktop:sha-'+'a'.repeat(64);
 expect(defaultAppKinds({AI_PHONE_IMAGE_PULL_POLICY:'never',AI_PHONE_DESKTOP_IMAGE:image})).toEqual(['android']);
 for(const env of [{},{AI_PHONE_IMAGE_PULL_POLICY:'missing',AI_PHONE_DESKTOP_IMAGE:image},{AI_PHONE_IMAGE_PULL_POLICY:'never',AI_PHONE_DESKTOP_IMAGE:'custom:latest'}])expect(defaultAppKinds(env)).toEqual(['android','linux']);
});

test('bundled app selection uses the guest ABI, rejects overrides and does not create a source fallback',()=>{
 const root=mkdtempSync(join(tmpdir(),'aiphone-guest-apps-')),bytes=Buffer.from('apk fixture');
 const pin={file:'vendor.apk',size_bytes:bytes.length,sha256:createHash('sha256').update(bytes).digest('hex')};
 try{
  const folder=join(root,'runtime/guest-apps');mkdirSync(folder,{recursive:true});
  expect(()=>appAsset(root,'android',pin,{})).toThrow('selection');
  for(const abi of ['arm64-v8a','x86_64']){
   const path=join(folder,'clash-'+abi+'.apk');writeFileSync(path,bytes);
   const env={AI_PHONE_ANDROID_ABI:abi,AI_PHONE_CLASH_ANDROID_APK:path};
   expect(appAsset(root,'android',pin,env)).toEqual({path,bundled:true});
   expect(()=>verifyAppAsset(path,{...pin,sha256:'b'.repeat(64)})).toThrow('integrity');
   verifyAppAsset(path,pin);
   expect(()=>appAsset(root,'android',pin,{...env,AI_PHONE_CLASH_ANDROID_APK:resolve(root,'outside.apk')})).toThrow('selection');
  }
  expect(()=>appAsset(root,'android',pin,{AI_PHONE_ANDROID_ABI:'mips',AI_PHONE_CLASH_ANDROID_APK:join(folder,'clash-arm64-v8a.apk')})).toThrow('ABI');
  expect(appAsset(root,'linux',pin,{AI_PHONE_CLASH_DESKTOP_DIR:'linux-amd64'})).toEqual({path:join(root,'.tools/clash-apps/linux-amd64/vendor.apk'),bundled:false});
  rmSync(folder,{recursive:true});
  expect(appAsset(root,'android',pin,{})).toEqual({path:join(root,'.tools/clash-apps/android/vendor.apk'),bundled:false});
  symlinkSync(join(root,'missing'),folder,'dir');
  expect(()=>appAsset(root,'android',pin,{})).toThrow('component');
  rmSync(folder);const outside=join(root,'outside');mkdirSync(outside);symlinkSync(outside,folder,'dir');
  expect(()=>appAsset(root,'android',pin,{})).toThrow('component');
  rmSync(folder);mkdirSync(folder);const path=join(folder,'clash-arm64-v8a.apk');const file=join(root,'outside.apk');writeFileSync(file,bytes);symlinkSync(file,path);
  expect(()=>appAsset(root,'android',pin,{AI_PHONE_CLASH_ANDROID_APK:path})).toThrow('Invalid bundled');
  rmSync(path);mkdirSync(path);expect(()=>appAsset(root,'android',pin,{AI_PHONE_CLASH_ANDROID_APK:path})).toThrow('Invalid bundled');
  rmSync(folder,{recursive:true});writeFileSync(folder,'not a directory');expect(()=>appAsset(root,'android',pin,{})).toThrow('component');
 }finally{rmSync(root,{recursive:true,force:true});}
});
