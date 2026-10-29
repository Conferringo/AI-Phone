'use client';
import {SelectField} from "@/components/ui/select";
import {useI18n} from "./locale-provider";
import {useEffect,useRef,useState} from 'react';
import type {MessageShape} from '@bufbuild/protobuf';
import {DesktopRequestSchema,DesktopExploreStateSchema,DesktopExploreUpdateSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {settingsRpc} from '@/lib/settings-client';
import {Compass,Pause,LoaderCircle} from 'lucide-react';
import {FloatingWindow} from './floating-window';
import {ExplorationAura} from './proactive-controls';
type State=MessageShape<typeof DesktopExploreStateSchema>;
export function useDesktopExploration(deviceId:string,enabled:boolean){
 const [state,setState]=useState<State|null>(null),[error,setError]=useState(''),[saving,setSaving]=useState(false),pending=useRef(false);
 useEffect(()=>{if(!enabled)return;let live=true;let timer:ReturnType<typeof setTimeout>;const refresh=async()=>{if(!document.hidden&&!pending.current)try{const value=await settingsRpc('explore-state',DesktopRequestSchema,{deviceId},DesktopExploreStateSchema,'desktop');if(live&&!pending.current){setState(value);setError('');}}catch(e){if(live)setError(e instanceof Error?e.message:'探索服务暂不可用');}if(live)timer=setTimeout(refresh,2500);};void refresh();return()=>{live=false;clearTimeout(timer);};},[deviceId,enabled]);
 async function update(patch:{enabled?:boolean;intervalSeconds?:number}){if(!state||pending.current)return;pending.current=true;setSaving(true);try{setState(await settingsRpc('explore-update',DesktopExploreUpdateSchema,{deviceId,enabled:state.enabled,intervalSeconds:state.intervalSeconds,...patch},DesktopExploreStateSchema,'desktop'));setError('');}catch(e){setError(e instanceof Error?e.message:'保存失败');}finally{pending.current=false;setSaving(false);}}
 return {state,error,saving,update};
}
type Control=ReturnType<typeof useDesktopExploration>;
export function DesktopExploreToggle({control,focused}:{control:Control;focused:boolean}){const {t}=useI18n();const {state,error,saving,update}=control;return <><div className="proactive-dock" data-exploring={!!state?.enabled}><button className="exploration-toggle" aria-pressed={!!state?.enabled} disabled={!state||saving} onClick={()=>void update({enabled:!state?.enabled})}>{saving?<LoaderCircle size={16} className="spinning"/>:state?.enabled?<Pause size={15}/>:<Compass size={16}/>}<span>{state?.enabled?t("暂停探索"):t("自主探索")}</span></button></div>{error&&<span className="proactive-inline-error" role="alert">{t(error)}</span>}<ExplorationAura kind="desktop" active={focused&&!!state?.enabled} paused={!!error}/></>;}
export function DesktopExploreWindow({control,deviceId,open,onClose}:{control:Control;deviceId:string;open:boolean;onClose:()=>void}){const {t}=useI18n();const {state,error,saving,update}=control;return <FloatingWindow id={'desktop-explore-'+deviceId} title={t("自主探索")} open={open} onClose={onClose} width={360} height={260} fitContent><div className="proactive-options"><div className="proactive-status"><span className="proactive-indicator" data-live={!!state?.enabled}/>{state?.enabled?(state.runId?t("正在探索"):t("等待下一轮")):t("待机")}</div>{state&&<label><span>{t("每轮间隔")}</span><SelectField aria-label={t("探索间隔")} disabled={saving} value={String(state.intervalSeconds)} onValueChange={value=>void update({intervalSeconds:Number(value)})} options={[30,60,120,300,600].map(n=>({value:String(n),label:String(n<60?t("{0} 秒", {"0":n}):t("{0} 分钟", {"0":n/60}))}))}/> </label>}{(error||state?.lastError)&&<p role="alert" className="proactive-problem">{error||state?.lastError}</p>}</div></FloatingWindow>;}
