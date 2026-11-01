import {test,expect,mock} from 'bun:test';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
const root=process.env.AI_PHONE_TEST_SOURCE_ROOT||join(import.meta.dir,'..');
const candidate=process.env.AI_PHONE_TEST_CANDIDATE_ROOT||root;
const protobuf=await import(pathToFileURL(Bun.resolveSync('@bufbuild/protobuf',root)).href);
mock.module('@bufbuild/protobuf',()=>protobuf);
const protocol=await import(pathToFileURL(join(candidate,'packages/protocol/src/gen/ai_phone/v1/assistant_pb.ts')).href);
mock.module('@ai-phone/protocol/ai_phone/v1/assistant_pb',()=>protocol);
const {create}=protobuf,{ChatStateSchema,ChatMessageSchema,ChatMessageWindowSchema,ChatWindowKind}=protocol;
const {acceptSnapshot,acceptInputReceipt}=await import(pathToFileURL(process.env.AI_PHONE_TEST_SNAPSHOT||join(candidate,'apps/web/src/lib/chat-snapshot.ts')).href);
const message=(id:string,ordinal:bigint,text=id,role='assistant')=>create(ChatMessageSchema,{id,ordinal,text,role,runId:role==='assistant'?'run':''});
const full=(messages:any[],extra:any={})=>create(ChatStateSchema,{conversationId:'conversation',deviceId:'phone',runId:'run',runStatus:'running',revision:messages.at(-1)?.ordinal||0n,messages,...extra});
const window=(messages:any[],kind=ChatWindowKind.TAIL,extra:any={})=>full(messages,{messageWindow:create(ChatMessageWindowSchema,{kind,pageMessageIds:messages.map(m=>m.id),nextBeforeMessageId:messages[0]?.id||'',hasOlder:true}),...extra});

test('partial tail preserves loaded old history and updates current assistant at the same ordinal',()=>{
 const old={chat:full([message('older',1n),message('current',2n,'working')]),order:1};
 const next=acceptSnapshot(old,window([message('current',2n,'done')],ChatWindowKind.TAIL,{runStatus:'succeeded'}),2);
 expect(next.chat.messages.map((m:any)=>m.id)).toEqual(['older','current']);
 expect(next.chat.messages[1].text).toBe('done');expect(next.chat.runStatus).toBe('succeeded');
});
test('older page merges a lower ordinal without rolling back newer run metadata',()=>{
 const newest=acceptSnapshot({chat:null,order:0},window([message('newest',100n)],ChatWindowKind.TAIL,{runStatus:'succeeded'}),10);
 const old=window([message('old',1n)],ChatWindowKind.BEFORE,{revision:100n,runStatus:'running'});
 const next=acceptSnapshot(newest,old,5,'newest');
 expect(next.chat.messages.map((m:any)=>m.id)).toEqual(['old','newest']);expect(next.chat.runStatus).toBe('succeeded');
});
test('a delayed window adds unseen rows but cannot replace a newer known assistant',()=>{
 const newest=acceptSnapshot({chat:null,order:0},window([message('current',10n,'done')],ChatWindowKind.TAIL,{runStatus:'succeeded'}),10);
 const delayed=window([message('late-commit',4n),message('current',10n,'old')]);
 const next=acceptSnapshot(newest,delayed,5);
 expect(next.chat.messages.map((m:any)=>m.id)).toEqual(['late-commit','current']);expect(next.chat.messages[1].text).toBe('done');expect(next.chat.runStatus).toBe('succeeded');
});
test('ACK merges a verified immutable input without discarding window versions or old rows',()=>{
 let value=acceptSnapshot({chat:null,order:0},window([message('old',1n),message('current',3n,'done')],ChatWindowKind.TAIL,{runStatus:'succeeded'}),10);
 value=acceptInputReceipt(value,{conversationId:'conversation',deviceId:'phone',message:create(ChatMessageSchema,{id:'receipt',requestId:'request',role:'user',ordinal:4n,text:'accepted'})},12);
 value=acceptSnapshot(value,window([message('current',3n,'stale')],ChatWindowKind.TAIL,{revision:3n,runStatus:'running'}),9);
 expect(value.chat.messages.map((m:any)=>m.id)).toEqual(['old','current','receipt']);expect(value.chat.messages[1].text).toBe('done');expect(value.chat.runStatus).toBe('succeeded');
});
test('an older page changes the pagination boundary and a tail refresh does not reset it',()=>{
 let value=acceptSnapshot({chat:null,order:0},window([message('tail',100n)]),1);
 value=acceptSnapshot(value,window([message('older',20n)],ChatWindowKind.BEFORE),2,'tail');
 value=acceptSnapshot(value,window([message('latest',101n)]),3);
 expect(value.history.beforeMessageId).toBe('older');expect(value.chat.messages.map((m:any)=>m.id)).toEqual(['older','tail','latest']);
});
test('a partial response cannot rebind an existing device conversation',()=>{
 const previous=acceptSnapshot({chat:null,order:0},window([message('old',1n)]),1);
 expect(()=>acceptSnapshot(previous,window([message('foreign',2n)],ChatWindowKind.TAIL,{deviceId:'desktop'}),2)).toThrow();
});
test('legacy full snapshots retain their existing replacement and stale protection contract',()=>{
 const previous={chat:full([message('old',1n)]),order:5};
 expect(acceptSnapshot(previous,full([message('older',0n)]),6)).toBe(previous);
 const next=full([message('new',2n)]);expect(acceptSnapshot(previous,next,7).chat).toBe(next);
});

test.each([ChatWindowKind.TAIL,ChatWindowKind.BEFORE])('a later-issued lower-revision window adds history without rolling back known state (%s)',kind=>{
 const newest=acceptSnapshot({chat:null,order:0},window([message('current',10n,'done')],ChatWindowKind.TAIL,{revision:20n,runStatus:'succeeded'}),10);
 const delayed=window([message('older',1n),message('current',10n,'working')],kind,{revision:19n,runStatus:'running'});
 const next=acceptSnapshot(newest,delayed,11,'current');
 expect(next.chat.messages.map((m:any)=>m.id)).toEqual(['older','current']);
 expect(next.chat.messages[1].text).toBe('done');
 expect(next.chat.runStatus).toBe('succeeded');expect(next.chat.revision).toBe(20n);
});

test('a newer database revision wins even when its request was issued earlier',()=>{
 const previous=acceptSnapshot({chat:null,order:0},window([message('current',10n,'working')],ChatWindowKind.TAIL,{revision:19n}),11);
 const next=acceptSnapshot(previous,window([message('current',10n,'done')],ChatWindowKind.TAIL,{revision:20n,runStatus:'succeeded'}),10);
 expect(next.chat.messages[0].text).toBe('done');expect(next.chat.runStatus).toBe('succeeded');
});

test('a row discovered in an old window keeps its own revision rather than the current header revision',()=>{
 let value=acceptSnapshot({chat:null,order:0},window([message('current',10n,'done')],ChatWindowKind.TAIL,{revision:20n,runStatus:'succeeded'}),10);
 value=acceptSnapshot(value,window([message('older',1n,'old')],ChatWindowKind.BEFORE,{revision:18n}),30,'current');
 value=acceptSnapshot(value,window([message('older',1n,'updated')],ChatWindowKind.BEFORE,{revision:20n,runStatus:'succeeded'}),20,'current');
 expect(value.chat.messages[0].text).toBe('updated');expect(value.chat.runStatus).toBe('succeeded');expect(value.chat.revision).toBe(20n);
});

test('a lower revision cannot overwrite known assistant state after a newer accepted input',()=>{
 let value=acceptSnapshot({chat:null,order:0},window([message('current',3n,'done')],ChatWindowKind.TAIL,{runStatus:'succeeded'}),10);
 value=acceptInputReceipt(value,{conversationId:'conversation',deviceId:'phone',message:create(ChatMessageSchema,{id:'receipt',requestId:'request',role:'user',ordinal:4n,text:'accepted'})},12);
 value=acceptSnapshot(value,window([message('current',3n,'stale')],ChatWindowKind.TAIL,{revision:3n,runStatus:'running'}),13);
 expect(value.chat.messages[0].text).toBe('done');expect(value.chat.runStatus).toBe('succeeded');expect(value.chat.revision).toBe(4n);
});
