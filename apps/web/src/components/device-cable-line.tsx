'use client';
import {useEffect,useLayoutEffect,useRef,useState,type RefObject} from 'react';
type Device={id:string;name:string;kind:string};
export function DeviceCableLine({root,devices,linked}:{root:RefObject<HTMLElement|null>;devices:Device[];linked:string[]}){
 const svg=useRef<SVGSVGElement>(null);
 const desktop=devices.find(d=>d.kind==='desktop');
 const active=desktop&&linked.includes(desktop.id)?devices.filter(d=>d.kind==='phone'&&linked.includes(d.id)).map(d=>d.id):[];
 const [retained,setRetained]=useState<string[]>([]);
 const activeKey=active.join('|');
 useEffect(()=>{setRetained(old=>[...new Set([...old,...active])]);const timer=setTimeout(()=>setRetained(active),240);return()=>clearTimeout(timer);},[activeKey]);
 const phones=devices.filter(d=>d.kind==='phone'&&(active.includes(d.id)||retained.includes(d.id)));
 const key=phones.map(d=>d.id).join('|')+desktop?.id;
 useLayoutEffect(()=>{
  const host=root.current,node=svg.current;if(!host||!node||!desktop)return;
  const draw=()=>{
   const origin=host.getBoundingClientRect();
   const screens=[...host.querySelectorAll<HTMLElement>('[data-scene-device]')];
   const target=screens.find(e=>e.dataset.sceneDevice===desktop.id)?.getBoundingClientRect();
   if(!target?.width)return;
   for(const phone of phones){
    const screen=screens.find(e=>e.dataset.sceneDevice===phone.id)?.getBoundingClientRect();
    const group=[...node.querySelectorAll<SVGGElement>('[data-cable-phone]')].find(e=>e.dataset.cablePhone===phone.id);
    if(!screen?.width||!group)continue;
    const x1=screen.x+screen.width/2-origin.x,y1=screen.bottom-origin.y;
    const x2=target.x+target.width/2-origin.x,y2=target.bottom-origin.y;
    const bend=Math.max(y1,y2)+22;
    const d=`M ${x1} ${y1+4} C ${x1} ${bend}, ${x2} ${bend}, ${x2} ${y2+4}`;
    group.querySelectorAll('path').forEach(p=>p.setAttribute('d',d));
    const plugs=group.querySelectorAll('rect');
    plugs[0]?.setAttribute('x',String(x1-4));plugs[0]?.setAttribute('y',String(y1-2));
    plugs[1]?.setAttribute('x',String(x2-4));plugs[1]?.setAttribute('y',String(y2-2));
   }
  };
  draw();const observer=new ResizeObserver(draw);observer.observe(host);
  host.querySelectorAll('[data-scene-device]').forEach(e=>observer.observe(e));
  host.addEventListener('workbench-layout-frame',draw);host.addEventListener('scroll',draw,true);
  return()=>{observer.disconnect();host.removeEventListener('workbench-layout-frame',draw);host.removeEventListener('scroll',draw,true);};
 },[root,key]);
 return <svg ref={svg} className="device-cable-overlay" aria-label={active.length?'设备数据线已连接':undefined} role={active.length?'img':undefined} aria-hidden={!active.length}>
  {phones.map(phone=><g key={phone.id} data-cable-phone={phone.id} data-connected={active.includes(phone.id)} className="device-cable-wire"><path className="cable-wire-shadow"/><path className="cable-wire-body" pathLength={1}/><rect width={8} height={9} rx={2}/><rect width={8} height={9} rx={2}/></g>)}
 </svg>;
}
