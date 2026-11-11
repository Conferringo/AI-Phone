'use client';
import {LoaderCircle,Monitor,Smartphone} from 'lucide-react';
import {setupProgress,type SetupStep} from '@/lib/registry-setup';
import {useI18n} from './locale-provider';
import {Button} from './ui/button';
import {SelectField} from './ui/select';
import type {Registry} from './registry-window';

const steps=[['agents','模型'],['devices','设备'],['review','检查配置']] as const;
const deviceIcon=(kind:string)=>kind==='phone'?<Smartphone size={18}/>:<Monitor size={18}/>;

export function SetupSteps({step,busy,onStep}:{step:SetupStep;busy:boolean;onStep:(step:SetupStep)=>void}) {
 const {t}=useI18n(),index=steps.findIndex(([value])=>value===step);
 return <nav aria-label={t('初次配置')}><ol className="registry-steps">
  {steps.map(([value,label],i)=><li key={value} aria-current={step===value?'step':undefined}>
   {i<index?<button type="button" disabled={busy} onClick={()=>onStep(value)}>{t(label)}</button>:<span>{t(label)}</span>}
  </li>)}
 </ol></nav>;
}

export function SetupDevices({state,drafts,busy,error,onEdit,onNext}:{state:Registry;drafts:Record<string,string>;busy:boolean;error:string;onEdit:(id:string,value:string)=>void;onNext:()=>void}) {
 const {t}=useI18n(),{agentIds}=setupProgress(state);
 const assigned=(device:Registry['devices'][number])=>drafts[device.id]??device.defaultAgentId;
 const eligible=state.agents.filter(agent=>agentIds.includes(agent.id));
 const canContinue=state.devices.some(device=>device.runtimeState==='managed'&&agentIds.includes(assigned(device)));
 return <form className="registry-setup-stage" onSubmit={event=>{event.preventDefault();if(canContinue&&!busy)onNext();}}>
  <div className="registry-setup-scroll">
   <h3 className="registry-setup-heading" data-setup-heading tabIndex={-1}>{t('连接设备')}</h3>
   <ul className="registry-setup-rows">
    {state.devices.map(device=><li className="registry-setup-row" key={device.id}>
     <div className="registry-setup-identity"><span aria-hidden="true">{deviceIcon(device.kind)}</span>
      <span><strong>{device.name}</strong><small>{[device.system.replace(' · 待安装',''),device.runtimeState!=='managed'&&t('待安装')].filter(Boolean).join(' · ')}</small></span>
     </div>
     <SelectField aria-label={device.name+' · '+t('默认 Agent')} disabled={busy} value={agentIds.includes(assigned(device))?assigned(device):''}
      onValueChange={value=>onEdit(device.id,value)}
      options={[{value:'',label:t('未分配'),disabled:device.kind==='phone'},...eligible.map(agent=>({value:agent.id,label:agent.name}))]}/>
    </li>)}
   </ul>
   {error&&<p role="alert" className="registry-setup-error">{error}</p>}
  </div>
  <footer className="registry-save"><Button type="submit" disabled={busy||!canContinue}>
   {busy&&<LoaderCircle className="spinning" size={15}/>} {busy?t('保存中…'):t('下一步')}
  </Button></footer>
 </form>;
}

export function SetupReview({state,busy,error,onFinish}:{state:Registry;busy:boolean;error:string;onFinish:()=>void}) {
 const {t}=useI18n(),progress=setupProgress(state);
 return <section className="registry-setup-stage" aria-labelledby="registry-review-heading">
  <div className="registry-setup-scroll">
   <h3 id="registry-review-heading" className="registry-setup-heading" data-setup-heading tabIndex={-1}>{t('检查配置')}</h3>
   <ul className="registry-setup-rows">
    {state.devices.map(device=>{
     const agent=state.agents.find(a=>a.id===device.defaultAgentId);
     const model=state.models.find(m=>m.id===agent?.modelConnectionId)?.profile?.model;
     const ready=progress.deviceIds.includes(device.id);
     return <li className="registry-setup-row registry-setup-summary" key={device.id}>
      <span className="registry-setup-identity"><span aria-hidden="true">{deviceIcon(device.kind)}</span>
       <span><strong>{device.name}</strong><small>{[agent?.name,model].filter(Boolean).join(' · ')||device.system.replace(' · 待安装','')}</small></span>
      </span>
      {!ready&&<span className="registry-setup-status">{t(device.runtimeState==='managed'?'待配置':'待安装')}</span>}
     </li>;
    })}
   </ul>
   {error&&<p role="alert" className="registry-setup-error">{error}</p>}
  </div>
  <footer className="registry-save"><Button disabled={busy||!progress.ready} onClick={onFinish}>
   {busy&&<LoaderCircle className="spinning" size={15}/>} {t(busy?'检查设备…':'检查并进入')}
  </Button></footer>
 </section>;
}
