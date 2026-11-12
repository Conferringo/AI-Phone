'use client';
import {useWorkbenchNotice} from './workbench-notices';
import {clientId} from '@/lib/client-id';
import {useLayoutEffect,useRef,useState} from 'react';
import type {MessageInitShape,MessageShape} from '@bufbuild/protobuf';
import {DesktopRequestSchema,DesktopObservationSchema,DesktopActionRequestSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {settingsRpc} from '@/lib/settings-client';
import {PhoneStreamImage} from './phone-stream-image';
import {DesktopInputQueue} from './desktop-input-queue';
type Frame=MessageShape<typeof DesktopObservationSchema>;
type Action=MessageInitShape<typeof DesktopActionRequestSchema>;
type Point={x:number;y:number};
type PendingClick=Point&{at:number;timer:ReturnType<typeof setTimeout>};
const DESKTOP_KEYS:Partial<Record<string,string>>={
 Enter:'Return',Backspace:'BackSpace',Delete:'Delete',Escape:'Escape',
 ArrowLeft:'Left',ArrowRight:'Right',ArrowUp:'Up',ArrowDown:'Down',
 Tab:'Tab',Home:'Home',End:'End',PageUp:'Prior',PageDown:'Next',
};
export function DesktopSurface({deviceId,active}:{deviceId:string;active:boolean}){
 const frame=useRef<Frame|null>(null),input=useRef<HTMLTextAreaElement>(null),pointer=useRef<Point|null>(null),composing=useRef(false),wheel=useRef(0);
 const click=useRef<PendingClick|null>(null);
 const activeRef=useRef(active),generation=useRef(0),receivedAt=useRef(0);
 const [queue]=useState(()=>new DesktopInputQueue());
 function takeClick(){
  const pending=click.current;
  if(pending)clearTimeout(pending.timer);
  click.current=null;
  return pending;
 }
 useLayoutEffect(()=>{
  generation.current++;activeRef.current=active;frame.current=null;receivedAt.current=0;pointer.current=null;composing.current=false;queue.barrier();
  return()=>{activeRef.current=false;generation.current++;queue.barrier();takeClick();};
 },[active,deviceId,queue]);
 const [error,setError]=useState(''),[online,setOnline]=useState(false);
 useWorkbenchNotice(active?error:'');
 const send=(value:Action)=>{
  if(!activeRef.current)return;const version=generation.current,current=()=>activeRef.current&&generation.current===version;
  void queue.enqueue(value,async input=>{
   if(!current())return;
   let previous=frame.current;
   if(!previous||performance.now()-receivedAt.current>35000){
    queue.barrier();
    previous=await settingsRpc('control',DesktopRequestSchema,{deviceId},DesktopObservationSchema,'desktop');
    if(!current())return;
    queue.barrier();
   }
   const next=await settingsRpc('action',DesktopActionRequestSchema,{...input,deviceId,actionId:clientId(),observationId:previous.observationId,epoch:previous.epoch},DesktopObservationSchema,'desktop');
   if(current()){
    if(frame.current?.epoch!==next.epoch)queue.barrier();
    frame.current=next;receivedAt.current=performance.now();setError('');
   }
  }).catch(e=>{if(current()){queue.barrier();frame.current=null;setError(e instanceof Error?e.message:'电脑操作失败');}});
 };
 function flushClick(){
  const pending=takeClick();
  if(pending)send({kind:'click',x:pending.x,y:pending.y,button:1,clicks:1});
 }
 function sendKeyboard(action:Action){
  flushClick();
  send(action);
 }
 const point=(e:React.PointerEvent<HTMLImageElement>|React.MouseEvent<HTMLImageElement>|React.WheelEvent<HTMLImageElement>)=>{const r=e.currentTarget.getBoundingClientRect();return{x:Math.max(0,Math.min(1279,Math.round((e.clientX-r.left)/r.width*1280))),y:Math.max(0,Math.min(799,Math.round((e.clientY-r.top)/r.height*800)))};};
 return <div className="desktop-screen" data-scene-device={deviceId}>
  <PhoneStreamImage endpoint={`/api/desktop/media?device_id=${encodeURIComponent(deviceId)}`} fallback="" enabled onConnectionState={setOnline} alt="Linux实时桌面" draggable={false}
   onPointerDown={e=>{if(!active||e.button!==0)return;queue.barrier();e.preventDefault();input.current?.focus({preventScroll:true});e.currentTarget.setPointerCapture(e.pointerId);pointer.current=point(e);}}
   onPointerUp={e=>{
    if(!active||!pointer.current)return;
    const start=pointer.current,end=point(e);
    pointer.current=null;
    if(Math.hypot(end.x-start.x,end.y-start.y)>8){
     flushClick();
     send({kind:'drag',...start,toX:end.x,toY:end.y});
     return;
    }
    const previous=click.current,now=performance.now();
    if(previous&&now-previous.at<320&&Math.hypot(previous.x-end.x,previous.y-end.y)<12){
     takeClick();
     send({kind:'click',...end,button:1,clicks:2});
     return;
    }
    flushClick();
    click.current={...end,at:now,timer:setTimeout(flushClick,320)};
   }}
   onPointerCancel={()=>{queue.barrier();pointer.current=null;}} onContextMenu={e=>{queue.barrier();e.preventDefault();pointer.current=null;if(active){flushClick();send({kind:'click',...point(e),button:3});}}}
   onWheel={e=>{queue.barrier();if(!active||Date.now()-wheel.current<180)return;wheel.current=Date.now();flushClick();send({kind:'scroll',...point(e),amount:Math.sign(e.deltaY)*3});}}/>
  {active&&<textarea ref={input} className="desktop-keyboard" aria-label="电脑键盘输入" autoCapitalize="off" autoComplete="off" spellCheck={false}
   onFocus={()=>queue.barrier()} onBlur={()=>queue.barrier()}
   onCompositionStart={()=>{queue.barrier();composing.current=true;}}
   onCompositionEnd={e=>{
    composing.current=false;
    e.currentTarget.value='';
    if(e.data)sendKeyboard({kind:'text',text:e.data});
    queue.barrier();
   }}
   onInput={e=>{
    if(composing.current)return;
    const text=e.currentTarget.value;
    e.currentTarget.value='';
    const event=e.nativeEvent;
    if(event instanceof InputEvent&&(
     event.isComposing||event.inputType==='insertCompositionText'||event.inputType==='insertFromComposition'
    ))return;
    if(text)sendKeyboard({kind:'text',text});
   }}
   onPaste={e=>{e.preventDefault();queue.barrier();sendKeyboard({kind:'text',text:e.clipboardData.getData('text/plain')});queue.barrier();}}
   onKeyDown={e=>{
    if(e.nativeEvent.isComposing||composing.current||e.key==='Process'||(e.metaKey&&e.key===','))return;
    const control=e.ctrlKey||e.metaKey,key=DESKTOP_KEYS[e.key];
    if(control&&e.key.toLowerCase()==='v')return;
    if(key||control||e.altKey){
     e.preventDefault();
     sendKeyboard({kind:'key',key:[control?'ctrl':'',e.altKey?'alt':'',e.shiftKey?'shift':'',key||e.key].filter(Boolean).join('+')});
    }
   }}/>}
  {!online&&<span className="desktop-connecting">连接桌面…</span>}
 </div>;
}
