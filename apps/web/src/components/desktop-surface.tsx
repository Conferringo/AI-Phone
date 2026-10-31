'use client';
import {useWorkbenchNotice} from './workbench-notices';
import {clientId} from '@/lib/client-id';
import {useLayoutEffect,useRef,useState} from 'react';
import {create,type MessageInitShape,type MessageShape} from '@bufbuild/protobuf';
import {DesktopRequestSchema,DesktopObservationSchema,DesktopActionRequestSchema} from '@ai-phone/protocol/ai_phone/v1/desktop_pb';
import {settingsRpc} from '@/lib/settings-client';
import {PhoneStreamImage} from './phone-stream-image';
type Frame=MessageShape<typeof DesktopObservationSchema>;
type Action=MessageInitShape<typeof DesktopActionRequestSchema>;
export function DesktopSurface({deviceId,active}:{deviceId:string;active:boolean}){
 const frame=useRef<Frame|null>(null),queue=useRef(Promise.resolve()),input=useRef<HTMLTextAreaElement>(null),pointer=useRef<{x:number;y:number}|null>(null),composing=useRef(false),wheel=useRef(0);
 const clickTimer=useRef<ReturnType<typeof setTimeout>|null>(null),lastClick=useRef<{x:number;y:number;at:number}|null>(null);
 const activeRef=useRef(active),generation=useRef(0),receivedAt=useRef(0);
 useLayoutEffect(()=>{
  generation.current++;activeRef.current=active;frame.current=null;receivedAt.current=0;pointer.current=null;
  if(clickTimer.current)clearTimeout(clickTimer.current);lastClick.current=null;
  return()=>{activeRef.current=false;generation.current++;if(clickTimer.current)clearTimeout(clickTimer.current);};
 },[active]);
 const [error,setError]=useState(''),[online,setOnline]=useState(false);
 useWorkbenchNotice(active?error:'');
 const send=(value:Action)=>{
  if(!activeRef.current)return;const version=generation.current,current=()=>activeRef.current&&generation.current===version;
  queue.current=queue.current.then(async()=>{
   if(!current())return;
   try{
    let previous=frame.current;
    if(!previous||performance.now()-receivedAt.current>35000)previous=await settingsRpc('control',DesktopRequestSchema,{deviceId},DesktopObservationSchema,'desktop');
    if(!current())return;
    const next=await settingsRpc('action',DesktopActionRequestSchema,{...value,deviceId,actionId:clientId(),observationId:previous.observationId,epoch:previous.epoch},DesktopObservationSchema,'desktop');
    if(current()){frame.current=next;receivedAt.current=performance.now();setError('');}
   }catch(e){if(current()){frame.current=null;setError(e instanceof Error?e.message:'电脑操作失败');}}
  });
 };
 const point=(e:React.PointerEvent<HTMLImageElement>|React.MouseEvent<HTMLImageElement>|React.WheelEvent<HTMLImageElement>)=>{const r=e.currentTarget.getBoundingClientRect();return{x:Math.max(0,Math.min(1279,Math.round((e.clientX-r.left)/r.width*1280))),y:Math.max(0,Math.min(799,Math.round((e.clientY-r.top)/r.height*800)))};};
 const flush=()=>{if(input.current?.value&&!composing.current){send({kind:'text',text:input.current.value});input.current.value='';}};
 return <div className="desktop-screen" data-scene-device={deviceId}>
  <PhoneStreamImage endpoint={`/api/desktop/media?device_id=${encodeURIComponent(deviceId)}`} fallback="" enabled onConnectionState={setOnline} alt="Linux实时桌面" draggable={false}
   onPointerDown={e=>{if(!active||e.button!==0)return;e.preventDefault();input.current?.focus({preventScroll:true});e.currentTarget.setPointerCapture(e.pointerId);pointer.current=point(e);}}
   onPointerUp={e=>{if(!active||!pointer.current)return;const start=pointer.current,end=point(e);pointer.current=null;if(Math.hypot(end.x-start.x,end.y-start.y)>8){if(clickTimer.current)clearTimeout(clickTimer.current);lastClick.current=null;send({kind:'drag',...start,toX:end.x,toY:end.y});}else{const previous=lastClick.current;if(previous&&Date.now()-previous.at<320&&Math.hypot(previous.x-end.x,previous.y-end.y)<12){if(clickTimer.current)clearTimeout(clickTimer.current);lastClick.current=null;send({kind:'click',...end,button:1,clicks:2});}else{lastClick.current={...end,at:Date.now()};clickTimer.current=setTimeout(()=>{lastClick.current=null;send({kind:'click',...end,button:1,clicks:1});},320);}}}}
   onPointerCancel={()=>{pointer.current=null;}} onContextMenu={e=>{e.preventDefault();if(clickTimer.current)clearTimeout(clickTimer.current);lastClick.current=null;pointer.current=null;if(active)send({kind:'click',...point(e),button:3});}}
   onWheel={e=>{if(!active||Date.now()-wheel.current<180)return;wheel.current=Date.now();send({kind:'scroll',...point(e),amount:Math.sign(e.deltaY)*3});}}/>
  {active&&<textarea ref={input} className="desktop-keyboard" aria-label="电脑键盘输入" autoCapitalize="off" autoComplete="off" spellCheck={false} onCompositionStart={()=>{composing.current=true;}} onCompositionEnd={()=>{composing.current=false;flush();}} onInput={flush} onPaste={e=>{e.preventDefault();send({kind:'text',text:e.clipboardData.getData('text/plain')});}} onKeyDown={e=>{if(e.nativeEvent.isComposing||composing.current||e.key==='Process'||(e.metaKey&&e.key===','))return;const map:Record<string,string>={Enter:'Return',Backspace:'BackSpace',Delete:'Delete',Escape:'Escape',ArrowLeft:'Left',ArrowRight:'Right',ArrowUp:'Up',ArrowDown:'Down',Tab:'Tab',Home:'Home',End:'End',PageUp:'Prior',PageDown:'Next'};if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='v')return;if(map[e.key]||e.ctrlKey||e.metaKey||e.altKey){e.preventDefault();send({kind:'key',key:[e.ctrlKey||e.metaKey?'ctrl':'',e.altKey?'alt':'',e.shiftKey?'shift':'',map[e.key]||e.key].filter(Boolean).join('+')});}}}/>}
  {!online&&<span className="desktop-connecting">连接桌面…</span>}
 </div>;
}
