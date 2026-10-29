/** Official Bot API long polling: outbound only; never replaces an existing webhook. */
export type TelegramConnection={state:string;error:string;stop():void;send(job:{target:any},text:string):Promise<unknown>;sendImage(job:{target:any},bytes:Buffer):Promise<unknown>};
type Update={update_id:number;message?:{message_id:number;text?:string;chat:{id:number;type:string};from?:{id:number;is_bot?:boolean}}};
type Options={appId:string;token:string;offset:number;onMessage:(sender:string,id:string,text:string,target:{chat:string})=>void;checkpoint:(offset:number)=>void;fetcher?:typeof fetch};
class ApiFailure extends Error {constructor(readonly code:number){super('telegram_api_failure');}}
export function telegramIdentity(token:string){return token.match(/^(\d{5,20}):[A-Za-z0-9_-]{20,}$/)?.[1]||'';}
export function telegramMessage(update:Update){
 const m=update.message;
 if(!Number.isSafeInteger(update.update_id)||!m?.text||m.text.length>24000||m.chat.type!=='private'||!m.from||m.from.is_bot||m.chat.id!==m.from.id||!Number.isSafeInteger(m.from.id)||!Number.isSafeInteger(m.message_id))return null;
 return {sender:String(m.from.id),id:String(update.update_id),text:m.text,target:{chat:String(m.chat.id)}};
}
export function startTelegram(options:Options):TelegramConnection {
 let stopped=false,offset=options.offset||0;
 const abort=new AbortController(),fetcher=options.fetcher||fetch;
 const status:TelegramConnection={state:'connecting',error:'',stop(){stopped=true;abort.abort();},async send(job,text){
  const clipped=text.length>4000?text.slice(0,3999).replace(/[\uD800-\uDBFF]$/,'')+'…':text;
  return api('sendMessage',{chat_id:job.target.chat,text:clipped});
 },async sendImage(job,bytes){const form=new FormData();form.append('chat_id',job.target.chat);form.append('photo',new Blob([new Uint8Array(bytes)],{type:'image/png'}),'screen.png');return api('sendPhoto',form);}};
 async function api(method:string,body:Record<string,unknown>|FormData){
  const response=await fetcher(`https://api.telegram.org/bot${options.token}/${method}`,{method:'POST',headers:body instanceof FormData?undefined:{'content-type':'application/json'},body:body instanceof FormData?body:JSON.stringify(body),signal:AbortSignal.any([abort.signal,AbortSignal.timeout(40000)])});
  if(!response.ok)throw new ApiFailure(response.status);
  const bytes=await response.arrayBuffer();if(bytes.byteLength>2*1024*1024)throw new ApiFailure(502);
  const value=JSON.parse(new TextDecoder().decode(bytes));if(!value.ok)throw new ApiFailure(Number(value.error_code)||502);
  return value.result;
 }
 void (async()=>{
  try {
   if(telegramIdentity(options.token)!==options.appId)throw new ApiFailure(401);
   const me=await api('getMe',{});if(String(me.id)!==options.appId||!me.is_bot)throw new ApiFailure(401);
   const hook=await api('getWebhookInfo',{});if(hook.url){status.state='failed';status.error='此机器人已配置 Webhook；请使用独立机器人或先在原服务解除';return;}
  }catch{if(!stopped){status.state='failed';status.error='Telegram 认证失败，请检查 Bot Token 和网络';}return;}
  let failures=0;
  while(!stopped){
   try {
    const updates=await api('getUpdates',{offset,timeout:25,limit:100,allowed_updates:['message']});
    if(stopped)return;
    if(!Array.isArray(updates))throw new ApiFailure(502);
    status.state='connected';status.error='';failures=0;
    for(const update of updates as Update[]){
     if(stopped)return;
     if(!Number.isSafeInteger(update.update_id)||update.update_id<offset)continue;
     const message=telegramMessage(update);
     if(message)options.onMessage(message.sender,message.id,message.text,message.target);
     const next=update.update_id+1;options.checkpoint(next);offset=next;
    }
   }catch(e){
    if(stopped)return;
    if(e instanceof ApiFailure&&[401,403,409].includes(e.code)){status.state='failed';status.error='Telegram 凭据、权限或长轮询冲突；请核对后重连';return;}
    status.state='reconnecting';status.error='Telegram 网络中断，正在重连';
    await new Promise<void>(resolve=>{const done=()=>{clearTimeout(timer);abort.signal.removeEventListener('abort',done);resolve();};const timer=setTimeout(done,Math.min(30000,1000*2**Math.min(failures++,5)));abort.signal.addEventListener('abort',done,{once:true});});
   }
  }
 })();
 return status;
}
