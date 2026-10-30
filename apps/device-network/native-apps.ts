/** Verified guest applications. The CLI and tests use the same owned context. */
import {randomBytes} from 'node:crypto';
import {readFileSync,writeFileSync,mkdirSync,existsSync,renameSync,unlinkSync} from 'node:fs';
import {resolve,dirname} from 'node:path';
import {appAsset,defaultAppKinds,verifyAppAsset,verifyAppBytes,type AppKind,type AppPin} from './app-assets';

type Environment=Record<string,string|undefined>;
type DownloadPin=AppPin&{url:string};
type Adb=(args:string[])=>Promise<string>;

async function deviceCommand(root:string,args:string[]){
 const process=Bun.spawn([root+'/scripts/android-adb',...args],{cwd:root,stdout:'pipe',stderr:'ignore'});
 const text=await new Response(process.stdout).text();
 if(await process.exited)throw Error('Android app installation command failed');
 return text;
}
async function responseReader(response:Response){
 if(!response.ok){await response.body?.cancel();throw Error('Clash app download failed');}
 if(!response.body)throw Error('Clash app response missing');
 return response.body.getReader();
}

export class NativeApps {
 private readonly root:string;
 private readonly env:Environment;
 private readonly adb:Adb;
 constructor(root:string,env:Environment=process.env,adb?:Adb){
  this.root=resolve(root);this.env={...env};this.adb=adb||((args)=>deviceCommand(this.root,args));
 }
 private pins(){
  const config=JSON.parse(readFileSync(this.root+'/config/clash-apps.json','utf8'));
  const x64=this.env.AI_PHONE_ANDROID_ABI==='x86_64';
  return {...config,android:x64?config.alternatives['android-x86_64']:config.android,linux:x64?config.alternatives['linux-amd64']:config.linux};
 }
 verify(kinds:AppKind[]=defaultAppKinds(this.env)){
  const metadata=this.pins();
  for(const kind of kinds){
   const pin=metadata[kind],{path}=appAsset(this.root,kind,pin,this.env);
   if(!existsSync(path))throw Error('Clash application cache missing');
   verifyAppAsset(path,pin);
  }
  return {verified:true};
 }
 async download(kinds:AppKind[]=defaultAppKinds(this.env)){
  const metadata=this.pins();
  await Promise.all(kinds.map(kind=>this.prepareApp(kind,metadata[kind])));
  return {verified:true,versions:{android:metadata.android.version,linux:metadata.linux.version}};
 }
 private async prepareApp(kind:AppKind,pin:DownloadPin){
  const {path,bundled}=appAsset(this.root,kind,pin,this.env);
  if(existsSync(path)){verifyAppAsset(path,pin);return;}
  if(bundled)throw Error('Bundled Clash application missing');
  const bytes=await this.downloadBytes(pin);
  mkdirSync(dirname(path),{recursive:true});
  const temp=path+'.'+randomBytes(8).toString('hex')+'.tmp';
  try{writeFileSync(temp,bytes,{mode:0o644});renameSync(temp,path);}finally{if(existsSync(temp))unlinkSync(temp);}
 }
 private async downloadBytes(pin:DownloadPin){
  const response=await fetch(pin.url,{proxy:this.env.AI_PHONE_BOOTSTRAP_PROXY||null,signal:AbortSignal.timeout(180000)} as any);
  const reader=await responseReader(response),parts:Uint8Array[]=[];let size=0;
  try{while(true){const next=await reader.read();if(next.done)break;size+=next.value.length;if(size>pin.size_bytes||size>300*1024*1024)throw Error('Clash app response exceeds pinned size');parts.push(next.value);}}finally{await reader.cancel().catch(()=>{});}
  const bytes=Buffer.concat(parts);verifyAppBytes(bytes,pin);return bytes;
 }
 async installPhone(){
  const pin=this.pins().android;await this.download(['android']);
  // Preserve newer/user-managed installs and all existing application data.
  if(!(await this.adb(['shell','pm path com.github.metacubex.clash.meta 2>/dev/null || true'])).trim())await this.adb(['install','-r',appAsset(this.root,'android',pin,this.env).path]);
  const id=this.env.AI_PHONE_DEVICE_ID||'phone-local-01';if(!/^[a-zA-Z0-9-]{1,64}$/.test(id))throw Error('Invalid device ID');
  const dir=this.root+'/data/device-network/'+id;mkdirSync(dir,{recursive:true,mode:0o700});const old=existsSync(dir+'/source.json')?JSON.parse(readFileSync(dir+'/source.json','utf8')):{};
  if(!old.enabled){await this.adb(['shell','mkdir -p /data/local/ai-phone-network; chmod 700 /data/local/ai-phone-network; touch /data/local/ai-phone-network/app-managed']);if(!existsSync(dir+'/app-managed.json'))writeFileSync(dir+'/app-managed.json',JSON.stringify({application:'Clash Meta',at:Date.now()}),{mode:0o600});}
  return {installed:true,package:'com.github.metacubex.clash.meta'};
 }
}

if(import.meta.main){
 try{
  const value=JSON.parse(await Bun.stdin.text());
  const apps=new NativeApps(process.env.AI_PHONE_PROJECT_ROOT||resolve(import.meta.dir,'../..'));
  const result=value.operation==='download'?await apps.download():value.operation==='verify'?apps.verify():value.operation==='phone-install'?await apps.installPhone():null;
  if(!result)throw Error('Unknown app installation operation');
  console.log(JSON.stringify(result));
 }catch(error){console.log(JSON.stringify({error:error instanceof Error?error.message:'Clash app installation failed'}));process.exitCode=1;}
}
