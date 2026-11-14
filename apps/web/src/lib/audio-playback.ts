export function playbackVolume(value:number){return Number.isFinite(value)?Math.max(0,Math.min(1,value)):0;}
const NO_PCM_CARRY=new Uint8Array();

/** Raw S16LE stereo packets need not end on a sample or frame boundary. */
export class PcmDecoder {
 private carry=NO_PCM_CARRY;
 reset(){this.carry=NO_PCM_CARRY;}
 decode(part:Uint8Array):[Float32Array<ArrayBuffer>,Float32Array<ArrayBuffer>]|null {
  let bytes=part;
  if(this.carry.length){
   bytes=new Uint8Array(this.carry.length+part.length);
   bytes.set(this.carry);bytes.set(part,this.carry.length);
  }
  const complete=bytes.length-bytes.length%4;
  this.carry=complete===bytes.length?NO_PCM_CARRY:bytes.slice(complete);
  if(!complete)return null;
  // Live playback catches up after a stalled transport instead of playing old audio.
  const start=Math.max(0,complete-38400),view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
  const channels:[Float32Array<ArrayBuffer>,Float32Array<ArrayBuffer>]=[new Float32Array((complete-start)/4),new Float32Array((complete-start)/4)];
  for(let i=0;i<channels[0].length;i++){
   channels[0][i]=view.getInt16(start+i*4,true)/32768;
   channels[1][i]=view.getInt16(start+i*4+2,true)/32768;
  }
  return channels;
 }
}

/** One device owns one audio graph; network connections never own/suspend it. */
export class AudioPlayback {
 private graph:{context:AudioContext;gain:GainNode}|null=null;
 private decoder=new PcmDecoder();
 private sources=new Set<AudioBufferSourceNode>();
 private nextAt=0;
 private volume=1;
 constructor(
  private readonly create=()=>new AudioContext(),
  private readonly onRunningChange?:(running:boolean)=>void,
 ){}
 async resume(){
  if(!this.graph){
   const context=this.create(),gain=context.createGain();
   context.onstatechange=()=>this.onRunningChange?.(context.state==='running');
   gain.gain.value=this.volume;gain.connect(context.destination);
   this.graph={context,gain};
  }
  const {context}=this.graph;
  await context.resume();
  const running=context.state==='running';
  this.onRunningChange?.(running);
  if(!running)throw Error('声音未开启，请再次点击');
 }
 setVolume(value:number){
  this.volume=playbackVolume(value);
  const graph=this.graph;
  if(graph)graph.gain.gain.setTargetAtTime(this.volume,graph.context.currentTime,.02);
 }
 reset(){
  this.decoder.reset();this.clearScheduled();
 }
 private clearScheduled(){
  this.nextAt=0;
  for(const source of this.sources){source.stop();source.disconnect();}
  this.sources.clear();
 }
 push(part:Uint8Array){
  const graph=this.graph,channels=this.decoder.decode(part);
  if(!graph||graph.context.state!=='running'||!channels)return false;
  const {context:audio,gain}=graph;
  const buffer=audio.createBuffer(2,channels[0].length,48000);
  buffer.copyToChannel(channels[0],0);buffer.copyToChannel(channels[1],1);
  if(this.nextAt<audio.currentTime||this.nextAt-audio.currentTime>.35){
   this.clearScheduled();this.nextAt=audio.currentTime+.06;
  }
  const source=audio.createBufferSource();source.buffer=buffer;source.connect(gain);
  this.sources.add(source);
  source.onended=()=>{this.sources.delete(source);source.disconnect();};
  source.start(this.nextAt);this.nextAt+=buffer.duration;
  return true;
 }
 suspend(){this.reset();void this.graph?.context.suspend();}
 close(){
  this.reset();
  const graph=this.graph;this.graph=null;
  if(graph){graph.gain.disconnect();graph.context.onstatechange=null;void graph.context.close();}
 }
}
