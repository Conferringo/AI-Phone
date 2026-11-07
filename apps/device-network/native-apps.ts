/** Verified guest applications. The CLI and tests use the same owned context. */
import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {resolve} from 'node:path';
type AppKind='android'|'linux';

type Environment=Record<string,string|undefined>;
type Adb=(args:string[])=>Promise<string>;

async function deviceCommand(root:string,args:string[]){
 const process=Bun.spawn([root+'/scripts/android-adb',...args],{cwd:root,stdout:'pipe',stderr:'ignore'});
 const text=await new Response(process.stdout).text();
 if(await process.exited)throw Error('Android app installation command failed');
 return text;
}
export class NativeApps {
 private readonly root:string;
 private readonly env:Environment;
 private readonly adb:Adb;
 constructor(root:string,env:Environment=process.env,adb?:Adb){
  this.root=resolve(root);this.env={...env};this.adb=adb||((args)=>deviceCommand(this.root,args));
  if(existsSync(this.root+'/runtime/services')){
   const manifest=JSON.parse(readFileSync(this.root+'/runtime/services/manifest.json','utf8'));
   if(manifest.guest_app_resources!=='ai-phone-guest-app-resources-v1')throw Error('Native guest application capability missing; rebuild the complete client');
  }
 }
 private command(operation:string,kinds?:AppKind[]){
  const program=this.env.AI_PHONE_NATIVE_HOST_PROGRAM;
  if(!program)throw Error('Native guest application program missing');
  return [program,'--guest-app-resources',this.root,operation,...(kinds||[])];
 }
 private result(text:string,error:string,code:number){
  if(code!==0){
   const messages:Record<string,string>={guest_app_cache_missing:'Clash application cache missing',guest_app_integrity:'Clash application integrity failed',guest_app_size:'Clash app response exceeds pinned size',guest_app_download:'Clash app download failed',guest_app_proxy:'Clash app proxy invalid',guest_app_component:'Invalid guest application component',guest_app_abi:'Unsupported guest application ABI',guest_app_selection:'Bundled guest application selection missing or invalid',guest_app_bundled_missing:'Bundled Clash application missing'};
   throw Error(messages[error.trim()]||'Clash application command failed');
  }
  return JSON.parse(text);
 }
 verify(kinds?:AppKind[]){
  const child=Bun.spawnSync(this.command('verify',kinds),{cwd:this.root,env:this.env,stdin:'ignore',stdout:'pipe',stderr:'pipe'});
  return this.result(new TextDecoder().decode(child.stdout),new TextDecoder().decode(child.stderr),child.exitCode);
 }
 async download(kinds?:AppKind[]){
  const owned=process.platform!=='win32';
  const child=Bun.spawn([...this.command('download',kinds),...(owned?['--stdin-lease']:[])],{cwd:this.root,env:this.env,stdin:owned?'pipe':'ignore',stdout:'pipe',stderr:'pipe'});
  try{
   const [text,error,code]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);
   return this.result(text,error,code);
  }finally{if(owned)child.stdin!.end();}
 }
 async installPhone(){
  const resource=await this.download(['android']);
  // Preserve newer/user-managed installs and all existing application data.
  if(!(await this.adb(['shell','pm path com.github.metacubex.clash.meta 2>/dev/null || true'])).trim())await this.adb(['install','-r',resource.paths.android]);
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
