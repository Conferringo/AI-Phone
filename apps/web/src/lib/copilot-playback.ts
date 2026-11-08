type Frame={rate:number;channels:Float32Array<ArrayBuffer>[]};
type Format={rate:number;channels:number;pcm:boolean};
const text=(bytes:Uint8Array,at:number)=>String.fromCharCode(...bytes.subarray(at,at+4));
function signature(bytes:Uint8Array){if(text(bytes,0)!=='RIFF'||text(bytes,8)!=='WAVE')throw Error('语音服务没有返回 WAV 音频');}
function formatAt(view:DataView,at:number,length:number):Format {
  if(length<16)throw Error('WAV 音频格式无效');
  const channels=view.getUint16(at+2,true),rate=view.getUint32(at+4,true);
  if(channels<1||channels>8||rate<8000||rate>192000)throw Error('WAV 音频格式无效');
  return {rate,channels,pcm:view.getUint16(at,true)===1&&view.getUint16(at+14,true)===16&&view.getUint16(at+12,true)===channels*2};
}
function pcm(bytes:Uint8Array,format:Format):Frame {
  const block=format.channels*2,frames=bytes.length/block,view=new DataView(bytes.buffer,bytes.byteOffset,bytes.length);
  const channels=Array.from({length:format.channels},()=>new Float32Array(frames));
  for(let frame=0;frame<frames;frame++)for(let channel=0;channel<channels.length;channel++)channels[channel][frame]=view.getInt16(frame*block+channel*2,true)/32768;
  return {rate:format.rate,channels};
}
/** Parse the container once. PCM16 starts immediately; other WAV codecs keep browser decoding. */
export class WaveDecoder {
  private pending=new Uint8Array();
  private header=true;
  private fallback=false;
  private buffered:Uint8Array[]=[];
  private format:Format|null=null;
  private remaining:number|null=null;
  private samples=0;
  push(bytes:Uint8Array):Frame[]{
    if(this.fallback){this.buffered.push(bytes);return [];}
    const joined=new Uint8Array(this.pending.length+bytes.length);joined.set(this.pending);joined.set(bytes,this.pending.length);this.pending=joined;
    if(this.header&&!this.readHeader())return [];
    if(this.fallback)return [];
    const format=this.format!,block=format.channels*2;
    const length=Math.floor(Math.min(this.pending.length,this.remaining??Infinity)/block)*block;
    if(!length)return [];
    const frame=pcm(this.pending.subarray(0,length),format);
    this.pending=this.pending.slice(length);if(this.remaining!==null)this.remaining-=length;this.samples+=length/block;
    return [frame];
  }
  private beginData(length:number,body:number){
    if(!this.format)throw Error('WAV 缺少音频格式');
    this.header=false;this.fallback=!this.format.pcm;
    if(this.fallback){this.buffered=[this.pending];this.pending=new Uint8Array();return;}
    const block=this.format.channels*2;
    if(length!==0xffffffff&&length%block)throw Error('WAV 音频帧不完整');
    this.remaining=length===0xffffffff?null:length;this.pending=this.pending.slice(body);
  }
  private readHeader(){
    if(this.pending.length<12)return false;
    signature(this.pending);
    const view=new DataView(this.pending.buffer,this.pending.byteOffset,this.pending.length);
    let at=12;
    while(at+8<=this.pending.length){
      const name=text(this.pending,at),length=view.getUint32(at+4,true),body=at+8;
      if(name==='data'){this.beginData(length,body);return true;}
      if(body+length>65536)throw Error('WAV 头部过大');
      if(body+length>this.pending.length)return false;
      if(name==='fmt ')this.format=formatAt(view,body,length);
      at=body+length+(length%2);
    }
    if(this.pending.length>65536)throw Error('WAV 头部过大');
    return false;
  }
  finish():Uint8Array|null {
    if(this.header)throw Error('WAV 头部不完整');
    if(this.fallback){
      const bytes=new Uint8Array(this.buffered.reduce((total,part)=>total+part.length,0));let at=0;
      for(const part of this.buffered){bytes.set(part,at);at+=part.length;}
      return bytes;
    }
    if((this.remaining!==null&&this.remaining!==0)||(this.remaining===null&&this.pending.length)||!this.samples)throw Error('WAV 音频不完整');
    return null;
  }
}
class AudioQueue {
  private sources=new Set<AudioBufferSourceNode>();
  private next:number;
  private drain=()=>{};
  constructor(private context:AudioContext,private signal:AbortSignal){this.next=context.currentTime;}
  private remove(source:AudioBufferSourceNode){
    if(!this.sources.delete(source))return;source.onended=null;source.disconnect();if(!this.sources.size)this.drain();
  }
  abort=()=>{for(const source of [...this.sources]){try{source.stop();}catch{}finally{this.remove(source);}}};
  schedule(buffer:AudioBuffer){
    if(this.signal.aborted)return;
    const source=this.context.createBufferSource();source.buffer=buffer;source.connect(this.context.destination);this.sources.add(source);source.onended=()=>this.remove(source);
    const at=Math.max(this.next,this.context.currentTime+.01);source.start(at);this.next=at+buffer.duration;
  }
  pcm(frame:Frame){
    const buffer=this.context.createBuffer(frame.channels.length,frame.channels[0].length,frame.rate);
    frame.channels.forEach((channel,index)=>buffer.copyToChannel(channel,index));this.schedule(buffer);
  }
  async finish(){if(this.sources.size)await new Promise<void>(resolve=>{this.drain=resolve;if(this.signal.aborted)this.abort();});}
}
/** The AudioContext owns timing; capture and Agent execution never wait for playback. */
export async function playWave(context:AudioContext,chunks:AsyncIterable<Uint8Array>,signal:AbortSignal){
  if(signal.aborted)return;
  const decoder=new WaveDecoder(),queue=new AudioQueue(context,signal);
  signal.addEventListener('abort',queue.abort,{once:true});
  try{
    for await(const bytes of chunks){if(signal.aborted)return;for(const frame of decoder.push(bytes))queue.pcm(frame);}
    if(signal.aborted)return;
    const fallback=decoder.finish();if(fallback)queue.schedule(await context.decodeAudioData(fallback.slice().buffer));
    await queue.finish();
  }finally{signal.removeEventListener('abort',queue.abort);queue.abort();}
}
