import {createHash,randomBytes} from 'node:crypto';
import {readFileSync,writeFileSync,renameSync,mkdirSync,existsSync,copyFileSync,chmodSync,symlinkSync,lstatSync,realpathSync,unlinkSync} from 'node:fs';
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
export function prepare(value:any){const t=target(value);mkdirSync(folder(t),{recursive:true,mode:0o700});if(t.kind==='desktop'){const alias=root+'/data/device-network/desktop';if(existsSync(alias)||(()=>{try{return lstatSync(alias).isSymbolicLink();}catch{return false;}})()){if(realpathSync(alias)!==realpathSync(folder(t)))throw Error('电脑网络目录属于其他设备');}else symlinkSync(folder(t),alias,'dir');}return {prepared:true};}
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
async function controller(t:Target,path:string){
 const token=readFileSync(folder(t)+'/token','utf8');
 if(t.kind==='phone'){
  await adb('forward','tcp:17868','tcp:19090');
  const response=await fetch('http://127.0.0.1:17868'+path,{headers:{authorization:'Bearer '+token},proxy:null,signal:AbortSignal.timeout(2500)} as any);
  if(!response.ok)throw Error('核心未返回状态');return response.json();
 }
 const script="import json,urllib.request; c=json.load(open('/etc/ai-phone-network/config.json')); r=urllib.request.Request('http://127.0.0.1:19090'+input(),headers={'Authorization':'Bearer '+c['secret']}); print(urllib.request.build_opener(urllib.request.ProxyHandler({})).open(r,timeout=2).read().decode())";
 return JSON.parse(await run([docker,'exec','-i','ai-phone-desktop','python3','-c',script],path+'\n',5));
}
async function processes(t:Target){
 const dir=t.kind==='phone'?remote:desktopFolder;
 try{const result=await shell(t,`for f in supervisor core; do if [ -r ${dir}/$f.pid ]; then p=$(cat ${dir}/$f.pid); case "$p" in ''|*[!0-9]*) continue;; esac; if [ -r /proc/$p/cmdline ] && tr '\\000' ' ' </proc/$p/cmdline | grep -q '${dir}'; then echo running; exit 0; fi; fi; done; echo stopped`);return result==='running'?'running':result==='stopped'?'stopped':'unknown';}catch{return 'unknown';}
}
async function health(t:Target){
 try{const [version,config]=await Promise.all([controller(t,'/version'),controller(t,'/configs')]);const link=await shell(t,'ip link show aiphone0 >/dev/null 2>&1 && echo active');return {running:true,confirmed:true,version:version.version||'',tun:config.tun?.enable===true&&link==='active'};}catch{const actual=await processes(t);return {running:actual==='running',confirmed:actual==='stopped',version:'',tun:false};}
}
export async function state(value:any){
 const t=target(value),source=json(folder(t)+'/source.json'),job=json(folder(t)+'/job.json'),checks=json(folder(t)+'/checks.json');
 if(existsSync(folder(t)+'/app-managed.json')){
  // The GUI owns its settings and lifecycle; never diagnose it using the retired controller.
  let running=false,tunActive=false,confirmed=false;
  try{const output=await shell(t,t.kind==='phone'?"pidof com.github.metacubex.clash.meta:background >/dev/null && echo app; ip link show tun0 >/dev/null 2>&1 && echo tun; true":"pgrep -x mihomo >/dev/null && echo app; ip link show Mihomo >/dev/null 2>&1 && echo tun; true");running=output.split('\n').includes('app');tunActive=output.split('\n').includes('tun');confirmed=true;}catch{}
  return {deviceId:t.deviceId,managedBy:'application',revision:source.revision||0,enabled:running,hasSubscription:!!source.subscriptionUrl,status:!confirmed?'unknown':running?(tunActive?'running':'application'):'off',error:confirmed?'':'设备状态暂未确认',coreVersion:'',tunActive,nodeCount:source.nodeCount||0,checkedAtMs:checks.at||0,checks:checks.items||[]};
 }
 const h=await health(t);
 const committed=job.requestId&&source.requestId===job.requestId;let alive=false;try{if(Number.isSafeInteger(job.pid)&&job.pid>0){process.kill(job.pid,0);alive=true;}}catch{}
 const interrupted=job.status==='applying'&&!committed&&!alive;
 return {deviceId:t.deviceId,revision:source.revision||0,enabled:!!source.enabled,hasSubscription:!!source.subscriptionUrl,status:interrupted?'interrupted':job.status==='applying'&&!committed?'applying':!h.confirmed?'unknown':h.running?(h.tun?'running':'degraded'):(source.enabled?'failed':'off'),error:interrupted?'网络应用中断，请检查设备状态后重试':(!committed?job.error:'')||(!h.confirmed?'设备网络状态暂未确认，请稍后重试':!h.running&&source.enabled?'设备内 Clash 未运行':h.running&&!h.tun?'TUN 未生效':''),coreVersion:h.version,tunActive:h.tun,nodeCount:source.nodeCount||0,checkedAtMs:checks.at||0,checks:checks.revision===source.revision?checks.items||[]:[]};
}
async function stop(t:Target){
 const dir=t.kind==='phone'?remote:desktopFolder;
 await shell(t,`rm -f ${dir}/enabled; if [ -f ${dir}/supervisor.pid ]; then p=$(cat ${dir}/supervisor.pid); case "$p" in ''|*[!0-9]*) exit 1;; esac; if [ -r /proc/$p/cmdline ] && tr '\\000' ' ' </proc/$p/cmdline | grep -q '${dir}'; then kill -TERM "$p"; fi; fi`);
 for(let i=0;i<40;i++){if(await processes(t)==='stopped'){if(t.kind==='phone')await shell(t,'ip rule del priority 8995 uidrange 10000-99999 table 2022 2>/dev/null || true');else await shell(t,'ip rule del priority 8990 fwmark 9488 table main 2>/dev/null || true');return;}await Bun.sleep(150);}
 throw Error('旧网络核心未完全退出');
}
async function start(t:Target){
 if(existsSync(folder(t)+'/app-managed.json'))throw Error('网络已交由设备内 Clash 应用管理');
 const dir=t.kind==='phone'?remote:desktopFolder,core=t.kind==='phone'?remote+'/mihomo':'/opt/ai-phone/network-core/mihomo';
 const current=await processes(t);if(current==='unknown')throw Error('设备状态尚未确认，未重复启动网络核心');
 const mark=t.kind==='desktop'?"ip rule show | grep -q 'fwmark 0x2510 lookup main' || ip rule add priority 8990 fwmark 9488 table main; ":"ip rule show | grep -q '8995:.*uidrange 10000-99999 lookup 2022' || ip rule add priority 8995 uidrange 10000-99999 table 2022; ";
 if(current==='stopped')await shell(t,`mkdir -p /dev/net; if [ ! -c /dev/net/tun ]; then mknod /dev/net/tun c 10 200; fi; chmod 600 /dev/net/tun; ${mark}touch ${dir}/enabled; chmod 600 ${dir}/config.json ${dir}/enabled; nohup sh ${dir}/supervisor.sh ${dir} ${core} >/dev/null 2>&1 </dev/null &`);
 for(let i=0;i<30;i++){const h=await health(t);if(h.running&&h.tun)return;await Bun.sleep(300);}
 throw Error('Clash TUN 启动失败');
}
export async function resume(value:any){const t=target(value);if(existsSync(folder(t)+'/app-managed.json'))return {managedBy:'application'};const source=json(folder(t)+'/source.json');if(source.enabled){if(t.kind==='phone'&&(await adb('shell','id -u'))!=='0'){await adb('root');for(let i=0;i<15;i++){try{if(await adb('shell','id -u')==='0')break;}catch{}await Bun.sleep(500);}}const h=await health(t);if(h.confirmed&&!h.running)await start(t);}return state(t);}
export async function probe(value:any){
 const t=target(value),source=json(folder(t)+'/source.json');const items=[];const download=json(root+'/config/mihomo-release.json').verification_download;
 for(const [name,url] of [['国内 HTTPS','https://www.baidu.com/'],['境外 HTTPS','https://example.com/'],['下载','https://raw.githubusercontent.com/MetaCubeX/mihomo/v1.19.33/LICENSE']]){
  const start=Date.now();try{
   let detail='';
   if(t.kind==='phone'){
    // UID 2000 follows the same TUN route as apps. Root is reserved for the core.
    const path='/data/local/tmp/aiphone-netprobe-'+randomBytes(8).toString('hex');
    const command=`umask 077; curl --noproxy '*' -fsSL --max-time 15 '${url}' -o '${path}' && { wc -c < '${path}'; sha256sum '${path}'; }; status=$?; rm -f '${path}'; exit $status`;
    const quoted="'"+command.replaceAll("'","'\\''")+"'";
    const output=await adb('shell','su 2000 sh -c '+quoted);
    const [count,checksum]=output.split(/\s+/),bytes=Number(count);if(!Number.isSafeInteger(bytes)||bytes<30||name==='下载'&&(checksum!==download?.sha256||bytes!==download?.size_bytes))throw Error('response');detail=bytes+' bytes';
   }else{
    const script="import urllib.request,hashlib,json; u=input(); b=urllib.request.build_opener(urllib.request.ProxyHandler({})).open(u,timeout=15).read(1024*1024); open('/etc/ai-phone-network/probe-download','wb').write(b); print(json.dumps({'bytes':len(b),'sha256':hashlib.sha256(b).hexdigest()}))";
    const result=JSON.parse(await run([docker,'exec','-i','ai-phone-desktop','python3','-c',script],url+'\n'));if(result.bytes<30||name==='下载'&&(result.sha256!==download?.sha256||result.bytes!==download?.size_bytes))throw Error('response');detail=result.bytes+' bytes';
   }
   items.push({name,passed:true,detail,durationMs:Date.now()-start});
  }catch{items.push({name,passed:false,detail:'设备内直连请求失败；未使用宿主代理',durationMs:Date.now()-start});}
 }
 atomic(folder(t)+'/checks.json',{revision:source.revision,at:Date.now(),items});return state(t);
}
export async function apply(value:any){
 const t=target(value),dir=folder(t);prepare(t);chmodSync(dir,0o700);
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
 if(value.operation==='install-apps'){const {downloadApps}=await import('./native-apps');return downloadApps();}
 if(value.operation==='install')return install();
 if(value.operation==='prepare')return prepare(value);
 if(value.operation==='state')return state(value);
 if(value.operation==='apply')return apply(value);
 if(value.operation==='probe')return probe(value);
 if(value.operation==='resume')return resume(value);
 if(value.operation==='handoff'){
  const t=target(value),marker=folder(t)+'/app-managed.json';
  if(existsSync(marker))return {managedBy:'application'};
  atomic(marker,{application:t.kind==='phone'?'Clash Meta':'Clash Party',at:Date.now()});
  try{await stop(t);}catch(e){unlinkSync(marker);throw e;}
  return {managedBy:'application'};
 }
 if(value.operation==='credential'){const t=target(value),s=json(folder(t)+'/source.json');return {subscriptionUrl:s.subscriptionUrl||'',revision:s.revision||0};}
 throw Error('未知网络操作');
}
if(import.meta.main){try{const input=JSON.parse(await Bun.stdin.text());console.log(JSON.stringify(await execute(input)));}catch(e){console.log(JSON.stringify({error:e instanceof Error?e.message:'设备网络失败'}));process.exitCode=1;}}
