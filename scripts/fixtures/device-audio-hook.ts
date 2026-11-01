import {mock} from 'bun:test';
import assert from 'node:assert/strict';
import {pathToFileURL} from 'node:url';
import {join} from 'node:path';

const deny=()=>{throw Error('unexpected_external_operation');};
mock.module('node:child_process',()=>({spawn:deny,spawnSync:deny,exec:deny,execSync:deny}));
for(const module of ['node:http','node:https','node:net'])mock.module(module,()=>({request:deny,get:deny,connect:deny,createServer:deny}));
Bun.spawn=deny;Bun.spawnSync=deny;Bun.connect=deny;Bun.listen=deny;
globalThis.WebSocket=deny as never;
const root=process.env.AI_PHONE_TEST_AUDIO_SOURCE!;
assert(root);
let current:Hooks|null=null;
const same=(a:any[]|undefined,b:any[])=>!!a&&a.length===b.length&&a.every((value,index)=>Object.is(value,b[index]));
class Hooks {
 slots:any[]=[];cursor=0;effects:Array<()=>void>=[];mounted=true;view:any;
 render(id:string,endpoint:string){current=this;this.cursor=0;this.view=useAudio(id,endpoint);current=null;for(const effect of this.effects.splice(0))effect();return this.view;}
 unmount(){this.mounted=false;for(const slot of this.slots)slot?.cleanup?.();}
}
mock.module('react',()=>({
 useRef(value:any){const hooks=current!,index=hooks.cursor++;return hooks.slots[index]||(hooks.slots[index]={current:value});},
 useState(value:any){const hooks=current!,index=hooks.cursor++;if(!(index in hooks.slots))hooks.slots[index]=value;return[hooks.slots[index],(next:any)=>{if(hooks.mounted)hooks.slots[index]=typeof next==='function'?next(hooks.slots[index]):next;}];},
 useEffect(create:any,deps:any[]){const hooks=current!,index=hooks.cursor++,slot=hooks.slots[index]||(hooks.slots[index]={});if(!same(slot.deps,deps))hooks.effects.push(()=>{slot.cleanup?.();slot.cleanup=create();slot.deps=deps;});},
}));
const playback=await import(pathToFileURL(join(root,'apps/web/src/lib/audio-playback.ts')).href);
mock.module('@/lib/audio-playback',()=>playback);
const module=await import(pathToFileURL(process.env.AI_PHONE_TEST_AUDIO_HOOK||join(root,'apps/web/src/components/device-audio.ts')).href);
const useAudio=module.useDeviceAudio||module.usePhoneAudio;
assert.equal(typeof useAudio,'function');
const storage=new Map<string,string>();
Object.defineProperty(globalThis,'localStorage',{value:{getItem:(key:string)=>storage.get(key)??null,setItem:(key:string,value:string)=>storage.set(key,value)},configurable:true});
const contexts:FakeContext[]=[];
let rejectNext=false;
class FakeContext {
 state='suspended';currentTime=1;destination={};closed=0;stopped=0;frames=0;samples:number[][]=[];
 gain={gain:{value:1,setTargetAtTime(value:number){this.value=value;}},connect(){},disconnect(){}};
 reject=rejectNext;
 constructor(){rejectNext=false;contexts.push(this);}
 async resume(){if(this.reject)throw Error('fixture_playback_rejected');this.state='running';}
 async suspend(){this.state='suspended';}
 async close(){this.closed++;this.state='closed';}
 createGain(){return this.gain;}
 createBuffer(_channels:number,length:number,rate:number){this.frames+=length;return{duration:length/rate,copyToChannel:(data:Float32Array,channel:number)=>{(this.samples[channel]??=[]).push(...data);}};}
 createBufferSource(){return{buffer:null,connect(){},disconnect(){},onended:null,start(){},stop:()=>{this.stopped++;}};}
}
globalThis.AudioContext=FakeContext as unknown as typeof AudioContext;
const timers=new Map<number,()=>void>();let timerId=0;
globalThis.setTimeout=((callback:()=>void)=>{const id=++timerId;timers.set(id,callback);return id;}) as unknown as typeof setTimeout;
globalThis.clearTimeout=((id:number)=>timers.delete(Number(id))) as typeof clearTimeout;
type Transport={endpoint:string;signal:AbortSignal;controller:ReadableStreamDefaultController<Uint8Array>;cancelled:boolean};
const transports:Transport[]=[];
globalThis.fetch=async(input:any,options:any)=>{
 const endpoint=String(input);assert(['/fixture/audio/phone','/fixture/audio/computer'].includes(endpoint));
 let record:Transport;
 const stream=new ReadableStream<Uint8Array>({start(controller){record={endpoint,signal:options.signal,controller,cancelled:false};transports.push(record);},cancel(){record.cancelled=true;}});
 options.signal.addEventListener('abort',()=>{record.controller.error(new DOMException('stopped','AbortError'));},{once:true});
 return new Response(stream,{headers:{'content-type':'audio/L16;rate=48000;channels=2'}});
};
const flush=async()=>{for(let i=0;i<80;i++)await Promise.resolve();};
function device(id:string){const hooks=new Hooks(),endpoint=`/fixture/audio/${id}`;const render=()=>hooks.render(id,endpoint);render();return{hooks,render,async start(){await render().toggle();render();await flush();render();},transport:()=>transports.find(value=>value.endpoint===endpoint)!};}
const packet=new Uint8Array([0,32,0,224,0,64,0,192]);
const mode=process.argv[2];
if(mode==='focus-continuity'){
 const phone=device('phone');await phone.start();const transport=phone.transport();transport.controller.enqueue(packet.slice(0,3));await flush();
 for(let focus=0;focus<6;focus++){phone.render();assert(!transport.signal.aborted);}
 transport.controller.enqueue(packet.slice(3));await flush();assert.equal(transports.length,1);assert.equal(contexts[0].frames,2);assert.deepEqual(contexts[0].samples,[[.25,.5],[-.25,-.5]]);assert.equal(contexts[0].closed,0);phone.hooks.unmount();
}else if(mode==='independent-devices'){
 const phone=device('phone'),computer=device('computer');await phone.start();phone.render().setVolume(.25);phone.render();await computer.start();computer.render().setVolume(.75);computer.render();
 phone.transport().controller.enqueue(packet);computer.transport().controller.enqueue(packet);await flush();assert.equal(contexts.length,2);assert.equal(contexts[0].gain.gain.value,.25);assert.equal(contexts[1].gain.gain.value,.75);assert.equal(storage.get('ai-phone:volume:phone'),'0.25');assert.equal(storage.get('ai-phone:volume:computer'),'0.75');
 await computer.render().toggle();computer.render();await flush();assert(!phone.transport().signal.aborted);assert(computer.transport().signal.aborted);phone.transport().controller.enqueue(packet);await flush();assert.equal(contexts[0].frames,4);phone.hooks.unmount();computer.hooks.unmount();
}else if(mode==='device-disposal'){
 const phone=device('phone'),computer=device('computer');await phone.start();await computer.start();phone.hooks.unmount();await flush();assert(phone.transport().signal.aborted);assert(!computer.transport().signal.aborted);assert.equal(contexts[0].closed,1);assert.equal(contexts[1].closed,0);computer.transport().controller.enqueue(packet);await flush();assert.equal(contexts[1].frames,2);computer.hooks.unmount();
}else if(mode==='failed-device-isolation'){
 const phone=device('phone'),computer=device('computer');await phone.start();rejectNext=true;await computer.start();assert.equal(computer.render().enabled,false);assert(computer.render().error);assert.equal(transports.length,1);assert(!phone.transport().signal.aborted);phone.transport().controller.enqueue(packet);await flush();assert.equal(contexts[0].frames,2);phone.hooks.unmount();computer.hooks.unmount();
}else if(mode==='retry-disposal'){
 const phone=device('phone');await phone.start();phone.transport().controller.error(Error('fixture_disconnect'));await flush();assert.equal(timers.size,1);phone.hooks.unmount();await flush();assert.equal(timers.size,0,'Device disposal must clear the pending reconnect timer');assert.equal(transports.length,1);
}else throw Error('unknown_fixture_case');
await flush();assert(transports.every(value=>value.signal.aborted));console.log('device audio fixture passed');
