'use client';
import {useEffect,useRef,useState} from 'react';
import {Select as Primitive} from 'radix-ui';
import {Check,ChevronDown,ChevronUp} from 'lucide-react';
import './select.css';

type Option={value:string;label:string;disabled?:boolean};
type Props={value:string;options:Option[];onValueChange:(value:string)=>void;disabled?:boolean;id?:string;className?:string;'aria-label':string};
const empty='__ai_phone_unassigned__';
export function SelectField({value,options,onValueChange,disabled,id,className,'aria-label':label}:Props){
 const trigger=useRef<HTMLButtonElement>(null),[open,setOpen]=useState(false);
 const owner=trigger.current?.closest<HTMLElement>('[data-window-id]')?.dataset.windowId;
 useEffect(()=>{if(!open)return;const close=(e:Event)=>{if((e as CustomEvent).detail===owner)setOpen(false);};window.addEventListener('phone:window-closed',close);return()=>window.removeEventListener('phone:window-closed',close);},[open,owner]);
 return <Primitive.Root value={value||empty} onValueChange={next=>onValueChange(next===empty?'':next)} disabled={disabled} open={open} onOpenChange={setOpen}>
  <Primitive.Trigger ref={trigger} id={id} type="button" className={`field-select ${className||''}`} aria-label={label}><Primitive.Value/><Primitive.Icon asChild><ChevronDown size={15}/></Primitive.Icon></Primitive.Trigger>
  <Primitive.Portal><Primitive.Content className="field-select-menu" data-window-owner={owner} position="popper" sideOffset={6} collisionPadding={12} onEscapeKeyDown={e=>e.stopPropagation()}>
   <Primitive.ScrollUpButton className="field-select-scroll"><ChevronUp size={14}/></Primitive.ScrollUpButton>
   <Primitive.Viewport>{options.map(option=><Primitive.Item key={option.value} value={option.value||empty} disabled={option.disabled} className="field-select-option"><Primitive.ItemText>{option.label}</Primitive.ItemText><Primitive.ItemIndicator><Check size={14}/></Primitive.ItemIndicator></Primitive.Item>)}</Primitive.Viewport>
   <Primitive.ScrollDownButton className="field-select-scroll"><ChevronDown size={14}/></Primitive.ScrollDownButton>
  </Primitive.Content></Primitive.Portal>
 </Primitive.Root>;
}
