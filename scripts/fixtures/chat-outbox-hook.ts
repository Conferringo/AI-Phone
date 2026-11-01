import {mock} from 'bun:test';
import assert from 'node:assert/strict';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const deny=()=>{throw Error('fixture_unexpected_external_operation');};
const processes={spawn:deny,spawnSync:deny,exec:deny,execSync:deny,execFile:deny,execFileSync:deny,fork:deny};
mock.module('node:child_process',()=>processes);mock.module('child_process',()=>processes);
for(const name of ['node:http','http','node:https','https'])mock.module(name,()=>({request:deny,get:deny,createServer:deny}));
for(const name of ['node:net','net'])mock.module(name,()=>({connect:deny,createConnection:deny,createServer:deny}));
for(const name of ['node:dns','dns'])mock.module(name,()=>({lookup:deny,resolve:deny,promises:{lookup:deny,resolve:deny}}));
for(const name of ['node:dns/promises','dns/promises'])mock.module(name,()=>({lookup:deny,resolve:deny}));
Bun.spawn=deny;Bun.spawnSync=deny;Bun.connect=deny;Bun.listen=deny;
globalThis.fetch=deny;globalThis.WebSocket=deny as never;globalThis.EventSource=deny as never;
const root=process.env.AI_PHONE_TEST_SOURCE_ROOT!,candidate=process.env.AI_PHONE_TEST_CANDIDATE_ROOT!;
assert(root&&candidate&&process.env.AI_PHONE_TEST_PROTOBUF);
const load=(base:string,path:string)=>import(pathToFileURL(join(base,path)).href);
const protobuf=await import(pathToFileURL(process.env.AI_PHONE_TEST_PROTOBUF!).href);
mock.module('@bufbuild/protobuf',()=>protobuf);
const assistant=await load(root,'packages/protocol/src/gen/ai_phone/v1/assistant_pb.ts');
const system=await load(root,'packages/protocol/src/gen/ai_phone/v1/system_pb.ts');
mock.module('@ai-phone/protocol/ai_phone/v1/assistant_pb',()=>assistant);
mock.module('@ai-phone/protocol/ai_phone/v1/system_pb',()=>system);
const outboxModule=await import(pathToFileURL(process.env.AI_PHONE_TEST_OUTBOX||join(candidate,'apps/web/src/lib/chat-outbox.ts')).href);mock.module('@/lib/chat-outbox',()=>outboxModule);
const wire=await load(root,'apps/web/src/lib/chat-wire.ts');mock.module('@/lib/chat-wire',()=>wire);
const {create,fromBinary,toBinary}=protobuf;
const {ChatStateSchema,ChatMessageSchema,ChatSendRequestSchema,ChatHistoryRequestSchema}=assistant;
const {acceptSnapshot,acceptInputReceipt}=await load(candidate,'apps/web/src/lib/chat-snapshot.ts');

// Only the primitives used by this hook are modeled; these are commit/disposal tests, not DOM tests.
let slots:any[]=[],cursor=0,layout:Array<()=>void>=[],passive:Array<()=>void>=[],mounted=true,lateWrites=0;
const sameDeps=(a:any[]|undefined,b:any[])=>!!a&&a.length===b.length&&a.every((v,i)=>Object.is(v,b[i]));
const effect=(queue:Array<()=>void>,create:()=>void|(()=>void),deps:any[])=>{
 const index=cursor++,slot=slots[index]||(slots[index]={});
 if(!sameDeps(slot.deps,deps))queue.push(()=>{slot.cleanup?.();slot.cleanup=create();slot.deps=deps;});
};
mock.module('react',()=>({
 useRef:(value:any)=>{const index=cursor++;return slots[index]||(slots[index]={current:value});},
 useState:(value:any)=>{const index=cursor++;if(!(index in slots))slots[index]=value;return[slots[index],(next:any)=>{if(!mounted){lateWrites++;return;}slots[index]=typeof next==='function'?next(slots[index]):next;}];},
 useCallback:(value:any,deps:any[])=>{const index=cursor++,slot=slots[index];if(!slot||!sameDeps(slot.deps,deps))slots[index]={deps,value};return slots[index].value;},
 useLayoutEffect:(create:any,deps:any[])=>effect(layout,create,deps),
 useEffect:(create:any,deps:any[])=>effect(passive,create,deps),
}));
const {useChatOutbox}=await import(pathToFileURL(process.env.AI_PHONE_TEST_HOOK||join(candidate,'apps/web/src/components/use-chat-outbox.ts')).href);
const disk=new Map<string,string>();
let storageFailure:((value:string)=>boolean)|null=null;
Object.defineProperty(globalThis,'localStorage',{value:{getItem:(key:string)=>disk.get(key)??null,setItem:(key:string,value:string)=>{if(storageFailure?.(value))throw Error('fixture_storage_full');disk.set(key,value);}},configurable:true});
const binding={conversationId:'conversation',deviceId:'phone'};
const state=(extra:any={})=>create(ChatStateSchema,{...binding,runId:'current-run',runStatus:'succeeded',revision:2n,messages:[{id:'assistant-current',role:'assistant',text:'current answer',ordinal:2n}],...extra});
let snapshot={chat:state(),order:20},generation=20,callbackFails=false;
const begin=()=>++generation;
const accepted=(chat:any,order:number)=>{snapshot=acceptSnapshot(snapshot,chat,order);};
const acknowledged=(receipt:any,order:number)=>{if(callbackFails)throw Error('fixture_projection_failed');snapshot=acceptInputReceipt(snapshot,receipt,order);};
let view:any;
function render(next=binding){cursor=0;view=useChatOutbox(next.conversationId,next.deviceId,accepted,begin,acknowledged);return view;}
function commitLayout(){const next=layout;layout=[];for(const fn of next)fn();}
function commitPassive(){const next=passive;passive=[];for(const fn of next)fn();}
function mount(){render();commitLayout();commitPassive();render();}
function unmount(){for(const slot of slots)if(slot?.cleanup)slot.cleanup();mounted=false;}
function remount(){slots=[];layout=[];passive=[];mounted=true;mount();}
const flush=async()=>{for(let i=0;i<60;i++)await Promise.resolve();};
const until=async(predicate:()=>boolean)=>{for(let i=0;i<1000&&!predicate();i++)await Promise.resolve();assert(predicate(),'fixture promise did not settle');};
const input=(id:string)=>({id,...binding,text:id,attachments:[]});
const bytes=(schema:any,value:any,status=200)=>new Response(toBinary(schema,create(schema,value)),{status,headers:{'content-type':'application/x-protobuf'}});
const sends:any[]=[],histories:any[]=[],ledger=new Map<string,any>();
let sendMode='normal',holdSend:((response:Response)=>void)|null=null;
globalThis.fetch=async(url:any,init:any)=>{
 assert.equal(init.method,'POST');assert.equal(init.headers['content-type'],'application/x-protobuf');
 if(url==='/api/assistant/send'){
  const body=new Uint8Array(init.body),request=fromBinary(ChatSendRequestSchema,body);sends.push({request,body});
  const old=ledger.get(request.requestId);
  if(old)assert.deepEqual(body,old.body,'retry must preserve exact encoded body');
  const message=old?.message||create(ChatMessageSchema,{id:'message-'+request.requestId,requestId:request.requestId,role:'user',text:request.text,ordinal:BigInt(ledger.size+3)});
  ledger.set(request.requestId,{body:body.slice(),message});
  const receipt=state({...request,revision:999n,runId:'old-run',runStatus:'running',error:'old error',messages:[{id:'old-assistant',role:'assistant',text:'obsolete',ordinal:1n},message]});
  const response=bytes(ChatStateSchema,receipt,202);
  if(sendMode==='unknown'){sendMode='normal';throw Error('fixture_unknown_post');}
  if(sendMode==='hold')return new Promise<Response>(resolve=>{holdSend=resolve;});
  if(sendMode==='wrong-scope')return bytes(ChatStateSchema,state({deviceId:'other',messages:[message]}),202);
  if(sendMode==='missing-receipt')return bytes(ChatStateSchema,state({messages:[]}),202);
  if(sendMode==='invalid-status')return bytes(ChatStateSchema,receipt,200);
  if(sendMode==='invalid-media')return new Response(toBinary(ChatStateSchema,receipt),{status:202,headers:{'content-type':'application/json'}});
  if(sendMode==='duplicate-receipt')return bytes(ChatStateSchema,state({messages:[message,message]}),202);
  if(sendMode==='empty-id')return bytes(ChatStateSchema,state({messages:[{...message,id:''}]}),202);
  if(sendMode==='zero-ordinal')return bytes(ChatStateSchema,state({messages:[{...message,ordinal:0n}]}),202);
  if(sendMode==='parameter-media')return new Response(toBinary(ChatStateSchema,receipt),{status:202,headers:{'content-type':'Application/X-Protobuf; charset=binary'}});
  return response;
 }
 if(url==='/api/assistant/history'){
  const request=fromBinary(ChatHistoryRequestSchema,new Uint8Array(init.body));
  return new Promise<Response>(resolve=>histories.push({request,signal:init.signal,resolve}));
 }
 throw Error('fixture_unexpected_request');
};
const history=(index:number,value:any,status=200)=>histories[index].resolve(bytes(status===200?ChatStateSchema:system.ApiErrorSchema,value,status));
const saved=()=>JSON.parse(disk.get('ai-phone:outbox:phone:conversation')||'[]');
const users=()=>snapshot.chat.messages.filter((m:any)=>m.role==='user').map((m:any)=>m.requestId);
const visibleInputs=()=>{render();return[...users(),...view.entries.filter((item:any)=>!users().includes(item.id)).map((item:any)=>item.id)];};
const canonical=()=>state({revision:BigInt(ledger.size+2),messages:[snapshot.chat.messages[0],...Array.from(ledger.values(),value=>value.message)]});
const mode=process.argv[2];
if(mode==='slow-history'){
 mount();for(const id of ['one','two','three'])view.submit(input(id));
 await until(()=>sends.length===3&&saved().length===0);render();
 assert.deepEqual(users(),['one','two','three']);assert.equal(histories.length,3);assert.equal(view.entries.length,0);
 assert(histories[0].signal.aborted&&histories[1].signal.aborted&&!histories[2].signal.aborted);
 assert.equal(snapshot.chat.runId,'current-run');assert.equal(snapshot.chat.runStatus,'succeeded');assert.equal(snapshot.chat.error,'');assert.equal(snapshot.chat.revision,5n);
 assert.equal(snapshot.chat.messages[0].text,'current answer');unmount();
}else if(mode==='history-failure'){
 mount();view.submit(input('one'));await until(()=>histories.length===1&&saved().length===0);
 history(0,{message:'fixture_history_failed'},503);await flush();render();assert.equal(view.error,'fixture_history_failed');assert.deepEqual(view.entries,[]);
 view.submit(input('two'));await until(()=>sends.length===2&&saved().length===0);assert.deepEqual(users(),['one','two']);
 history(1,state({messages:snapshot.chat.messages,revision:4n}));await flush();render();assert.equal(view.error,'');
 view.retry();await until(()=>histories.length===3);assert.equal(sends.length,2);histories[2].resolve(new Response(Uint8Array.from([255])));await flush();render();assert(view.error);assert.deepEqual(view.entries,[]);
 view.retry();await until(()=>histories.length===4);history(3,state({messages:snapshot.chat.messages,revision:4n}));await flush();render();assert.equal(view.error,'');assert.equal(sends.length,2);unmount();
}else if(mode==='projection-order'){
 mount();const before=snapshot,oldOrder=begin();view.submit(input('one'));await until(()=>histories.length===1&&saved().length===0);
 const after=snapshot;snapshot=acceptSnapshot(snapshot,state({revision:3n,messages:before.chat.messages}),oldOrder);assert.equal(snapshot,after,'ACK watermark must reject an older equal-revision poll');
 view.submit(input('two'));await until(()=>histories.length===2&&saved().length===0);
 history(1,state({revision:4n,messages:snapshot.chat.messages,runStatus:'running'}));await flush();assert.equal(snapshot.chat.runStatus,'running','equal-revision newer history must remain applicable');
 history(0,state({revision:4n,messages:[],runStatus:'failed'}));await flush();assert.equal(snapshot.chat.runStatus,'running');assert.deepEqual(users(),['one','two']);unmount();
}else if(mode==='binding-dispose'){
 mount();view.submit(input('one'));await until(()=>histories.length===1&&saved().length===0);const old=snapshot;
 const other={conversationId:'other-conversation',deviceId:'computer'};render(other);assert.equal(view.ready,false);assert.deepEqual(view.entries,[]);assert.throws(()=>view.submit({...input('new'),...other}),/连接/);
 commitLayout();history(0,state({revision:9n,runStatus:'failed'}));await flush();assert.equal(snapshot,old);commitPassive();render(other);assert.equal(view.ready,true);
 view.retry();await until(()=>histories.length===2);unmount();history(1,{...state(),...other,revision:10n});await flush();assert.equal(snapshot,old);assert.equal(lateWrites,0);
}else if(mode==='unknown-post'){
 mount();sendMode='unknown';view.submit(input('one'));view.submit(input('two'));await until(()=>{render();return view.entries[0]?.status==='failed';});
 assert.equal(sends.length,1);assert.equal(histories.length,0);assert.deepEqual(saved().map((m:any)=>m.id),['one','two']);
 view.retry();await until(()=>sends.length===3&&saved().length===0);assert.deepEqual(sends.map(s=>s.request.requestId),['one','one','two']);assert.equal(ledger.size,2);assert.deepEqual(users(),['one','two']);unmount();
}else if(mode==='late-post'){
 mount();sendMode='hold';view.submit(input('retained'));await until(()=>holdSend!==null);unmount();
 holdSend!(bytes(ChatStateSchema,state({messages:[ledger.get('retained').message]}),202));await flush();assert.equal(saved()[0].id,'retained');assert.deepEqual(users(),[]);assert.equal(lateWrites,0);
 sendMode='normal';remount();await until(()=>sends.length===2&&saved().length===0);
 assert.deepEqual(sends.map(s=>s.request.requestId),['retained','retained']);assert.deepEqual(users(),['retained']);assert.equal(ledger.size,1);unmount();
}else if(mode==='projection-callback-failure'){
 mount();callbackFails=true;for(const id of ['one','two','three'])view.submit(input(id));await until(()=>sends.length===3&&histories.length===3);render();
 assert.deepEqual(view.entries.map((item:any)=>item.status),['accepted','accepted','accepted']);assert(view.entries.every((item:any)=>item.error==='fixture_projection_failed'));
 assert.deepEqual(saved().map((item:any)=>item.status),['accepted','accepted','accepted']);assert.deepEqual(visibleInputs(),['one','two','three']);
 history(2,{message:'fixture_history_failed'},503);await flush();render();assert.equal(view.error,'fixture_history_failed');assert.deepEqual(visibleInputs(),['one','two','three']);
 unmount();callbackFails=false;remount();await until(()=>histories.length===4);assert.equal(sends.length,3);assert.deepEqual(visibleInputs(),['one','two','three']);
 history(3,canonical());await until(()=>saved().length===0);assert.deepEqual(visibleInputs(),['one','two','three']);assert.equal(sends.length,3);unmount();
}else if(mode==='invalid-receipt'){
 mount();sendMode='wrong-scope';view.submit(input('one'));await until(()=>{render();return view.entries[0]?.status==='failed';});assert.equal(saved()[0].id,'one');assert.equal(histories.length,0);assert.deepEqual(users(),[]);
 sendMode='missing-receipt';view.retry();await until(()=>sends.length===2);await flush();render();assert.equal(view.entries[0].status,'failed');assert.equal(saved()[0].id,'one');assert.deepEqual(users(),[]);unmount();
}else if(mode==='accepted-storage-fault'){
 mount();storageFailure=value=>JSON.parse(value).some((item:any)=>item.status==='accepted');view.submit(input('one'));view.submit(input('two'));
 await until(()=>{render();return view.entries[0]?.status==='accepted'&&view.entries[0]?.error==='fixture_storage_full';});
 assert.deepEqual(visibleInputs(),['one','two']);assert.equal(saved()[0].id,'one');assert.equal(saved()[0].text,'one');assert.equal(sends.length,1);
 storageFailure=null;view.retry();await until(()=>sends.length===2&&histories.length===2);assert.deepEqual(sends.map(s=>s.request.requestId),['one','two']);
 history(histories.length-1,canonical());await until(()=>saved().length===0);assert.deepEqual(visibleInputs(),['one','two']);unmount();
}else if(mode==='acceptance-write-reload'){
 mount();storageFailure=value=>JSON.parse(value).some((item:any)=>item.status==='accepted');view.submit(input('one'));
 await until(()=>{render();return view.entries[0]?.status==='accepted'&&view.entries[0]?.error==='fixture_storage_full';});assert.equal(saved()[0].id,'one');
 unmount();storageFailure=null;remount();await until(()=>sends.length===2&&saved().length===0);
 assert.deepEqual(sends.map(s=>s.request.requestId),['one','one']);assert.equal(ledger.size,1);assert.deepEqual(visibleInputs(),['one']);unmount();
}else if(mode==='projection-cleanup-fault'){
 mount();storageFailure=value=>value==='[]';view.submit(input('one'));await until(()=>{render();return view.entries[0]?.status==='accepted'&&view.entries[0]?.error==='fixture_storage_full';});
 assert.equal(saved()[0].status,'accepted');assert.deepEqual(visibleInputs(),['one']);unmount();storageFailure=null;remount();await until(()=>histories.length===2);assert.equal(sends.length,1);
 history(1,canonical());await until(()=>saved().length===0);assert.deepEqual(visibleInputs(),['one']);unmount();
}else if(['invalid-status','invalid-media','duplicate-receipt'].includes(mode)){
 mount();sendMode=mode;view.submit(input('one'));await until(()=>{render();return view.entries[0]?.status==='failed';});assert.equal(saved()[0].id,'one');assert.deepEqual(users(),[]);assert.equal(histories.length,0);unmount();
}else if(mode==='invalid-user-receipt'){
 mount();sendMode='empty-id';view.submit(input('one'));await until(()=>{render();return view.entries[0]?.status==='failed';});assert.equal(saved()[0].id,'one');
 sendMode='zero-ordinal';view.retry();await until(()=>sends.length===2);await flush();render();assert.equal(view.entries[0].status,'failed');assert.deepEqual(users(),[]);unmount();
}else if(mode==='history-contract'){
 mount();sendMode='parameter-media';view.submit(input('one'));await until(()=>histories.length===1&&saved().length===0);assert.deepEqual(users(),['one']);
 histories[0].resolve(bytes(ChatStateSchema,canonical(),202));await flush();render();assert(view.error);view.retry();await until(()=>histories.length===2);
 histories[1].resolve(new Response(toBinary(ChatStateSchema,canonical()),{headers:{'content-type':'text/plain'}}));await flush();render();assert(view.error);assert.equal(sends.length,1);
 view.retry();await until(()=>histories.length===3);history(2,canonical());await flush();render();assert.equal(view.error,'');unmount();
}else if(mode==='accepted-failed-fifo'){
 mount();callbackFails=true;view.submit(input('one'));await until(()=>histories.length===1);callbackFails=false;sendMode='unknown';view.submit(input('two'));view.submit(input('three'));
 await until(()=>{render();return view.entries[1]?.status==='failed';});assert.deepEqual(view.entries.map((item:any)=>item.status),['accepted','failed','queued']);assert.equal(sends.length,2);assert.deepEqual(visibleInputs(),['one','two','three']);
 view.retry();await until(()=>sends.length===4&&histories.length===4);assert.deepEqual(sends.map(s=>s.request.requestId),['one','two','two','three']);
 history(histories.length-1,canonical());await until(()=>saved().length===0);assert.deepEqual(visibleInputs(),['one','two','three']);unmount();
}else if(mode==='history-proof'){
 mount();callbackFails=true;view.submit(input('one'));await until(()=>histories.length===1);callbackFails=false;
 history(0,state({revision:3n,messages:[{id:'assistant',role:'assistant',requestId:'one',ordinal:3n}]}));await flush();assert.equal(saved()[0].status,'accepted');assert.deepEqual(visibleInputs(),['one']);
 view.retry();await until(()=>histories.length===2);history(1,state({revision:3n,messages:[{id:'user',role:'user',requestId:'one',ordinal:0n}]}));await flush();assert.equal(saved()[0].status,'accepted');
 view.retry();await until(()=>histories.length===3);history(2,{...canonical(),deviceId:'other'});await flush();assert.equal(saved()[0].status,'accepted');render();assert(view.error);
 view.retry();await until(()=>histories.length===4);history(3,state({revision:3n,messages:[ledger.get('one').message]}));await until(()=>saved().length===0);assert.equal(sends.length,1);assert.deepEqual(visibleInputs(),['one']);unmount();
}else if(mode==='snapshot-contract'){
 const message=create(ChatMessageSchema,{id:'message',requestId:'request',role:'user',text:'immutable',ordinal:3n});
 const receipt={...binding,message};const next=acceptInputReceipt(snapshot,receipt,21);assert.equal(next.chat.runId,'current-run');assert.equal(next.chat.revision,3n);
 message.text='mutated';assert.equal(next.chat.messages[1].text,'immutable');
 const duplicate=acceptInputReceipt(next,receipt,22);assert.equal(duplicate.chat,next.chat);assert.equal(duplicate.order,22);
 assert.equal(acceptInputReceipt(next,{...receipt,deviceId:'other'},23),next);
 assert.equal(acceptInputReceipt(next,{...receipt,message:{...message,role:'assistant'}},23),next);
 const later=acceptSnapshot(duplicate,state({revision:3n,runStatus:'running',messages:next.chat.messages}),23);assert.equal(later.chat.runStatus,'running');
 assert.equal(acceptSnapshot(later,state({revision:3n,runStatus:'failed',messages:next.chat.messages}),21),later);
}else throw Error('fixture_unknown_case');
console.log('projection fixture passed');
