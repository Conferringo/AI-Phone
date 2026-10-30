/** Verified vendor applications; no subscriptions or developer accounts are bundled. */
import {createHash,randomBytes} from 'node:crypto';
import {readFileSync,writeFileSync,mkdirSync,existsSync,renameSync,unlinkSync} from 'node:fs';
import {resolve} from 'node:path';
const root=process.env.AI_PHONE_PROJECT_ROOT||resolve(import.meta.dir,'../..');
const pins=()=>{
 const config=JSON.parse(readFileSync(root+'/config/clash-apps.json','utf8'));
 const x64=process.env.AI_PHONE_ANDROID_ABI==='x86_64';
 return {...config,android:x64?config.alternatives['android-x86_64']:config.android,linux:x64?config.alternatives['linux-amd64']:config.linux};
};
const cacheKind=(kind:'android'|'linux')=>kind==='linux'?(process.env.AI_PHONE_CLASH_DESKTOP_DIR||'linux'):kind;
export function verifyApps(kinds:('android'|'linux')[]=['android','linux']){
 const metadata=pins();
 for(const kind of kinds){
  const pin=metadata[kind],path=root+'/.tools/clash-apps/'+cacheKind(kind)+'/'+pin.file;
  if(!existsSync(path))throw Error('Clash application cache missing');
  const bytes=readFileSync(path);
  if(bytes.length!==pin.size_bytes||createHash('sha256').update(bytes).digest('hex')!==pin.sha256)throw Error('Clash application cache integrity failed');
 }
 return {verified:true};
}
export async function downloadApps(kinds:('android'|'linux')[]=['android','linux']){
 const metadata=pins();
 await Promise.all(kinds.map(async kind=>{
  const pin=metadata[kind],dir=root+'/.tools/clash-apps/'+cacheKind(kind),path=dir+'/'+pin.file;mkdirSync(dir,{recursive:true});
  if(existsSync(path)){const bytes=readFileSync(path);if(bytes.length!==pin.size_bytes||createHash('sha256').update(bytes).digest('hex')!==pin.sha256)throw Error('Clash app cache integrity failed');return;}
  const response=await fetch(pin.url,{proxy:process.env.AI_PHONE_BOOTSTRAP_PROXY||null,signal:AbortSignal.timeout(180000)} as any);
  if(!response.ok||!response.body)throw Error('Clash app download failed');
  const reader=response.body.getReader(),parts:Uint8Array[]=[];let size=0;
  try{while(true){const next=await reader.read();if(next.done)break;size+=next.value.length;if(size>pin.size_bytes||size>300*1024*1024)throw Error('Clash app response exceeds pinned size');parts.push(next.value);}}finally{await reader.cancel().catch(()=>{});}
  const bytes=Buffer.concat(parts);if(size!==pin.size_bytes||createHash('sha256').update(bytes).digest('hex')!==pin.sha256)throw Error('Clash app integrity failed');
  const temp=path+'.'+randomBytes(8).toString('hex')+'.tmp';try{writeFileSync(temp,bytes,{mode:0o644});renameSync(temp,path);}finally{if(existsSync(temp))unlinkSync(temp);}
 }));return {verified:true,versions:{android:metadata.android.version,linux:metadata.linux.version}};
}
async function adb(args:string[]){const p=Bun.spawn([root+'/scripts/android-adb',...args],{cwd:root,stdout:'pipe',stderr:'ignore'});const text=await new Response(p.stdout).text();if(await p.exited)throw Error('Android app installation command failed');return text;}
export async function installPhone(){
 const pin=pins().android;await downloadApps(['android']);
 // Preserve newer/user-managed installs and all existing application data.
 if(!(await adb(['shell','pm path com.github.metacubex.clash.meta 2>/dev/null || true'])).trim())await adb(['install','-r',root+'/.tools/clash-apps/android/'+pin.file]);
 const id=process.env.AI_PHONE_DEVICE_ID||'phone-local-01';if(!/^[a-zA-Z0-9-]{1,64}$/.test(id))throw Error('Invalid device ID');
 const dir=root+'/data/device-network/'+id;mkdirSync(dir,{recursive:true,mode:0o700});const old=existsSync(dir+'/source.json')?JSON.parse(readFileSync(dir+'/source.json','utf8')):{};
 if(!old.enabled){await adb(['shell','mkdir -p /data/local/ai-phone-network; chmod 700 /data/local/ai-phone-network; touch /data/local/ai-phone-network/app-managed']);if(!existsSync(dir+'/app-managed.json'))writeFileSync(dir+'/app-managed.json',JSON.stringify({application:'Clash Meta',at:Date.now()}),{mode:0o600});}
 return {installed:true,package:'com.github.metacubex.clash.meta'};
}
if(import.meta.main){try{const value=JSON.parse(await Bun.stdin.text());const result=value.operation==='download'?await downloadApps():value.operation==='verify'?verifyApps():value.operation==='phone-install'?await installPhone():null;if(!result)throw Error('Unknown app installation operation');console.log(JSON.stringify(result));}catch(e){console.log(JSON.stringify({error:e instanceof Error?e.message:'Clash app installation failed'}));process.exitCode=1;}}
