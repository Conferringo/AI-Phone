'use client';
import {useI18n} from "./locale-provider";
import {createContext,useContext,useCallback,useEffect,useRef,useState,type ReactNode} from 'react';
import {createPortal} from 'react-dom';
import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ProactiveRequestSchema,ProactiveStateSchema,ProactiveUpdateRequestSchema} from '@ai-phone/protocol/ai_phone/v1/proactive_pb';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {Compass,Pause,LoaderCircle} from 'lucide-react';
import './proactive-controls.css';
type State=MessageShape<typeof ProactiveStateSchema>;
type Options=Pick<State,'exploring'>;
const statusLabel:Record<string,string>={exploring:'正在探索',notification:'查看手机动态',user_busy:'正在处理你的任务',paused:'任务已暂停',waiting:'等待下一轮',idle:'待机'};
type Controller={state:State|null;error:string;saving:boolean;update:(patch:Partial<Options>)=>Promise<void>};
const ProactiveContext=createContext<Controller|null>(null);
function useProactive(){const value=useContext(ProactiveContext);if(!value)throw new Error('ProactiveProvider is required');return value;}
async function readState(signal?:AbortSignal){
 const response=await fetch('/api/proactive/state',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(ProactiveRequestSchema,create(ProactiveRequestSchema)),cache:'no-store',signal});
 const bytes=new Uint8Array(await response.arrayBuffer());if(!response.ok)throw new Error(fromBinary(ApiErrorSchema,bytes).message);return fromBinary(ProactiveStateSchema,bytes);
}
export function ExplorationAura({active,paused,kind="phone"}:{active:boolean;paused:boolean;kind?:"phone"|"desktop"}){
 const [mounted,setMounted]=useState(false);
 useEffect(()=>{if(active){setMounted(true);return;}const timer=setTimeout(()=>setMounted(false),700);return()=>clearTimeout(timer);},[active]);
 return mounted?createPortal(<div className="exploration-aura" aria-hidden="true" data-kind={kind} data-enabled={active} data-paused={paused}><span className="exploration-edge"><span className="exploration-halo"/><span className="exploration-rim"/></span></div>,document.body):null;
}
export function ProactiveProvider({children}:{children:ReactNode}){
 const [state,setState]=useState<State|null>(null),[connectionError,setConnectionError]=useState(''),[saveError,setSaveError]=useState(''),[saving,setSaving]=useState(false),[visible,setVisible]=useState(true);
 const current=useRef<State|null>(null),updating=useRef(false),generation=useRef(0);
 const receive=useCallback((next:State)=>{if(current.current&&next.revision<current.current.revision)return;current.current=next;setState(next);},[]);
 useEffect(()=>{
  const abort=new AbortController();let timer:ReturnType<typeof setTimeout>;
  const refresh=async()=>{const version=generation.current;if(!document.hidden&&!updating.current)try{const next=await readState(abort.signal);if(!abort.signal.aborted&&version===generation.current){receive(next);setConnectionError('');}}catch(e){if(!abort.signal.aborted)setConnectionError(e instanceof Error?e.message:'主动服务暂不可用');}if(!abort.signal.aborted)timer=setTimeout(refresh,2500);};
  const visibility=()=>{setVisible(!document.hidden);if(!document.hidden){clearTimeout(timer);void refresh();}};
  void refresh();document.addEventListener('visibilitychange',visibility);return()=>{abort.abort();clearTimeout(timer);document.removeEventListener('visibilitychange',visibility);};
 },[receive]);
 async function update(patch:Partial<Options>){
  if(!current.current||updating.current)return;updating.current=true;generation.current++;setSaving(true);setSaveError('');
  const value=current.current;
  try{
   const response=await fetch('/api/proactive/update',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(ProactiveUpdateRequestSchema,create(ProactiveUpdateRequestSchema,{expectedRevision:value.revision,exploring:value.exploring,notificationsEnabled:true,forwardNotifications:true,autonomousReply:true,autonomousInstall:true,intervalSeconds:value.intervalSeconds,wechatWatch:true,...patch}))});
   const bytes=new Uint8Array(await response.arrayBuffer());if(!response.ok)throw new Error(fromBinary(ApiErrorSchema,bytes).message);receive(fromBinary(ProactiveStateSchema,bytes));
  }catch(e){setSaveError(e instanceof Error?e.message:'保存失败');try{receive(await readState());}catch{}}
  finally{updating.current=false;setSaving(false);}
 }
 return <ProactiveContext.Provider value={{state,error:saveError||connectionError,saving,update}}>{children}<ExplorationAura active={!!state?.exploring} paused={!visible||!!connectionError}/></ProactiveContext.Provider>;
}
/** Both device types expose the same durable pause/resume control and failure feedback. */
export function ExplorationControl({active,ready,saving,error,status,onToggle}:{active:boolean;ready:boolean;saving:boolean;error:string;status:string;onToggle:()=>void}){
 const {t}=useI18n();
 return <div className="exploration-control"><div className="proactive-dock" data-exploring={active}><button className="exploration-toggle" type="button" aria-pressed={active} disabled={!ready||saving} onClick={onToggle} title={t(status)}>{saving?<LoaderCircle size={16} className="spinning"/>:active?<Pause size={15}/>:<Compass size={16}/>}<span>{active?t("暂停探索"):t("自主探索")}</span></button></div>{error&&<span className="proactive-inline-error" role="alert">{t(error)}</span>}</div>;
}
export function ExplorationToggle(){
 const {state,saving,error,update}=useProactive();
 return <ExplorationControl active={!!state?.exploring} ready={!!state} saving={saving} error={error||state?.lastError||''} status={state?statusLabel[state.status]||state.status:'连接主动服务'} onToggle={()=>void update({exploring:!state?.exploring})}/>;
}
