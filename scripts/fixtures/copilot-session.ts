import {create,fromBinary,toBinary,type DescMessage,type MessageShape} from '@bufbuild/protobuf';
import {ApiErrorSchema} from '../../packages/protocol/src/gen/ai_phone/v1/system_pb';
import {VoiceSettingsRequestSchema,VoiceSettingsSchema,VoiceTranscribeRequestSchema,VoiceTranscriptSchema,VoiceSpeechRequestSchema,VoiceAudioSchema} from '../../packages/protocol/src/gen/ai_phone/v1/voice_pb';
import {ChatSendRequestSchema} from '../../packages/protocol/src/gen/ai_phone/v1/assistant_pb';
import {CopilotSession,type CopilotBinding,type MicrophonePort} from '../../apps/web/src/lib/copilot-session';
import {ChatOutbox,type OutboxInput} from '../../apps/web/src/lib/chat-outbox';
import {encodeChatSend} from '../../apps/web/src/lib/chat-wire';
import {wav} from '../../apps/web/src/lib/copilot-audio';
function assert(value:unknown,message:string):asserts value {if(!value)throw Error(message);}
let phase='setup';
const wait=async(check:()=>boolean)=>{const deadline=performance.now()+2000;while(!check()){if(performance.now()>deadline)throw Error('No progress: '+phase+'; '+controller.snapshot().error);await Bun.sleep(5);}};
class Mic implements MicrophonePort {
  frame:((pcm:Int16Array,at:number)=>void)|null=null;
  closed=false;
  failed:(message:string)=>void=()=>{};
  played:AbortSignal[]=[];
  prepare(){return Promise.resolve();}
  async open(frame:(pcm:Int16Array,at:number)=>void,failed:(message:string)=>void){this.frame=frame;this.failed=failed;}
  async play(bytes:Uint8Array,signal:AbortSignal){assert(new TextDecoder().decode(bytes.slice(0,4))==='RIFF','Not WAV');this.played.push(signal);await new Promise<void>(resolve=>signal.addEventListener('abort',()=>resolve(),{once:true}));}
  close(){this.closed=true;this.frame=null;}
}
const transcriptions:Array<MessageShape<typeof VoiceTranscribeRequestSchema>>=[];
const spokenRevisions:bigint[]=[];
let speechQuota=true,late=false,releaseLate:()=>void=()=>{};
const server=Bun.serve({port:0,hostname:'127.0.0.1',async fetch(request){
  const path=new URL(request.url).pathname,bytes=new Uint8Array(await request.arrayBuffer());
  assert(request.headers.get('content-type')==='application/x-protobuf','Non-protobuf voice request');
  const response=<T extends DescMessage>(schema:T,value:MessageShape<T>)=>new Response(toBinary(schema,value),{headers:{'content-type':'application/x-protobuf'}});
  if(path.endsWith('/settings')){
    const input=fromBinary(VoiceSettingsRequestSchema,bytes);
    return response(VoiceSettingsSchema,create(VoiceSettingsSchema,{agentId:input.agentId,revision:1n,configured:true,baseUrl:'http://voice-fixture/v1',recognitionModel:'audio',speechModel:'speech',voice:'fixture',inputProtocol:2}));
  }
  if(path.endsWith('/transcribe')){
    const input=fromBinary(VoiceTranscribeRequestSchema,bytes);transcriptions.push(input);
    assert(new TextDecoder().decode(input.wav.slice(0,4))==='RIFF','Missing audio');
    if(late)await new Promise<void>(resolve=>{releaseLate=resolve;});
    const prior=input.context[0];
    return response(VoiceTranscriptSchema,create(VoiceTranscriptSchema,{literal:prior?'不是，是北京':'打开天气',text:prior?'打开北京天气':'打开上海天气',correctsRequestId:prior?.requestId||''}));
  }
  if(path.endsWith('/speech')){
    spokenRevisions.push(fromBinary(VoiceSpeechRequestSchema,bytes).configurationRevision);
    if(speechQuota)return new Response(toBinary(ApiErrorSchema,create(ApiErrorSchema,{code:'voice_error',message:'语音服务 HTTP 402'})),{status:402,headers:{'content-type':'application/x-protobuf'}});
    return response(VoiceAudioSchema,create(VoiceAudioSchema,{wav:wav([new Int16Array(1600)])}));
  }
  return new Response(null,{status:404});
}});
const nativeFetch=globalThis.fetch;
globalThis.fetch=((url:Parameters<typeof fetch>[0],options?:Parameters<typeof fetch>[1])=>nativeFetch(typeof url==='string'&&url.startsWith('/api/voice/')?new URL(url,server.url):url,options)) as typeof fetch;
const captures:Mic[]=[],recovered:string[]=[],stored:Record<string,string>={},sent:OutboxInput[]=[],received:OutboxInput[]=[],finish:Array<()=>void>=[],signals:AbortSignal[]=[];
const boxes=new Map<string,ChatOutbox<void>>();
function binding(deviceId:string):CopilotBinding {
  const box=new ChatOutbox<void>({read:()=>stored[deviceId]||'[]',write:value=>{stored[deviceId]=value;}},async(item,signal)=>{received.push(item);signals.push(signal);await new Promise<void>(resolve=>finish.push(resolve));},()=>{});boxes.set(deviceId,box);box.start();
  return {deviceId,agentId:'agent-'+deviceId,conversationId:'conversation-'+deviceId,ready:true,submit:input=>{sent.push(input);box.enqueue(input);},recover:text=>recovered.push(text),replies:[{id:'old',ordinal:1n,text:'旧消息。',complete:true}]};
}
const controller=new CopilotSession(()=>{throw Error('Unexpected configuration prompt');},()=>{const mic=new Mic();captures.push(mic);return mic;});
try{
  const phone=binding('phone'),desktop=binding('desktop');controller.register(phone);controller.register(desktop);await controller.start('phone');
  assert(controller.snapshot().enabled,'Capture did not enable');const mic=captures[0];
  const speech=new Int16Array(800).fill(3000),quiet=new Int16Array(800),origin=performance.now();
  function utterance(at:number,switchTo?:string){
    for(let i=0;i<400;i+=50){mic.frame!(speech,origin+at+i);if(i===150&&switchTo)controller.select(switchTo);}
    for(let i=400;i<=1850;i+=50)mic.frame!(quiet,origin+at+i);
  }
  phase='phone-input';utterance(0,'desktop');await wait(()=>sent.length===1);
  assert(sent[0].deviceId==='phone','In-flight speech moved to another device');
  phase='desktop-input';utterance(2000);await wait(()=>sent.length===2);assert(sent[1].deviceId==='desktop','Next speech did not follow focus');
  assert(transcriptions[1].context.length===0,'Another device’s voice context leaked');
  phase='correction';controller.select('phone');utterance(4000);await wait(()=>sent.length===3);
  const corrected=fromBinary(ChatSendRequestSchema,encodeChatSend(sent[2]));
  assert(corrected.voice?.correctsRequestId===sent[0].id,'Spoken correction lost its durable receipt');
  assert(corrected.voice.sessionId===sent[0].voice?.sessionId,'Capture session was truncated');
  assert(sent[2].text==='打开北京天气','Semantic edit was not applied');
  assert(JSON.parse(stored.phone).length===2,'Input was not durable while an earlier send was pending');
  // A synthesis failure must not stall subsequent speech input.
  phase='tts-quota';controller.register({...phone,replies:[...phone.replies,{id:'new',ordinal:2n,text:'正在处理。',complete:false}]});
  await wait(()=>!!controller.snapshot().error);
  phase='tts-new-configuration';controller.profileChanged(create(VoiceSettingsSchema,{agentId:phone.agentId,revision:2n,configured:true,inputProtocol:2}));
  await controller.retry();await wait(()=>spokenRevisions.length===2);assert(spokenRevisions[1]===2n,'Playback retry used superseded voice settings');
  phase='input-during-tts-failure';utterance(6000);await wait(()=>sent.length===4);
  assert(controller.snapshot().enabled,'Output failure disabled capture');
  phase='tts-after-next-input';speechQuota=false;controller.register({...phone,replies:[...phone.replies,{id:'next-reply',ordinal:3n,text:'已经收到。',complete:false}]});await wait(()=>mic.played.length===1);
  // A new syllable interrupts playback before waiting for an ASR/network result.
  mic.frame!(speech,origin+8000);mic.frame!(speech,origin+8050);
  assert(mic.played[0].aborted,'Barge-in did not stop playback');
  phase='late-cancellation';late=true;
  for(let i=8100;i<8400;i+=50)mic.frame!(speech,origin+i);
  for(let i=8400;i<=9850;i+=50)mic.frame!(quiet,origin+i);
  await wait(()=>transcriptions.length===5);controller.stop();releaseLate();await Bun.sleep(20);
  assert(sent.length===4,'Late cancelled recognition executed a command');assert(mic.closed,'Microphone was left open');
  assert(signals.every(s=>!s.aborted),'Closing Copilot cancelled accepted background work');
  phase='duplicate-retry';await controller.start('phone');captures[1].failed('fixture microphone disconnected');
  const first=controller.retry(),duplicate=controller.retry();
  assert(captures.length===3,'Repeated retry opened competing microphones');await Promise.all([first,duplicate]);
  assert(captures[1].closed&&!captures[2].closed,'Capture retry lost resource ownership');
  for(let i=0;i<4;i++){finish.shift()?.();await Bun.sleep(0);}
  console.log(JSON.stringify({passed:true,spokenInputs:sent.length,devicePinning:true,correctionReceipt:true,durableOutbox:true,ttsFailureDoesNotBlockInput:true,bargeIn:true,lateCancellation:true,acceptedWorkPreserved:true,realMicrophoneOrProvider:false}));
}finally{
  controller.stop();for(const box of boxes.values())box.dispose();globalThis.fetch=nativeFetch;server.stop(true);
}
