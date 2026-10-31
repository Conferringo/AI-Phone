import {MessageMedia} from 'whatsapp-web.js';
import QRCode from 'qrcode';
import {WhatsAppClient} from './whatsapp-client';
import {BrowserConfigurationError,BrowserSession,sessionProfile} from './whatsapp-runtime';

type Job={id:string;target:{chat:string}};
type Message={from:string;body:string;fromMe:boolean;isStatus:boolean;id:{_serialized:string}};
type ClientLike={on:(name:string,fn:(...args:any[])=>void)=>unknown;initialize:()=>Promise<void>;destroy:()=>Promise<void>;logout:()=>Promise<void>;sendMessage:(chat:string,content:any)=>Promise<{id:{_serialized:string}}>;info?:{wid:{_serialized:string}}};
type Options={root:string;sessionId:string;proxyUrl:string;onIdentity:(id:string)=>boolean;onMessage:(sender:string,id:string,text:string,target:{chat:string})=>void};
type Runtime=Pick<BrowserSession,'options'|'revoked'|'close'|'erase'>;
type Dependencies={client?:(options:any)=>ClientLike;browser?:()=>string;runtime?:(input:ConstructorParameters<typeof BrowserSession>[0])=>Runtime;qr?:(text:string)=>Promise<Buffer>;retryMs?:number};
const expiredSessions=new Set(['LOGOUT','UNPAIRED','UNPAIRED_IDLE','CONFLICT','TOS_BLOCK','SMB_TOS_BLOCK']);
function privateText(message:Message){return !message.fromMe&&!message.isStatus&&!!message.body?.trim()&&(message.from.endsWith('@c.us')||message.from.endsWith('@lid'));}

class ConnectionAttempt{
 readonly abort=new AbortController();
 readonly runtime:Runtime;
 readonly client:ClientLike;
 private closing?:Promise<void>;
 constructor(options:Options,dependencies:Dependencies){
  const input={...options,signal:this.abort.signal};
  this.runtime=dependencies.runtime?dependencies.runtime(input):new BrowserSession(input,{hostBrowser:dependencies.browser});
  this.client=(dependencies.client||((config:any)=>new WhatsAppClient(config)))({...this.runtime.options,authTimeoutMs:45000,qrMaxRetries:0});
 }
 close(){
  this.abort.abort();
  return this.closing??=Promise.resolve().then(async()=>{
   await this.runtime.close(this.client);
   if(this.runtime.revoked)await this.runtime.erase();
  });
 }
}

class WhatsAppConnection{
 state='connecting';error='';qrPng=new Uint8Array();
 private generation=0;
 private qrGeneration=0;
 private retries=0;
 private attempt?:ConnectionAttempt;
 private retry?:ReturnType<typeof setTimeout>;
 private deadline?:ReturnType<typeof setTimeout>;
 constructor(private readonly options:Options,private readonly dependencies:Dependencies){void this.connect(this.generation);}
 private current(version:number){return version===this.generation;}
 private clearQr(){this.qrGeneration++;this.qrPng=new Uint8Array();}
 private clearTimers(){clearTimeout(this.retry);clearTimeout(this.deadline);}
 async stop(){
  if(this.state!=='failed')this.state='idle';
  this.generation++;this.clearTimers();this.clearQr();
  try{await this.attempt?.close();}catch(error){this.closeFailed();throw error;}
 }
 async logout(){
  const attempt=this.attempt;if(!attempt)throw Error('WhatsApp浏览器未就绪，账号数据已保留');
  let confirmed=this.state==='waiting_qr';
  try{if(this.state==='connected'){await attempt.client.logout();confirmed=true;}}catch{}
  await this.stop();await attempt.runtime.erase();return confirmed;
 }
 private connected(){
  if(this.state!=='connected'||!this.attempt)throw Error('WhatsApp未连接');
  return this.attempt.client;
 }
 async send(job:Job,text:string){
  const client=this.connected();if(!text||text.length>65000)throw Error('WhatsApp消息长度无效');
  return (await client.sendMessage(job.target.chat,text)).id._serialized;
 }
 async sendImage(job:Job,bytes:Buffer){return (await this.connected().sendMessage(job.target.chat,new MessageMedia('image/png',bytes.toString('base64'),'device.png'))).id._serialized;}
 private closeFailed(){this.generation++;this.state='failed';this.error='WhatsApp浏览器尚未确认关闭，账号数据已保留';this.clearTimers();this.clearQr();}
 private fail(version:number,message:string){
  if(!this.current(version))return;
  this.state='failed';this.error=message;void this.stop().catch(()=>{});
 }
 private timeout(version:number,message:string){clearTimeout(this.deadline);this.deadline=setTimeout(()=>this.fail(version,message),90000);}
 private async connect(expected:number){
  if(!this.current(expected))return;
  const version=++this.generation;this.clearQr();this.state=this.retries?'reconnecting':'connecting';
  try{
   const attempt=new ConnectionAttempt(this.options,this.dependencies);this.attempt=attempt;
   this.listen(attempt,version);this.timeout(version,'WhatsApp连接超时，请检查网络');
   await attempt.client.initialize();if(!this.current(version))await attempt.close();
  }catch(error){this.fail(version,error instanceof BrowserConfigurationError?error.message:'WhatsApp无法连接，请检查浏览器组件与网络');}
 }
 private listen(attempt:ConnectionAttempt,version:number){
  const peer=attempt.client;
  peer.on('qr',(text:string)=>void this.renderQr(version,text));
  peer.on('authenticated',()=>this.authenticated(version));
  peer.on('ready',()=>this.ready(attempt,version));
  peer.on('auth_failure',()=>this.fail(version,'WhatsApp登录已失效，请重新扫码'));
  peer.on('message',(message:Message)=>this.message(version,message));
  peer.on('disconnected',(reason:string)=>this.disconnected(attempt,version,reason));
 }
 private async renderQr(version:number,text:string){
  if(!this.current(version))return;clearTimeout(this.deadline);const qr=++this.qrGeneration;
  try{
   const bytes=await (this.dependencies.qr||((value:string)=>QRCode.toBuffer(value,{type:'png',width:280,margin:2,errorCorrectionLevel:'M'})))(text);
   if(this.current(version)&&qr===this.qrGeneration){this.qrPng=new Uint8Array(bytes);this.state='waiting_qr';this.error='';}
  }catch{this.fail(version,'WhatsApp二维码生成失败');}
 }
 private authenticated(version:number){
  if(!this.current(version))return;
  this.clearQr();this.state='authenticating';this.timeout(version,'WhatsApp登录初始化超时');
 }
 private ready(attempt:ConnectionAttempt,version:number){
  if(!this.current(version))return;clearTimeout(this.deadline);this.clearQr();
  const id=attempt.client.info?.wid._serialized;let accepted=false;
  try{accepted=!!id&&this.options.onIdentity(id);}catch{this.fail(version,'WhatsApp连接未能保存，请检查本机存储');return;}
  if(!accepted){this.fail(version,'此WhatsApp账号已绑定其他Agent');return;}
  this.retries=0;this.error='';this.state='connected';
 }
 private message(version:number,message:Message){
  if(!this.current(version)||this.state!=='connected')return;
  if(!privateText(message))return;
  try{this.options.onMessage(message.from,message.id._serialized,message.body,{chat:message.from});}catch{this.fail(version,'WhatsApp消息未能保存，请检查本机存储');}
 }
 private disconnected(attempt:ConnectionAttempt,version:number,reason:string){
  if(!this.current(version))return;clearTimeout(this.deadline);this.clearQr();
  if(expiredSessions.has(reason)){this.fail(version,'WhatsApp登录已失效，请重新扫码');return;}
  this.state='reconnecting';this.error='WhatsApp连接已断开';const epoch=++this.generation;
  const delay=this.dependencies.retryMs??Math.min(60000,2000*2**Math.min(this.retries++,5));
  void attempt.close().then(()=>{
   if(this.current(epoch))this.retry=setTimeout(()=>void this.connect(epoch),delay);
  },()=>this.closeFailed());
 }
}

export function startWhatsApp(options:Options,dependencies:Dependencies={}){
 sessionProfile(options.root,options.sessionId);
 return new WhatsAppConnection(options,dependencies);
}
