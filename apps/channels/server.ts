import {connectionKey,migrateAgentScopes,managedDevice,authorizedDevice} from "./scopes";
import {DesktopRequestSchema,DesktopObservationSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {startTelegram,telegramIdentity} from './telegram';
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { ChannelListRequestSchema, ChannelListResponseSchema, ChannelBindRequestSchema, ChannelUnbindRequestSchema, ChannelPairRequestSchema, ChannelPushRequestSchema, ChannelPushResultSchema, ChannelPushStatusRequestSchema } from "@ai-phone/protocol/ai_phone/v1/channels_pb";
import {RegistryRequestSchema,RegistryStateSchema} from "@ai-phone/protocol/ai_phone/v1/registry_pb";
import { ChatSendRequestSchema, ChatStateSchema, ChatRunRequestSchema } from "@ai-phone/protocol/ai_phone/v1/assistant_pb";
import { DeviceRequestSchema, DeviceObservationSchema } from "@ai-phone/protocol/ai_phone/v1/phone_runtime_pb";
import {ownerTarget,payloadHash,isScreenshotRequest,wantsScreenshot,validPng,deliverOnce,nextPending,type DeliveryStage} from "./outbox";
import { ApiErrorSchema } from "@ai-phone/protocol/ai_phone/v1/system_pb";
import { WSClient as WecomClient } from "@wecom/aibot-node-sdk";
import * as Lark from "@larksuiteoapi/node-sdk";
import { QQBot } from "@tencent-connect/qqbot-nodejs";
import { HttpsProxyAgent } from "https-proxy-agent";
import axios from "axios";
import https from "node:https";
import { createCipheriv, createDecipheriv, randomBytes, timingSafeEqual } from "node:crypto";
import { mkdirSync, readFileSync, existsSync, writeFileSync, renameSync, openSync, fsyncSync, closeSync, chmodSync } from "node:fs";
import { resolve } from "node:path";
import { providers, type Provider, eventId, permitted, validBinding, makePairing, matchPairing, type Pairing } from "./core";

const root = process.env.AI_PHONE_PROJECT_ROOT || resolve(import.meta.dir, "../..");
const folder = root + "/data/channels";
mkdirSync(folder, { recursive: true, mode: 0o700 }); chmodSync(folder, 0o700);
const token = readFileSync(root + "/data/assistant/token", "utf8").trim();
const keyPath = folder + "/vault.key";
if (!existsSync(keyPath)) writeFileSync(keyPath, randomBytes(32), { mode: 0o600, flag: "wx" });
const key = readFileSync(keyPath);
type Config = { appId: string; secret: string; allowed: string[]; version: string; pairing?: Pairing;offset?:number };
type Job = { agentId: string; deviceId?:string; provider: Provider; version: string; id: string; text: string; target: any; runId?: string; stage: DeliveryStage; at: number; outbound?:boolean; pushId?:string; png?:string; fingerprint?:string; textSent?:boolean; imageSent?:boolean; screenshot?:boolean; screenshotOnly?:boolean; error?:string };
type Vault = { configs: Record<string, Config>; jobs: Job[] };
let vault: Vault = { configs: {}, jobs: [] };
if (existsSync(folder + "/vault.bin")) {
  const data = readFileSync(folder + "/vault.bin");
  const decipher = createDecipheriv("aes-256-gcm", key, data.subarray(0,12)); decipher.setAuthTag(data.subarray(12,28));
  vault = JSON.parse(Buffer.concat([decipher.update(data.subarray(28)), decipher.final()]).toString());
}
migrateAgentScopes(vault);
async function registry(){
 const response=await fetch("http://127.0.0.1:17863/v1/registry/state",{method:"POST",headers:{authorization:"Bearer "+token,"content-type":"application/x-protobuf"},body:toBinary(RegistryRequestSchema,create(RegistryRequestSchema)),signal:AbortSignal.timeout(10000)});
 if(!response.ok)throw new Error("registry_unavailable");
 return fromBinary(RegistryStateSchema,new Uint8Array(await response.arrayBuffer()));
}
async function scopeAgent(id:string){const state=await registry();const selected=id||state.devices.find(d=>d.id===state.activeDeviceId)?.defaultAgentId||"";if(!state.agents.some(a=>a.id===selected))throw new Error("agent_not_found");return selected;}
async function routeDevice(agentId:string){const device=managedDevice(agentId,await registry());if(!device)throw new RpcError(409,"device_unavailable");return device;}
for (const job of vault.jobs) if (job.stage === "sending") job.stage = "unknown";
function save() {
  const nonce = randomBytes(12), cipher = createCipheriv("aes-256-gcm", key, nonce);
  const data = Buffer.concat([cipher.update(JSON.stringify(vault)), cipher.final()]);
  const temp = folder + "/vault.tmp"; writeFileSync(temp, Buffer.concat([nonce,cipher.getAuthTag(),data]), {mode:0o600});
  const fd = openSync(temp,"r"); fsyncSync(fd); closeSync(fd); renameSync(temp, folder+"/vault.bin");
}
save();
// Use the private host profile; empty means direct networking.
const proxy = process.env.AI_PHONE_HOST_PROXY || "", agent = proxy ? new HttpsProxyAgent(proxy) : undefined;
if(agent) https.globalAgent = agent as any;
const nativeFetch = globalThis.fetch;
globalThis.fetch = ((url: any, options: any = {}) => {
  const target = new URL(typeof url === "string" ? url : url instanceof URL ? url.href : url.url);
  return nativeFetch(url, { ...options, proxy: target.hostname === "127.0.0.1" ? null : (proxy || null) });
}) as typeof fetch;
const network = axios.create({ httpsAgent: agent, proxy:false, timeout:15000 });
const logger = { trace(){}, debug(){}, info(){}, warn(){}, error(){} };
type Connection = { state: string; error: string; stop(): void; send(job: Job, text: string): Promise<unknown>; sendImage(job:Job, bytes:Buffer):Promise<unknown>; live?(): string };
const connections = new Map<string,Connection>();
const labels = {wecom:"企业微信",feishu:"飞书",qq:"QQ",telegram:"Telegram"};
function enqueue(agentId:string, provider: Provider, version: string, sender: string, sourceId: string, text: string, target: any, bot=false, direct=false) {
  const config = vault.configs[connectionKey(agentId,provider)];
  if(config && config.version===version && !bot && sender && config.allowed.length===0 && matchPairing(config.pairing,text,direct)) {
    config.allowed=[sender];delete config.pairing;save();
    const receipt={agentId,provider,version,id:eventId(provider,config.appId,sourceId),text:"",target,stage:"delivered" as const,at:Date.now()};
    void connections.get(connectionKey(agentId,provider))?.send(receipt,"已配对，可以发送手机操作指令。").catch(()=>{});return;
  }
  if(config?.pairing && /^(配对|\/pair)\b|^配对\s/.test(text.trim())){save();return;}
  if (!config || config.version !== version || !permitted(config.allowed,sender,bot) || !sourceId || sourceId.length>512 || !text.trim() || text.length>24000) return;
  const id = eventId(provider,config.appId,sourceId);
  if (vault.jobs.length>=10000 || vault.jobs.some(j=>j.agentId===agentId&&j.id===id) || vault.jobs.filter(j=>["queued","running"].includes(j.stage)).length>=200) return;
  vault.jobs.push({agentId,provider,version,id,text:`[${labels[provider]} · ${sender}]\n${text}`,target,stage:"queued",at:Date.now(),screenshot:wantsScreenshot(text),screenshotOnly:isScreenshotRequest(text)});
  save();
}
function start(agentId:string,provider: Provider) {
  connections.get(connectionKey(agentId,provider))?.stop(); connections.delete(connectionKey(agentId,provider));
  const config = vault.configs[connectionKey(agentId,provider)]; if (!config) return;
  const status: Connection = {state:"connecting",error:"",stop(){},async send(){throw new Error("unavailable");},async sendImage(){throw new Error("unavailable");}};
  connections.set(connectionKey(agentId,provider),status);
  const ready=()=>{status.state="connected";status.error="";};
  const failed=()=>{status.state="failed";status.error="连接失败，请检查凭据、平台权限和网络";};
  if(provider==='telegram') {
    const client=startTelegram({appId:config.appId,token:config.secret,offset:config.offset||0,onMessage:(sender,id,text,target)=>enqueue(agentId,provider,config.version,sender,id,text,target,false,true),checkpoint:offset=>{config.offset=offset;save();}});
    connections.set(connectionKey(agentId,provider),client);
  } else if (provider==="wecom") {
    const client=new WecomClient({botId:config.appId,secret:config.secret,wsOptions:{agent},logger,maxReconnectAttempts:-1});
    client.on("authenticated",ready);client.on("disconnected",()=>{status.state="reconnecting";});client.on("reconnecting",()=>{status.state="reconnecting";});client.on("error",failed);
    client.on("message.text",frame=>{const body=frame.body;if(body)enqueue(agentId,provider,config.version,body.from.userid,body.msgid,body.text.content,{chat:body.chatid||body.from.userid},false,body.chattype==="single");});
    status.stop=()=>client.disconnect();
    status.send=(job,text)=>client.sendMessage(job.target.chat,{msgtype:"markdown",markdown:{content:text}});
    status.sendImage=async(job,bytes)=>{const media=await client.uploadMedia(bytes,{type:"image",filename:"phone.png"});return client.sendMediaMessage(job.target.chat,"image",media.media_id);};
    client.connect();
  } else if(provider==="feishu") {
    const base={appId:config.appId,appSecret:config.secret,httpInstance:network as any,logger};
    const api=new Lark.Client(base);
    const ws=new Lark.WSClient({...base,agent,onReady:ready,onReconnected:ready,onReconnecting:()=>{status.state="reconnecting";},onError:failed});
    status.stop=()=>ws.close({force:true});
    status.live=()=>ws.getConnectionStatus().state;
    const sendLark=async(job:Job,msg_type:string,content:string,uuid:string)=>{
      const data={msg_type,content,uuid};
      const response=job.target.openId?await api.im.v1.message.create({params:{receive_id_type:"open_id"},data:{...data,receive_id:job.target.openId}}):await api.im.v1.message.reply({path:{message_id:job.target.message},data});
      if(response.code!==0)throw new Error("delivery_failed");
    };
    status.send=(job,text)=>sendLark(job,"text",JSON.stringify({text}),job.id);
    status.sendImage=async(job,bytes)=>{const image=await api.im.v1.image.create({data:{image_type:"message",image:bytes}});if(!image?.image_key)throw new Error("upload_failed");await sendLark(job,"image",JSON.stringify({image_key:image.image_key}),eventId("feishu",config.appId,job.id+":image"));};
    void ws.start({eventDispatcher:new Lark.EventDispatcher({}).register({"im.message.receive_v1":async data=>{
      if(data.message.message_type!=="text" || (data.message.chat_type==="group"&&!data.message.mentions?.length))return;
      let content;try{content=JSON.parse(data.message.content).text;}catch{return;}
      enqueue(agentId,provider,config.version,data.sender.sender_id?.open_id||"",data.message.message_id,content,{message:data.message.message_id},data.sender.sender_type!=="user",data.message.chat_type==="p2p");
    }})}).catch(failed);
  } else {
    const bot=new QQBot({appId:config.appId,appSecret:config.secret,logger});
    bot.on("ready",ready);bot.on("resumed",ready);bot.on("disconnected",()=>{status.state="reconnecting";});bot.on("error",failed);
    bot.on("message",(_ctx,message)=>{
      if(!["c2c","group"].includes(message.kind))return;
      enqueue(agentId,provider,config.version,message.senderId,message.messageId,message.content,message.replyTarget,message.senderIsBot,message.kind==="c2c");
    });
    status.stop=()=>bot.stop();status.send=(job,text)=>bot.sendText(job.target,text);status.sendImage=async(job,bytes)=>{const result=await bot.sendImage(job.target,{buffer:bytes});if(!result.message)throw new Error("image_not_sent");};
    void bot.start().catch(failed);
  }
}
function list(agentId:string) {
  return create(ChannelListResponseSchema,{bindings:providers.map(provider=>{
    const config=vault.configs[connectionKey(agentId,provider)], current=connections.get(connectionKey(agentId,provider));
    const unknown=vault.jobs.some(j=>j.agentId===agentId&&j.provider===provider&&j.stage==="unknown");
    return {provider,appId:config?.appId||"",paired:!!config?.allowed.length,pairingCommand:config?.pairing&&config.pairing.expires>Date.now()?`/pair ${config.pairing.code}`:"",pairingExpiresAtMs:BigInt(config?.pairing?.expires||0),configured:!!config,status:config?(current?.live?.()||current?.state||"connecting"):"unbound",error:unknown?"一条回复的送达结果未知，请在原应用中核对":current?.error||""};
  })});
}
class RpcError extends Error { constructor(public status:number,public code:string){super(code);} }
async function assistant(operation: string, schema: any, value: any) {
  const response=await fetch("http://127.0.0.1:17863/v1/"+operation,{method:"POST",headers:{authorization:"Bearer "+token,"content-type":"application/x-protobuf"},body:toBinary(schema,create(schema,value)),signal:AbortSignal.timeout(30000)});
  const data=new Uint8Array(await response.arrayBuffer());
  if(!response.ok){const error=fromBinary(ApiErrorSchema,data);throw new RpcError(response.status,error.code);}
  return fromBinary(ChatStateSchema,data);
}
async function screenshot(agentId:string,deviceId:string){
 const device=authorizedDevice(agentId,deviceId,await registry());
 if(!device)throw new Error('screenshot_device_unavailable');
 if(device.kind==='desktop'){
  const response=await fetch('http://127.0.0.1:17863/v1/desktop/observe',{method:'POST',headers:{authorization:'Bearer '+token,'content-type':'application/x-protobuf'},body:toBinary(DesktopRequestSchema,create(DesktopRequestSchema,{deviceId})),signal:AbortSignal.timeout(15000)});
  if(!response.ok)throw new Error('screen_unavailable');
  const observation=fromBinary(DesktopObservationSchema,new Uint8Array(await response.arrayBuffer()));
  if(observation.deviceId!==deviceId||!validPng(observation.png))throw new Error('screen_unavailable');
  return Buffer.from(observation.png).toString('base64');
 }
 if(device.id!=='phone-local-01')throw new Error('unsupported_phone');
 const phoneToken=readFileSync(root+"/data/phone-runner/token","utf8").trim();
 const response=await fetch("http://127.0.0.1:17862/v1/observe",{method:"POST",headers:{authorization:"Bearer "+phoneToken,"content-type":"application/x-protobuf"},body:toBinary(DeviceRequestSchema,create(DeviceRequestSchema,{phoneId:"phone-local-01",ownerId:"local-user"})),signal:AbortSignal.timeout(12000)});
 if(!response.ok)throw new Error("screen_unavailable");
 const observation=fromBinary(DeviceObservationSchema,new Uint8Array(await response.arrayBuffer()));
 if(observation.phoneId!==deviceId||!validPng(observation.png))throw new Error("screen_unavailable");
 return Buffer.from(observation.png).toString("base64");
}
function push(agentId:string,input:ReturnType<typeof create<typeof ChannelPushRequestSchema>>){
 if(!/^[0-9a-f-]{36}$/i.test(input.requestId)||input.text.length>12000||(!input.text.trim()&&!input.png.length)||(input.png.length&&!validPng(input.png)))throw new Error("invalid_push");
 const selected=input.providers.length?[...new Set(input.providers)]:providers;
 if(selected.some(p=>!providers.includes(p as Provider)))throw new Error("invalid_provider");
 const fingerprint=payloadHash(input.text,input.png);
 const deliveries=selected.map(raw=>{
  const provider=raw as Provider,config=vault.configs[connectionKey(agentId,provider)];
  if(!config)return {provider,status:"unbound",error:"尚未绑定"};
  const target=ownerTarget(provider,config.allowed);if(!target)return {provider,status:"unpaired",error:"尚未配对主人"};
  const id=eventId(provider,config.appId,input.requestId),old=vault.jobs.find(j=>j.agentId===agentId&&j.id===id);
  if(old){if(old.fingerprint!==fingerprint)throw new Error("request_conflict");return {provider,status:old.stage,error:old.error||""};}
  if(vault.jobs.length>=10000||vault.jobs.filter(j=>["queued","running"].includes(j.stage)).length>=200)return {provider,status:"failed",error:"发送队列已满"};
  vault.jobs.push({agentId,provider,version:config.version,id,text:input.text,target,stage:"queued",at:Date.now(),outbound:true,pushId:input.requestId,fingerprint,png:input.png.length?Buffer.from(input.png).toString("base64"):undefined});
  return {provider,status:"queued",error:""};
 });
 save();return create(ChannelPushResultSchema,{deliveries});
}
let pumping=false,lastJob="";
async function pump() {
 if(pumping)return;pumping=true;
 let job:Job|undefined;
 try {
  const pending=vault.jobs.filter(j=>["queued","running"].includes(j.stage));
  // Rotate fairly: a long model run must not block notifications or later user instructions.
  job=nextPending(pending,lastJob);if(!job)return;lastJob=job.id;
  const config=vault.configs[connectionKey(job.agentId,job.provider)],connection=connections.get(connectionKey(job.agentId,job.provider));
  if(!config||config.version!==job.version){job.stage="canceled";delete job.png;save();return;}
  if(Date.now()-job.at>60*60*1000&&!job.runId){job.stage="failed";job.error="连接等待超时，未发送";delete job.png;save();return;}
  if((connection?.live?.()||connection?.state)!=="connected")return;
  let text=job.text;
  if(!job.outbound){
   if(job.screenshotOnly){job.deviceId ||= await routeDevice(job.agentId);try{job.png=await screenshot(job.agentId,job.deviceId);text="当前设备截图 · "+new Date().toISOString();}catch{text="当前设备截图读取失败，请检查设备状态与Agent授权。";}}
   else {
    if(!job.runId){
     job.deviceId ||= await routeDevice(job.agentId);
     const chat=await assistant("send",ChatSendRequestSchema,{text:job.text,requestId:job.id,agentId:job.agentId,deviceId:job.deviceId});job.runId=chat.runId;
     if(vault.configs[connectionKey(job.agentId,job.provider)]?.version!==job.version){job.stage="canceled";save();await assistant("cancel",ChatRunRequestSchema,{runId:job.runId}).catch(()=>{});return;}
     job.stage="running";save();
    }
    const run=await assistant("run",ChatRunRequestSchema,{runId:job.runId});
    if(["queued","running"].includes(run.runStatus))return;
    text=run.runStatus==="succeeded"?(run.messages[0]?.text||"任务已结束"):(run.error||run.messages[0]?.text||"任务已停止");
    if(job.screenshot||run.runStatus==="paused")try{job.png=await screenshot(job.agentId,job.deviceId!);text+="\n附当前设备截图 · "+new Date().toISOString();}catch{text+="\n当前截图读取失败。";}
   }
  }
  if(vault.configs[connectionKey(job.agentId,job.provider)]?.version!==job.version){job.stage="canceled";save();return;}
  const current=job;
  await deliverOnce(current,"AI Phone · "+text.slice(0,5900),value=>connection!.send(current,value),bytes=>connection!.sendImage(current,bytes),save);
 }catch(error){
  if(job&&error instanceof RpcError&&![409,503,500].includes(error.status)){job.stage="failed";job.error="指令被拒绝，未执行";save();}
 }finally{pumping=false;}
}
for(const scope of Object.keys(vault.configs)){const [agentId,provider]=scope.split("/");if(providers.includes(provider as Provider))start(agentId,provider as Provider);}
setInterval(()=>void pump(),1000);
function reply(schema:any,value:any,status=200){return new Response(toBinary(schema,value),{status,headers:{"content-type":"application/x-protobuf","cache-control":"no-store"}});}
function error(status:number,code:string,message:string){return reply(ApiErrorSchema,create(ApiErrorSchema,{code,message}),status);}
const server=Bun.serve({hostname:"127.0.0.1",port:17864,maxRequestBodySize:5*1024*1024,async fetch(request){
  const supplied=Buffer.from(request.headers.get("authorization")||""),expected=Buffer.from("Bearer "+token);
  if(supplied.length!==expected.length||!timingSafeEqual(supplied,expected))return error(401,"unauthorized","请先登录");
  if(request.method!=="POST"||request.headers.get("content-type")!=="application/x-protobuf")return error(415,"unsupported_media_type","请求格式错误");
  const operation=new URL(request.url).pathname;
  try{
    const bytes=new Uint8Array(await request.arrayBuffer());
    if(operation==="/v1/push-status"){
      const input=fromBinary(ChannelPushStatusRequestSchema,bytes),agentId=await scopeAgent(input.agentId);
      if(!/^[0-9a-f-]{36}$/i.test(input.requestId))return error(400,"invalid_id","推送标识无效");
      return reply(ChannelPushResultSchema,create(ChannelPushResultSchema,{deliveries:providers.map(provider=>{const job=vault.jobs.find(j=>j.agentId===agentId&&j.provider===provider&&j.pushId===input.requestId);return {provider,status:job?.stage||(!vault.configs[connectionKey(agentId,provider)]?"unbound":"not_found"),error:job?.error||""};})}));
    }
    if(operation==="/v1/push"){const input=fromBinary(ChannelPushRequestSchema,bytes);return reply(ChannelPushResultSchema,push(await scopeAgent(input.agentId),input));}
    if(bytes.length>65536)return error(413,"too_large","请求过大");
    if(operation==="/v1/list"){const input=fromBinary(ChannelListRequestSchema,bytes);return reply(ChannelListResponseSchema,list(await scopeAgent(input.agentId)));}
    if(operation==="/v1/bind"){
      const input=fromBinary(ChannelBindRequestSchema,bytes),agentId=await scopeAgent(input.agentId);if(!providers.includes(input.provider as Provider))return error(400,"invalid_provider","应用不存在");
      const provider=input.provider as Provider,old=vault.configs[connectionKey(agentId,provider)];
      const config:Config={appId:input.appId.trim(),secret:input.secret||(old?.secret||""),allowed:old?.appId===input.appId.trim()?old.allowed:[],version:crypto.randomUUID()};
      if(provider==='telegram'){
        const id=telegramIdentity(config.secret);if(!id)return error(400,'invalid_binding','请填写有效的 Telegram Bot Token');
        config.appId=id;config.allowed=old?.appId===id?old.allowed:[];config.offset=old?.appId===id?old.offset:0;
      }
      if(Object.entries(vault.configs).some(([id,c])=>id!==connectionKey(agentId,provider)&&id.endsWith("/"+provider)&&c.appId===config.appId))return error(409,"bot_in_use","此应用已绑定其他 Agent");
      if(!validBinding(config.appId,config.secret))return error(400,"invalid_binding","请填写应用ID和密钥");
      if(!config.allowed.length)config.pairing=makePairing();
      vault.configs[connectionKey(agentId,provider)]=config;
      for(const job of vault.jobs.filter(j=>j.agentId===agentId&&j.provider===provider&&["queued","running"].includes(j.stage))){job.stage="canceled";if(job.runId)await assistant("cancel",ChatRunRequestSchema,{runId:job.runId}).catch(()=>{});}
      save();start(agentId,provider);return reply(ChannelListResponseSchema,list(agentId));
    }
    if(operation==="/v1/pair"){
      const input=fromBinary(ChannelPairRequestSchema,bytes),agentId=await scopeAgent(input.agentId),provider=input.provider as Provider,config=vault.configs[connectionKey(agentId,provider)];
      if(!providers.includes(provider)||!config)return error(400,"not_configured","请先绑定应用");
      config.allowed=[];config.pairing=makePairing();config.version=crypto.randomUUID();
      for(const job of vault.jobs.filter(j=>j.agentId===agentId&&j.provider===provider&&["queued","running"].includes(j.stage))){job.stage="canceled";if(job.runId)await assistant("cancel",ChatRunRequestSchema,{runId:job.runId}).catch(()=>{});}
      save();start(agentId,provider);return reply(ChannelListResponseSchema,list(agentId));
    }
    if(operation==="/v1/unbind"){
      const input=fromBinary(ChannelUnbindRequestSchema,bytes),agentId=await scopeAgent(input.agentId),provider=input.provider as Provider;if(!providers.includes(provider))return error(400,"invalid_provider","应用不存在");
      connections.get(connectionKey(agentId,provider))?.stop();connections.delete(connectionKey(agentId,provider));delete vault.configs[connectionKey(agentId,provider)];
      for(const job of vault.jobs.filter(j=>j.agentId===agentId&&j.provider===provider&&["queued","running"].includes(j.stage))){job.stage="canceled";if(job.runId)await assistant("cancel",ChatRunRequestSchema,{runId:job.runId}).catch(()=>{});}
      save();return reply(ChannelListResponseSchema,list(agentId));
    }
    return error(404,"not_found","操作不存在");
  }catch{return error(400,"invalid_request","无法读取配置，请重试");}
}});
console.log("AI Phone channels listening on 127.0.0.1:"+server.port);
for(const signal of ["SIGTERM","SIGINT"] as const)process.on(signal,()=>{for(const connection of connections.values())connection.stop();save();server.stop();process.exit(0);});
