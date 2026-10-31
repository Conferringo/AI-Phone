import {LocalAuth,MessageMedia} from 'whatsapp-web.js';
import {WhatsAppClient} from './whatsapp-client';
import QRCode from 'qrcode';
import {mkdirSync} from 'node:fs';
import {rm} from 'node:fs/promises';
import {resolve} from 'node:path';
import {whatsappBrowser,whatsappProxy} from './whatsapp-browser';

type Job={id:string;target:{chat:string}};
type Message={from:string;body:string;fromMe:boolean;isStatus:boolean;id:{_serialized:string}};
type ClientLike={on:(name:string,fn:(...args:any[])=>void)=>unknown;initialize:()=>Promise<void>;destroy:()=>Promise<void>;logout:()=>Promise<void>;sendMessage:(chat:string,content:any)=>Promise<{id:{_serialized:string}}>;info?:{wid:{_serialized:string}}};
type Options={root:string;sessionId:string;proxyUrl:string;onIdentity:(id:string)=>boolean;onMessage:(sender:string,id:string,text:string,target:{chat:string})=>void};
type Dependencies={client?:(options:any)=>ClientLike;browser?:()=>string;qr?:(text:string)=>Promise<Buffer>;retryMs?:number};
export function startWhatsApp(options:Options,dependencies:Dependencies={}){
 if(!/^[a-zA-Z0-9_-]{1,128}$/.test(options.sessionId))throw Error('WhatsApp会话标识无效');
 const profiles=resolve(options.root,'data/channels/whatsapp');mkdirSync(profiles,{recursive:true,mode:0o700});
 const proxy=whatsappProxy(options.proxyUrl);
 const build=dependencies.client||((config:any)=>new WhatsAppClient(config));
 const createQr=dependencies.qr||((text:string)=>QRCode.toBuffer(text,{type:'png',width:280,margin:2,errorCorrectionLevel:'M'}));
 let client:ClientLike|null=null,stopped=false,generation=0,qrGeneration=0,retries=0,retry:ReturnType<typeof setTimeout>|undefined,deadline:ReturnType<typeof setTimeout>|undefined;
 let abort:AbortController|null=null,cleanup=Promise.resolve();
 const closing=new WeakMap<ClientLike,Promise<void>>();
 const closeClient=(old:ClientLike|null)=>{
  if(!old)return cleanup;const previous=closing.get(old);if(previous)return previous;
  cleanup=cleanup.then(()=>old.destroy());closing.set(old,cleanup);return cleanup;
 };
 const closeFailed=()=>{stopped=true;control.state='failed';control.error='WhatsApp浏览器尚未确认关闭，账号数据已保留';};
 const control={state:'connecting',error:'',qrPng:new Uint8Array(),
  async stop(){if(control.state!=='failed')control.state='idle';stopped=true;generation++;qrGeneration++;clearTimeout(retry);clearTimeout(deadline);control.qrPng=new Uint8Array();abort?.abort();const old=client;client=null;await closeClient(old);},
  async logout(){let confirmed=control.state==='waiting_qr';try{if(client&&control.state==='connected'){await client.logout();confirmed=true;}}catch{}await control.stop();await rm(resolve(profiles,'session-'+options.sessionId),{recursive:true,force:true});return confirmed;},
  async send(job:Job,text:string){if(control.state!=='connected'||!client)throw Error('WhatsApp未连接');if(!text||text.length>65000)throw Error('WhatsApp消息长度无效');return (await client.sendMessage(job.target.chat,text)).id._serialized;},
  async sendImage(job:Job,bytes:Buffer){if(control.state!=='connected'||!client)throw Error('WhatsApp未连接');return (await client.sendMessage(job.target.chat,new MessageMedia('image/png',bytes.toString('base64'),'device.png'))).id._serialized;}
 };
 const connect=async()=>{
  if(stopped)return;const version=++generation;qrGeneration++;control.qrPng=new Uint8Array();control.state=retries?'reconnecting':'connecting';
  abort=new AbortController();
  const current=()=>!stopped&&version===generation;
  const fail=(message:string)=>{if(!current())return;clearTimeout(deadline);qrGeneration++;control.qrPng=new Uint8Array();control.state='failed';control.error=message;void control.stop().catch(closeFailed);};
  try{
   const binary=(dependencies.browser||(()=>whatsappBrowser(options.root)))();
   const active=build({authStrategy:new LocalAuth({clientId:options.sessionId,dataPath:profiles}),authTimeoutMs:45000,qrMaxRetries:0,proxyAuthentication:proxy.auth,puppeteer:{headless:true,executablePath:binary,args:['--no-first-run','--no-default-browser-check',...proxy.args],signal:abort.signal}});client=active;
   active.on('qr',(text:string)=>{if(!current())return;clearTimeout(deadline);const qr=++qrGeneration;void createQr(text).then(bytes=>{if(current()&&qr===qrGeneration){control.qrPng=new Uint8Array(bytes);control.state='waiting_qr';control.error='';}}).catch(()=>fail('WhatsApp二维码生成失败'));});
   active.on('authenticated',()=>{if(!current())return;qrGeneration++;control.qrPng=new Uint8Array();control.state='authenticating';deadline=setTimeout(()=>{fail('WhatsApp登录初始化超时');abort?.abort();},90000);});
   active.on('ready',()=>{if(!current())return;clearTimeout(deadline);qrGeneration++;control.qrPng=new Uint8Array();const id=active.info?.wid._serialized;let accepted=false;try{accepted=!!id&&options.onIdentity(id);}catch{fail('WhatsApp连接未能保存，请检查本机存储');return;}if(!accepted){fail('此WhatsApp账号已绑定其他Agent');return;}retries=0;control.error='';control.state='connected';});
   active.on('auth_failure',()=>{fail('WhatsApp登录已失效，请重新扫码');abort?.abort();});
   active.on('message',(message:Message)=>{if(!current()||control.state!=='connected'||message.fromMe||message.isStatus||!message.from.endsWith('@c.us')&&!message.from.endsWith('@lid')||!message.body?.trim())return;try{options.onMessage(message.from,message.id._serialized,message.body,{chat:message.from});}catch{fail('WhatsApp消息未能保存，请检查本机存储');}});
   active.on('disconnected',(reason:string)=>{
    if(!current())return;clearTimeout(deadline);qrGeneration++;control.qrPng=new Uint8Array();
    if(['LOGOUT','UNPAIRED','UNPAIRED_IDLE','CONFLICT','TOS_BLOCK','SMB_TOS_BLOCK'].includes(reason)){fail('WhatsApp登录已失效，请重新扫码');return;}
    control.state='reconnecting';control.error='WhatsApp连接已断开';const old=client;client=null;abort?.abort();generation++;
    const delay=dependencies.retryMs??Math.min(60000,2000*2**Math.min(retries++,5));
    void closeClient(old).then(()=>{if(!stopped)retry=setTimeout(()=>void connect(),delay);},closeFailed);
   });
   deadline=setTimeout(()=>{fail('WhatsApp连接超时，请检查网络');abort?.abort();},90000);
   await active.initialize();if(!current())await closeClient(active);
  }catch{if(current()&&control.state!=='failed'){fail('WhatsApp无法连接，请检查浏览器组件与网络');}}
 };
 void connect();return control;
}
