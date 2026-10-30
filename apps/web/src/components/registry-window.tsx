'use client';
import {SelectField} from "@/components/ui/select";
import {clientId} from '@/lib/client-id';
import {useI18n} from '@/components/locale-provider';
import {useCallback,useEffect,useLayoutEffect,useRef,useState} from 'react';
import {create,type MessageShape} from '@bufbuild/protobuf';
import {RegistryRequestSchema,RegistryStateSchema,DeviceSaveRequestSchema,AgentConfigurationSaveRequestSchema,ConnectionTestRequestSchema,ConnectionCredentialRequestSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {ModelDraftSchema,ModelCredentialSchema,ModelTestResultSchema} from '@ai-phone/protocol/ai_phone/v1/settings_pb';
import {Check,ChevronDown,Eye,EyeOff,LoaderCircle,Monitor,Plus,RefreshCw,Smartphone,Sparkles,X} from 'lucide-react';
import {FloatingWindow} from './floating-window';
import {PhoneInstall} from './phone-install';
import {DesktopInstall} from './desktop-install';
import {AgentMemory} from './memory-window';
import {AgentConnections} from './connections-dialog';
import {Button} from './ui/button';
import {settingsRpc} from '@/lib/settings-client';
import {presetDraft,modelPreset,modelPresets,modelIssue,setupProgress,hasDeviceFrame,type ModelPreset,type SetupStep} from '@/lib/registry-setup';
import {SetupSteps,SetupReview} from './registry-onboarding';
import {DeviceRequestSchema,DeviceObservationSchema} from '@ai-phone/protocol/ai_phone/v1/phone_runtime_pb';
import {DesktopRequestSchema,DesktopObservationSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import './registry-window.css';
export type Registry=MessageShape<typeof RegistryStateSchema>;
type Config=MessageShape<typeof ModelDraftSchema>;
type Tab='devices'|'agents';
type Section='model'|'connections'|'memory';
type Form={id:string;revision:bigint;name:string;kind:string;defaultAgentId:string;modelId:string;modelRevision:bigint;deviceIds:string[];config:Config};
const fresh=()=>presetDraft('deepseek');
function record(state:Registry,tab:Tab,id:string):Form {
 const base:Form={id,revision:0n,name:'',kind:'phone',defaultAgentId:'',modelId:'',modelRevision:0n,deviceIds:[],config:fresh()};
 if(tab==='devices'){const d=state.devices.find(d=>d.id===id);return d?{...base,revision:d.revision,name:d.name,kind:d.kind,defaultAgentId:d.defaultAgentId}:base;}
 const a=state.agents.find(a=>a.id===id),m=state.models.find(m=>m.id===a?.modelConnectionId),p=m?.profile;
 return a?{...base,revision:a.revision,name:a.name,modelId:a.modelConnectionId,modelRevision:m?.revision??0n,deviceIds:a.deviceIds,config:create(ModelDraftSchema,{provider:p?.provider||'deepseek',protocol:p?.protocol||'chat-completions',baseUrl:p?.baseUrl||'https://api.deepseek.com',model:p?.model||'deepseek-flash',proxyUrl:p?.proxyUrl||'',useSavedKey:!!p?.hasApiKey})}:base;
}
const formKey=(tab:Tab,id:string)=>`${tab}:${id}`;
const deviceIcon=(kind:string)=>kind==='phone'?<Smartphone size={18}/>:<Monitor size={18}/>;
export function RegistryWindow({open,onClose,onNeedsSetup,onChange,activeDeviceId}:{open:boolean;onClose:()=>void;onNeedsSetup:()=>void;onChange:(state:Registry)=>void;activeDeviceId:string}) {
 const {t}=useI18n();

 const [state,setState]=useState<Registry|null>(null),[tab,setTab]=useState<Tab>('devices'),[section,setSection]=useState<Section>('model');
 const [selection,setSelection]=useState({devices:'',agents:''}),[drafts,setDrafts]=useState<Record<string,Form>>({}),[keys,setKeys]=useState<Record<string,string>>({});
 const [busy,setBusy]=useState(''),[error,setError]=useState(''),[notice,setNotice]=useState(''),[keyVisible,setKeyVisible]=useState(true);
 const [report,setReport]=useState<MessageShape<typeof ModelTestResultSchema>|null>(null);
 const [onboarding,setOnboarding]=useState(false),[step,setStep]=useState<SetupStep>('agents');
 const initialized=useRef(false),body=useRef<HTMLDivElement>(null);
 const pending=useRef(false),generation=useRef(0),mounted=useRef(true),change=useRef(onChange),setup=useRef(onNeedsSetup);change.current=onChange;setup.current=onNeedsSetup;
 useEffect(()=>{mounted.current=true;return()=>{mounted.current=false;generation.current++;};},[]);
 const accept=useCallback((value:Registry)=>{setState(value);change.current(value);},[]);
 const load=useCallback(async()=>{const g=++generation.current;try{const next=await settingsRpc('state',RegistryRequestSchema,{},RegistryStateSchema,'registry');if(!mounted.current||g!==generation.current)return;accept(next);setError('');setSelection(old=>({devices:old.devices||next.activeDeviceId,agents:old.agents||next.agents[0]?.id||''}));if(!initialized.current){initialized.current=true;const progress=setupProgress(next);if(!progress.ready){const nextStep=progress.modelReady?'devices':'agents';setOnboarding(true);setStep(nextStep);setTab(nextStep);setup.current();}}}catch(e){if(mounted.current&&g===generation.current)setError(e instanceof Error?e.message:t("读取失败"));}},[accept]);
 useEffect(()=>{void load();},[load]);
 useEffect(()=>{setSelection(old=>({...old,devices:activeDeviceId}));setTab('devices');setSection('model');},[activeDeviceId]);
 useEffect(()=>{if(open&&!pending.current)void load();},[open,load]);
 useLayoutEffect(()=>{const panel=body.current;if(!open||!panel)return;const preference=matchMedia('(prefers-reduced-motion:reduce)');if(preference.matches)return;const animation=panel.animate([{opacity:.35,transform:'translateY(8px)'},{opacity:1,transform:'none'}],{duration:240,easing:'cubic-bezier(.2,.8,.2,1)'});const finish=()=>animation.finish();preference.addEventListener('change',finish);return()=>{preference.removeEventListener('change',finish);animation.cancel();};},[open,tab,section,selection,step]);
 const id=selection[tab],key=formKey(tab,id),saved=state?record(state,tab,id):null,form=drafts[key]??saved,dirty=!!drafts[key],isNew=form?.revision===0n;
 const model=state?.models.find(m=>m.id===form?.modelId),credentialKey=model?`${model.id}:${model.revision}`:'';
 useEffect(()=>{if(!open||tab!=='agents'||!model?.profile?.hasApiKey||keys[credentialKey]!==undefined)return;let live=true;void settingsRpc('credential',ConnectionCredentialRequestSchema,{id:model.id,expectedRevision:model.revision},ModelCredentialSchema,'registry').then(v=>{if(live)setKeys(old=>({...old,[credentialKey]:v.apiKey}));}).catch(e=>{if(live)setError(e instanceof Error?e.message:t("读取密钥失败"));});return()=>{live=false;};},[open,tab,model,credentialKey,keys]);
 function edit(patch:Partial<Form>){if(!form||pending.current)return;setDrafts(old=>({...old,[key]:{...form,...patch}}));setReport(null);setError('');setNotice('');}
 function editConfig(patch:Partial<Config>){if(form)edit({config:{...form.config,...patch}});}
 function choose(next:Tab,nextId?:string){if(pending.current)return;setTab(next);if(onboarding)setStep(next);setSection('model');if(nextId)setSelection(old=>({...old,[next]:nextId}));setError('');setNotice('');setReport(null);}
 function add(){if(!state||pending.current)return;const next=clientId();setDrafts(old=>({...old,[formKey(tab,next)]:{...record(state,tab,next),kind:state.devices.filter(d=>d.kind==='phone').length>=(state.phoneLimit||1)?'desktop':'phone',modelId:clientId()}}));setSelection(old=>({...old,[tab]:next}));setSection('model');setError('');setNotice('');setReport(null);}
 function discard(){setDrafts(old=>{const next={...old};delete next[key];return next;});if(isNew&&state)setSelection(old=>({...old,[tab]:state[tab][0]?.id||''}));setReport(null);setError('');setNotice('');}
 const configChanged=tab==='agents'&&!!form&&(!saved||isNew||form.modelId!==saved.modelId||JSON.stringify(form.config)!==JSON.stringify(saved.config)||!model?.profile);
 const shared=state?.agents.filter(a=>a.modelConnectionId===form?.modelId&&a.id!==id).length??0;
 function goStep(next:SetupStep,nextId?:string){if(pending.current)return;setStep(next);if(next!=='review')choose(next,nextId);setError('');setNotice('');}
 async function finishSetup(){if(!state||pending.current)return;pending.current=true;setBusy('check');setError('');try{const next=await settingsRpc('state',RegistryRequestSchema,{},RegistryStateSchema,'registry');accept(next);const progress=setupProgress(next);if(!progress.ready)throw Error(t('请先完成 Agent 和设备配置'));for(const id of progress.deviceIds){const device=next.devices.find(d=>d.id===id)!;try{const frame=device.kind==='phone'?await settingsRpc('observe',DeviceRequestSchema,{phoneId:id,ownerId:'local-user',includeElements:true},DeviceObservationSchema,'phone'):await settingsRpc('observe',DesktopRequestSchema,{deviceId:id},DesktopObservationSchema,'desktop');if(!hasDeviceFrame(id,frame))throw Error(t('未收到设备画面'));}catch(e){throw Error(`${device.name} · ${e instanceof Error?e.message:t('连接失败')}`);}}if(mounted.current){setOnboarding(false);onClose();}}catch(e){if(mounted.current)setError(e instanceof Error?e.message:t('检查失败'));}finally{pending.current=false;if(mounted.current)setBusy('');}}
 async function save(testOnly=false){if(!form||pending.current)return;const issue=tab==='agents'?modelIssue(form.config):'';if(issue){setError(t(issue));return;}pending.current=true;setBusy('save');setError('');setNotice('');setReport(null);const target=key,value={...form},category=tab;
  try{let next:Registry;
   if(category==='devices')next=await settingsRpc('device-save',DeviceSaveRequestSchema,{id:value.id,expectedRevision:value.revision,name:value.name,kind:value.kind,defaultAgentId:value.defaultAgentId},RegistryStateSchema,'registry');
   else{let verificationId='';if(configChanged||testOnly){setBusy('test');const checked=await settingsRpc('model-test',ConnectionTestRequestSchema,{id:value.modelId,expectedRevision:value.modelRevision,draft:value.config},ModelTestResultSchema,'registry');if(mounted.current)setReport(checked);if(!checked.ready){setError(t("连接检测失败"));return;}verificationId=checked.verificationId;if(testOnly){setNotice(t("连接正常"));return;}}
    setBusy('save');next=await settingsRpc('agent-config-save',AgentConfigurationSaveRequestSchema,{agent:{id:value.id,expectedRevision:value.revision,name:value.name,modelConnectionId:value.modelId,deviceIds:value.deviceIds},connection:configChanged?{id:value.modelId,expectedRevision:value.modelRevision,name:value.name,draft:value.config,verificationId}:undefined},RegistryStateSchema,'registry');}
   if(!mounted.current)return;generation.current++;accept(next);setDrafts(old=>{const rest={...old};delete rest[target];return rest;});setNotice(t("已保存"));if(onboarding){setReport(null);const nextStep=category==='agents'?'devices':setupProgress(next).ready?'review':'devices';setStep(nextStep);if(nextStep!=='review')setTab(nextStep);}
  }catch(e){if(mounted.current)setError(e instanceof Error?e.message:t("保存失败"));}finally{pending.current=false;if(mounted.current)setBusy('');}}
 const rows=state?[...state[tab].map(v=>({id:v.id,name:v.name})),...Object.entries(drafts).filter(([k,v])=>k.startsWith(`${tab}:`)&&v.revision===0n&&!state[tab].some(x=>x.id===v.id)).map(([,v])=>({id:v.id,name:v.name||t("未命名")}))]:[];
 const device=state?.devices.find(d=>d.id===id);
 return <FloatingWindow fitContent={onboarding} id="configuration-center" title={t("配置中心")} open={open} onClose={onClose} width={780} height={650}>
  <div className="registry-center" aria-busy={!!busy}>
   {onboarding&&state&&<SetupSteps state={state} step={step} busy={!!busy} onStep={goStep}/>}
   <nav className="registry-tabs" hidden={onboarding} aria-label={t("配置分类")}><button hidden={onboarding} type="button" aria-pressed={tab==='devices'} disabled={!!busy} onClick={()=>choose('devices')}><Monitor size={16}/>{t("设备")}</button><button hidden={onboarding} type="button" aria-pressed={tab==='agents'} disabled={!!busy} onClick={()=>choose('agents')}><Sparkles size={16}/>Agent</button><button type="button" className="registry-reload" aria-label={t("重新载入配置")} title={t("重新载入配置")} disabled={!!busy} onClick={()=>void load()}><RefreshCw size={15}/></button></nav>
   {!state?<div className="registry-loading" role="status">{error||t("正在读取配置…")}</div>:<div className="registry-content" ref={body}>{onboarding&&step==='review'&&<SetupReview state={state} busy={!!busy} error={error} onStep={goStep} onFinish={()=>void finishSetup()}/>}<div className="registry-layout" data-single-setup={onboarding&&tab==='agents'&&rows.length===1} hidden={onboarding&&step==='review'}>
    <nav className="registry-list" aria-label={tab==='devices'?t("设备列表"):t("Agent 列表")}><div className="registry-entities">{rows.map(row=>{const d=state.devices.find(d=>d.id===row.id),a=state.agents.find(a=>a.id===row.id),m=state.models.find(m=>m.id===a?.modelConnectionId);return <button key={row.id} type="button" aria-pressed={id===row.id} disabled={!!busy} onClick={()=>choose(tab,row.id)}>{tab==='devices'?deviceIcon(d?.kind||drafts[formKey(tab,row.id)]?.kind||'phone'):<Sparkles size={18}/>}<span>{row.name}<small>{tab==='devices'?(d?.runtimeState==='managed'?t("已接入"):t("待安装")):(m?.profile?.model||t("未配置"))}</small></span>{drafts[formKey(tab,row.id)]&&<i aria-label={t("未保存")}/>}</button>;})}</div><button type="button" className="registry-add" disabled={!!busy||rows.length>=(tab==='devices'?(state.phoneLimit||1)+(state.desktopLimit||1):64)} onClick={add}><Plus size={16}/>{tab==='devices'?t("添加设备"):t("添加 Agent")}</button></nav>
    {form&&<div className="registry-detail">
     {tab==='agents'&&!onboarding&&<nav className="registry-sections" aria-label={t("Agent 配置")}>{([['model',t("模型与设备")],['connections',t("连接应用")],['memory',t("记忆")]] as const).map(([value,label])=><button type="button" key={value} aria-pressed={section===value} disabled={!!busy||(isNew&&value!=='model')} onClick={()=>{setSection(value);setError('');}}>{label}</button>)}</nav>}
     {error&&<div className="registry-alert" role="alert">{error}</div>}
     <form className="registry-form" hidden={section!=='model'} onSubmit={e=>{e.preventDefault();void save();}}>
      <div className="registry-scroll"><fieldset disabled={!!busy}>
       <label>{t("名称")}<input aria-label={t("配置名称")} required maxLength={80} value={form.name} onChange={e=>edit({name:e.target.value})} placeholder={tab==='devices'?t("设备名称"):t("Agent 名称")}/></label>
       {tab==='devices'?<>
        {isNew?<label>{t("系统")}<SelectField aria-label={t("设备类型")} value={form.kind} onValueChange={value=>edit({kind:value})} options={[{value:"phone",label:t("Android 手机"),disabled:state.devices.filter(d=>d.kind==='phone').length>=(state.phoneLimit||1)},{value:"desktop",label:t("Linux 电脑"),disabled:state.devices.some(d=>d.kind==='desktop')}]}/> </label>:<div className="registry-system">{deviceIcon(form.kind)}<span>{device?.system.replace(' · 待安装','')}</span></div>}
        <label>{t("使用的 Agent")}<SelectField aria-label={t("默认 Agent")} value={form.defaultAgentId} onValueChange={value=>edit({defaultAgentId:value})} options={[{value:"",label:t("未分配")},...(state.agents.map(a=>({value:a.id,label:a.name})))]}/> </label>
        {form.kind==='desktop'&&!isNew&&<DesktopInstall key={id} deviceId={id} installed={device?.runtimeState==='managed'} onInstalled={()=>void load()}/>}
        {isNew&&form.kind==='desktop'&&<p className="registry-note">{t("保存设备后安装系统")}</p>}
        {form.kind==='phone'&&!isNew&&<PhoneInstall key={id} deviceId={id} installed={device?.runtimeState==='managed'} onInstalled={()=>void load()}/>}
        {isNew&&form.kind==='phone'&&<p className="registry-note">{t("保存设备后安装系统")}</p>}
       </>:<>
        <div className="registry-model-row"><label>{t("服务商")}<SelectField aria-label={t("服务商")} value={modelPreset(form.config)} onValueChange={value=>editConfig(presetDraft(value as ModelPreset))} options={modelPresets.map(p=>({value:p.id,label:p.id==='custom'?t("兼容服务"):p.label}))}/> </label><label>{t("模型")}<input aria-label={t("模型 ID")} required placeholder={t("填写模型 ID")} value={form.config.model} spellCheck={false} onChange={e=>editConfig({model:e.target.value})}/></label></div>
        <label>{t("API 地址")}<input aria-label={t("API 地址")} required type="url" value={form.config.baseUrl} spellCheck={false} autoCapitalize="none" onChange={e=>editConfig({baseUrl:e.target.value,useSavedKey:false,apiKey:''})}/></label>
        <label>API Key<span className="registry-key"><input aria-label="API Key" type={keyVisible?'text':'password'} autoComplete="off" spellCheck={false} autoCapitalize="none" readOnly={form.config.useSavedKey&&keys[credentialKey]===undefined} value={form.config.useSavedKey?keys[credentialKey]??'':form.config.apiKey} onChange={e=>editConfig({apiKey:e.target.value,useSavedKey:false})} placeholder={form.config.useSavedKey?t("正在读取…"):t("填写 API Key")}/><button type="button" aria-label={keyVisible?t("隐藏密钥"):t("显示密钥")} onClick={()=>setKeyVisible(v=>!v)}>{keyVisible?<EyeOff size={16}/>:<Eye size={16}/>}</button></span></label>
        {!!shared&&<div className="registry-shared">{t("此连接还被")} {shared} {t("个 Agent 使用")}<Button type="button" variant="outline" disabled={form.config.useSavedKey&&keys[credentialKey]===undefined} onClick={()=>edit({modelId:clientId(),modelRevision:0n,config:{...form.config,useSavedKey:false,apiKey:form.config.useSavedKey?keys[credentialKey]:form.config.apiKey}})}>{t("复制为独立连接")}</Button></div>}
        <label>{t("API 协议")}<SelectField aria-label={t("API 协议")} disabled={form.config.useSavedKey&&keys[credentialKey]===undefined} value={String(form.config.protocol)} onValueChange={value=>editConfig({protocol:value,useSavedKey:false,apiKey:form.config.useSavedKey?keys[credentialKey]||'':form.config.apiKey})} options={[{value:"responses",label:"Responses"},{value:"chat-completions",label:"Chat Completions"},{value:"anthropic-messages",label:"Anthropic Messages"}]}/> </label><details className="registry-advanced"><summary>{t("高级")}<ChevronDown size={13}/></summary><label>{t("代理地址")}<input aria-label={t("代理地址")} type="url" value={form.config.proxyUrl} placeholder={t("直连")} onChange={e=>editConfig({proxyUrl:e.target.value})}/></label></details>
        <div className="registry-bindings"><h3>{t("可用设备")}</h3>{state.devices.map(d=><label key={d.id}>{deviceIcon(d.kind)}<span>{d.name}</span><input aria-label={t("授权 {0}",{"0":d.name})} type="checkbox" checked={form.deviceIds.includes(d.id)} onChange={e=>edit({deviceIds:e.target.checked?[...form.deviceIds,d.id]:form.deviceIds.filter(id=>id!==d.id)})}/></label>)}</div>
       </>}
      </fieldset>{report&&<details className="registry-report" open={!report.ready}><summary>{report.ready?t("连接检测通过"):t("连接检测失败")}</summary>{report.checks.map(c=><p key={c.name}>{c.passed?'✓':'×'} {c.name} · {c.detail}</p>)}</details>}</div>
      <footer className="registry-save"><Button type="submit" disabled={!!busy||!dirty&&!configChanged}>{busy&&<LoaderCircle className="spinning" size={15}/>} {busy==='test'?t("验证连接…"):busy?t("保存中…"):t("保存")}</Button>{dirty&&<Button type="button" variant="ghost" disabled={!!busy} onClick={discard}>{t("撤销")}</Button>}{tab==='agents'&&!dirty&&!!model?.profile&&<Button type="button" variant="ghost" disabled={!!busy} onClick={()=>void save(true)}>{t("检测连接")}</Button>}{onboarding&&<Button type="button" variant="outline" disabled={!!busy||dirty} onClick={()=>goStep(tab==='agents'?'devices':'review')}>{t("下一步")}</Button>}{notice&&<span role="status"><Check size={14}/>{notice}</span>}</footer>
     </form>
     {!onboarding&&state.agents.map(agent=><div className="registry-scoped" key={agent.id} hidden={tab!=='agents'||id!==agent.id||section==='model'}><div hidden={section!=='connections'}><AgentConnections agentId={agent.id} open={open&&tab==='agents'&&id===agent.id&&section==='connections'}/></div><div hidden={section!=='memory'}><AgentMemory agentId={agent.id} open={open&&tab==='agents'&&id===agent.id&&section==='memory'}/></div></div>)}
    </div>}
   </div></div>}
  </div>
 </FloatingWindow>;
}
