"use client";
import {useI18n} from '@/components/locale-provider';
import {useEffect,useState,useId} from "react";
import {create,fromBinary,toBinary,type MessageShape} from "@bufbuild/protobuf";
import {ChannelListRequestSchema,ChannelListResponseSchema,ChannelBindRequestSchema,ChannelUnbindRequestSchema,ChannelPairRequestSchema} from "@ai-phone/protocol/ai_phone/v1/channels_pb";
import {ApiErrorSchema} from "@ai-phone/protocol/ai_phone/v1/system_pb";
import {Button} from "@/components/ui/button";

const names:Record<string,string>={wecom:"企业微信",feishu:"飞书",qq:"QQ",telegram:"Telegram"};
const docs:Record<string,string>={wecom:"https://github.com/WecomTeam/aibot-node-sdk",feishu:"https://open.feishu.cn/document/server-docs/event-subscription-guide/event-receive/receive-events-through-persistent-connection",qq:"https://github.com/tencent-connect/qqbot-nodejs",telegram:"https://core.telegram.org/bots/tutorial#obtain-your-bot-token"};
const statuses:Record<string,string>={unbound:"未绑定",connecting:"连接中",connected:"已连接",reconnecting:"重连中",failed:"连接失败",idle:"待连接"};
type Binding=MessageShape<typeof ChannelListResponseSchema>["bindings"][number];
async function rpc(operation:string,schema:any,value:any){
 const response=await fetch("/api/channels/"+operation,{method:"POST",headers:{"content-type":"application/x-protobuf"},body:toBinary(schema,create(schema,value)),cache:"no-store"});
 const bytes=new Uint8Array(await response.arrayBuffer());
 if(response.status===401)location.assign("/login");
 if(!response.ok)throw new Error(fromBinary(ApiErrorSchema,bytes).message);
 return fromBinary(ChannelListResponseSchema,bytes).bindings;
}
export function AgentConnections({open,agentId}:{open:boolean;agentId:string}){
 const {t}=useI18n();
 const formId=useId();

 const [provider,setProvider]=useState("wecom"),[bindings,setBindings]=useState<Binding[]>([]),[drafts,setDrafts]=useState<Record<string,{appId:string;secret:string}>>({}),[error,setError]=useState(""),[loadError,setLoadError]=useState(""),[busy,setBusy]=useState(false),[loaded,setLoaded]=useState(false);
 const binding=bindings.find(b=>b.provider===provider);
 const secret=drafts[provider]?.secret??"";
 const appId=provider==="telegram"?(secret.split(":")[0]||binding?.appId||""):(drafts[provider]?.appId??binding?.appId??"");
 function setAppId(value:string){setDrafts(v=>({...v,[provider]:{appId:value,secret}}));}
 function setSecret(value:string){setDrafts(v=>({...v,[provider]:{appId,secret:value}}));}
 useEffect(()=>{if(!open||busy)return;let live=true;const load=async()=>{try{const data=await rpc("list",ChannelListRequestSchema,{agentId});if(live){setBindings(data);setLoaded(true);setLoadError("");}}catch(e){if(live)setLoadError(e instanceof Error?e.message:t("连接失败"));}};void load();const timer=setInterval(()=>void load(),2500);return()=>{live=false;clearInterval(timer);};},[open,agentId,busy]);
 return <section className="agent-connections">
 <div className="channel-tabs" aria-label={t("应用")}>{Object.entries(names).map(([id,name])=><Button key={id} type="button" variant={id===provider?"secondary":"ghost"} aria-pressed={provider===id} disabled={busy} onClick={()=>{setProvider(id);setError("");}}>{t(name)}</Button>)}</div>
 <form className="channel-form" onSubmit={async e=>{e.preventDefault();if(busy||!loaded)return;setBusy(true);setError("");try{setBindings(await rpc("bind",ChannelBindRequestSchema,{provider,appId,secret,agentId}));setSecret("");}catch(e){setError(e instanceof Error?e.message:t("绑定失败"));}finally{setBusy(false);}}}>
 <div className="channel-status" role="status">{loaded?t(statuses[binding?.status||"unbound"]||"连接中"):t("读取中…")}<a href={docs[provider]} target="_blank" rel="noreferrer">{t("接入文档")}</a></div>
 {provider!=="telegram"&&<><label htmlFor={`${formId}-app-id`}>{provider==="wecom"?"Bot ID":"App ID"}</label><input id={`${formId}-app-id`} value={appId} onChange={e=>setAppId(e.target.value)} autoComplete="off" spellCheck={false} required disabled={busy||!loaded}/></>}
 <label htmlFor={`${formId}-secret`}>{provider==="telegram"?"Bot Token":provider==="wecom"?"Secret":"App Secret"}</label><input id={`${formId}-secret`} type="password" value={secret} onChange={e=>setSecret(e.target.value)} autoComplete="new-password" placeholder={binding?.configured?t("已保存，留空保留"):""} required={!binding?.configured} disabled={busy||!loaded}/>
 {binding?.configured&&<div className="pair-account"><span>{binding.paired?t("已配对"):t("配对账号")}</span>{!binding.paired&&binding.pairingCommand&&<><code>{binding.pairingCommand}</code><Button type="button" variant="outline" onClick={async()=>{try{if(navigator.clipboard?.writeText){await navigator.clipboard.writeText(binding.pairingCommand);}else{const input=document.createElement("textarea");input.value=binding.pairingCommand;document.body.append(input);input.select();document.execCommand("copy");input.remove();}}catch{setError(t("复制失败，请选择上方配对指令"));}}}>{t("复制配对指令")}</Button></>}<Button type="button" variant="ghost" disabled={busy} onClick={async()=>{setBusy(true);try{setBindings(await rpc("pair",ChannelPairRequestSchema,{provider,agentId}));}catch(e){setError(e instanceof Error?e.message:t("配对失败"));}finally{setBusy(false);}}}>{binding.paired?t("更换账号"):binding.pairingCommand?t("更新配对码"):t("生成配对码")}</Button></div>}
 {(error||loadError||binding?.error)&&<p role="alert">{error||loadError||binding?.error}</p>}
 <div className="channel-buttons">{binding?.configured&&<Button type="button" variant="outline" disabled={busy} onClick={async()=>{setBusy(true);setError("");try{setBindings(await rpc("unbind",ChannelUnbindRequestSchema,{provider,agentId}));}catch(e){setError(e instanceof Error?e.message:t("解绑失败"));}finally{setBusy(false);}}}>{t("解绑")}</Button>}<Button type="submit" disabled={busy||!loaded||!appId.trim()||(!secret.trim()&&!binding?.configured)}>{busy?t("保存中…"):binding?.configured?t("保存并重连"):t("绑定")}</Button></div>
 </form></section>;
}
