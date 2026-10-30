/** Asset selection only. A selected image already contains its desktop GUI app. */
import {createHash} from 'node:crypto';
import {readFileSync,lstatSync} from 'node:fs';
import {resolve} from 'node:path';

export type AppKind='android'|'linux';
export type AppPin={file:string;size_bytes:number;sha256:string};
type Environment=Record<string,string|undefined>;

export function defaultAppKinds(env:Environment):AppKind[]{
 return env.AI_PHONE_IMAGE_PULL_POLICY==='never'&&/^ai-phone-bundle\/desktop:sha-[0-9a-f]{64}$/.test(env.AI_PHONE_DESKTOP_IMAGE||'')?['android']:['android','linux'];
}
function guestFolder(root:string){
 try{const value=lstatSync(resolve(root,'runtime/guest-apps'));if(value.isSymbolicLink()||!value.isDirectory())throw Error('Invalid guest application component');return true;}
 catch(error){if((error as NodeJS.ErrnoException).code==='ENOENT')return false;throw error;}
}
export function appAsset(root:string,kind:AppKind,pin:AppPin,env:Environment):{path:string;bundled:boolean}{
 if(kind==='android'&&(guestFolder(root)||env.AI_PHONE_CLASH_ANDROID_APK)){
  return {path:bundledAndroidPath(root,env),bundled:true};
 }
 const folder=kind==='linux'?(env.AI_PHONE_CLASH_DESKTOP_DIR||'linux'):'android';
 return {path:resolve(root,'.tools/clash-apps',folder,pin.file),bundled:false};
}
function bundledAndroidPath(root:string,env:Environment){
 const abi=env.AI_PHONE_ANDROID_ABI||'arm64-v8a';
 if(!['arm64-v8a','x86_64'].includes(abi))throw Error('Unsupported guest application ABI');
 const expected=resolve(root,'runtime/guest-apps','clash-'+abi+'.apk');
 if(env.AI_PHONE_CLASH_ANDROID_APK!==expected)throw Error('Bundled guest application selection missing or invalid');
 const file=lstatSync(expected);if(file.isSymbolicLink()||!file.isFile())throw Error('Invalid bundled guest application');
 return expected;
}
export function verifyAppAsset(path:string,pin:AppPin){
 verifyAppBytes(readFileSync(path),pin);
}
export function verifyAppBytes(bytes:Uint8Array,pin:AppPin){
 if(bytes.length!==pin.size_bytes||createHash('sha256').update(bytes).digest('hex')!==pin.sha256)throw Error('Clash application integrity failed');
}
