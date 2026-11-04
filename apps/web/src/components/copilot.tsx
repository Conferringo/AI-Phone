'use client';
import {createContext,useContext,useEffect,useLayoutEffect,useRef,useState,useSyncExternalStore,type ReactNode} from 'react';
import {LoaderCircle,Mic,MicOff} from 'lucide-react';
import {CopilotSession,type CopilotBinding} from '@/lib/copilot-session';
import {useWorkbenchNotice} from './workbench-notices';
import {useI18n} from './locale-provider';
import './copilot.css';

const Context=createContext<CopilotSession|null>(null);
export function CopilotProvider({currentDeviceId,onConfigure,children}:{currentDeviceId:string;onConfigure:(agentId:string)=>void;children:ReactNode}){
  const configure=useRef(onConfigure);configure.current=onConfigure;
  const [controller]=useState(()=>new CopilotSession(id=>configure.current(id)));
  const state=useSyncExternalStore(controller.subscribe,controller.snapshot,controller.snapshot);
  useLayoutEffect(()=>controller.select(currentDeviceId),[controller,currentDeviceId]);
  useEffect(()=>()=>controller.stop(),[controller]);
  useWorkbenchNotice(state.error,()=>{void controller.retry();});
  return <Context.Provider value={controller}>{children}</Context.Provider>;
}
export function useCopilot(){const value=useContext(Context);if(!value)throw Error('CopilotProvider is required');return value;}
export function useCopilotBinding(binding:CopilotBinding){
  const controller=useCopilot();
  useLayoutEffect(()=>{controller.register(binding);},[controller,binding]);
  useEffect(()=>()=>controller.unregister(binding.deviceId),[controller,binding.deviceId]);
  const state=useSyncExternalStore(controller.subscribe,controller.snapshot,controller.snapshot);
  return state.drafts[binding.deviceId];
}
export function CopilotToggle({deviceId,ready,onActivate}:{deviceId:string;ready:boolean;onActivate:()=>void}){
  const controller=useCopilot(),state=useSyncExternalStore(controller.subscribe,controller.snapshot,controller.snapshot),{t}=useI18n();
  const active=(state.enabled||state.opening)&&state.currentDeviceId===deviceId;
  return <button className="copilot-toggle" type="button" aria-pressed={active} disabled={!ready&&!active} title={t(active?'关闭 Copilot':'开启 Copilot')} onClick={()=>{
    onActivate();
    if(active)controller.stop();else if(state.enabled)controller.select(deviceId);else void controller.start(deviceId);
  }}>{state.opening&&active?<LoaderCircle size={15} className="spinning"/>:active?<Mic size={15}/>:<MicOff size={15}/>}<span>Copilot</span></button>;
}
