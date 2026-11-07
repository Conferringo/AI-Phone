import {createHash,randomBytes} from 'node:crypto';
import {readFileSync,writeFileSync,renameSync,mkdirSync,existsSync,copyFileSync,chmodSync,unlinkSync} from 'node:fs';
import {resolve} from 'node:path';
import {makeConfig,downloadSubscription,subscriptionUrl} from './config';
export const root=process.env.AI_PHONE_PROJECT_ROOT||resolve(import.meta.dir,'../..');
const docker=process.env.AI_PHONE_DOCKER||Bun.which('docker')||['/usr/local/bin/docker','/opt/homebrew/bin/docker'].find(existsSync)||'docker';
type Target={deviceId:string;kind:'phone'|'desktop'};
const remote='/data/local/ai-phone-network';
const desktopFolder='/etc/ai-phone-network';
const folder=(t:Target)=>root+'/data/device-network/'+t.deviceId;
function target(value:any):Target{
 if(value.kind==='phone'&&value.deviceId==='phone-local-01')return value;
 if(value.kind==='desktop'&&/^[0-9a-f-]{36}$/.test(value.deviceId))return value;
 throw Error('此设备的网络运行时尚未接入');
}
function json(path:string,fallback:any={}){if(!existsSync(path))return fallback;try{const value=JSON.parse(readFileSync(path,'utf8'));if(!value||typeof value!=='object'||Array.isArray(value))throw Error();return value;}catch{throw Error('设备网络配置损坏，请从备份恢复');}}
function atomic(path:string,value:any){mkdirSync(resolve(path,'..'),{recursive:true,mode:0o700});const temp=path+'.'+randomBytes(8).toString('hex')+'.tmp';try{writeFileSync(temp,JSON.stringify(value),{mode:0o600});renameSync(temp,path);}finally{if(existsSync(temp))unlinkSync(temp);}}
async function run(args:string[],input?:string,seconds=20){
 const proc=Bun.spawn(args,{cwd:root,stdin:input===undefined?'ignore':new TextEncoder().encode(input),stdout:'pipe',stderr:'ignore',timeout:seconds*1000,env:process.env});
 const output=await new Response(proc.stdout).text(),code=await proc.exited;
 if(code!==0)throw Error('设备网络命令失败，请检查设备状态与核心配置');
 if(output.length>2*1024*1024)throw Error('设备网络响应过大');
 return output.trim();
}
const adb=(...args:string[])=>run([root+'/scripts/android-adb',...args]);
async function shell(t:Target,command:string){return t.kind==='phone'?adb('shell',command):run([docker,'exec','-i','ai-phone-desktop','sh','-c',command]);}
async function put(t:Target,local:string,name:string){if(t.kind==='phone')await adb('push',local,remote+'/'+name);else if(resolve(local)!==resolve(folder(t),name))copyFileSync(local,folder(t)+'/'+name);}
export async function prepare(value:any){
 const t=target(value);
 const program=process.env.AI_PHONE_NATIVE_HOST_PROGRAM;
 if(!program)throw Error('原生设备管理程序未配置');
 if(await run([program,'--prepare-device-network',root,t.kind,t.deviceId])!=='prepared')throw Error('设备目录准备未确认');
 return {prepared:true};
}
export async function install(){
 const pins=json(root+'/config/mihomo-release.json');
 if(!pins.version||!pins.geoip)throw Error('核心发行清单不完整');
 for(const [kind,pin] of Object.entries<any>(pins.assets)){
  const dir=root+'/.tools/mihomo/'+kind;mkdirSync(dir,{recursive:true});
  if(existsSync(dir+'/mihomo')&&createHash('sha256').update(readFileSync(dir+'/mihomo')).digest('hex')!==pin.binary_sha256)throw Error('缓存核心校验失败，请重新安装');
  if(!existsSync(dir+'/mihomo')){
   const response=await fetch(pin.url,{proxy:process.env.AI_PHONE_BOOTSTRAP_PROXY||null,signal:AbortSignal.timeout(90000)} as any);
   if(!response.ok)throw Error('核心下载失败');const bytes=new Uint8Array(await response.arrayBuffer());
   if(bytes.length>64*1024*1024||createHash('sha256').update(bytes).digest('hex')!==pin.sha256)throw Error('核心校验失败');
   const binary=Bun.gunzipSync(bytes);if(createHash('sha256').update(binary).digest('hex')!==pin.binary_sha256)throw Error('核心解压校验失败');const temp=dir+'/mihomo.'+randomBytes(8).toString('hex')+'.tmp';writeFileSync(temp,binary,{mode:0o755});renameSync(temp,dir+'/mihomo');
  }
 }
 const geo=root+'/.tools/mihomo/Country.mmdb';
 if(existsSync(geo)&&createHash('sha256').update(readFileSync(geo)).digest('hex')!==pins.geoip.sha256)throw Error('缓存路由数据校验失败');
 if(!existsSync(geo)){
  const response=await fetch(pins.geoip.url,{proxy:process.env.AI_PHONE_BOOTSTRAP_PROXY||null,signal:AbortSignal.timeout(60000)} as any);
  if(!response.ok)throw Error('路由数据下载失败');const bytes=new Uint8Array(await response.arrayBuffer());
  if(bytes.length>32*1024*1024||createHash('sha256').update(bytes).digest('hex')!==pins.geoip.sha256)throw Error('路由数据校验失败');
  writeFileSync(geo,bytes,{mode:0o644});
 }
 return {installed:true,version:pins.version};
}
async function observe(t:Target,operation:string,mutating=false){
 const program=process.env.AI_PHONE_NATIVE_HOST_PROGRAM;
 if(!program)throw Error("原生设备管理程序未配置");
 const child=Bun.spawn([program,mutating?'--control-device-network':'--observe-device-network',root,t.kind,t.deviceId,operation,...(mutating?['--stdin-lease']:[])],{cwd:root,stdin:mutating?'pipe':'ignore',stdout:'pipe',stderr:'pipe',timeout:mutating?undefined:65000,env:process.env});
 try{
 const [text,error,code]=await Promise.all([new Response(child.stdout).text(),new Response(child.stderr).text(),child.exited]);
 if(code!==0){
  const messages:Record<string,string>={device_network_metadata:'设备网络配置损坏，请从备份恢复',device_network_application_owned:'网络已交由设备内 Clash 应用管理',device_network_state_unknown:'设备状态尚未确认，未重复启动网络核心',device_network_stop_unconfirmed:'旧网络核心未完全退出',device_network_tun_unconfirmed:'Clash TUN 启动失败',device_network_timeout:'网络操作超时；请读取设备状态后再操作'};
  const key=Object.keys(messages).find(key=>error.includes(key));
  throw Error(key?messages[key]:'设备网络命令失败，请检查设备状态与核心配置');
 }
 if(text.length>2*1024*1024)throw Error('设备网络响应过大');
 return JSON.parse(text);
 }finally{if(mutating)child.stdin!.end();}
}
export async function state(value:any){return observe(target(value),"state");}
async function stop(t:Target){
 await observe(t,'stop',true);
}
async function start(t:Target){
 await observe(t,'start',true);
}
export async function resume(value:any){return observe(target(value),'resume',true);}
export async function probe(value:any){return observe(target(value),"probe");}
export async function apply(value:any){
 const t=target(value),dir=folder(t);await prepare(t);chmodSync(dir,0o700);
 if(existsSync(dir+'/app-managed.json'))throw Error('请在设备内的 Clash 应用中管理网络');
 const old=json(dir+'/source.json'),last=json(dir+'/last-request.json');
 if(!Number.isSafeInteger(value.expectedRevision)||value.expectedRevision<0)throw Error('网络配置版本无效');
 const fingerprint=createHash('sha256').update(JSON.stringify([value.expectedRevision,value.enabled,value.subscriptionUrl,value.useSavedSubscription])).digest('hex');
 if((old.requestId||last.id)===value.requestId){if((old.requestFingerprint||last.fingerprint)!==fingerprint)throw Error('同一请求编号对应不同配置');return state(t);}
 if((old.revision||0)!==value.expectedRevision)throw Error('配置版本冲突，请重新载入');
 const url=value.useSavedSubscription?old.subscriptionUrl:(value.subscriptionUrl?subscriptionUrl(value.subscriptionUrl):'');
 if(value.enabled&&!url)throw Error('请填写订阅地址');
 atomic(dir+'/job.json',{status:'applying',requestId:value.requestId,pid:process.pid,error:''});let changed=false,committed=false;
 try{
  if(value.enabled){
   await install();const yaml=await downloadSubscription(url);
   if(!existsSync(dir+'/token'))writeFileSync(dir+'/token',randomBytes(32).toString('hex'),{mode:0o600});
   const config=makeConfig(yaml,readFileSync(dir+'/token','utf8'),t.kind);
   atomic(dir+'/candidate.json',config);
   if(t.kind==='phone'){await adb('shell','mkdir -p '+remote+'; chmod 700 '+remote);await adb('push',root+'/.tools/mihomo/android-arm64/mihomo',remote+'/mihomo');await adb('shell','chmod 700 '+remote+'/mihomo');}
   await put(t,root+'/.tools/mihomo/Country.mmdb','Country.mmdb');await put(t,root+'/infra/network/supervisor.sh','supervisor.sh');await put(t,dir+'/candidate.json','candidate.json');
   const guest=t.kind==='phone'?remote:desktopFolder,core=t.kind==='phone'?remote+'/mihomo':'/opt/ai-phone/network-core/mihomo';
   await shell(t,`${core} -t -d ${guest} -f ${guest}/candidate.json >/dev/null 2>&1`);
   await shell(t,`if [ -f ${guest}/config.json ]; then cp ${guest}/config.json ${guest}/previous.json; fi`);
   changed=true;await stop(t);
   await shell(t,`cp ${guest}/candidate.json ${guest}/config.json; chmod 600 ${guest}/config.json`);
   await start(t);
   atomic(dir+'/source.json',{deviceId:t.deviceId,revision:(old.revision||0)+1,enabled:true,subscriptionUrl:url,nodeCount:config.proxies.length,requestId:value.requestId,requestFingerprint:fingerprint});committed=true;
  }else{
   changed=true;await stop(t);atomic(dir+'/source.json',{...old,revision:(old.revision||0)+1,enabled:false,subscriptionUrl:url,requestId:value.requestId,requestFingerprint:fingerprint});committed=true;
  }
  try{atomic(dir+'/job.json',{status:'done',requestId:value.requestId,error:''});}catch{}return state(t);
 }catch(e){
  if(committed)return state(t);
  let restored=true;if(changed){try{await stop(t);const guest=t.kind==='phone'?remote:desktopFolder;if(old.enabled){if(value.enabled)await shell(t,`cp ${guest}/previous.json ${guest}/config.json`);await start(t);}}catch{restored=false;}}
  const message=(e instanceof Error?e.message:'网络配置失败')+(!restored?'；原配置恢复未确认，请检查设备网络':'');atomic(dir+'/job.json',{status:'failed',error:message});throw Error(message);
 }
}
export async function execute(value:any){
 if(value.operation==='install-apps'){const {NativeApps}=await import('./native-apps');return new NativeApps(root).download();}
 if(value.operation==='install')return install();
 if(value.operation==='prepare')return prepare(value);
 if(value.operation==='state')return state(value);
 if(value.operation==='apply')return apply(value);
 if(value.operation==='probe')return probe(value);
 if(value.operation==='resume')return resume(value);
 if(value.operation==='handoff')return observe(target(value),'handoff',true);
 if(value.operation==='credential')return observe(target(value),'credential');
 throw Error('未知网络操作');
}
if(import.meta.main){try{const input=JSON.parse(await Bun.stdin.text());console.log(JSON.stringify(await execute(input)));}catch(e){console.log(JSON.stringify({error:e instanceof Error?e.message:'设备网络失败'}));process.exitCode=1;}}
