import {mock} from 'bun:test';
import assert from 'node:assert/strict';
import {resolve,dirname} from 'node:path';
import {pathToFileURL} from 'node:url';
const source=process.env.AI_PHONE_TEST_SOURCE_ROOT!,component=process.env.AI_PHONE_TEST_DESKTOP_SURFACE!;
assert(source&&component);
let externalAttempts=0;
const deny=()=>{externalAttempts++;throw Error('fixture_external_operation');};
const processCalls={spawn:deny,spawnSync:deny,exec:deny,execSync:deny,execFile:deny,execFileSync:deny,fork:deny};
for(const name of ['node:child_process','child_process'])mock.module(name,()=>processCalls);
for(const name of ['node:http','http','node:https','https'])mock.module(name,()=>({request:deny,get:deny,createServer:deny}));
for(const name of ['node:net','net'])mock.module(name,()=>({connect:deny,createConnection:deny,createServer:deny}));
Bun.spawn=deny;Bun.spawnSync=deny;Bun.connect=deny;Bun.listen=deny;Bun.serve=deny as never;
globalThis.fetch=deny;globalThis.WebSocket=deny as never;
const element=(type:any,props:any,key?:any)=>({type,props:{...props,key}});
mock.module('react/jsx-runtime',()=>({jsx:element,jsxs:element,Fragment:'fragment'}));
mock.module('react/jsx-dev-runtime',()=>({jsxDEV:element,Fragment:'fragment'}));
let slots:any[]=[],cursor=0,layouts:Array<()=>void>=[],mounted=true,lateWrites=0;
const same=(a:any[]|undefined,b:any[])=>!!a&&a.length===b.length&&a.every((v,i)=>Object.is(v,b[i]));
mock.module('react',()=>({
 useRef:(value:any)=>{const index=cursor++;return slots[index]||(slots[index]={current:value});},
 useState:(value:any)=>{const index=cursor++;if(!(index in slots))slots[index]=value;return[slots[index],(next:any)=>{if(!mounted){lateWrites++;return;}slots[index]=typeof next==='function'?next(slots[index]):next;}];},
 useLayoutEffect:(create:any,deps:any[])=>{const index=cursor++,slot=slots[index]||(slots[index]={});if(!same(slot.deps,deps))layouts.push(()=>{slot.cleanup?.();slot.cleanup=create();slot.deps=deps;});},
}));
for(const name of ['workbench-notices','workbench-notices.tsx'])mock.module(resolve(dirname(component),name),()=>({useWorkbenchNotice:()=>{}}));
for(const name of ['phone-stream-image','phone-stream-image.tsx'])mock.module(resolve(dirname(component),name),()=>({PhoneStreamImage:()=>null}));
const protobuf=await import(pathToFileURL(Bun.resolveSync('@bufbuild/protobuf',source)).href);
mock.module('@bufbuild/protobuf',()=>protobuf);
const desktop=await import(pathToFileURL(resolve(source,'packages/protocol/src/gen/ai_phone/v1/desktop_pb.ts')).href);
mock.module('@ai-phone/protocol/ai_phone/v1/desktop_pb',()=>desktop);
const {create,toBinary,fromBinary}=protobuf;
type Call={op:string;value:any;body:Uint8Array;schema:any};
const calls:Call[]=[];let revision=0,minimumBridgeWait=0;
let heldAction:((fail?:boolean)=>void)|null=null,heldControl:(()=>void)|null=null;
const mode=process.argv[2];
let holds=mode==='warm-inflight'?3:mode==='merged-unknown'?2:1,holdControl=['control-barrier','late-control'].includes(mode);
const observed=(id:string)=>create(desktop.DesktopObservationSchema,{deviceId:id,observationId:'observed-'+revision,epoch:3n,width:1280,height:800});
const rpc=async(op:string,schema:any,value:any)=>{
 assert(['control','action'].includes(op));
 const body=toBinary(schema,create(schema,value)),decoded=fromBinary(schema,body);calls.push({op,value,body:body.slice(),schema});
 if(op==='control'){
  if(holdControl){holdControl=false;await new Promise<void>(resolve=>{heldControl=resolve;});}
  return observed(value.deviceId);
 }
 assert.equal(value.observationId,'observed-'+revision);assert.equal(decoded.epoch,3n);assert.match(decoded.actionId,/^[0-9a-f-]{36}$/);
 minimumBridgeWait+=120;
 if(holds>0){holds--;await new Promise<void>((resolve,reject)=>{heldAction=(fail=false)=>{revision++;if(fail)reject(Error('fixture_outcome_unknown'));else resolve();};});}
 else revision++;
 return observed(value.deviceId);
};
mock.module('@/lib/settings-client',()=>({settingsRpc:rpc}));
mock.module(resolve(source,'apps/web/src/lib/settings-client.ts'),()=>({settingsRpc:rpc}));
const {DesktopSurface}=await import(pathToFileURL(component).href);
let tree:any,textarea:any,oldTextarea:any;
const walk=(node:any):any[]=>!node||typeof node!=='object'?[]:[node,...(Array.isArray(node.props?.children)?node.props.children:[node.props?.children]).flatMap(walk)];
function render(active=true,deviceId='desktop-fixture'){
 cursor=0;tree=DesktopSurface({deviceId,active});
 for(const commit of layouts.splice(0))commit();
 oldTextarea=textarea;textarea=walk(tree).find(node=>node.type==='textarea');
 if(oldTextarea&&!textarea)oldTextarea.props.ref.current=null;
 if(textarea)textarea.props.ref.current={value:'',focus:()=>textarea.props.onFocus?.()};
}
function unmount(){for(const slot of slots)slot?.cleanup?.();mounted=false;}
const flush=async()=>{for(let i=0;i<400;i++)await Promise.resolve();};
const until=async(predicate:()=>boolean)=>{for(let i=0;i<4000&&!predicate();i++)await Promise.resolve();assert(predicate(),'fixture promise did not settle');};
const actions=()=>calls.filter(call=>call.op==='action');
function type(value:string){textarea.props.ref.current.value=value;textarea.props.onInput();}
function key(value:string){textarea.props.onKeyDown({nativeEvent:{isComposing:false},key:value,preventDefault(){}});}
const release=(fail=false)=>{assert(heldAction);const finish=heldAction;heldAction=null;finish(fail);};
const text=()=>actions().filter(call=>call.value.kind==='text').map(call=>call.value.text).join('');
const bodiesUnchanged=()=>{for(const call of calls)assert.deepEqual(toBinary(call.schema,create(call.schema,call.value)),call.body,'a dispatched body was mutated');assert.equal(new Set(actions().map(call=>call.value.actionId)).size,actions().length);};
let timerAttempts=0;globalThis.setTimeout=(()=>{timerAttempts++;throw Error('fixture_unexpected_input_timer');}) as never;
render();
if(mode==='late-control'){
 type('a');await until(()=>!!heldControl);type('b');unmount();heldControl!();heldControl=null;await flush();assert.equal(actions().length,0);assert.equal(lateWrites,0);
}else if(mode==='sync-burst'){
 for(const letter of 'abcdefghijklmnopqrstuvwxyz1234')type(letter);
 await until(()=>!!heldAction);release();await flush();
 assert.equal(actions().length,1);assert.equal(text(),'abcdefghijklmnopqrstuvwxyz1234');
}else if(mode==='control-barrier'){
 type('a');await until(()=>!!heldControl);type('b');heldControl!();heldControl=null;await until(()=>!!heldAction);type('c');release();await flush();
 assert.deepEqual(actions().map(call=>call.value.text),['a','b','c']);
}else{
 type('a');await until(()=>!!heldAction);const first=calls.find(call=>call.op==='action')!.body.slice();
 if(mode==='burst'){
  for(const letter of 'bcdefghijklmnopqrstuvwxyz1234'){type(letter);await Promise.resolve();}
  release();await flush();assert.equal(actions().length,2);assert.equal(text(),'abcdefghijklmnopqrstuvwxyz1234');assert.equal(minimumBridgeWait,240);
 }else if(mode==='warm-inflight'){
  type('b');type('c');release();await until(()=>!!heldAction&&actions().length===2);type('d');release();await until(()=>!!heldAction&&actions().length===3);const warm=actions()[2].body.slice();type('e');release();await flush();assert.deepEqual(actions().map(call=>call.value.text),['a','bc','d','e']);assert.deepEqual(actions()[2].body,warm);
 }else if(mode==='key-barrier'){
  type('b');type('c');key('Enter');type('d');type('e');release();await flush();assert.deepEqual(actions().map(call=>[call.value.kind,call.value.text||call.value.key]),[['text','a'],['text','bc'],['key','Return'],['text','de']]);
 }else if(mode==='ime'){
  type('b');textarea.props.onCompositionStart();type('你');type('你好');assert.equal(actions().length,1);textarea.props.onCompositionEnd();textarea.props.onInput();type('c');release();await flush();assert.deepEqual(actions().map(call=>call.value.text),['a','b','你好','c']);
 }else if(mode==='paste'){
  type('b');type('c');let prevented=false;textarea.props.onPaste({preventDefault(){prevented=true;},clipboardData:{getData:()=> '世界\n第二行'}});type('d');type('e');release();await flush();assert(prevented);assert.deepEqual(actions().map(call=>call.value.text),['a','bc','世界\n第二行','de']);
 }else if(mode==='focus-barrier'){
  type('b');type('c');textarea.props.onBlur?.();textarea.props.onFocus?.();type('d');type('e');release();await flush();assert.deepEqual(actions().map(call=>call.value.text),['a','bc','de']);
 }else if(mode==='drag-barrier'){
  type('b');type('c');const image=walk(tree).find(node=>node.props?.onPointerDown);assert(image);const target={getBoundingClientRect:()=>({left:0,top:0,width:1280,height:800}),setPointerCapture(){}};
  image.props.onPointerDown({button:0,clientX:10,clientY:20,currentTarget:target,pointerId:1,preventDefault(){}});image.props.onPointerUp({clientX:80,clientY:90,currentTarget:target});type('d');type('e');release();await flush();assert.deepEqual(actions().map(call=>call.value.kind),['text','text','drag','text']);assert.equal(actions()[1].value.text,'bc');assert.equal(actions()[2].value.toX,80);assert.equal(actions()[3].value.text,'de');
 }else if(mode==='merged-unknown'){
  type('b');type('c');release();await until(()=>!!heldAction&&actions().length===2);const merged=actions()[1].body.slice();type('d');type('e');release(true);await flush();assert.deepEqual(actions().map(call=>call.value.text),['a','bc','de']);assert.deepEqual(actions()[1].body,merged);assert.equal(calls.filter(call=>call.op==='control').length,2);assert.equal(actions()[2].value.observationId,'observed-2');
 }else if(mode==='unknown'){
  type('b');type('c');release(true);await flush();assert.equal(calls.filter(call=>call.op==='control').length,2);assert.deepEqual(actions().map(call=>call.value.text),['a','bc']);assert.notEqual(actions()[0].value.actionId,actions()[1].value.actionId);assert.equal(actions()[1].value.observationId,'observed-1');
 }else if(mode==='stop-focus'){
  type('b');render(false);render(true);type('新');release();await flush();assert.deepEqual(actions().map(call=>call.value.text),['a','新']);assert.equal(calls.filter(call=>call.op==='control').length,2);
 }else if(mode==='ime-stop'){
  textarea.props.onCompositionStart();type('你');render(false);render(true);type('c');release();await flush();assert.deepEqual(actions().map(call=>call.value.text),['a','c']);assert.equal(calls.filter(call=>call.op==='control').length,2);
 }else if(mode==='device-change'){
  type('b');render(true,'other-fixture');type('新');release();await flush();assert.deepEqual(actions().map(call=>[call.value.deviceId,call.value.text]),[['desktop-fixture','a'],['other-fixture','新']]);assert.equal(calls.filter(call=>call.op==='control').length,2);
 }else if(mode==='dispose'){
  type('b');unmount();release();await flush();assert.equal(actions().length,1);assert.equal(lateWrites,0);
 }else if(mode==='character-limit'){
  type('中'.repeat(9000));type('文'.repeat(9000));release();await flush();assert.equal(actions().length,3);assert.equal([...actions()[1].value.text].length,9000);assert.equal([...actions()[2].value.text].length,9000);
 }else if(mode==='unicode-byte-limit'){
  type('鲸'.repeat(5000));type('🐳'.repeat(8000));release();await flush();assert.equal(actions().length,2);assert.equal(new TextEncoder().encode(actions()[1].value.text).byteLength,47000);assert.equal([...actions()[1].value.text].length,13000);
 }else throw Error('unknown mode');
 assert.deepEqual(actions()[0].body,first,'later enqueue changed the first issued body');
}
bodiesUnchanged();assert.equal(externalAttempts,0);assert.equal(lateWrites,0);assert.equal(timerAttempts,0);
console.log(JSON.stringify({mode,actual_component:true,modeled_react_and_transport:true,actions:actions().length,control_requests:calls.filter(call=>call.op==='control').length,minimum_existing_bridge_wait_ms:minimumBridgeWait,added_debounce_ms:0,timer_attempts:timerAttempts,external_attempts:externalAttempts,not_end_to_end_latency:true}));
