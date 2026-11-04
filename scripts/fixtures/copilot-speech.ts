import {strict as assert} from 'node:assert';
import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {VoiceSpeechRequestSchema,VoiceSpeechStreamOutputSchema} from '../../packages/protocol/src/gen/ai_phone/v1/voice_pb';
import {speechFrames} from '../../apps/web/src/lib/copilot-speech';
import {playWave} from '../../apps/web/src/lib/copilot-playback';
import {wav} from '../../apps/web/src/lib/copilot-audio';

const audio=wav([new Int16Array(1600)]),peers:Bun.ServerWebSocket<unknown>[]=[],closed=new Set<number>();
let calls=0,mode:'stream'|'quota'|'disconnect'='stream';
const send=(peer:Bun.ServerWebSocket<unknown>,event:MessageShape<typeof VoiceSpeechStreamOutputSchema>['event'])=>peer.send(toBinary(VoiceSpeechStreamOutputSchema,create(VoiceSpeechStreamOutputSchema,{event})));
const server=Bun.serve({port:0,hostname:'127.0.0.1',fetch(request,server){if(server.upgrade(request))return;return new Response(null,{status:400});},websocket:{
  message(peer,data){
    const request=fromBinary(VoiceSpeechRequestSchema,new Uint8Array(data as Uint8Array));assert.equal(request.text,'正在处理');assert.equal(request.configurationRevision,7n);
    calls++;peers.push(peer);
    if(mode==='quota'){send(peer,{case:'error',value:{message:'语音服务 HTTP 402'}});peer.close();return;}
    if(mode==='disconnect'){peer.close();return;}
    send(peer,{case:'wavChunk',value:audio.slice(0,1644)});
  },close(peer){closed.add(peers.indexOf(peer));},
}});
const sources:Array<{stopped:boolean;onended:(()=>void)|null}>=[];
const context={currentTime:0,destination:{},createBuffer:(_channels:number,length:number,rate:number)=>({duration:length/rate,copyToChannel:()=>{}}),createBufferSource:()=>{
  const node={stopped:false,onended:null as (()=>void)|null,buffer:null,connect:()=>{},disconnect:()=>{},start:()=>{},stop:()=>{node.stopped=true;node.onended?.();}};sources.push(node);return node;
}} as unknown as AudioContext;
const wait=async(check:()=>boolean)=>{const until=performance.now()+2000;while(!check()){if(performance.now()>until)throw Error('Speech fixture stalled');await Bun.sleep(1);}};
const open=()=>new WebSocket(String(server.url).replace('http:','ws:'));
const request=create(VoiceSpeechRequestSchema,{deviceId:'phone',agentId:'agent',configurationRevision:7n,text:'正在处理'});
try{
  const first=new AbortController(),playing=playWave(context,speechFrames(request,first.signal,open),first.signal);
  await wait(()=>sources.length===1);assert.equal(calls,1);assert(!closed.has(0));
  send(peers[0],{case:'wavChunk',value:audio.slice(1644)});send(peers[0],{case:'done',value:true});
  await wait(()=>sources.length===2);for(const source of sources)source.onended?.();await playing;await wait(()=>closed.has(0));
  const second=new AbortController(),interrupted=playWave(context,speechFrames(request,second.signal,open),second.signal);
  await wait(()=>sources.length===3);second.abort();await interrupted;await wait(()=>closed.has(1));assert(sources[2].stopped);
  mode='quota';await assert.rejects(async()=>{for await(const _ of speechFrames(request,new AbortController().signal,open)){assert.fail('Quota produced audio');}},/402/);
  mode='disconnect';await assert.rejects(async()=>{for await(const _ of speechFrames(request,new AbortController().signal,open)){assert.fail('Closed socket produced audio');}},/连接中断/);
  assert.equal(calls,4);
  console.log(JSON.stringify({passed:true,firstSamplesBeforeEof:true,bargeInClosesSocket:true,quotaWithoutReplay:true,realMicrophoneOrProvider:false}));
}finally{for(const peer of peers)peer.close();server.stop(true);}
