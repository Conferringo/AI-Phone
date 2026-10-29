"use client";
import {useEffect,useRef,useState} from 'react';
import {AudioPlayback,playbackVolume} from '@/lib/audio-playback';
export {applyPlaybackVolume,playbackVolume} from '@/lib/audio-playback';

export function usePhoneAudio(deviceId:string,endpoint='/api/phone/media/audio'){
 const [enabled,setEnabled]=useState(false),[volume,setVolumeState]=useState(1),[error,setError]=useState(''),[busy,setBusy]=useState(false);
 const player=useRef<AudioPlayback|null>(null),level=useRef(1),audibleLevel=useRef(1),starting=useRef(false);
 if(!player.current)player.current=new AudioPlayback();
 useEffect(()=>{
  try{const stored=localStorage.getItem(`ai-phone:volume:${deviceId}`);
   const next=stored===null?1:playbackVolume(Number(stored));
   level.current=next;audibleLevel.current=next||1;setVolumeState(next);player.current!.setVolume(next);
  }catch{/* Local playback still works when preferences are unavailable. */}
 },[deviceId]);
 const setVolume=(value:number)=>{
  const next=playbackVolume(value);
  try{localStorage.setItem(`ai-phone:volume:${deviceId}`,String(next));}catch{}
  level.current=next;if(next>0)audibleLevel.current=next;setVolumeState(next);player.current!.setVolume(next);
 };
 useEffect(()=>{
  const output=player.current!;
  if(!enabled){output.suspend();return;}
  const abort=new AbortController();let live=true;
  void(async()=>{
   while(live){
    let reader:ReadableStreamDefaultReader<Uint8Array>|undefined;
    try{
     const response=await fetch(endpoint,{signal:abort.signal,cache:'no-store'});
     if(response.status===401)throw Error('请重新登录');
     if(!response.ok||!response.body)throw Error('声音连接失败');
     if(!live)break;
     reader=response.body.getReader();output.reset();
     while(live){
      const part=await reader.read();if(!live)break;
      if(part.done)throw Error('声音连接已断开');
      output.push(part.value);setError('');
     }
    }catch(issue){
     if(live&&!abort.signal.aborted){
      const message=issue instanceof Error?issue.message:'声音连接失败';setError(message);output.reset();
      if(message==='请重新登录'||message==='声音播放已暂停，请重新开启'){setEnabled(false);break;}
     }
    }finally{void reader?.cancel().catch(()=>{});}
    if(live)await new Promise(resolve=>setTimeout(resolve,1000));
   }
  })();
  // Switching endpoints clears old samples, but must not suspend the shared graph.
  return()=>{live=false;abort.abort();output.reset();};
 },[enabled,endpoint]);
 useEffect(()=>()=>{player.current!.close();},[]);
 const toggle=async()=>{
  if(starting.current)return;
  if(enabled&&level.current>0){setEnabled(false);setError('');return;}
  if(level.current===0)setVolume(audibleLevel.current);
  starting.current=true;setBusy(true);
  try{await player.current!.resume();setError('');setEnabled(true);}
  catch{setEnabled(false);setError('声音未开启，请再次点击');}
  finally{starting.current=false;setBusy(false);}
 };
 return {enabled,volume,error,busy,toggle,setVolume};
}
