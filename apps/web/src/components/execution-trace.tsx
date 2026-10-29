"use client";
import {useI18n} from "./locale-provider";
import {memo,useEffect,useId,useMemo,useRef,useState} from 'react';
import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {HarnessEventRequestSchema,HarnessEventListSchema} from '@ai-phone/protocol/ai_phone/v1/workbench_pb';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {ChevronRight,Check,LoaderCircle,AlertCircle,Terminal,Pause,Square,Clock3} from 'lucide-react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import {createTraceLoader,mergeTimeline,timelineEntries,splitToolRecord,traceWaitingSeconds,type TimelineEvent} from './execution-timeline';
import './execution-trace.css';
type HarnessEvent=MessageShape<typeof HarnessEventListSchema>['events'][number];
const labels:Record<string,string>={queued:'排队中',running:'正在工作',paused:'已暂停',failed:'执行失败',canceled:'已停止',succeeded:'已完成',completed:'已完成'};
function StatusIcon({status}:{status:string}){const {t}=useI18n();return status==='running'||status==='queued'?<LoaderCircle size={14} className="spinning" aria-label={t(labels[status])}/>:status==='completed'||status==='succeeded'?<Check size={14} aria-label={t("已完成")}/>:status==='paused'?<Pause size={14} aria-label={t("已暂停")}/>:status==='canceled'?<Square size={12} aria-label={t("已停止")}/>:status==='failed'?<AlertCircle size={14} aria-label={t("执行失败")}/>:<Terminal size={14} aria-label={status||t("执行记录")}/>;}
function formatted(value:string){if(!value.trim())return value;try{return JSON.stringify(JSON.parse(value),null,2);}catch{return value;}}
async function fetchEvents(runId:string,signal:AbortSignal){
 const response=await fetch('/api/workbench/events',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(HarnessEventRequestSchema,create(HarnessEventRequestSchema,{runId})),cache:'no-store',signal:AbortSignal.any([signal,AbortSignal.timeout(10000)])});
 const bytes=new Uint8Array(await response.arrayBuffer());if(response.status===401)location.assign('/login');if(!response.ok)throw Error(fromBinary(ApiErrorSchema,bytes).message);return fromBinary(HarnessEventListSchema,bytes).events;
}
const ToolStep=memo(function ToolStep({event,now}:{event:TimelineEvent;now:number}){const {t}=useI18n();
 const parts=useMemo(()=>{const value=splitToolRecord(event.content);return{input:formatted(value.input),output:formatted(value.output)};},[event.content]),finished=Number(event.finishedAtMs),start=Number(event.createdAtMs),elapsed=finished?Math.max(0,finished-start):event.status==='running'?Math.max(0,now-start):0;
 return <details className="timeline-tool" data-status={event.status}><summary><StatusIcon status={event.status}/><span className="timeline-tool-title">{event.title}</span><span className="timeline-tool-state">{event.kind==='audit'?(event.status==='completed'?t("核验通过"):event.status==='failed'?t("核验未通过"):t(labels[event.status]||event.status)):t(labels[event.status]||event.status)}</span>{elapsed>0&&<time dateTime={`PT${(elapsed/1000).toFixed(1)}S`}>{(elapsed/1000).toFixed(1)}s</time>}<ChevronRight className="timeline-tool-chevron" size={14}/></summary><div className="timeline-tool-body">{parts.input&&<section><h4>{t("输入")}</h4><pre>{parts.input}</pre></section>}{parts.output&&<section><h4>{t("输出")}</h4><pre>{parts.output}</pre></section>}</div></details>;
});
const PublicSummary=memo(function PublicSummary({content}:{content:string}){return <div className="timeline-public-summary"><Markdown remarkPlugins={[remarkGfm]}>{content}</Markdown></div>;});
export const ExecutionTrace=memo(function ExecutionTrace({runId,messageId,active=false,status,nextWakeAtMs=0,afterMs=0}:{runId:string;messageId?:string;active?:boolean;status?:string;nextWakeAtMs?:number;afterMs?:number}){const {t}=useI18n();
 const root=useRef<HTMLDivElement>(null),contentId=useId(),[expanded,setExpanded]=useState(active),[record,setRecord]=useState<{runId:string;events:HarnessEvent[]}>({runId,events:[]}),[error,setError]=useState(''),[loaded,setLoaded]=useState(false),[reload,setReload]=useState(0),[now,setNow]=useState(Date.now());
 const events=record.runId===runId?record.events:[],task=events.find(event=>event.kind==='task'),state=status||(active?task?.status||'running':messageId?'':task?.status||''),steps=timelineEntries(events.filter(event=>!messageId||event.messageId===messageId)).filter(event=>Number(event.createdAtMs)>afterMs||Number(event.finishedAtMs)>afterMs),waiting=traceWaitingSeconds(nextWakeAtMs,now,state);
 useEffect(()=>{if(active)setExpanded(true);},[active,runId]);
 useEffect(()=>{if(!active&&!waiting)return;const timer=setInterval(()=>setNow(Date.now()),1000);return()=>clearInterval(timer);},[active,waiting>0]);
 useEffect(()=>{
  // A collapsed historical message must not start even its first request.
  if(!runId||(!active&&!expanded))return;
  const loader=createTraceLoader({enabled:true,poll:active,visible:()=>!document.hidden,request:(signal)=>fetchEvents(runId,signal),onData:(incoming)=>{setRecord(previous=>({runId,events:mergeTimeline(previous.runId===runId?previous.events:[],incoming)}));setLoaded(true);setError('');},onError:(value)=>{setError(value instanceof Error?value.message:'过程读取失败');}});
  const visibility=()=>{if(!document.hidden)loader.refresh();};document.addEventListener('visibilitychange',visibility);
  return()=>{loader.dispose();document.removeEventListener('visibilitychange',visibility);};
 },[runId,active,expanded,status,reload]);
 if(!runId||(afterMs>0&&!steps.length))return null;
 const title=waiting?t("等待 {seconds}s",{seconds:waiting}):active?t(labels[state]||"正在工作"):labels[state]?t(labels[state]):steps.length?t("{count} 条执行记录",{count:steps.length}):t("执行记录");
 return <div ref={root} className="execution-trace execution-timeline" data-status={state}><button type="button" className="trace-trigger" aria-expanded={expanded} aria-controls={contentId} onClick={()=>setExpanded(value=>!value)}><ChevronRight size={14} className={expanded?'rotated':''}/>{waiting?<Clock3 size={14}/>:active?<StatusIcon status={state}/>:<Terminal size={14}/>}<span>{title}</span></button>{expanded&&<div className="trace-timeline-content" id={contentId}>{error&&<div className="trace-load-error" role="alert"><span>{t(error)}</span><button type="button" onClick={()=>setReload(value=>value+1)}>{t("重试")}</button></div>}<ol className="trace-timeline" aria-label={t("执行时间线")}>{steps.map(event=><li key={event.id} className={event.kind==='summary'?'timeline-summary':'timeline-operation'}>{event.kind==='summary'?<PublicSummary content={event.content}/>:<ToolStep event={event} now={event.status==='running'?now:0}/>}</li>)}</ol>{!steps.length&&!error&&(!loaded?<div className="trace-loading" role="status"><LoaderCircle className="spinning" size={14}/><span>{t("读取过程…")}</span></div>:!active?<p className="trace-empty">{t("没有操作记录")}</p>:null)}</div>}</div>;});
