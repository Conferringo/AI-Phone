export function playbackVolume(value:number){return Number.isFinite(value)?Math.max(0,Math.min(1,value)):0;}
export function applyPlaybackVolume(gain:Pick<GainNode,'gain'>|null,value:number,time:number){
 const volume=playbackVolume(value);gain?.gain.setTargetAtTime(volume,time,.02);return volume;
}

/** Raw S16LE stereo packets need not end on a sample or frame boundary. */
export class PcmDecoder {
 private carry=new Uint8Array();
 reset(){this.carry=new Uint8Array();}
 decode(part:Uint8Array):[Float32Array<ArrayBuffer>,Float32Array<ArrayBuffer>]|null {
  const bytes=new Uint8Array(this.carry.length+part.length);
  bytes.set(this.carry);bytes.set(part,this.carry.length);
  const complete=bytes.length-bytes.length%4;
  this.carry=bytes.slice(complete);
  if(!complete)return null;
  // Live playback catches up after a stalled transport instead of playing old audio.
  const start=Math.max(0,complete-38400),view=new DataView(bytes.buffer);
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
 private context:AudioContext|null=null;
 private gain:GainNode|null=null;
 private decoder=new PcmDecoder();
 private sources=new Set<AudioBufferSourceNode>();
 private nextAt=0;
 private volume=1;
 constructor(private readonly create=()=>new AudioContext()){}
 async resume(){
  if(!this.context){
   this.context=this.create();this.gain=this.context.createGain();
   this.gain.gain.value=this.volume;this.gain.connect(this.context.destination);
  }
  await this.context.resume();
  if(this.context.state!=='running')throw Error('声音未开启，请再次点击');
 }
 setVolume(value:number){
  this.volume=playbackVolume(value);
  applyPlaybackVolume(this.gain,this.volume,this.context?.currentTime||0);
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
  const audio=this.context;
  if(!audio||audio.state!=='running')throw Error('声音播放已暂停，请重新开启');
  const channels=this.decoder.decode(part);if(!channels)return;
  const buffer=audio.createBuffer(2,channels[0].length,48000);
  buffer.copyToChannel(channels[0],0);buffer.copyToChannel(channels[1],1);
  if(this.nextAt<audio.currentTime||this.nextAt-audio.currentTime>.35){
   this.clearScheduled();this.nextAt=audio.currentTime+.06;
  }
  const source=audio.createBufferSource();source.buffer=buffer;source.connect(this.gain!);
  this.sources.add(source);
  source.onended=()=>{this.sources.delete(source);source.disconnect();};
  source.start(this.nextAt);this.nextAt+=buffer.duration;
 }
 suspend(){this.reset();void this.context?.suspend();}
 close(){
  this.reset();this.gain?.disconnect();this.gain=null;
  const context=this.context;this.context=null;void context?.close();
 }
}
