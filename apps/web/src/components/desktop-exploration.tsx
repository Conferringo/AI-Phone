'use client';
import {useEffect,useRef,useState} from 'react';
import type {MessageShape} from '@bufbuild/protobuf';
import {DesktopRequestSchema,DesktopExploreStateSchema,DesktopExploreUpdateSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {settingsRpc} from '@/lib/settings-client';
import {ExplorationAura,ExplorationControl} from './proactive-controls';
type State=MessageShape<typeof DesktopExploreStateSchema>;
export function useDesktopExploration(deviceId:string,enabled:boolean){
 const [state,setState]=useState<State|null>(null),[error,setError]=useState(''),[saving,setSaving]=useState(false),pending=useRef(false),generation=useRef(0);
 useEffect(()=>{if(!enabled)return;let live=true;let timer:ReturnType<typeof setTimeout>;const refresh=async()=>{const version=generation.current;if(!document.hidden&&!pending.current)try{const value=await settingsRpc('explore-state',DesktopRequestSchema,{deviceId},DesktopExploreStateSchema,'desktop');if(live&&!pending.current&&version===generation.current){setState(value);setError('');}}catch(e){if(live)setError(e instanceof Error?e.message:'探索服务暂不可用');}if(live)timer=setTimeout(refresh,2500);};void refresh();return()=>{live=false;clearTimeout(timer);};},[deviceId,enabled]);
 async function update(patch:{enabled:boolean}){if(!state||pending.current)return;pending.current=true;generation.current++;setSaving(true);try{setState(await settingsRpc('explore-update',DesktopExploreUpdateSchema,{deviceId,enabled:patch.enabled,intervalSeconds:state.intervalSeconds},DesktopExploreStateSchema,'desktop'));setError('');}catch(e){setError(e instanceof Error?e.message:'保存失败');}finally{pending.current=false;setSaving(false);}}
 return {state,error,saving,update};
}
type Control=ReturnType<typeof useDesktopExploration>;
export function DesktopExploreToggle({control,focused}:{control:Control;focused:boolean}){
 const {state,error,saving,update}=control;
 return <><ExplorationControl active={!!state?.enabled} ready={!!state} saving={saving} error={error||state?.lastError||''} status={state?.enabled?(state.runId?'正在探索':'等待下一轮'):'待机'} onToggle={()=>void update({enabled:!state?.enabled})}/><ExplorationAura kind="desktop" active={focused&&!!state?.enabled} paused={!!error}/></>;
}
