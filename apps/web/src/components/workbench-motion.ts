'use client';
import {useCallback,useLayoutEffect,useRef,type RefObject} from 'react';
import {gsap} from 'gsap';
import {Flip} from 'gsap/Flip';
gsap.registerPlugin(Flip);
type Snapshot={layout:ReturnType<typeof Flip.getState>;cameras:Map<HTMLElement,string>;chats:ReturnType<typeof Flip.getState>};

/** Layout and camera transforms have separate owners; screens/text are never stretched. */
export function useWorkbenchMotion(root:RefObject<HTMLElement|null>,layoutKey:string){
 const before=useRef<Snapshot|null>(null),timeline=useRef<gsap.core.Timeline|null>(null),context=useRef<gsap.Context|null>(null);
 const nodes=useCallback((selector:string)=>[...root.current?.querySelectorAll<HTMLElement>(selector)??[]],[root]);
 const frame=useCallback(()=>root.current?.dispatchEvent(new Event('workbench-layout-frame')),[root]);
 const finish=useCallback(()=>{timeline.current?.progress(1).kill();timeline.current=null;before.current=null;frame();},[frame]);
 const capture=useCallback(()=>{
  if(before.current)return;
  if(document.hidden||matchMedia('(prefers-reduced-motion:reduce)').matches){finish();return;}
  const panes=nodes('.device-pane');if(!panes.length)return;
  const options={props:'opacity',kill:false};const layout=Flip.getState(panes,options);
  const cameras=new Map(nodes('.device-visual').map(el=>[el,getComputedStyle(el).transform]));
  const chats=Flip.getState(nodes('.conversation-pane').filter(el=>el.getClientRects().length),options);
  // Read the visible interrupted frame first, then settle old owned styles in the same task.
  timeline.current?.progress(1).kill();before.current={layout,cameras,chats};
 },[nodes,finish]);
 useLayoutEffect(()=>{
  context.current=gsap.context(()=>{},root);
  const hidden=()=>{if(document.hidden)finish();};const reduced=matchMedia('(prefers-reduced-motion:reduce)');
  document.addEventListener('visibilitychange',hidden);reduced.addEventListener('change',finish);window.addEventListener('resize',finish);
  return()=>{document.removeEventListener('visibilitychange',hidden);reduced.removeEventListener('change',finish);window.removeEventListener('resize',finish);timeline.current?.kill();context.current?.revert();};
 },[root,finish]);
 useLayoutEffect(()=>{
  const state=before.current;before.current=null;
  if(!state||document.hidden||matchMedia('(prefers-reduced-motion:reduce)').matches){frame();return;}
  context.current?.add(()=>{
   const cameraTargets=nodes('.device-visual').map(el=>({el,transform:getComputedStyle(el).transform}));
   const next=gsap.timeline({onUpdate:frame,onComplete:frame});timeline.current=next;
   next.add(Flip.from(state.layout,{targets:nodes('.device-pane'),duration:.56,ease:'power2.inOut',scale:false,absolute:true,prune:true}),0);
   for(const {el,transform} of cameraTargets){const old=state.cameras.get(el);if(old&&old!==transform)next.fromTo(el,{transform:old},{transform,duration:.56,ease:'power2.inOut',clearProps:'transform'},0);}
   // Animate the panel's real geometry. FLIP scaling would stretch text and the input.
   next.add(Flip.from(state.chats,{targets:nodes('.conversation-pane').filter(el=>el.getClientRects().length),duration:.56,ease:'power2.inOut',scale:false,absolute:true,prune:true}),0);
  });
 },[layoutKey,frame,nodes]);
 return capture;
}
