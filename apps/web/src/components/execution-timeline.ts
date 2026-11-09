type ModelTiming={preparationMs:bigint;headersMs?:bigint;firstByteMs?:bigint;firstTextMs?:bigint;modelMs:bigint;elapsedMs:bigint;requestBytes:bigint};
export type TimelineEvent={id:string;kind:string;title:string;content:string;status:string;createdAtMs:bigint;finishedAtMs:bigint;modelTiming?:ModelTiming};
const timingKey=(value?:ModelTiming)=>value?`${value.preparationMs}/${value.headersMs}/${value.firstByteMs}/${value.firstTextMs}/${value.modelMs}/${value.elapsedMs}/${value.requestBytes}`:'';
/** Read replicas/retries may temporarily return a shorter list; already seen steps remain. */
export function mergeTimeline<T extends TimelineEvent>(previous:T[],incoming:T[]):T[]{
 const items=new Map(previous.map(item=>[item.id,item]));for(const item of incoming){const old=items.get(item.id);items.set(item.id,old&&old.kind===item.kind&&old.title===item.title&&old.content===item.content&&old.status===item.status&&old.createdAtMs===item.createdAtMs&&old.finishedAtMs===item.finishedAtMs&&timingKey(old.modelTiming)===timingKey(item.modelTiming)?old:item);}
 return [...items.values()].sort((a,b)=>a.createdAtMs===b.createdAtMs?a.id.localeCompare(b.id):a.createdAtMs<b.createdAtMs?-1:1);
}
export function timelineEntries<T extends TimelineEvent>(events:T[]){return events.filter(event=>['summary','tool','audit'].includes(event.kind));}
export function tracePerformance(events:TimelineEvent[]){
 const calls=events.filter(event=>event.kind==='model'&&event.modelTiming).map(event=>event.modelTiming!);
 if(!calls.length)return null;
 const result={preparation:0,headers:0,firstByte:0,stream:0,tools:0,audits:0,requests:calls.length};
 for(const call of calls){const total=Number(call.modelMs),headers=Number(call.headersMs??call.modelMs),first=Number(call.firstByteMs??call.modelMs);result.preparation+=Number(call.preparationMs);result.headers+=headers;result.firstByte+=Math.max(0,first-headers);result.stream+=Math.max(0,total-first);}
 for(const event of events){if(event.finishedAtMs<=0n)continue;const duration=Math.max(0,Number(event.finishedAtMs-event.createdAtMs));if(event.kind==='tool'&&event.title!=='任务验收')result.tools+=duration;if(event.kind==='audit')result.audits+=duration;}
 const firstText=calls.find(call=>call.firstTextMs!==undefined)?.firstTextMs;
 return {...result,firstText:firstText===undefined?undefined:Number(firstText)};
}
export function waitingForModel(events:TimelineEvent[]){return events.some(event=>event.kind==='model'&&event.status==='running');}
export function traceWaitingSeconds(wakeAt:number,now:number,status?:string){if(!Number.isFinite(wakeAt)||['paused','failed','canceled','succeeded','completed'].includes(status||''))return 0;return Math.max(0,Math.ceil((wakeAt-now)/1000));}
export function splitToolRecord(content:string){const match=content.match(/^输入\n([\s\S]*?)\n\n输出\n([\s\S]*)$/);if(match)return{input:match[1],output:match[2]};if(content.startsWith('输入\n'))return{input:content.slice(3),output:''};return{input:'',output:content};}
type LoaderOptions<T>={enabled:boolean;poll:boolean;request:(signal:AbortSignal)=>Promise<T>;onData:(data:T)=>void;onError:(error:unknown)=>void;visible?:()=>boolean;delayMs?:number};
/** Serial requests keep the one active run from occupying multiple HTTP/1 slots. */
export function createTraceLoader<T>(options:LoaderOptions<T>){
 let disposed=false,controller:AbortController|null=null,timer:ReturnType<typeof setTimeout>|null=null;
 const visible=options.visible||(()=>true),delay=options.delayMs??1000;
 const refresh=async()=>{if(disposed||controller||!options.enabled||!visible())return;controller=new AbortController();const current=controller;
  try{const data=await options.request(current.signal);if(!disposed&&!current.signal.aborted)options.onData(data);}
  catch(error){if(!disposed&&!current.signal.aborted)options.onError(error);}
  finally{if(controller===current)controller=null;if(!disposed&&options.poll&&visible())timer=setTimeout(()=>{timer=null;void refresh();},delay);}
 };
 if(options.enabled)void refresh();
 return{refresh(){if(timer){clearTimeout(timer);timer=null;}void refresh();},dispose(){disposed=true;if(timer)clearTimeout(timer);controller?.abort();}};
}
