'use client';
import {useCallback,useEffect,useRef,useState} from 'react';
import {Cable,LoaderCircle} from 'lucide-react';
import {RegistryRequestSchema} from '@ai-phone/protocol/ai_phone/v1/registry_pb';
import {DeviceLinkRequestSchema,DeviceLinkStateSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {settingsRpc} from '@/lib/settings-client';
import {useI18n} from './locale-provider';
import {Button} from './ui/button';
import type {Registry} from './registry-window';

export function DeviceCableToggle({devices,linked,onLinks,onError}:{devices:Registry['devices'];linked:string[];onLinks:(ids:string[])=>void;onError:(message:string)=>void}){
 const {t}=useI18n(),[busy,setBusy]=useState(false),callbacks=useRef({onLinks,onError}),sequence=useRef(0),mutating=useRef(false);
 callbacks.current={onLinks,onError};
 const phone=devices.find(d=>d.kind==='phone'&&d.runtimeState==='managed'),computer=devices.find(d=>d.kind==='desktop'&&d.runtimeState==='managed');
 const connected=!!phone&&!!computer&&linked.includes(phone.id)&&linked.includes(computer.id);
 const read=useCallback(async()=>{
  if(mutating.current)return;const version=++sequence.current;
  const state=await settingsRpc('state',RegistryRequestSchema,{},DeviceLinkStateSchema,'desktop');
  if(version===sequence.current)callbacks.current.onLinks([...new Set(state.links.filter(l=>l.connected).flatMap(l=>[l.sourceDeviceId,l.targetDeviceId]))]);
 },[]);
 useEffect(()=>{
  const refresh=()=>{if(!document.hidden)void read().catch(error=>callbacks.current.onError(error instanceof Error?error.message:String(error)));};
  refresh();const timer=setInterval(refresh,4000);document.addEventListener('visibilitychange',refresh);
  return()=>{++sequence.current;clearInterval(timer);document.removeEventListener('visibilitychange',refresh);};
 },[read]);
 async function toggle(){
  if(!phone||!computer||mutating.current)return;
  mutating.current=true;++sequence.current;setBusy(true);callbacks.current.onError('');
  try{
   const state=await settingsRpc('link',DeviceLinkRequestSchema,{sourceDeviceId:phone.id,targetDeviceId:computer.id,connected:!connected},DeviceLinkStateSchema,'desktop');
   callbacks.current.onLinks([...new Set(state.links.filter(l=>l.connected).flatMap(l=>[l.sourceDeviceId,l.targetDeviceId]))]);
  }catch(error){callbacks.current.onError(error instanceof Error?error.message:String(error));}
  finally{mutating.current=false;setBusy(false);}
 }
 return <Button variant="ghost" size="icon" className="device-cable-toggle" data-connected={connected} aria-label={t(connected?'断开数据线':'连接数据线')} title={t(connected?'断开数据线':'连接数据线')} aria-pressed={connected} aria-busy={busy} disabled={!phone||!computer||busy} onClick={()=>void toggle()}>{busy?<LoaderCircle size={18} className="spinning"/>:<Cable size={18}/>}</Button>;
}
