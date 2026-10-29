'use client';
import {SelectField} from "@/components/ui/select";
import {clientId} from '@/lib/client-id';
import {useI18n} from "./locale-provider";
import {useEffect,useRef,useState} from 'react';
import type {MessageShape} from '@bufbuild/protobuf';
import {RegistryRequestSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {DeviceLinkStateSchema,DeviceLinkRequestSchema,DeviceFilesRequestSchema,DeviceFilesSchema,TransferRequestSchema,DeviceTransferSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {settingsRpc} from '@/lib/settings-client';
import type {Registry} from './registry-window';
import {FloatingWindow} from './floating-window';
import {Button} from './ui/button';
import {ArrowLeft,ArrowRight,File,Folder,LoaderCircle,Plug,Unplug} from 'lucide-react';
type State=MessageShape<typeof DeviceLinkStateSchema>;
type Entry=MessageShape<typeof DeviceFilesSchema>['files'][number];
export function DeviceCable({devices,current,open,onClose,onLinks}:{devices:Registry['devices'];current:string;open:boolean;onClose:()=>void;onLinks:(ids:string[])=>void}){const {t}=useI18n();
 const linksCallback=useRef(onLinks);linksCallback.current=onLinks;
 const [state,setState]=useState<State|null>(null),[source,setSource]=useState(current),[phone,setPhone]=useState('phone-local-01'),[path,setPath]=useState(''),[entries,setEntries]=useState<Entry[]>([]),[selected,setSelected]=useState(''),[busy,setBusy]=useState(''),[error,setError]=useState(''),[loading,setLoading]=useState(false);
 const computer=devices.find(d=>d.kind==='desktop'&&d.runtimeState==='managed'),phones=devices.filter(d=>d.kind==='phone'&&d.runtimeState==='managed');
 const pair=[phone,computer?.id||''],target=source===phone?computer?.id:phone,connected=!!state?.links.find(l=>l.connected&&pair.includes(l.sourceDeviceId)&&pair.includes(l.targetDeviceId));
 async function refresh(){const value=await settingsRpc('state',RegistryRequestSchema,{},DeviceLinkStateSchema,'desktop');setState(value);linksCallback.current([...new Set(value.links.filter(l=>l.connected).flatMap(l=>[l.sourceDeviceId,l.targetDeviceId]))]);}
 useEffect(()=>{let live=true;const read=()=>{if(live)void refresh().catch(()=>{});};read();const timer=setInterval(read,4000);return()=>{live=false;clearInterval(timer);};},[]);
 useEffect(()=>{if(open){setSource(current);setPath('');setSelected('');setError('');if(devices.find(d=>d.id===current)?.kind==='phone')setPhone(current);}},[open,current]);
 useEffect(()=>{if(!open||!connected||!source)return;let live=true;setLoading(true);setEntries([]);setSelected('');void settingsRpc('files',DeviceFilesRequestSchema,{deviceId:source,path},DeviceFilesSchema,'desktop').then(v=>{if(live){setEntries(v.files);setLoading(false);}}).catch(e=>{if(live){setError(e.message);setLoading(false);}});return()=>{live=false;};},[open,connected,source,path,state?.transfers.length]);
 async function link(){if(!computer)return;setBusy('link');setError('');try{const value=await settingsRpc('link',DeviceLinkRequestSchema,{sourceDeviceId:phone,targetDeviceId:computer.id,connected:!connected},DeviceLinkStateSchema,'desktop');setState(value);await refresh();}catch(e){setError(e instanceof Error?e.message:'连接失败');}finally{setBusy('');}}
 async function transfer(){if(!target||!selected)return;setBusy('transfer');setError('');try{const value=await settingsRpc('transfer',TransferRequestSchema,{requestId:clientId(),sourceDeviceId:source,targetDeviceId:target,sourcePath:selected},DeviceTransferSchema,'desktop');if(value.status!=='completed')setError(value.error||'传输结果待核对');await refresh();}catch(e){setError(e instanceof Error?e.message:'传输失败');}finally{setBusy('');}}
 const name=(id:string)=>devices.find(d=>d.id===id)?.name||id;
 return <FloatingWindow id="device-cable" title={t("数据线")} open={open} onClose={onClose} width={470} height={550}><div className="device-cable-panel">
  <div className="cable-pair"><SelectField aria-label={t("连接手机")} value={phone} disabled={!!busy} onValueChange={value=>{setPhone(value);setSource(value);setPath('');}} options={phones.map(d=>({value:d.id,label:d.name}))}/> <span className="cable-line" data-connected={connected}/><span>{computer?.name||t("尚未安装电脑")}</span><Button variant="secondary" disabled={!computer||!phones.length||!!busy} onClick={()=>void link()}>{busy==='link'?<LoaderCircle size={16} className="spinning"/>:connected?<Unplug size={16}/>:<Plug size={16}/>}<span>{connected?t("断开"):t("连接")}</span></Button></div>
  {connected&&<><div className="cable-direction"><SelectField aria-label={t("传输来源")} value={source} onValueChange={value=>{setSource(value);setPath('');setSelected('');}} options={pair.map(id=>({value:String(id),label:name(id)}))}/> <ArrowRight size={16}/><span>{name(target||'')}</span></div><div className="cable-directory"><Button variant="ghost" size="icon" aria-label={t("上级目录")} disabled={!path||!!busy} onClick={()=>setPath(path.split('/').slice(0,-1).join('/')||'')}><ArrowLeft size={16}/></Button><span>{path||(source===phone?t("下载"):t("工作文件"))}</span></div><div className="cable-files">{entries.map(file=><button key={file.path} aria-pressed={selected===file.path} disabled={!!busy} onClick={()=>file.directory?setPath(file.path):setSelected(file.path)}>{file.directory?<Folder size={17}/>:<File size={17}/>}<span>{file.name}</span>{!file.directory&&<small>{Math.ceil(Number(file.bytes)/1024)} KB</small>}</button>)}{loading?<LoaderCircle className="spinning" size={18} aria-label={t("读取文件")}/>:!entries.length&&<p>{t("此目录为空")}</p>}</div><Button disabled={!selected||!!busy} onClick={()=>void transfer()}>{busy==='transfer'?<LoaderCircle size={16} className="spinning"/>:<ArrowRight size={16}/>}{t("传到 {name}",{name:name(target||'')})}</Button></>}
  {error&&<p role="alert" className="workbench-error">{t(error)}</p>}
  {!!state?.messages.length&&<div className="cable-history">{state.messages.filter(m=>pair.includes(m.sourceDeviceId)&&pair.includes(m.targetDeviceId)).slice(0,5).map(m=><details key={m.id}><summary><span>{name(m.sourceDeviceId)} → {name(m.targetDeviceId)}</span><span>{({queued:t("等待Agent"),running:t("处理中"),completed:t("已完成"),failed:t("需处理"),canceled:t("已取消")}as Record<string,string>)[m.status]}</span></summary><p>{m.text}</p>{m.error&&<p role="alert">{m.error}</p>}</details>)}</div>}
  {!!state?.transfers.length&&<div className="cable-history">{state.transfers.filter(t=>pair.includes(t.sourceDeviceId)&&pair.includes(t.targetDeviceId)).slice(0,8).map(transfer=><details key={transfer.id}><summary><span>{transfer.name}</span><span>{({completed:t("已送达"),pending:t("等待中"),copying:t("传输中"),failed:t("失败"),unknown:t("待核对")}as Record<string,string>)[transfer.status]||transfer.status}</span></summary><p>{name(transfer.sourceDeviceId)} → {name(transfer.targetDeviceId)}</p><p>{transfer.targetPath}</p>{transfer.error&&<p role="alert">{transfer.error}</p>}</details>)}</div>}
 </div></FloatingWindow>;
}
