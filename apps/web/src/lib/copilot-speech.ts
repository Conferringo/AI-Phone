import {toBinary,fromBinary,type MessageShape} from '@bufbuild/protobuf';
import {VoiceSpeechRequestSchema,VoiceSpeechStreamOutputSchema} from '@ai-phone/protocol/ai_phone/v1/voice_pb';

export function speechSocket(){return new WebSocket(`${location.protocol==='https:'?'wss':'ws'}://${location.host}/api/voice/playback`);}
/** One request lifetime; closing playback also closes the server's provider response. */
export async function* speechFrames(request:MessageShape<typeof VoiceSpeechRequestSchema>,signal:AbortSignal,open:()=>WebSocket=speechSocket):AsyncGenerator<Uint8Array>{
  if(signal.aborted)return;
  const socket=open(),chunks:Uint8Array[]=[];let done=false,error:Error|null=null,total=0,wake=()=>{};
  socket.binaryType='arraybuffer';
  const abort=()=>{done=true;socket.close();wake();};
  socket.onopen=()=>{if(!signal.aborted)socket.send(toBinary(VoiceSpeechRequestSchema,request));};
  socket.onmessage=event=>{
    try{
      const frame=fromBinary(VoiceSpeechStreamOutputSchema,new Uint8Array(event.data)).event;
      if(frame.case==='wavChunk'){
        total+=frame.value.length;if(total>16*1024*1024)throw Error('语音服务响应过大');chunks.push(frame.value);
      }else if(frame.case==='done'&&frame.value)done=true;
      else if(frame.case==='error')throw Error(frame.value.message);
      else throw Error('语音播放响应格式错误');
    }catch(e){error=e instanceof Error?e:Error('语音播放响应格式错误');done=true;socket.close();}
    wake();
  };
  socket.onerror=()=>{error=Error('语音播放连接失败');done=true;wake();};
  socket.onclose=()=>{if(!done&&!signal.aborted)error=Error('语音播放连接中断');done=true;wake();};
  signal.addEventListener('abort',abort,{once:true});
  try{
    while(!signal.aborted){
      if(error)throw error;
      const bytes=chunks.shift();if(bytes){yield bytes;continue;}
      if(done)break;
      await new Promise<void>(resolve=>{wake=resolve;});
    }
  }finally{signal.removeEventListener('abort',abort);socket.onopen=null;socket.onmessage=null;socket.onerror=null;socket.onclose=null;socket.close();}
}
