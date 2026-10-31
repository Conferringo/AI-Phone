import {LocalAuth,type ClientOptions} from 'whatsapp-web.js';
import {accessSync,constants,lstatSync,readdirSync} from 'node:fs';
import {rm} from 'node:fs/promises';
import {isAbsolute,resolve} from 'node:path';
import {randomBytes} from 'node:crypto';
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {whatsappBrowser,whatsappProxy} from './whatsapp-browser';

type Environment=NodeJS.ProcessEnv;
type Selection={mode:'host';executable:string}|{mode:'container';python:string;helper:string;image:string};
type Input={root:string;sessionId:string;proxyUrl:string;signal:AbortSignal};
type CommandOptions={env:Environment;timeout:number;maxBuffer:number;encoding:'utf8';windowsHide:boolean};
type Run=(file:string,args:string[],options:CommandOptions)=>Promise<{stdout:string}>;
type Dependencies={env?:Environment;hostBrowser?:()=>string;run?:Run};
const execute=promisify(execFile);
export class BrowserConfigurationError extends Error{}

export function sessionProfile(root:string,sessionId:string){
 if(!/^[a-zA-Z0-9_-]{1,128}$/.test(sessionId))throw new BrowserConfigurationError('WhatsApp会话标识无效');
 return resolve(root,'data/channels/whatsapp','session-'+sessionId);
}
function file(path:string,executable=false){
 try{if(!isAbsolute(path)||!lstatSync(path).isFile())throw Error();accessSync(path,executable?constants.X_OK:constants.R_OK);}
 catch{throw new BrowserConfigurationError('WhatsApp随包浏览器组件未就绪');}
}
function profileKind(profile:string){
 const entry=lstatSync(profile,{throwIfNoEntry:false});
 if(entry?.isSymbolicLink())throw new BrowserConfigurationError('WhatsApp账号目录已变化，原档案已保留');
 if(lstatSync(resolve(profile,'.aiphone-runtime.json'),{throwIfNoEntry:false}))return 'owned';
 return entry&&readdirSync(profile).length?'legacy':'empty';
}
export function browserSelection(root:string,sessionId:string,env:Environment=process.env,hostBrowser=()=>whatsappBrowser(root,env)):Selection{
 const kind=profileKind(sessionProfile(root,sessionId));
 const image=env.AI_PHONE_DESKTOP_IMAGE;
 if(kind!=='owned'&&(kind==='legacy'||env.AI_PHONE_WHATSAPP_CHROMIUM||!image))return {mode:'host',executable:hostBrowser()};
 return containerSelection(root,image,env.AI_PHONE_PYTHON);
}
function containerSelection(root:string,image:string|undefined,python:string|undefined):Selection{
 if(!image||!/^ai-phone-bundle\/desktop:sha-[0-9a-f]{64}$/.test(image)||!python)throw new BrowserConfigurationError('WhatsApp随包浏览器组件未就绪');
 const helper=resolve(root,'scripts/channel_browser.py');file(python,true);file(helper);
 return {mode:'container',python,helper,image};
}
function reachableProxy(value:string){
 if(!value)return;
 const host=new URL(value).hostname.toLowerCase();
 if(host==='localhost'||host==='[::1]'||host==='[::]'||host==='0.0.0.0'||/^127\./.test(host))throw new BrowserConfigurationError('WhatsApp容器浏览器无法使用宿主回环代理，请配置容器可达的代理地址');
}
class SessionAuth extends LocalAuth{
 revoked=false;
 override logout=async()=>{this.revoked=true;};
}
function launchOptions(selected:Selection,profile:string,signal:AbortSignal,args:string[],env:Environment):ClientOptions['puppeteer']{
 const common={headless:true as const,args,signal,userDataDir:profile};
 if(selected.mode==='host')return {...common,executablePath:selected.executable};
 return {...common,executablePath:selected.python,ignoreDefaultArgs:true,pipe:true,args:[selected.helper,'--user-data-dir='+profile,...args],env};
}

/** One launch owns SDK auth, shutdown confirmation and explicit account erasure. */
export class BrowserSession{
 readonly options:ClientOptions&{authStrategy:SessionAuth};
 private readonly selected:Selection;
 private readonly profile:string;
 private readonly env:Environment;
 private readonly run:Run;
 private closed=false;
 private erasure?:Promise<void>;
 constructor(input:Input,dependencies:Dependencies={}){
  const source=dependencies.env||process.env;
  this.profile=sessionProfile(input.root,input.sessionId);
  this.selected=browserSelection(input.root,input.sessionId,source,dependencies.hostBrowser);
  this.env={...source,AI_PHONE_PROJECT_ROOT:resolve(input.root),AI_PHONE_BROWSER_SESSION:input.sessionId,AI_PHONE_BROWSER_OWNER:randomBytes(32).toString('hex')};
  this.run=dependencies.run||((file,args,options)=>execute(file,args,options));
  const proxy=whatsappProxy(input.proxyUrl);if(this.selected.mode==='container')reachableProxy(input.proxyUrl);
  this.options={authStrategy:new SessionAuth({clientId:input.sessionId,dataPath:resolve(input.root,'data/channels/whatsapp')}),proxyAuthentication:proxy.auth,
   puppeteer:launchOptions(this.selected,this.profile,input.signal,['--no-first-run','--no-default-browser-check',...proxy.args],this.env)};
 }
 get revoked(){return this.options.authStrategy.revoked;}
 async close(peer:{destroy:()=>Promise<void>}){
  if(this.selected.mode==='host')await peer.destroy();
  else{await peer.destroy().catch(()=>{});await this.manage('--stop');}
  this.closed=true;
 }
 async erase(){
  if(!this.closed)throw Error('WhatsApp浏览器尚未确认关闭，账号数据已保留');
  return this.erasure??=this.eraseAccount();
 }
 private async eraseAccount(){
  if(this.selected.mode==='container')await this.manage('--erase');
  else{
   if(lstatSync(this.profile,{throwIfNoEntry:false})?.isSymbolicLink())throw Error('WhatsApp账号目录已变化，原档案已保留');
   await rm(this.profile,{recursive:true,force:true});
  }
 }
 private async manage(mode:'--stop'|'--erase'){
  const selected=this.selected;if(selected.mode!=='container')throw Error('Container browser required');
  const result=await this.run(selected.python,[selected.helper,mode],{env:this.env,timeout:25000,maxBuffer:16384,encoding:'utf8',windowsHide:true});
  const status=JSON.parse(result.stdout);
  if(status?.stopped!==true||mode==='--erase'&&status.erased!==true)throw Error('WhatsApp浏览器状态未确认，账号数据已保留');
 }
}
