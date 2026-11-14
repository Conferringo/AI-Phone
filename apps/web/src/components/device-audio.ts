"use client";
import {useEffect,useRef,useState} from 'react';
import {AudioPlayback,playbackVolume} from '@/lib/audio-playback';

function reconnectDelay(signal:AbortSignal){
 if(signal.aborted)return Promise.resolve();
 return new Promise<void>(resolve=>{
  function finish(){clearTimeout(timer);signal.removeEventListener('abort',finish);resolve();}
  const timer=setTimeout(finish,1000);
  signal.addEventListener('abort',finish,{once:true});
 });
}

function savedVolume(deviceId:string){
 try{const stored=localStorage.getItem(`ai-phone:volume:${deviceId}`);return stored===null?1:playbackVolume(Number(stored));}
 catch{return 1;}
}

function usePlaybackVolume(deviceId:string,output:AudioPlayback){
 const [volume,setVolumeState]=useState(1),level=useRef(1),audibleLevel=useRef(1);
 useEffect(()=>{
  const next=savedVolume(deviceId);level.current=next;audibleLevel.current=next||1;setVolumeState(next);output.setVolume(next);
 },[deviceId,output]);
 const setVolume=(value:number)=>{
  const next=playbackVolume(value);
  try{localStorage.setItem(`ai-phone:volume:${deviceId}`,String(next));}catch{}
  level.current=next;if(next>0)audibleLevel.current=next;setVolumeState(next);output.setVolume(next);
 };
 return {volume,setVolume,level,audibleLevel};
}

async function readAudio(endpoint:string,signal:AbortSignal,output:AudioPlayback,onFrame:()=>void){
 const response=await fetch(endpoint,{signal,cache:'no-store'});
 if(response.status===401)throw Error('请重新登录');
 if(!response.ok||!response.body)throw Error('声音连接失败');
 if(signal.aborted)return;
 const reader=response.body.getReader();output.reset();
 try{
  while(!signal.aborted){
   const part=await reader.read();if(signal.aborted)return;
   if(part.done)throw Error('声音连接已断开');
   if(output.push(part.value))onFrame();
  }
 }finally{void reader.cancel().catch(()=>{});}
}

async function streamAudio(endpoint:string,signal:AbortSignal,output:AudioPlayback,onFrame:()=>void,onFailure:(message:string,terminal:boolean)=>void){
 while(!signal.aborted){
  try{await readAudio(endpoint,signal,output,onFrame);}
  catch(issue){
   if(signal.aborted)return;
   const message=issue instanceof Error?issue.message:'声音连接失败';
   const terminal=message==='请重新登录';
   output.reset();onFailure(message,terminal);
   if(terminal)return;
  }
  if(!signal.aborted)await reconnectDelay(signal);
 }
}

/** Owned by the device workspace, independently of its visible controls. */
export function useDeviceAudio(deviceId:string,endpoint:string){
 const [enabled,setEnabled]=useState(false),[error,setError]=useState(''),[busy,setBusy]=useState(false);
 const [running,setRunning]=useState(false);
 const [player]=useState(()=>new AudioPlayback(undefined,setRunning));
 const starting=useRef(false);
 const {volume,setVolume,level,audibleLevel}=usePlaybackVolume(deviceId,player);
 useEffect(()=>{if(!enabled)player.suspend();},[enabled,player]);
 useEffect(()=>{
  if(!enabled||!running)return;
  const abort=new AbortController();
  void streamAudio(endpoint,abort.signal,player,()=>setError(''),(message,terminal)=>{setError(message);if(terminal)setEnabled(false);});
  // Switching endpoints clears old samples, but must not suspend the shared graph.
  return()=>{abort.abort();player.reset();};
 },[enabled,running,endpoint,player]);
 useEffect(()=>()=>{player.close();},[player]);
 const toggle=async()=>{
  if(starting.current)return;
  if(enabled&&running&&level.current>0){setEnabled(false);setError('');return;}
  if(level.current===0)setVolume(audibleLevel.current);
  starting.current=true;setBusy(true);
  try{await player.resume();setError('');setEnabled(true);}
  catch{setError('声音未开启，请再次点击');}
  finally{starting.current=false;setBusy(false);}
 };
 return {enabled:enabled&&running,volume,error:error||(enabled&&!running?'声音播放已暂停，请重新开启':''),busy,toggle,setVolume};
}
export type DeviceAudioControls=Pick<ReturnType<typeof useDeviceAudio>,'enabled'|'volume'|'busy'|'toggle'|'setVolume'>;
