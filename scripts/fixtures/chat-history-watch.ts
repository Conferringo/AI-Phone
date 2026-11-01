import {mock} from 'bun:test';
import assert from 'node:assert/strict';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';

const deny=()=>{throw Error('fixture_unexpected_external_operation');};
for(const name of ['node:child_process','child_process'])mock.module(name,()=>({spawn:deny,spawnSync:deny,exec:deny,execSync:deny,execFile:deny,execFileSync:deny,fork:deny}));
for(const name of ['node:http','http','node:https','https'])mock.module(name,()=>({request:deny,get:deny,createServer:deny}));
for(const name of ['node:net','net'])mock.module(name,()=>({connect:deny,createConnection:deny,createServer:deny}));
for(const name of ['node:dns','dns'])mock.module(name,()=>({lookup:deny,resolve:deny,promises:{lookup:deny,resolve:deny}}));
for(const name of ['node:dns/promises','dns/promises'])mock.module(name,()=>({lookup:deny,resolve:deny}));
Bun.spawn=deny;Bun.spawnSync=deny;Bun.connect=deny;Bun.listen=deny;
globalThis.fetch=deny;globalThis.WebSocket=deny as never;globalThis.EventSource=deny as never;
const source=process.env.AI_PHONE_TEST_SOURCE_ROOT!,candidate=process.env.AI_PHONE_TEST_CANDIDATE_ROOT!;
assert(source&&candidate&&process.env.AI_PHONE_TEST_PROTOBUF);
const load=(base:string,path:string)=>import(pathToFileURL(join(base,path)).href);
const protobuf=await import(pathToFileURL(process.env.AI_PHONE_TEST_PROTOBUF!).href);
mock.module('@bufbuild/protobuf',()=>protobuf);
const protocol=await load(source,'packages/protocol/src/gen/ai_phone/v1/assistant_pb.ts');
mock.module('@ai-phone/protocol/ai_phone/v1/assistant_pb',()=>protocol);
const errors=await load(source,'packages/protocol/src/gen/ai_phone/v1/system_pb.ts');
mock.module('@ai-phone/protocol/ai_phone/v1/system_pb',()=>errors);
const {readChatHistory}=await import(pathToFileURL(process.env.AI_PHONE_TEST_HISTORY_READER||join(candidate,'apps/web/src/lib/chat-history-reader.ts')).href);
const {acceptSnapshot}=await load(source,'apps/web/src/lib/chat-snapshot.ts');
const {create,toBinary,fromBinary}=protobuf;
const {ChatStateSchema,ChatMessageSchema,ChatHistoryRequestSchema,ChatMessageWindowSchema,ChatWindowKind}=protocol;
const id=(scope:number,index:number)=>`00000000-0000-4000-8000-${String(scope*1000+index).padStart(12,'0')}`;
function device(scope:number,deviceId:string){
 const conversationId=id(scope,900);
 const heads=Array.from({length:12},(_,index)=>create(ChatMessageSchema,{id:id(scope,index),runId:id(scope,index+100),role:'assistant',text:'waiting',ordinal:BigInt(index+1),active:true,hasActivity:true}));
 const tail=Array.from({length:40},(_,index)=>create(ChatMessageSchema,{id:id(scope,index+200),role:'user',text:'later input',ordinal:BigInt(index+100)}));
 const chat=create(ChatStateSchema,{conversationId,deviceId,runId:heads.at(-1).runId,runStatus:'paused',revision:139n,messages:[...heads,...tail]});
 const cell={current:chat},owner={conversation:{current:conversationId},fetching:{current:false},latest:cell,watchOffset:{current:0}};
 let snapshot:any={chat,order:0},order=0;
 const projection={begin:()=>++order,accept:(incoming:any,generation:number,before:string)=>{snapshot=acceptSnapshot(snapshot,incoming,generation,before);cell.current=snapshot.chat;},error:(value:string)=>assert.equal(value,'')};
 const canonical=new Map([...heads,...tail].map(message=>[message.id,message]));
 canonical.set(heads[0].id,{...heads[0],text:'final response from completed old notification',active:false});
 return{deviceId,conversationId,heads,tail,owner,projection,canonical,snapshot:()=>snapshot};
}
const phone=device(1,'phone-local-01'),desktop=device(2,'desktop-fixture');
const devices=new Map([phone,desktop].map(value=>[value.deviceId,value])),requests:any[]=[];
globalThis.fetch=async(url:any,init:any)=>{
 assert.equal(url,'/api/assistant/history');assert.equal(init.method,'POST');assert.equal(init.headers['content-type'],'application/x-protobuf');
 const request=fromBinary(ChatHistoryRequestSchema,new Uint8Array(init.body)),current=devices.get(request.deviceId)!;
 assert(current);assert.equal(request.conversationId,current.conversationId);assert.equal(request.limit,40);assert(request.watchMessageIds.length<=8);assert.deepEqual(request.receiptRequestIds,[]);
 requests.push(request);
 // Paused notifications are not a union lane: server supplements only the selected current header.
 const before=request.beforeMessageId?current.canonical.get(request.beforeMessageId)?.ordinal:undefined;
 assert(!request.beforeMessageId||before!==undefined);
 const page=current.tail.filter(message=>before===undefined||message.ordinal<before).slice(-40);
 const selected=new Map(page.map(message=>[message.id,current.canonical.get(message.id)]));
 const currentHead=current.heads.at(-1);selected.set(currentHead.id,current.canonical.get(currentHead.id));
 for(const watched of request.watchMessageIds){assert(current.canonical.has(watched));selected.set(watched,current.canonical.get(watched));}
 const window=create(ChatMessageWindowSchema,{kind:before===undefined?ChatWindowKind.TAIL:ChatWindowKind.BEFORE,pageMessageIds:page.map(message=>message.id),nextBeforeMessageId:page[0]?.id||'',hasOlder:true});
 const state=create(ChatStateSchema,{conversationId:current.conversationId,deviceId:current.deviceId,runId:currentHead.runId,runStatus:'paused',revision:139n,messages:[...selected.values()].sort((a,b)=>Number(a.ordinal-b.ordinal)),messageWindow:window});
 return new Response(toBinary(ChatStateSchema,state),{headers:{'content-type':'application/x-protobuf'}});
};
const mode=process.argv[2];
if(mode==='canceled'){
 const abort=new AbortController(),pending=readChatHistory(phone.owner,phone.deviceId,phone.projection,abort.signal);abort.abort();await pending;
 assert.equal(phone.snapshot().chat.messages[0].text,'waiting','Canceled read must not update the projected row');
}
for(let round=0;round<6;round++)await readChatHistory(phone.owner,phone.deviceId,phone.projection,undefined,mode==='page'&&round===1?phone.tail[20].id:'');
const oldest=phone.snapshot().chat.messages.find((message:any)=>message.id===phone.heads[0].id);
assert.equal(oldest.text,'final response from completed old notification','Normal history reads must eventually refresh an older loaded head even when the newest eight stay paused');
assert.equal(oldest.active,false,'Completed older head must stop appearing active');
assert.equal(phone.snapshot().chat.messages.length,52,'Watch rotation must retain the loaded history');
const phoneRequests=requests.filter(request=>request.deviceId===phone.deviceId),watched=new Set(phoneRequests.flatMap(request=>request.watchMessageIds));
assert(phone.heads.every(head=>watched.has(head.id)),'A stable backlog must not permanently starve any loaded active head');
await readChatHistory(desktop.owner,desktop.deviceId,desktop.projection);
assert.deepEqual(requests.at(-1).watchMessageIds,desktop.heads.slice(-8).map(message=>message.id),'The other device must start from its own watch offset');
assert.equal(phoneRequests.length,mode==='canceled'?7:6,'Rotation must not add polling requests');
assert.equal(phone.owner.fetching.current,false);assert.equal(desktop.owner.fetching.current,false);
process.stdout.write('history watch fixture passed\n');
