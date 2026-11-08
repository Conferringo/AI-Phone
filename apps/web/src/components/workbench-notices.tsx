'use client';
import {createContext,useCallback,useContext,useEffect,useId,useRef,useState,useSyncExternalStore,type ReactNode} from 'react';
import {AlertCircle,X} from 'lucide-react';
import {WorkbenchNoticeStore} from '@/lib/workbench-notices';
import {useI18n} from './locale-provider';

const Notices=createContext<WorkbenchNoticeStore|null>(null);
const DeviceScope=createContext({scope:'workbench',label:''});
export function WorkbenchNoticeProvider({children}:{children:ReactNode}){
 const [store]=useState(()=>new WorkbenchNoticeStore());
 return <Notices.Provider value={store}>{children}</Notices.Provider>;
}
export function DeviceNoticeScope({id,name,children}:{id:string;name:string;children:ReactNode}){
 return <DeviceScope.Provider value={{scope:id,label:name}}>{children}</DeviceScope.Provider>;
}
function useNoticeStore(){const store=useContext(Notices);if(!store)throw Error('WorkbenchNoticeProvider is required');return store;}
export function useWorkbenchNotice(message:string,retry?:()=>void){
 const store=useNoticeStore(),id=useId(),{scope,label}=useContext(DeviceScope),callback=useRef(retry);
 callback.current=retry;
 const invoke=useCallback(()=>callback.current?.(),[]),canRetry=!!retry;
 useEffect(()=>{store.set(id,message?{message,scope,label,retry:canRetry?invoke:undefined}:null);},[store,id,message,scope,label,canRetry,invoke]);
 useEffect(()=>()=>store.set(id,null),[store,id]);
}
export function WorkbenchNotices({activeFeature}:{activeFeature:string}){
 const store=useNoticeStore(),notices=useSyncExternalStore(store.subscribe,store.snapshot,store.snapshot),{t}=useI18n();
 const root=useRef<HTMLDivElement>(null),current=notices[0];
 useEffect(()=>{const details=root.current?.querySelector('details');if(details)details.open=false;},[activeFeature]);
 useEffect(()=>{
  const close=(event:PointerEvent|KeyboardEvent)=>{
   const details=root.current?.querySelector('details[open]') as HTMLDetailsElement|null;
   if(!details)return;
   const inside=event.target instanceof Node&&details.contains(event.target);
   if(event instanceof KeyboardEvent){if(event.key!=='Escape')return;details.open=false;if(inside)details.querySelector('summary')?.focus();}
   else if(!inside)details.open=false;
  };
  document.addEventListener('pointerdown',close);document.addEventListener('keydown',close);
  return()=>{document.removeEventListener('pointerdown',close);document.removeEventListener('keydown',close);};
 },[]);
 return <div ref={root} className="workbench-notices" aria-live="polite" aria-relevant="additions text">{current&&<details className="workbench-notice-disclosure">
  <summary className="workbench-notice-summary" title={t(current.message)}>
   <AlertCircle size={16} aria-hidden="true"/>
   <span className="workbench-notice-preview" dir="auto">{current.label&&<span>{current.label} · </span>}{t(current.message)}</span>
   {notices.length>1&&<span className="workbench-notice-count">{notices.length}</span>}
  </summary>
  <div className="workbench-notice-details">{notices.map(notice=><div className="workbench-notice" key={notice.id}>
   <p dir="auto">{notice.label&&<span className="workbench-notice-device">{notice.label} · </span>}{t(notice.message)}</p>
   {notice.retry&&<button type="button" className="notice-retry" onClick={notice.retry}>{t('重试')}</button>}
   <button type="button" className="notice-dismiss" aria-label={t('关闭错误')} onClick={()=>store.dismiss(notice.id)}><X size={14}/></button>
  </div>)}</div>
 </details>}</div>;
}
