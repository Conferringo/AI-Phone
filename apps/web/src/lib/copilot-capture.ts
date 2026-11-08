import {pcm16} from './copilot-audio';
import {playWave} from './copilot-playback';

export class Microphone {
  private context:AudioContext|null=null;
  private stream:MediaStream|null=null;
  private node:AudioWorkletNode|null=null;
  private source:MediaStreamAudioSourceNode|null=null;
  private generation=0;
  private playback:AbortController|null=null;
  /** Call from the activation gesture, before any configuration/network await. */
  async prepare():Promise<void>{
    if(!isSecureContext||!navigator.mediaDevices?.getUserMedia)throw Error('请通过本机地址或客户端使用麦克风');
    if(typeof AudioContext==='undefined')throw Error('当前客户端不支持麦克风');
    const context=new AudioContext();
    this.context=context;
    await Promise.all([context.resume(),context.audioWorklet.addModule('/copilot-capture.js')]);
  }
  async open(frame:(pcm:Int16Array,at:number)=>void,failed:(message:string)=>void){
    const context=this.context;if(!context)throw Error('麦克风尚未开启');
    const generation=this.generation;
    const stream=await navigator.mediaDevices.getUserMedia({audio:{channelCount:1,echoCancellation:true,noiseSuppression:true,autoGainControl:true},video:false});
    if(generation!==this.generation){stream.getTracks().forEach(t=>t.stop());return;}
    this.stream=stream;
    for(const track of stream.getAudioTracks())track.onended=()=>failed('麦克风已断开');
    const origin=performance.now()-context.currentTime*1000;
    this.node=new AudioWorkletNode(context,'ai-phone-copilot');
    this.node.port.onmessage=event=>{if(generation===this.generation)frame(pcm16(event.data.samples,event.data.rate),origin+event.data.atMs);};
    this.source=context.createMediaStreamSource(stream);this.source.connect(this.node);this.node.connect(context.destination);
  }
  async play(audio:(signal:AbortSignal)=>AsyncIterable<Uint8Array>,signal:AbortSignal){
    const context=this.context;if(!context||signal.aborted)return;
    const control=new AbortController();this.playback?.abort();this.playback=control;
    const abort=()=>control.abort();signal.addEventListener('abort',abort,{once:true});
    try{await playWave(context,audio(control.signal),control.signal);}
    finally{signal.removeEventListener('abort',abort);control.abort();if(this.playback===control)this.playback=null;}
  }
  close(){
    this.generation++;this.source?.disconnect();this.node?.disconnect();this.node?.port.close();this.node=null;this.source=null;
    this.stream?.getTracks().forEach(t=>{t.onended=null;t.stop();});this.stream=null;
    this.playback?.abort();this.playback=null;
    const context=this.context;this.context=null;if(context)void context.close();
  }
}
