import {test,expect,mock} from 'bun:test';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
const source=process.env.AI_PHONE_TEST_SOURCE_ROOT||join(import.meta.dir,'..');
const candidate=process.env.AI_PHONE_TEST_CANDIDATE_ROOT||source;
const protobuf=await import(pathToFileURL(Bun.resolveSync('@bufbuild/protobuf',source)).href);
mock.module('@bufbuild/protobuf',()=>protobuf);
const protocol=await import(pathToFileURL(join(candidate,'packages/protocol/src/gen/ai_phone/v1/assistant_pb.ts')).href);
mock.module('@ai-phone/protocol/ai_phone/v1/assistant_pb',()=>protocol);
const {historyRequest,receiptBatch}=await import(pathToFileURL(join(candidate,'apps/web/src/lib/chat-window.ts')).href);
test('window requests bound watched heads and never treat the last ordinal as a change cursor',()=>{
 const chat=protobuf.create(protocol.ChatStateSchema,{revision:999999n,messages:Array.from({length:30},(_,i)=>({id:'head-'+i,role:'assistant',ordinal:BigInt(i+1),active:true}))});
 const request=historyRequest('conversation','phone',chat,['receipt'],'older-anchor');
 expect(request.limit).toBe(40);expect(request.watchMessageIds.length).toBe(8);expect(request.beforeMessageId).toBe('older-anchor');
 expect(request.receiptRequestIds).toEqual(['receipt']);expect('afterOrdinal' in request).toBe(false);
});
test('accepted receipt batches are bounded, fair and read the existing queue only',()=>{
 const entries=Array.from({length:75},(_,i)=>({id:'receipt-'+i,status:'accepted'}));entries.push({id:'unknown-post',status:'failed'});
 let offset=0;const seen=new Set<string>();
 for(let i=0;i<3;i++){const batch=receiptBatch(entries,offset);offset=batch.next;expect(batch.ids.length).toBe(32);for(const id of batch.ids)seen.add(id);}
 expect(seen.size).toBe(75);expect(seen.has('unknown-post')).toBe(false);expect(entries.length).toBe(76);
 expect(receiptBatch([],offset)).toEqual({ids:[],next:0});
});
test('adding history windows leaves existing send protobuf bytes unchanged',()=>{
 // Captured with the actual476809e schema, not recomputed with this implementation.
 const legacy='0a0c636f6e766572736174696f6e120ce4b8ade6968720696e7075741a0a6174746163686d656e742207726571756573742a0570686f6e653a2b0a0773657373696f6e12097574746572616e636518032209636f727265637465642a0870726576696f7573';
 const input={conversationId:'conversation',deviceId:'phone',requestId:'request',text:'中文 input',attachmentIds:['attachment'],voice:{sessionId:'session',utteranceId:'utterance',revision:3,literal:'corrected',correctsRequestId:'previous'}};
 expect(Buffer.from(protobuf.toBinary(protocol.ChatSendRequestSchema,protobuf.create(protocol.ChatSendRequestSchema,input))).toString('hex')).toBe(legacy);
});
