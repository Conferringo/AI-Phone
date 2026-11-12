'use client';
import { LoadingOrb } from "@/components/ui/loading-orb";
import {createContext,useCallback,useContext,useEffect,useLayoutEffect,useRef,useState,useSyncExternalStore,type ReactNode} from 'react';
import {Mic,MicOff} from 'lucide-react';
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
  useWorkbenchNotice(state.error,()=>{void controller.retry();},state.localLanguagePack?'安装本机语言包':undefined);
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
export function CopilotWave({deviceId}:{deviceId:string}) {
  const controller = useCopilot();
  const activity = useSyncExternalStore(controller.subscribe, useCallback(() => {
    const state = controller.snapshot();
    return state.enabled && state.currentDeviceId === deviceId ? state.activity : null;
  }, [controller, deviceId]), () => null);
  if (!activity) return null;

  return (
    <svg className="copilot-wave" data-activity={activity} aria-hidden="true" focusable="false" viewBox="0 0 1200 240" preserveAspectRatio="none">
      <g>
        <path d="M-80 120 C80 120 142 94 240 95 S386 162 493 161 S661 58 770 66 S948 167 1055 144 S1170 117 1280 120" />
        <path d="M-80 120 C48 120 120 151 244 147 S423 70 537 83 S693 174 809 166 S977 103 1070 107 S1190 122 1280 120" />
        <path d="M-80 120 C104 120 148 124 257 122 S427 170 534 149 S700 89 806 96 S961 140 1068 134 S1190 119 1280 120" />
      </g>
    </svg>
  );
}
export function CopilotToggle({deviceId,ready,onActivate}:{deviceId:string;ready:boolean;onActivate:()=>void}){
  const controller=useCopilot(),state=useSyncExternalStore(controller.subscribe,controller.snapshot,controller.snapshot),{t}=useI18n();
  const active=(state.enabled||state.opening)&&state.currentDeviceId===deviceId;
  return <button className="copilot-toggle" type="button" aria-pressed={active} disabled={!ready&&!active} title={t(active?'关闭 Copilot':'开启 Copilot')} onClick={()=>{
    onActivate();
    if(active)controller.stop();else if(state.enabled)controller.select(deviceId);else void controller.start(deviceId);
  }}>{state.opening&&active?<LoadingOrb size={15} />:active?<Mic size={15}/>:<MicOff size={15}/>}<span>Copilot</span></button>;
}
