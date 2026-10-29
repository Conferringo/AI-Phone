'use client';
import {SelectField} from "@/components/ui/select";
import {useI18n} from "./locale-provider";
import {createContext,useContext,useCallback,useEffect,useRef,useState,type ReactNode} from 'react';
import {createPortal} from 'react-dom';
import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ProactiveRequestSchema,ProactiveStateSchema,ProactiveUpdateRequestSchema} from '@ai-phone/protocol/ai_phone/v1/proactive_pb';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {Compass,Pause,LoaderCircle} from 'lucide-react';
import {FloatingWindow} from './floating-window';
import './proactive-controls.css';
type State=MessageShape<typeof ProactiveStateSchema>;
type Options=Pick<State,'exploring'|'notificationsEnabled'|'forwardNotifications'|'autonomousReply'|'autonomousInstall'|'intervalSeconds'|'wechatWatch'>;
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
   const response=await fetch('/api/proactive/update',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(ProactiveUpdateRequestSchema,create(ProactiveUpdateRequestSchema,{expectedRevision:value.revision,exploring:value.exploring,notificationsEnabled:value.notificationsEnabled,forwardNotifications:value.forwardNotifications,autonomousReply:value.autonomousReply,autonomousInstall:value.autonomousInstall,intervalSeconds:value.intervalSeconds,wechatWatch:value.wechatWatch,...patch}))});
   const bytes=new Uint8Array(await response.arrayBuffer());if(!response.ok)throw new Error(fromBinary(ApiErrorSchema,bytes).message);receive(fromBinary(ProactiveStateSchema,bytes));
  }catch(e){setSaveError(e instanceof Error?e.message:'保存失败');try{receive(await readState());}catch{}}
  finally{updating.current=false;setSaving(false);}
 }
 return <ProactiveContext.Provider value={{state,error:saveError||connectionError,saving,update}}>{children}<ExplorationAura active={!!state?.exploring} paused={!visible||!!connectionError}/></ProactiveContext.Provider>;
}
export function ExplorationToggle(){const {t}=useI18n();
 const {state,saving,error,update}=useProactive();
 return <>
  <div className="proactive-dock" data-exploring={state?.exploring||false}>
   <button className="exploration-toggle" type="button" aria-pressed={state?.exploring||false} disabled={!state||saving} onClick={()=>void update({exploring:!state?.exploring})} title={state?statusLabel[state.status]||state.status:t("连接主动服务")}>
    {saving?<LoaderCircle size={16} className="spinning"/>:state?.exploring?<Pause size={15}/>:<Compass size={16}/>}<span>{state?.exploring?t("暂停探索"):t("自主探索")}</span>
   </button>
  </div>
  {error&&<span className="proactive-inline-error" role="alert">{t(error)}</span>}
 </>;
}
export function ProactiveWindow({open,onClose}:{open:boolean;onClose:()=>void}){const {t}=useI18n();
 const {state,saving,error,update}=useProactive();
 return <FloatingWindow fitContent id="proactive" title={t("自主探索")} open={open} onClose={onClose} width={360} height={400}>
   <div className="proactive-options">
    <div className="proactive-status" role="status"><span className="proactive-indicator" data-live={state?.exploring||false}/>{state?statusLabel[state.status]||state.status:t("连接中")}</div>
    {state&&<>
     <label><span>{t("通知摘要")}</span><input type="checkbox" role="switch" disabled={saving} checked={state.notificationsEnabled} onChange={e=>void update({notificationsEnabled:e.target.checked})}/></label>
     <label><span>{t("微信自动唤醒")}</span><input type="checkbox" role="switch" disabled={saving} checked={state.wechatWatch} onChange={e=>void update({wechatWatch:e.target.checked, ...(e.target.checked?{notificationsEnabled:true}:{})})}/></label>
     {state.wechatWatch&&!state.attentionConnected&&<p className="proactive-problem" role="status">{t("微信界面监听未连接")}</p>}
     {state.notificationsEnabled&&!state.listenerConnected&&<p className="proactive-problem" role="status">{t("通知连接已断开")}</p>}
     <label><span>{t("推送到远端")}</span><input type="checkbox" role="switch" disabled={saving||!state.notificationsEnabled} checked={state.forwardNotifications} onChange={e=>void update({forwardNotifications:e.target.checked})}/></label>
     <div className="proactive-divider"/>
     <label><span>{t("自主回复消息")}</span><input type="checkbox" role="switch" disabled={saving} checked={state.autonomousReply} onChange={e=>void update({autonomousReply:e.target.checked})}/></label>
     <label><span>{t("自主安装应用与 Skill")}</span><input type="checkbox" role="switch" disabled={saving} checked={state.autonomousInstall} onChange={e=>void update({autonomousInstall:e.target.checked})}/></label>
     <label><span>{t("每轮间隔")}</span><SelectField aria-label={t("探索间隔")} disabled={saving} value={String(state.intervalSeconds)} onValueChange={value=>void update({intervalSeconds:Number(value)})} options={[15,45,120,300].map(n=>({value:String(n),label:String(n<60?t("{0} 秒", {"0":n}):t("{0} 分钟", {"0":n/60}))}))}/> </label>
     {state.lastError&&<p role="status" className="proactive-problem">{state.lastError}</p>}
     {state.deliveryStatus&&<details className="proactive-delivery"><summary>{t("最近推送")}</summary><p>{state.deliveryStatus.replaceAll('wecom',t("企业微信")).replaceAll('feishu',t("飞书")).replaceAll('qq','QQ').replaceAll('unbound',t("未绑定")).replaceAll('unpaired',t("未配对")).replaceAll('delivered',t("平台已接收")).replaceAll('queued',t("等待发送")).replaceAll('unknown',t("结果未知")).replaceAll('failed',t("失败")).replaceAll('canceled',t("已撤销"))}</p></details>}
    </>}
    {error&&<p className="proactive-problem" role="alert">{t(error)}</p>}
   </div>
  </FloatingWindow>;
}
