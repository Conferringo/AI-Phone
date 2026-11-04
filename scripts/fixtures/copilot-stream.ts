import {strict as assert} from 'node:assert';
import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {VoiceSettingsSchema,VoiceStreamInputSchema,VoiceStreamOutputSchema,VoiceSettingsRequestSchema,VoiceStreamTranscriptSchema} from '../../packages/protocol/src/gen/ai_phone/v1/voice_pb';
import {CopilotSession,type MicrophonePort,type CopilotBinding} from '../../apps/web/src/lib/copilot-session';

async function main(){
  const sent:Array<{text:string;deviceId:string;voice?:{sessionId:string}}>=[],recovered:string[]=[];
  const received:Array<{event:string;id:string}>=[],peers:Array<{close:()=>void}>=[];
  let commits=0,failNext=false;
  const server=Bun.serve({port:0,hostname:'127.0.0.1',fetch(request,server){if(server.upgrade(request))return;return new Response(null,{status:400});},websocket:{
    message(peer,bytes){
      const event=fromBinary(VoiceStreamInputSchema,new Uint8Array(bytes as Uint8Array)).event;
      const reply=(event:MessageShape<typeof VoiceStreamOutputSchema>['event'])=>peer.send(toBinary(VoiceStreamOutputSchema,create(VoiceStreamOutputSchema,{event})));
      if(event.case==='start'){peers.push(peer);received.push({event:'start',id:event.value.sessionId});reply({case:'ready',value:true});}
      if(event.case==='audio'){
        assert(event.value.pcm16.length<=3200);received.push({event:'audio',id:event.value.utteranceId});
        reply({case:'transcript',value:create(VoiceStreamTranscriptSchema,{utteranceId:event.value.utteranceId,revision:received.filter(r=>r.id===event.value.utteranceId&&r.event==='audio').length,text:'打开天气',final:false})});
        if(failNext){failNext=false;peer.close();}
      }
      if(event.case==='commit'){received.push({event:'commit',id:event.value.utteranceId});commits++;reply({case:'transcript',value:create(VoiceStreamTranscriptSchema,{utteranceId:event.value.utteranceId,revision:100,text:'打开天气',final:true})});}
    },
  }});
  class Mic implements MicrophonePort {
    frame:((pcm:Int16Array,at:number)=>void)|null=null;closed=false;
    prepare(){return Promise.resolve();}async open(frame:(pcm:Int16Array,at:number)=>void){this.frame=frame;}
    async play(){}close(){this.frame=null;this.closed=true;}
  }
  const microphones:Mic[]=[],nativeFetch=globalThis.fetch;
  globalThis.fetch=(async(url,options)=>{
    if(typeof url==='string'&&url.endsWith('/api/voice/settings')){
      const input=fromBinary(VoiceSettingsRequestSchema,new Uint8Array(options?.body as Uint8Array));
      return new Response(toBinary(VoiceSettingsSchema,create(VoiceSettingsSchema,{agentId:input.agentId,revision:1n,configured:true,inputProtocol:3})),{headers:{'content-type':'application/x-protobuf'}});
    }
    throw Error('Live input unexpectedly used REST: '+url);
  }) as typeof fetch;
  const controller=new CopilotSession(()=>{throw Error('Unexpected configure');},()=>{const m=new Mic();microphones.push(m);return m;},()=>new WebSocket(String(server.url).replace('http:','ws:')));
  function binding(id:string):CopilotBinding{return {deviceId:id,agentId:'agent-'+id,conversationId:'conversation-'+id,ready:true,submit:input=>sent.push(input),recover:text=>recovered.push(text),replies:[]};}
  const wait=async(check:()=>boolean)=>{const until=performance.now()+2000;while(!check()){if(performance.now()>until)throw Error('Live fixture stalled: '+JSON.stringify({state:controller.snapshot(),sent,received:received.slice(-4),origin,now:performance.now()}));await Bun.sleep(2);}};
  const speech=new Int16Array(800).fill(3000),quiet=new Int16Array(800),origin=performance.now();
  function phrase(m:Mic,offset:number){for(let at=0;at<400;at+=50)m.frame!(speech,origin+offset+at);for(let at=400;at<=1850;at+=50)m.frame!(quiet,origin+offset+at);}
  try{
    controller.register(binding('phone'));controller.register(binding('desktop'));await controller.start('phone');const m=microphones[0];
    m.frame!(speech,origin);m.frame!(speech,origin+50);await wait(()=>!!controller.snapshot().drafts.phone?.text);
    assert.equal(sent.length,0);assert.equal(commits,0);controller.select('desktop');
    for(let at=100;at<=1550;at+=50)m.frame!(quiet,origin+at);await wait(()=>sent.length===1);
    assert.equal(sent[0].deviceId,'phone');assert.equal(commits,1);
    phrase(m,2000);await wait(()=>sent.length===2);assert.equal(sent[1].deviceId,'desktop');
    failNext=true;m.frame!(speech,origin+4000);m.frame!(speech,origin+4050);await wait(()=>!!controller.snapshot().error);
    assert(m.closed);await controller.retry();const resumed=microphones[1];
    phrase(resumed,6000);await wait(()=>sent.length===3);
    assert.equal(sent[2].voice?.sessionId,sent[1].voice?.sessionId);assert(recovered.includes('打开天气'));
    controller.stop();assert(resumed.closed);assert.equal(received.filter(r=>r.event==='start').length,3);
    assert.equal(new Set(received.filter(r=>r.event==='start').map(r=>r.id)).size,1);
  }finally{controller.stop();globalThis.fetch=nativeFetch;for(const p of peers)p.close();server.stop(true);}
}
await main();
console.log(JSON.stringify({passed:true,realMicrophoneOrProvider:false,incrementalBeforeCommit:true,reconnectPreservesSession:true}));
