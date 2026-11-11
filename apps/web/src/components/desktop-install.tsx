'use client';
import {SelectField} from "@/components/ui/select";
import {clientId} from '@/lib/client-id';
import {useI18n} from '@/components/locale-provider';
import {useDeviceDefaults} from './device-defaults';
import {useEffect,useRef,useState} from 'react';
import {type MessageShape} from '@bufbuild/protobuf';
import {DesktopProvisionRequestSchema,DesktopProvisionJobSchema,DeviceLinkStateSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {RegistryRequestSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {settingsRpc} from '@/lib/settings-client';
import {Button} from './ui/button';
import {LoaderCircle} from 'lucide-react';
export function DesktopInstall({deviceId,installed,onInstalled}:{deviceId:string;installed:boolean;onInstalled:()=>void}){
 const {t}=useI18n();

 const defaults=useDeviceDefaults().desktop;
 const [cpus,setCpus]=useState(defaults.cpus),[memory,setMemory]=useState(defaults.memoryMb),[job,setJob]=useState<MessageShape<typeof DesktopProvisionJobSchema>|null>(null),[error,setError]=useState(''),[starting,setStarting]=useState(false);
 const complete=useRef(onInstalled);complete.current=onInstalled;const notified=useRef('');const requestId=useRef('');
 useEffect(()=>{let live=true;const check=async()=>{try{const state=await settingsRpc('state',RegistryRequestSchema,{},DeviceLinkStateSchema,'desktop');if(!live)return;setError('');const next=state.jobs.find(j=>j.deviceId===deviceId);if(next){setJob(next);if(next.status==='completed'&&notified.current!==next.id){notified.current=next.id;complete.current();}}}catch(e){if(live)setError(e instanceof Error?e.message:t("系统状态不可用"));}};void check();const timer=setInterval(check,1800);return()=>{live=false;clearInterval(timer);};},[deviceId]);
 const running=starting||job?.status==='queued'||job?.status==='running';
 async function install(){if(running)return;setStarting(true);setError('');if(!requestId.current)requestId.current=clientId();try{const next=await settingsRpc('provision',DesktopProvisionRequestSchema,{deviceId,requestId:requestId.current,cpus,memoryMb:memory,templateId:'debian13-openbox'},DesktopProvisionJobSchema,'desktop');setJob(next);if(next.status==='failed')requestId.current='';}catch(e){setError(e instanceof Error?e.message:t("安装失败"));}finally{setStarting(false);}}
 if(installed)return <div className="registry-install-ready">Debian 13 · Openbox{job&&<span>{job.cpus} {t("核")} · {job.memoryMb/1024} GB</span>}</div>;
 return <section className="registry-install"><label>{t("系统镜像")}<SelectField aria-label={t("Linux系统模板")} disabled value={t("Debian 13 · 轻量桌面")} onValueChange={()=>{}} options={[{value:String(t("Debian 13 · 轻量桌面")),label:t("Debian 13 · 轻量桌面")}]}/> </label><div className="registry-model-row"><label>{t("处理器")}<SelectField aria-label={t("Linux处理器")} value={String(cpus)} disabled={running} onValueChange={value=>setCpus(Number(value))} options={[1,2,4].map(v=>({value:String(v),label:String(v)+" "+t("核")}))}/> </label><label>{t("内存")}<SelectField aria-label={t("Linux内存")} value={String(memory)} disabled={running} onValueChange={value=>setMemory(Number(value))} options={[2048,2560,3072,4096].map(v=>({value:String(v),label:String(v/1024)+" GB"}))}/> </label></div><Button type="button" disabled={running} onClick={()=>{if(job?.status==='failed')requestId.current='';void install();}}>{running?<><LoaderCircle className="spinning" size={15}/>{job?.stage||t("开始安装…")}</>:job?.status==='failed'?t("重试安装"):t("安装系统")}</Button>{(error||job?.error)&&<p role="alert">{error||job?.error}</p>}</section>;
}
