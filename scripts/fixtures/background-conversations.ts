// Actual PhoneWorkspace with modeled React commit/timers; no DOM/device/model claim.
import {mock} from 'bun:test';
import {plugin} from 'bun';
import assert from 'node:assert/strict';
import {readFileSync,existsSync} from 'node:fs';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const deny=()=>{throw Error('fixture_unexpected_external_operation');};
for(const name of ['node:http','http','node:https','https'])mock.module(name,()=>({request:deny,get:deny,createServer:deny}));
for(const name of ['node:net','net'])mock.module(name,()=>({connect:deny,createConnection:deny,createServer:deny}));
mock.module('node:child_process',()=>({spawn:deny,spawnSync:deny,exec:deny,execSync:deny,execFile:deny,execFileSync:deny,fork:deny}));
Bun.spawn=deny;Bun.spawnSync=deny;Bun.connect=deny;Bun.listen=deny;
globalThis.fetch=deny;globalThis.WebSocket=deny as never;globalThis.EventSource=deny as never;
const source=process.env.AI_PHONE_TEST_SOURCE_ROOT!;
const candidate=process.env.AI_PHONE_TEST_CANDIDATE_ROOT||source,windowMode=process.argv[2]==='window';
const consolePath=process.env.AI_PHONE_TEST_CONSOLE!;
assert(source&&consolePath&&process.env.AI_PHONE_TEST_PROTOBUF);
const load=(path:string)=>import(pathToFileURL(existsSync(join(candidate,path))?join(candidate,path):join(source,path)).href);
const protobuf=await import(pathToFileURL(process.env.AI_PHONE_TEST_PROTOBUF!).href);
mock.module('@bufbuild/protobuf',()=>protobuf);
const assistant=await load('packages/protocol/src/gen/ai_phone/v1/assistant_pb.ts');
const phone=await load('packages/protocol/src/gen/ai_phone/v1/phone_runtime_pb.ts');
const desktop=await load('packages/protocol/src/gen/ai_phone/v1/desktop_pb.ts');
const system=await load('packages/protocol/src/gen/ai_phone/v1/system_pb.ts');
for(const [name,module] of Object.entries({assistant_pb:assistant,phone_runtime_pb:phone,desktop_pb:desktop,system_pb:system}))mock.module('@ai-phone/protocol/ai_phone/v1/'+name,()=>module);
for(const [alias,path] of Object.entries({
 '@/lib/chat-outbox':'apps/web/src/lib/chat-outbox.ts',
 '@/lib/chat-wire':'apps/web/src/lib/chat-wire.ts',
 '@/lib/chat-snapshot':'apps/web/src/lib/chat-snapshot.ts',
 '@/lib/device-focus':'apps/web/src/lib/device-focus.ts',
 '@/lib/chat-window':'apps/web/src/lib/chat-window.ts',
 '@/lib/chat-history-reader':'apps/web/src/lib/chat-history-reader.ts',
})){const module=await load(path);mock.module(alias,()=>module);}
// Audio transport has a separate real-hook suite; this fixture isolates conversation ownership.
mock.module('@/components/device-audio',()=>({useDeviceAudio:()=>({enabled:false,volume:1,error:'',busy:false,toggle:async()=>{},setVolume:()=>{}})}));
const stub=()=>null;
const components:Record<string,string[]>={
 'language-window':['LanguageWindow','LanguageTrigger'],
 'workbench-notices':['WorkbenchNoticeProvider','WorkbenchNotices','DeviceNoticeScope','useWorkbenchNotice'],
 'device-defaults':['DeviceDefaultsProvider'],
 'ui/button':['Button'],'ui/dialog':['Dialog','DialogContent','DialogTitle'],
 'device-cable-line':['DeviceCableLine'],'skills-window':['SkillsWindow'],
 'location-window':['LocationWindow'],'knowledge-window':['KnowledgeWindow'],
 'workspace-window':['WorkspaceWindow'],'execution-trace':['ExecutionTrace'],
 'proactive-controls':['ProactiveProvider','ExplorationToggle'],'registry-window':['RegistryWindow'],
 'copilot':['CopilotProvider','CopilotToggle','useCopilotBinding'],
 'floating-window':['FloatingWindow','WindowTrigger'],'device-cable':['DeviceCable'],
 'desktop-surface':['DesktopSurface'],'phone-stream-image':['PhoneStreamImage'],'phone-toolbar':['PhoneToolbar'],
};
for(const [path,names] of Object.entries(components))mock.module('@/components/'+path,()=>Object.fromEntries(names.map(name=>[name,stub])));
mock.module('@/components/device-defaults',()=>({DeviceDefaultsProvider:stub,creationDefaults:{}}));
mock.module('@/components/locale-provider',()=>({useI18n:()=>({t:(text:string)=>text})}));
mock.module('@/components/workbench-motion',()=>({useWorkbenchMotion:()=>stub}));
mock.module('@/components/desktop-exploration',()=>({useDesktopExploration:()=>({}),DesktopExploreToggle:stub}));
mock.module('@/lib/client-id',()=>({clientId:()=>crypto.randomUUID()}));
mock.module('react-markdown',()=>({default:stub}));mock.module('remark-gfm',()=>({default:stub}));
mock.module('lucide-react',()=>Object.fromEntries('ArrowUp ArrowLeft ArrowUpRight Plus Check Paperclip Square Play X LoaderCircle Smartphone Monitor MessageSquare Wrench BookOpen FolderOpen RefreshCw Settings Cable MapPin'.split(' ').map(name=>[name,stub])));

type Instance={slots:any[];cursor:number;layout:Array<()=>void>;passive:Array<()=>void>;dirty:boolean;props:any;tree:any};
let owner:Instance;
const depsEqual=(a:any[]|undefined,b:any[])=>!!a&&a.length===b.length&&a.every((v,i)=>Object.is(v,b[i]));
const effect=(kind:'layout'|'passive',create:()=>void|(()=>void),deps:any[])=>{
 const instance=owner,index=instance.cursor++,slot=instance.slots[index]||(instance.slots[index]={});
 if(!depsEqual(slot.deps,deps))instance[kind].push(()=>{slot.cleanup?.();slot.cleanup=create();slot.deps=deps;});
};
mock.module('react',()=>({
 useRef:(value:any)=>{const i=owner.cursor++;return owner.slots[i]||(owner.slots[i]={current:value});},
 useState:(value:any)=>{const instance=owner,i=instance.cursor++;if(!(i in instance.slots))instance.slots[i]=value;return[instance.slots[i],(next:any)=>{instance.slots[i]=typeof next==='function'?next(instance.slots[i]):next;instance.dirty=true;}];},
 useCallback:(value:any,deps:any[])=>{const i=owner.cursor++,slot=owner.slots[i];if(!slot||!depsEqual(slot.deps,deps))owner.slots[i]={deps,value};return owner.slots[i].value;},
 useMemo:(create:()=>any,deps:any[])=>{const i=owner.cursor++,slot=owner.slots[i];if(!slot||!depsEqual(slot.deps,deps))owner.slots[i]={deps,value:create()};return owner.slots[i].value;},
 useLayoutEffect:(create:any,deps:any[])=>effect('layout',create,deps),useEffect:(create:any,deps:any[])=>effect('passive',create,deps),
}));
const element=(type:any,props:any,key:any)=>({type,props,key});
mock.module('react/jsx-runtime',()=>({jsx:element,jsxs:element,Fragment:'fragment'}));
mock.module('react/jsx-dev-runtime',()=>({jsxDEV:element,Fragment:'fragment'}));
const outboxHook=await load('apps/web/src/components/use-chat-outbox.ts');mock.module('@/components/use-chat-outbox',()=>outboxHook);
const pagesHook=await load('apps/web/src/components/use-chat-history-pages.ts');mock.module('@/components/use-chat-history-pages',()=>pagesHook);
plugin({name:'private-workspace-export',setup(build){
 build.onLoad({filter:/phone-console\.tsx$/},args=>{assert.equal(args.path,consolePath);return{contents:readFileSync(args.path,'utf8')+'\nexport {PhoneWorkspace};\n',loader:'tsx'};});
 build.onResolve({filter:/\.css$/},args=>({path:args.path,namespace:'private-css'}));
 build.onLoad({filter:/.*/,namespace:'private-css'},()=>({contents:'export {};',loader:'js'}));
 build.onLoad({filter:/\.css$/},()=>({contents:'export {};',loader:'js'}));
}});
const {PhoneWorkspace}=await import(pathToFileURL(consolePath).href);

const {create,fromBinary,toBinary}=protobuf;
const {ChatStateSchema,ChatMessageSchema,ChatHistoryRequestSchema,ChatSendRequestSchema}=assistant;
const disk=new Map<string,string>();
Object.defineProperty(globalThis,'localStorage',{value:{getItem:(key:string)=>disk.get(key)??null,setItem:(key:string,value:string)=>disk.set(key,value)},configurable:true});
let clock=0,timerId=0;
const timers=new Map<number,{at:number;call:()=>void}>();
globalThis.setTimeout=((call:()=>void,ms=0)=>{const id=++timerId;timers.set(id,{at:clock+ms,call});return id;}) as never;
globalThis.clearTimeout=((id:number)=>{timers.delete(id);}) as never;
const visibility=new Set<()=>void>();
Object.defineProperty(globalThis,'document',{value:{visibilityState:'visible',addEventListener:(name:string,fn:()=>void)=>{assert.equal(name,'visibilitychange');visibility.add(fn);},removeEventListener:(name:string,fn:()=>void)=>{assert.equal(name,'visibilitychange');visibility.delete(fn);}},configurable:true});
const calls:Array<{url:string;deviceId?:string;signal?:AbortSignal}>=[];
const conversations=new Map<string,any>();
for(const id of ['phone-local-01','desktop-local-01']){
 const messages=windowMode&&id==='phone-local-01'?Array.from({length:70},(_,index)=>create(ChatMessageSchema,{id:'prior-'+(index+1),role:'user',text:'earlier '+(index+1),ordinal:BigInt(index+1)})).filter((message:any)=>message.ordinal!==15n):[];
 conversations.set(id,create(ChatStateSchema,{deviceId:id,conversationId:'conversation-'+id,agentId:'shared-agent',revision:messages.at(-1)?.ordinal||0n,messages}));
}
let holdHistory=false,heldHistory:{resolve:(response:Response)=>void;signal:AbortSignal}|null=null;
let heldSend:{resolve:(response:Response)=>void;signal:AbortSignal;chat:any}|null=null;
const response=(schema:any,value:any,status=200)=>new Response(toBinary(schema,value),{status,headers:{'content-type':'application/x-protobuf'}});
globalThis.fetch=async(url:any,init:any)=>{
 assert.equal(init.method,'POST');assert.equal(init.headers['content-type'],'application/x-protobuf');
 const observed={url,deviceId:undefined as string|undefined,signal:init.signal};calls.push(observed);
 if(url==='/api/assistant/history'){
  const request=fromBinary(ChatHistoryRequestSchema,new Uint8Array(init.body));
  observed.deviceId=request.deviceId;
  const chat=conversations.get(request.deviceId);assert(chat);
  assert(!request.conversationId||request.conversationId===chat.conversationId);
  if(holdHistory&&request.deviceId==='phone-local-01')return new Promise<Response>(resolve=>{assert.equal(heldHistory,null);heldHistory={resolve,signal:init.signal};});
  if(windowMode){
   assert.equal(request.limit,40,'Actual component must request a bounded history window');
   const anchor=request.beforeMessageId?chat.messages.find((m:any)=>m.id===request.beforeMessageId):null;
   assert(!request.beforeMessageId||anchor,'Page anchor must be one of this conversation messages');
   const eligible=anchor?chat.messages.filter((m:any)=>m.ordinal<anchor.ordinal):chat.messages;
   const page=eligible.slice(-request.limit),metadata=create(assistant.ChatMessageWindowSchema,{kind:anchor?assistant.ChatWindowKind.BEFORE:assistant.ChatWindowKind.TAIL,pageMessageIds:page.map((m:any)=>m.id),nextBeforeMessageId:page[0]?.id||'',hasOlder:eligible.length>page.length});
   return response(ChatStateSchema,create(ChatStateSchema,{...chat,messages:page,messageWindow:metadata}));
  }
  return response(ChatStateSchema,chat);
 }
 if(url==='/api/assistant/send'){
  const request=fromBinary(ChatSendRequestSchema,new Uint8Array(init.body));
  observed.deviceId=request.deviceId;
  assert.equal(request.deviceId,'phone-local-01');
  const prior=conversations.get(request.deviceId);assert.equal(request.conversationId,prior.conversationId);
  const ordinal=prior.revision+1n;
  const chat=create(ChatStateSchema,{...prior,runId:'phone-run',runStatus:'running',revision:ordinal,messages:[...prior.messages,create(ChatMessageSchema,{id:'user-message',role:'user',text:request.text,requestId:request.requestId,ordinal})]});
  conversations.set(request.deviceId,chat);
  return new Promise<Response>(resolve=>{heldSend={resolve,signal:init.signal,chat};});
 }
 if(url==='/api/phone/observe'){
  const request=fromBinary(phone.DeviceRequestSchema,new Uint8Array(init.body));
  observed.deviceId=request.phoneId;
  return response(phone.DeviceObservationSchema,create(phone.DeviceObservationSchema,{phoneId:request.phoneId,width:720,height:1440,png:new Uint8Array()}));
 }
 throw Error('fixture_unexpected_request:'+url);
};
const registry={devices:['phone-local-01','desktop-local-01'].map((id,i)=>({id,kind:i?'desktop':'phone',runtimeState:'managed',defaultAgentId:'shared-agent',name:id})),agents:[{id:'shared-agent',name:'same agent'}]};
const stateViews=new Map<string,any>();
const onState=(id:string,value:any)=>{stateViews.set(id,value);};
const props=(deviceId:string,focused:boolean)=>({deviceId,registry,focused,current:focused,showChat:focused,linked:false,stageStyle:{},onSelect:stub,screenColumn:1,chatColumn:2,chatRow:1,onActivate:stub,onState,onOrientation:stub,registerRefresh:stub,feature:'',setFeature:stub});
const instances=['phone-local-01','desktop-local-01'].map((id,i):Instance=>({slots:[],cursor:0,layout:[],passive:[],dirty:true,props:props(id,!i),tree:null}));
function render(instance:Instance){owner=instance;instance.cursor=0;instance.dirty=false;instance.tree=PhoneWorkspace(instance.props);for(const phase of ['layout','passive'] as const){const queue=instance[phase];instance[phase]=[];for(const call of queue)call();}}
async function flush(){for(let cycle=0;cycle<80;cycle++){for(const instance of instances)if(instance.dirty)render(instance);await Promise.resolve();}}
async function tick(ms:number){clock+=ms;const due=[...timers].filter(([,timer])=>timer.at<=clock);for(const [id,timer] of due){timers.delete(id);timer.call();}await flush();}
function find(node:any,predicate:(node:any)=>boolean):any{if(!node||typeof node!=='object')return;if(Array.isArray(node)){for(const child of node){const found=find(child,predicate);if(found)return found;}return;}if(predicate(node))return node;return find(node.props?.children,predicate);}
function focusDesktop(){instances[0].props={...instances[0].props,focused:false,current:false,showChat:false};instances[1].props={...instances[1].props,focused:true,current:true,showChat:true};for(const instance of instances)instance.dirty=true;}
await flush();
const phoneInstance=instances[0];
if(windowMode){
 const older=find(phoneInstance.tree,node=>node.props?.children?.some?.((value:any)=>value==='较早消息')||node.props?.children==='较早消息');
 assert(older,'The bounded initial tail must have a working older-history entry');older.props.onClick();await flush();
 assert(find(phoneInstance.tree,node=>node.props?.['data-message-id']==='prior-1'),'Loading an older page must render a retained earlier row');
 assert(find(phoneInstance.tree,node=>node.props?.['data-message-id']==='prior-70'),'Loading an older page must retain the newer tail');
 const server=conversations.get('phone-local-01');server.messages.push(create(ChatMessageSchema,{id:'late-15',role:'user',text:'late lower ordinal',ordinal:15n}));server.messages.sort((a:any,b:any)=>a.ordinal<b.ordinal?-1:1);
 const scrolling=find(phoneInstance.tree,node=>node.props?.className==='message-scroll');
 const element={scrollTop:100,scrollHeight:2000,clientHeight:500,getBoundingClientRect:()=>({left:0,bottom:500,width:600})};
 scrolling.props.ref.current=element;document.elementFromPoint=(()=>({closest:()=>({dataset:{messageId:'prior-20'}})})) as never;
 scrolling.props.onScroll();await flush();
 assert(find(phoneInstance.tree,node=>node.props?.['data-message-id']==='late-15'),'Revisiting an older viewport must discover a delayed lower-ordinal commit');
 const before=calls.filter(call=>call.url==='/api/assistant/history').length;scrolling.props.onScroll();await flush();assert.equal(calls.filter(call=>call.url==='/api/assistant/history').length,before,'Repeated scroll in the same window is throttled');
 scrolling.props.ref.current=null;

}

find(phoneInstance.tree,node=>node.type==='textarea'&&node.props['aria-label']==='消息').props.onChange({target:{value:'Continue phone work in background'}});
await flush();
find(phoneInstance.tree,node=>node.type==='form'&&node.props.className==='chat-composer').props.onSubmit({preventDefault:stub});
await flush();assert(heldSend,'Actual composer must send a bound phone request');
focusDesktop();await flush();
assert(!heldSend.signal.aborted,'Changing focus must not abort an in-flight accepted-input transport');
heldSend.resolve(response(ChatStateSchema,heldSend.chat,202));await flush();
assert.equal(stateViews.get('phone-local-01').runId,'phone-run');
assert.equal(stateViews.get('desktop-local-01').runId,'','A shared Agent must not reuse the other device run');
holdHistory=true;await tick(700);
assert(heldHistory,'A running background conversation must still poll after focus changes');
// Switch twice while the exact phone history request is held.
instances[0].props={...instances[0].props,focused:true,current:true,showChat:true};instances[1].props={...instances[1].props,focused:false,current:false,showChat:false};for(const instance of instances)instance.dirty=true;
await flush();focusDesktop();await flush();
assert(!heldHistory.signal.aborted,'Device focus changes must not cancel the history owner');
const finished=create(ChatStateSchema,{...conversations.get('phone-local-01'),runStatus:'succeeded',revision:conversations.get('phone-local-01').revision+1n,messages:[...conversations.get('phone-local-01').messages,create(ChatMessageSchema,{id:'answer',role:'assistant',text:'phone finished',runId:'phone-run',ordinal:conversations.get('phone-local-01').revision+1n})]});
conversations.set('phone-local-01',finished);holdHistory=false;heldHistory.resolve(response(ChatStateSchema,finished));heldHistory=null;await flush();
assert.equal(stateViews.get('phone-local-01').status,'succeeded');
assert.equal(stateViews.get('phone-local-01').reply,'phone finished');
assert.equal(stateViews.get('desktop-local-01').runId,'');
for(const [index,value] of ['phone draft','desktop draft'].entries())find(instances[index].tree,node=>node.type==='textarea'&&node.props['aria-label']==='消息').props.onChange({target:{value}});
await flush();
instances[0].props={...instances[0].props,focused:true,current:true,showChat:true};instances[1].props={...instances[1].props,focused:false,current:false,showChat:false};for(const instance of instances)instance.dirty=true;
await flush();focusDesktop();await flush();
for(const [index,value] of ['phone draft','desktop draft'].entries())assert.equal(find(instances[index].tree,node=>node.type==='textarea'&&node.props['aria-label']==='消息').props.value,value);
const historyCount=()=>calls.filter(call=>call.url==='/api/assistant/history').length;
const beforeHide=historyCount();document.visibilityState='hidden';for(const notify of visibility)notify();await tick(700);
assert.equal(historyCount(),beforeHide,'Hidden app pauses only projection requests');
document.visibilityState='visible';for(const notify of visibility)notify();await flush();
assert(historyCount()>beforeHide,'Visible app immediately refreshes its device histories');
assert(!calls.some(call=>/cancel|resume|control/.test(call.url)),'Focus must not send run cancellation or manual control');
for(const instance of instances)for(const slot of instance.slots)slot?.cleanup?.();
assert.equal(timers.size,0,'All modeled observation timers must be disposed');
assert.equal(visibility.size,0,'All modeled visibility listeners must be disposed');
console.log('background conversation fixture passed');
