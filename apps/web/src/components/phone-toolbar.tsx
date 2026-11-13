"use client";
import { LoadingOrb } from "@/components/ui/loading-orb";
import {useWorkbenchNotice} from './workbench-notices';
import {useI18n} from '@/components/locale-provider';
import {type CSSProperties, useEffect, useRef, useState} from 'react';
import{ScanLine,Volume2,VolumeX,Smartphone}from'lucide-react';
import{Button}from'./ui/button';import type{DeviceAudioControls}from'./device-audio';
import {bindSpringRange} from '../../../client/ui/range-motion.js';
export type RotationMode='automatic'|'portrait'|'landscape';
export type RotationPolicy={automatic:boolean;rotation:number};
export function PhoneToolbar({audio,rotate,capture,landscape,onInteract,kind="phone"}:{kind?:"phone"|"desktop";audio:DeviceAudioControls;rotate:(mode:RotationMode)=>Promise<void>;capture:()=>Promise<void>;landscape:boolean;onInteract:()=>void}){
 const {t}=useI18n();

 const [busy,setBusy]=useState(''),[error,setError]=useState('');
 useWorkbenchNotice(error);
 const pending=useRef(false);
 const volumeInput=useRef<HTMLInputElement>(null);
 const volumeThumb=useRef<HTMLSpanElement>(null);
 useEffect(() => {
   const input = volumeInput.current;
   const thumb = volumeThumb.current;
   if (input && thumb) return bindSpringRange(input, thumb);
 }, []);
 async function action(name:string,work:()=>Promise<void>){if(pending.current)return;pending.current=true;setBusy(name);setError('');onInteract();try{await work();}catch(issue){setError(issue instanceof Error?issue.message:t("操作失败"));}finally{pending.current=false;setBusy('');}}
 const volume=audio.enabled?Math.round(audio.volume*100):0;
 function changeVolume(value:number){onInteract();audio.setVolume(value/100);if(value>0&&!audio.enabled)void audio.toggle();}
 return <div className="phone-toolbar" role="group" aria-label={kind==="desktop"?t("电脑控制"):t("手机控制")}>
  <div className="phone-toolbar-volume">
   <Button variant="ghost" size="icon" aria-label={volume>0?t("静音"):t("开启声音")} title={volume>0?t("静音"):t("开启声音")} aria-pressed={volume>0} disabled={audio.busy} onClick={()=>{onInteract();void audio.toggle();}}>{audio.busy?<LoadingOrb size={18} />:volume>0?<Volume2 size={18}/>:<VolumeX size={18}/>}</Button>
   <div className="spring-range" style={{"--range-progress": `${volume}%`} as CSSProperties}>
    <input
     ref={volumeInput}
     type="range"
     aria-label={kind==="desktop"?t("电脑播放音量"):t("手机播放音量")}
     min="0"
     max="100"
     step="1"
     value={volume}
     disabled={audio.busy}
     onChange={event=>changeVolume(Number(event.target.value))}
    />
    <span className="spring-range__rail" aria-hidden="true">
     <span className="spring-range__thumb" ref={volumeThumb}/>
    </span>
   </div>
  </div>
  {kind==="phone"&&<Button variant="ghost" size="icon" aria-label={landscape?t("切换竖屏"):t("切换横屏")} title={landscape?t("切换竖屏"):t("切换横屏")} disabled={!!busy} onClick={()=>void action('rotate',()=>rotate(landscape?'portrait':'landscape'))}>{busy==='rotate'?<LoadingOrb size={18} />:<Smartphone size={18} className={landscape?'phone-rotation-landscape':''}/>}</Button>}
  <Button variant="ghost" size="icon" aria-label={t("截图")} title={t("截图")} disabled={!!busy} onClick={()=>void action('capture',capture)}>{busy==='capture'?<LoadingOrb size={18} />:<ScanLine size={18}/>}</Button>
 </div>;
}
