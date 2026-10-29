'use client';
import {Check,ArrowRight,LoaderCircle,Monitor,Smartphone} from 'lucide-react';
import {useI18n,LanguageSelect} from './locale-provider';
import {Button} from './ui/button';
import {setupProgress,type SetupStep} from '@/lib/registry-setup';
import type {Registry} from './registry-window';

export function SetupSteps({state,step,busy,onStep}:{state:Registry;step:SetupStep;busy:boolean;onStep:(step:SetupStep)=>void}) {
 const {t}=useI18n(),progress=setupProgress(state);
 return <nav className="registry-steps" aria-label={t('初次配置')}>
  {(['agents','devices','review'] as const).map((value,i)=><button key={value} type="button" disabled={busy} aria-current={step===value?'step':undefined} onClick={()=>onStep(value)}><span>{(value==='agents'&&progress.modelReady||value==='devices'&&progress.ready)?<Check size={14}/>:i+1}</span>{t(value==='agents'?'配置 Agent':value==='devices'?'连接设备':'开始使用')}</button>)}
  <LanguageSelect/>
 </nav>;
}
export function SetupReview({state,busy,error,onStep,onFinish}:{state:Registry;busy:boolean;error:string;onStep:(step:SetupStep,deviceId?:string)=>void;onFinish:()=>void}) {
 const {t}=useI18n(),progress=setupProgress(state);
 return <section className="registry-review" aria-label={t('检查配置')}>
  <div className="registry-review-body"><h3>{t(progress.ready?'检查配置':'继续配置')}</h3>
   {state.devices.map(device=>{const agent=state.agents.find(a=>a.id===device.defaultAgentId),ready=progress.deviceIds.includes(device.id);return <button type="button" key={device.id} disabled={busy} onClick={()=>onStep('devices',device.id)}>
    {device.kind==='phone'?<Smartphone size={22}/>:<Monitor size={22}/>}<span><strong>{device.name}</strong><small>{ready?agent?.name:t('待配置')}</small></span>{ready&&<Check size={17}/>}
   </button>;})}
   {!progress.modelReady&&<Button variant="outline" onClick={()=>onStep('agents')}>{t('配置 Agent')}<ArrowRight size={14}/></Button>}
   {error&&<p role="alert" className="workbench-error">{error}</p>}
  </div>
  <footer className="registry-save"><Button disabled={busy||!progress.ready} onClick={onFinish}>{busy?<LoaderCircle className="spinning" size={15}/>:<ArrowRight size={15}/>} {t(busy?'检查设备…':'检查并进入')}</Button></footer>
 </section>;
}
