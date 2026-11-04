import {create,fromBinary,toBinary,type MessageShape,type MessageInitShape} from '@bufbuild/protobuf';
import {VoiceStreamInputSchema,VoiceStreamOutputSchema,VoiceStreamStartSchema,VoiceStreamTranscriptSchema} from '@ai-phone/protocol/ai_phone/v1/voice_pb';

type Start=MessageShape<typeof VoiceStreamStartSchema>;
type Transcript=MessageShape<typeof VoiceStreamTranscriptSchema>;
export function voiceSocket(){const url=new URL('/api/voice/stream',location.href);url.protocol=url.protocol==='https:'?'wss:':'ws:';return new WebSocket(url);}
/** One bounded, persistent transcription connection; accepted chat has a separate owner. */
export class CopilotStream {
  private socket:WebSocket;
  private queue:Uint8Array[]=[];
  private bytes=0;
  private opened=false;
  private stopped=false;
  private prepared:Promise<void>;
  private resolve!:()=>void;
  private reject!:(error:Error)=>void;
  constructor(start:Start,private transcript:(patch:Transcript)=>void,private failed:(message:string)=>void,open:()=>WebSocket=voiceSocket){
    this.prepared=new Promise((resolve,reject)=>{this.resolve=resolve;this.reject=reject;});
    void this.prepared.catch(()=>{});
    this.socket=open();this.socket.binaryType='arraybuffer';
    this.socket.onopen=()=>this.socket.send(toBinary(VoiceStreamInputSchema,create(VoiceStreamInputSchema,{event:{case:'start',value:start}})));
    this.socket.onmessage=event=>{
      if(this.stopped)return;
      try{
        this.receive(event.data);
      }catch(error){this.fail(error instanceof Error?error.message:'实时语音响应无效');}
    };
    this.socket.onerror=()=>this.fail('实时语音连接失败');
    this.socket.onclose=()=>this.fail('实时语音连接已断开');
  }
  private receive(data:unknown){
        if(!(data instanceof ArrayBuffer))throw Error('实时语音需要protobuf帧');
        const message=fromBinary(VoiceStreamOutputSchema,new Uint8Array(data)).event;
        switch(message.case){
          case 'ready':
            this.activate(message.value);break;
          case 'transcript':if(!this.opened)throw Error('实时语音尚未就绪');this.transcript(message.value);break;
          case 'error':throw Error(message.value.message);
          default:throw Error('实时语音响应无效');
        }
  }
  private activate(ready:boolean){
    if(!ready||this.opened)throw Error('实时语音会话响应无效');
    this.opened=true;for(const bytes of this.queue)this.write(bytes);this.queue=[];this.bytes=0;this.resolve();
  }
  ready(){return this.prepared;}
  private write(bytes:Uint8Array){
    if(this.socket.bufferedAmount+bytes.length>256*1024)throw Error('实时语音发送积压，录音已暂停');
    this.socket.send(bytes);
  }
  private send(event:MessageInitShape<typeof VoiceStreamInputSchema>['event']){
    if(this.stopped)throw Error('实时语音会话已关闭');
    const bytes=toBinary(VoiceStreamInputSchema,create(VoiceStreamInputSchema,{event}));
    if(this.opened)this.write(bytes);
    else{if(this.bytes+bytes.length>256*1024)throw Error('实时语音连接积压，录音已暂停');this.queue.push(bytes);this.bytes+=bytes.length;}
  }
  audio(id:string,frames:readonly Int16Array[],final:boolean){
    for(const frame of frames)for(let at=0;at<frame.length;at+=1600){
      const count=Math.min(1600,frame.length-at),pcm16=new Uint8Array(count*2),view=new DataView(pcm16.buffer);
      for(let i=0;i<count;i++)view.setInt16(i*2,frame[at+i],true);
      this.send({case:'audio',value:{utteranceId:id,pcm16}});
    }
    if(final)this.send({case:'commit',value:{utteranceId:id}});
  }
  private fail(message:string){if(this.stopped)return;this.close();this.failed(message);}
  close(){
    if(this.stopped)return;this.stopped=true;this.queue=[];this.bytes=0;this.reject(Error('实时语音会话已关闭'));
    this.socket.onopen=null;this.socket.onmessage=null;this.socket.onerror=null;this.socket.onclose=null;this.socket.close();
  }
}
