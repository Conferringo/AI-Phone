"use client";
import "@/styles/window.css";
import {useI18n} from "./locale-provider";
import {useEffect,useLayoutEffect,useId,useRef,useState,type ReactNode,type CSSProperties} from 'react';
import {createPortal} from 'react-dom';
import {Button} from './ui/button';
import {X} from 'lucide-react';
type Anchor={element:HTMLElement|null;mode:string};
let remembered:Anchor={element:null,mode:'keyboard'};
let activeWindow="";
const surfaces=new Map<string,{panel:HTMLElement;anchor:HTMLElement|null;parent?:string}>();
export function rememberAnchor(element:HTMLElement,mode='click'){remembered={element,mode};}
export function WindowTrigger({onOpen,children,label,open=false}:{onOpen:()=>void;children:ReactNode;label:string;open?:boolean}){
 const timer=useRef<ReturnType<typeof setTimeout>|null>(null);const cancel=()=>{if(timer.current)clearTimeout(timer.current);};useEffect(()=>cancel,[]);
 const show=(element:HTMLElement,mode:string)=>{rememberAnchor(element,mode);window.dispatchEvent(new CustomEvent("phone:window-source",{detail:{element,mode}}));onOpen();};
 return <Button variant="ghost" size="icon" aria-label={label} title={label} aria-expanded={open} onPointerEnter={e=>{if(e.pointerType!=='mouse'||e.buttons)return;const element=e.currentTarget;cancel();timer.current=setTimeout(()=>show(element,'mouse'),240);}} onPointerLeave={cancel} onPointerDown={cancel} onBlur={cancel} onKeyDown={e=>{if(e.key==='ArrowDown'){e.preventDefault();cancel();show(e.currentTarget,'keyboard');}}} onClick={e=>{cancel();show(e.currentTarget,e.detail===0?'keyboard':'click');}}>{children}</Button>;
}
function related(id:string){const ids=new Set([id]);let changed=true;while(changed){changed=false;for(const [key,value] of surfaces)if(value.parent&&ids.has(value.parent)&&!ids.has(key)){ids.add(key);changed=true;}}return [...ids].map(key=>surfaces.get(key)).filter(Boolean) as NonNullable<ReturnType<typeof surfaces.get>>[];}
export function FloatingWindow({id,title,open,onClose,children,width=760,height=570,actions,anchorKey,parentId,fitContent=false}:{id:string;title:string;open:boolean;onClose:()=>void;children:ReactNode;width?:number;height?:number;actions?:ReactNode;anchorKey?:string;parentId?:string;fitContent?:boolean}){const {t}=useI18n();
 const label=useId(),element=useRef<HTMLElement>(null),anchor=useRef<Anchor>({element:null,mode:'click'}),latest=useRef(onClose),motion=useRef<Animation|null>(null),composing=useRef(false);
 const interrupted=useRef<Keyframe|null>(null);
 latest.current=onClose;
 const [mounted,setMounted]=useState(false),[active,setActive]=useState(false);const [geometry,setGeometry]=useState({left:16,top:60,width,height});
 useEffect(()=>{const capture=(e:PointerEvent)=>{const button=(e.target as Element)?.closest<HTMLElement>('button,a,[data-window-trigger]');if(button)rememberAnchor(button,e.pointerType==='touch'?'touch':'click');};document.addEventListener('pointerdown',capture,true);return()=>document.removeEventListener('pointerdown',capture,true);},[]);
 useLayoutEffect(()=>{if(open){anchor.current={...remembered};setMounted(true);}},[open,anchorKey]);
 useLayoutEffect(()=>{
  const panel=element.current;if(!open||!mounted||!panel)return;
  const requested=anchor.current.element,inferredParent=requested?.closest<HTMLElement>('[data-window-id]')?.dataset.windowId,parent=parentId??inferredParent;
  // Restoring a child after a feature switch must retain its owning surface.
  const trigger=parentId&&inferredParent!==parentId?surfaces.get(parentId)?.panel.querySelector<HTMLElement>('.floating-header')??null:requested;
  anchor.current.element=trigger;
  const place=()=>{const view=window.visualViewport,w=view?.width||innerWidth,h=view?.height||innerHeight,x=view?.offsetLeft||0,y=view?.offsetTop||0;const r=trigger?.isConnected?trigger.getBoundingClientRect():null,child=!!parent;const pw=Math.min(width,w-32),ph=Math.min(height,Math.max(80,child||!r?h-88:h-(r.bottom+10-y)-16));const left=Math.max(x+16,Math.min(r?(child?r.right+10:r.right-pw):x+(w-pw)/2,x+w-pw-16));const top=Math.max(y+16,Math.min(r?(child?r.top:r.bottom+10):y+(h-ph)/2,y+h-ph-16));setGeometry(old=>old.left===left&&old.top===top&&old.width===pw&&old.height===ph?old:{left,top,width:pw,height:ph});};
  place();surfaces.set(id,{panel,anchor:trigger,parent});setActive(true);activeWindow=id;
  const focus=(e:Event)=>{activeWindow=(e as CustomEvent).detail;setActive(activeWindow===id);};
  const source=(e:Event)=>{const value=(e as CustomEvent).detail;if(value.element===trigger)anchor.current.mode=value.mode;};
  window.addEventListener("phone:window-source",source);window.dispatchEvent(new CustomEvent('phone:window-focus',{detail:id}));window.addEventListener('phone:window-focus',focus);
  let closeTimer:ReturnType<typeof setTimeout>|null=null,visited=anchor.current.mode==='mouse';const cancel=()=>{if(closeTimer)clearTimeout(closeTimer);closeTimer=null;};
  const inside=(target:EventTarget|null)=>{const owner=target instanceof Element?target.closest('[data-window-owner]')?.getAttribute('data-window-owner'):null;return target instanceof Node&&related(id).some(s=>s.panel.contains(target)||s.anchor?.contains(target)||!!owner&&s.panel.dataset.windowId===owner);};
  const protectedInput=()=>{const focused=document.activeElement;return document.hidden||!document.hasFocus()||composing.current||!!(focused instanceof HTMLElement&&inside(focused)&&focused.matches('input,textarea,[role=combobox],[role=option],[contenteditable=true]'));};
  const move=(e:PointerEvent)=>{if(e.pointerType!=='mouse'||e.buttons||anchor.current.mode!=='mouse')return;let within=inside(e.target);if(!within&&trigger?.isConnected){const a=trigger.getBoundingClientRect(),b=panel.getBoundingClientRect();const corridor={left:Math.min(a.left,b.left),right:Math.max(a.right,b.right),top:Math.min(a.top,b.top),bottom:Math.max(a.bottom,b.bottom)};within=e.clientX>=corridor.left&&e.clientX<=corridor.right&&e.clientY>=corridor.top&&e.clientY<=corridor.bottom;}if(within){visited=true;cancel();}else if(visited&&!protectedInput()&&!closeTimer){closeTimer=setTimeout(()=>{closeTimer=null;if(anchor.current.mode==='mouse'&&!protectedInput())latest.current();},280);}};
  const down=(e:PointerEvent)=>{if(!inside(e.target))latest.current();};
  const key=(e:KeyboardEvent)=>{const top=activeWindow;if(e.key==='Escape'&&top===id&&!e.defaultPrevented){e.preventDefault();latest.current();trigger?.focus({preventScroll:true});}};
  const parentClosed=(e:Event)=>{if((e as CustomEvent).detail===parent)latest.current();};
  document.addEventListener('pointermove',move);document.addEventListener('pointerdown',down);document.addEventListener('keydown',key);window.addEventListener('phone:window-closed',parentClosed);window.addEventListener('resize',place);window.visualViewport?.addEventListener('resize',place);
  return()=>{cancel();surfaces.delete(id);if(activeWindow===id){activeWindow=parent&&surfaces.has(parent)?parent:[...surfaces.keys()].at(-1)||"";window.dispatchEvent(new CustomEvent("phone:window-focus",{detail:activeWindow}));}window.removeEventListener("phone:window-source",source);window.removeEventListener('phone:window-focus',focus);document.removeEventListener('pointermove',move);document.removeEventListener('pointerdown',down);document.removeEventListener('keydown',key);window.removeEventListener('phone:window-closed',parentClosed);window.removeEventListener('resize',place);window.visualViewport?.removeEventListener('resize',place);};
 },[open,mounted,id,width,height,anchorKey,parentId]);
 useLayoutEffect(()=>{
  const panel=element.current;if(!mounted||!panel)return;
  const resumed=interrupted.current;interrupted.current=null;motion.current?.cancel();panel.inert=!open;
  if(!open)window.dispatchEvent(new CustomEvent('phone:window-closed',{detail:id}));
  const preference=matchMedia('(prefers-reduced-motion:reduce)');
  const finish=()=>{motion.current?.finish();panel.style.willChange='';interrupted.current=null;};
  const hidden=()=>{if(document.hidden)finish();};
  document.addEventListener('visibilitychange',hidden);preference.addEventListener('change',finish);
  const frame=requestAnimationFrame(()=>{
   const trigger=anchor.current.element,r=trigger?.isConnected?trigger.getBoundingClientRect():null,box=panel.getBoundingClientRect();
   panel.style.transformOrigin=r?`${r.left+r.width/2-box.left}px ${r.top+r.height/2-box.top}px`:'50% 0%';
   const folded={transform:`scale(${r?Math.max(.035,r.width/box.width):.9},${r?Math.max(.035,r.height/box.height):.9})`,opacity:0,borderRadius:getComputedStyle(trigger||panel).borderRadius};
   const expanded={transform:'none',opacity:1,borderRadius:getComputedStyle(panel).borderRadius};
   panel.style.willChange='transform,opacity';
   const animation=panel.animate(open?[resumed??folded,expanded]:[resumed??expanded,folded],{duration:preference.matches||document.hidden?0:open?460:240,easing:open?'cubic-bezier(.2,1.15,.3,1)':'cubic-bezier(.32,0,.65,1)',fill:open?'none':'forwards'});
   motion.current=animation;animation.onfinish=()=>{if(motion.current!==animation)return;panel.style.willChange='';if(!open)setMounted(false);};
   if(open&&anchor.current.mode==='keyboard'){
    const candidates=[...panel.querySelectorAll<HTMLElement>('input:not([type=file]),textarea,button')].filter(el=>el.getClientRects().length>0&&!el.closest('[hidden]')&&!el.hasAttribute('disabled'));
    (candidates.find(el=>el.closest('.floating-body'))??candidates[0])?.focus({preventScroll:true});
   }
  });
  return()=>{
   cancelAnimationFrame(frame);document.removeEventListener('visibilitychange',hidden);preference.removeEventListener('change',finish);
   if(motion.current?.playState==='running'){
    const style=getComputedStyle(panel);interrupted.current={transform:style.transform,opacity:style.opacity,borderRadius:style.borderRadius};
   }
   motion.current?.cancel();panel.style.willChange='';
  };
 },[open,mounted,id]);
 if(!mounted)return null;
 return createPortal(<section ref={element} role="dialog" aria-labelledby={label} tabIndex={-1} className="floating-window" data-window-id={id} data-active={active} style={{...geometry,...(fitContent?{height:"auto",maxHeight:geometry.height}:{})} as CSSProperties} onPointerDown={()=>{anchor.current.mode='click';window.dispatchEvent(new CustomEvent('phone:window-focus',{detail:id}));}} onCompositionStart={()=>{composing.current=true;}} onCompositionEnd={()=>{composing.current=false;}}><header className="floating-header"><h2 id={label}>{title}</h2><div className="floating-actions">{actions}<Button variant="ghost" size="icon" aria-label={t("关闭{0}", {"0":title})} onClick={onClose}><X size={17}/></Button></div></header><div className="floating-body">{children}</div></section>,document.body);
}
