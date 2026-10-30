import {describe,expect,test} from 'bun:test';
import {ChatOutbox,type OutboxInput} from '../apps/web/src/lib/chat-outbox';

const input=(id:string,text=id):OutboxInput=>({id,conversationId:'conversation',deviceId:'phone',text,attachments:[]});
const tick=()=>new Promise(resolve=>setTimeout(resolve,0));
function storage(){let value='[]';return{read:()=>value,write:(next:string)=>{value=next;}};}
describe('continuous chat outbox',()=>{
 test('accepts three messages during an unresolved send, preserving exact FIFO text',async()=>{
  const calls:string[]=[],acks:string[]=[],resolvers:Array<()=>void>=[];
  const box=new ChatOutbox(storage(),async item=>{calls.push(item.text);await new Promise<void>(resolve=>resolvers.push(resolve));return item.id;},v=>acks.push(v));
  box.enqueue(input('1','first\nline'));box.enqueue(input('2','second'));box.enqueue(input('3','third'));
  expect(box.entries.map(e=>e.text)).toEqual(['first\nline','second','third']);expect(calls).toEqual(['first\nline']);
  resolvers.shift()!();await tick();expect(calls).toEqual(['first\nline','second']);
  resolvers.shift()!();await tick();resolvers.shift()!();await tick();
  expect(acks).toEqual(['1','2','3']);expect(box.entries).toEqual([]);box.dispose();
 });
 test('uncertain acknowledgement retries the same receipt and blocks later sends',async()=>{
  const disk=storage(),calls:string[]=[];let fail=true;
  const box=new ChatOutbox(disk,async item=>{calls.push(item.id);if(fail)throw Error('connection lost');return item.id;},()=>{});
  box.enqueue(input('receipt'));box.enqueue(input('later'));await tick();
  expect(calls).toEqual(['receipt']);expect(box.entries[0].error).toBe('connection lost');
  fail=false;box.retry();await tick();expect(calls).toEqual(['receipt','receipt','later']);expect(box.entries).toEqual([]);box.dispose();
 });
 test('reload recovers outstanding messages and their attachments without replacing their IDs',async()=>{
  const disk=storage();let captured='';
  const first=new ChatOutbox(disk,async()=>new Promise(()=>{}),()=>{});
  first.enqueue({...input('same'),attachments:[{id:'file',name:'中文.txt',mediaType:'text/plain',sizeBytes:'123'}]});first.dispose();
  const next=new ChatOutbox(disk,async item=>{captured=JSON.stringify(item);return true;},()=>{});
  next.start();await tick();expect(JSON.parse(captured).id).toBe('same');expect(JSON.parse(captured).attachments[0].name).toBe('中文.txt');expect(next.entries).toEqual([]);next.dispose();
 });
 test('storage failure leaves the draft with its caller and never starts a request',()=>{
  let calls=0;const box=new ChatOutbox({read:()=> '[]',write:()=>{throw Error('full');}},async()=>{calls++;},()=>{});
  expect(()=>box.enqueue(input('unsaved'))).toThrow('full');expect(box.entries).toEqual([]);expect(calls).toBe(0);box.dispose();
 });
 test('a late response after disposal cannot erase persisted input',async()=>{
  const disk=storage();let finish!:(value:boolean)=>void,accepted=0;
  const box=new ChatOutbox(disk,async()=>new Promise<boolean>(resolve=>finish=resolve),()=>{accepted++;});
  box.enqueue(input('retained'));box.dispose();finish(true);await tick();expect(accepted).toBe(0);expect(JSON.parse(disk.read())).toHaveLength(1);
 });
 test('corrupt persistence is reported, never silently discarded',()=>{
  expect(()=>new ChatOutbox({read:()=>'{bad',write:()=>{}},async()=>true,()=>{})).toThrow();
  expect(()=>new ChatOutbox({read:()=>'[{}]',write:()=>{}},async()=>true,()=>{})).toThrow('无法读取');
  for(const malformed of [null,3,{...input('bad'),id:3},{...input('bad'),attachments:'wrong'},{...input('bad'),attachments:[null]},{...input('bad'),attachments:[{id:3}]}]){
   expect(()=>new ChatOutbox({read:()=>JSON.stringify([malformed]),write:()=>{}},async()=>true,()=>{})).toThrow('无法读取');
  }
 });
 test('the queued snapshot is immutable and duplicate receipts cannot be added',()=>{
  const box=new ChatOutbox(storage(),async()=>new Promise(()=>{}),()=>{});
  const item={...input('one'),attachments:[{id:'f',name:'keep',mediaType:'text/plain',sizeBytes:'1'}]};
  box.enqueue(item);item.text='changed';item.attachments[0].name='changed';
  expect(box.entries[0].text).toBe('one');expect(box.entries[0].attachments[0].name).toBe('keep');
  expect(()=>box.enqueue(input('one'))).toThrow('重复');box.dispose();expect(()=>box.enqueue(input('two'))).toThrow('切换');
 });
 test('failure to remove an acknowledged receipt retains it for idempotent recovery',async()=>{
  let saved='[]',block=false;const disk={read:()=>saved,write:(value:string)=>{if(block)throw Error('disk full');saved=value;}};
  const box=new ChatOutbox(disk,async()=>{block=true;return true;},()=>{});
  box.enqueue(input('confirmed'));await tick();expect(box.entries[0].status).toBe('failed');expect(JSON.parse(saved)[0].id).toBe('confirmed');
  block=false;box.dispose();
 });
 test('repeated start and retry during an outstanding request never create concurrent sends',async()=>{
  let calls=0,finish!:(value:boolean)=>void;
  const box=new ChatOutbox(storage(),async()=>{calls++;return new Promise<boolean>(resolve=>finish=resolve);},()=>{});
  expect(()=>box.start()).not.toThrow();box.enqueue(input('single'));
  box.start();box.retry();box.start();expect(calls).toBe(1);
  finish(true);await tick();expect(box.entries).toEqual([]);expect(()=>box.start()).not.toThrow();box.dispose();box.start();expect(calls).toBe(1);
 });
});
