'use client';
import {SelectField} from "@/components/ui/select";
import {clientId} from '@/lib/client-id';
import {useI18n} from '@/components/locale-provider';
import {useDeviceDefaults} from './device-defaults';
import {useEffect,useRef,useState} from 'react';
import type {MessageShape} from '@bufbuild/protobuf';
import {PhoneProvisionRequestSchema,PhoneProvisionJobSchema,PhoneProvisionStateSchema} from '@ai-phone/protocol/ai_phone/v1/phone_runtime_pb';
import {settingsRpc} from '@/lib/settings-client';
import {Button} from './ui/button';
import {LoaderCircle} from 'lucide-react';
export function PhoneInstall({deviceId,installed,onInstalled}:{deviceId:string;installed:boolean;onInstalled:()=>void}){
 const {t}=useI18n();

 const defaults=useDeviceDefaults().phone;
 const [cpus,setCpus]=useState(defaults.cpus),[memory,setMemory]=useState(defaults.memoryMb),[storage,setStorage]=useState(defaults.storageGb),[identity,setIdentity]=useState('aiphone');
 const [job,setJob]=useState<MessageShape<typeof PhoneProvisionJobSchema>|null>(null),[error,setError]=useState(''),[starting,setStarting]=useState(false);
 const callback=useRef(onInstalled),notified=useRef(''),requestId=useRef('');callback.current=onInstalled;
 useEffect(()=>{let live=true;const check=async()=>{try{const state=await settingsRpc('state',PhoneProvisionRequestSchema,{},PhoneProvisionStateSchema,'phone-system');if(!live)return;setError('');const next=state.jobs.find(j=>j.deviceId===deviceId);if(next){setJob(next);if(next.status==='completed'&&notified.current!==next.id){notified.current=next.id;callback.current();}}}catch(e){if(live)setError(e instanceof Error?e.message:t("系统状态不可用"));}};void check();const timer=setInterval(check,2500);return()=>{live=false;clearInterval(timer);};},[deviceId]);
 const running=starting||job?.status==='running'||job?.status==='queued';
 async function install(){if(running)return;setStarting(true);setError('');if(!requestId.current||job?.status==='failed')requestId.current=clientId();try{setJob(await settingsRpc('provision',PhoneProvisionRequestSchema,{deviceId,requestId:requestId.current,templateId:'android35-arm64',cpus,memoryMb:memory,storageGb:storage,identityProfile:identity},PhoneProvisionJobSchema,'phone-system'));}catch(e){setError(e instanceof Error?e.message:t("安装失败"));}finally{setStarting(false);}}
 if(installed)return job?<dl className="device-resource-summary"><div><dt>{t("处理器")}</dt><dd>{job.cpus} {t("核")}</dd></div><div><dt>{t("内存")}</dt><dd>{job.memoryMb/1024} GB</dd></div><div><dt>{t("存储")}</dt><dd>{job.storageGb} GB</dd></div><div><dt>{t("型号")}</dt><dd>{job.identityProfile==='xiaomi8'?t("小米 8"):'Phone 2'}</dd></div></dl>:null;
 return <section className="registry-install"><label>{t("系统版本")}<SelectField aria-label={t("Android系统版本")} disabled value={"Android 15 · ARM64"} onValueChange={()=>{}} options={[{value:"Android 15 · ARM64",label:"Android 15 · ARM64"}]}/> </label><div className="registry-model-row"><label>{t("处理器")}<SelectField aria-label={t("手机处理器")} value={String(cpus)} disabled={running} onValueChange={value=>setCpus(Number(value))} options={[1,2,4].map(n=>({value:String(n),label:String(n)+" "+t("核")}))}/> </label><label>{t("内存")}<SelectField aria-label={t("手机内存")} value={String(memory)} disabled={running} onValueChange={value=>setMemory(Number(value))} options={[1536,2048,3072,4096].map(n=>({value:String(n),label:String(n/1024)+" GB"}))}/> </label></div><div className="registry-model-row"><label>{t("存储")}<SelectField aria-label={t("手机存储")} value={String(storage)} disabled={running} onValueChange={value=>setStorage(Number(value))} options={[16,24,32].map(n=>({value:String(n),label:String(n)}))}/> </label><label>{t("型号")}<SelectField aria-label={t("手机型号")} value={identity} disabled={running} onValueChange={value=>setIdentity(value)} options={[{value:"aiphone",label:"Phone 2"},{value:"xiaomi8",label:t("小米 8")}]}/> </label></div><Button type="button" disabled={running} onClick={()=>void install()}>{running?<><LoaderCircle className="spinning" size={15}/>{job?.stage||t("开始安装…")}</>:job?.status==='failed'?t("重试安装"):t("安装系统")}</Button>{(error||job?.error)&&<p role="alert">{error||job?.error}</p>}</section>;
}
