/** Audio-time endpointing. This module never sends messages or owns an Agent. */
export function pcm16(input:Float32Array,rate:number):Int16Array {
  if(rate<16000||!Number.isFinite(rate))throw Error('Unsupported microphone sample rate');
  const ratio=rate/16000,output=new Int16Array(Math.floor(input.length/ratio));
  for(let i=0;i<output.length;i++){
    const start=Math.floor(i*ratio),end=Math.max(start+1,Math.floor((i+1)*ratio));
    let sum=0;for(let j=start;j<end;j++)sum+=input[j];
    const value=Math.max(-1,Math.min(1,sum/(end-start)));
    output[i]=Math.round(value*(value<0?32768:32767));
  }
  return output;
}
export function wav(frames:readonly Int16Array[]):Uint8Array {
  const samples=frames.reduce((n,f)=>n+f.length,0),bytes=new Uint8Array(44+samples*2),view=new DataView(bytes.buffer);
  const text=(offset:number,value:string)=>{for(let i=0;i<value.length;i++)bytes[offset+i]=value.charCodeAt(i);};
  text(0,'RIFF');view.setUint32(4,bytes.length-8,true);text(8,'WAVE');text(12,'fmt ');
  view.setUint32(16,16,true);view.setUint16(20,1,true);view.setUint16(22,1,true);view.setUint32(24,16000,true);
  view.setUint32(28,32000,true);view.setUint16(32,2,true);view.setUint16(34,16,true);text(36,'data');view.setUint32(40,samples*2,true);
  let at=44;for(const frame of frames)for(const value of frame){view.setInt16(at,value,true);at+=2;}
  return bytes;
}
type Frame={samples:Int16Array;at:number;ms:number};
type Segment<T>={id:string;target:T;frames:Frame[];last:number;voiced:number;active:boolean;hasAudio:boolean;started:number};
export type SpeechEvents<T>={
  choose:()=>T;
  start:(id:string,target:T,at:number)=>void;
  speech:(id:string,target:T,at:number)=>void;
  chunk:(id:string,target:T,frames:Int16Array[],final:boolean)=>void;
  end:(id:string,target:T,at:number)=>void;
  stream?:(id:string,target:T,frames:Int16Array[],final:boolean)=>void;
};
function energy(frame:Int16Array){let sum=0;for(const n of frame)sum+=n*n;return Math.sqrt(sum/Math.max(1,frame.length))/32768;}
/** Short pauses can stream a draft; only 1500 ms of actual silence seals input. */
export class SpeechSegments<T>{
  private segment:Segment<T>|null=null;
  private before:Frame[]=[];
  private floor=.002;
  constructor(private events:SpeechEvents<T>,private id=()=>crypto.randomUUID(),private phraseDrafts:((target:T)=>boolean)=()=>true,private live:((target:T)=>boolean)=()=>!!events.stream){}
  push(samples:Int16Array,at:number){
    const frame={samples,at,ms:samples.length/16};
    const level=energy(samples),voiced=level>Math.max(.008,this.floor*3);
    if(!voiced)this.floor=this.floor*.95+level*.05;
    if(!this.segment){
      if(!voiced){this.before=[...this.before.slice(-2),frame];return;}
      this.segment={id:this.id(),target:this.events.choose(),frames:[...this.before],last:at,voiced:0,active:false,hasAudio:false,started:at};
      this.before=[];
    }
    this.receive(this.segment,frame,voiced);
  }
  private receive(s:Segment<T>,frame:Frame,voiced:boolean){
    s.frames.push(frame);
    if(voiced){s.last=frame.at;s.voiced+=frame.ms;s.hasAudio=true;}
    if(!s.active){this.confirm(s,frame);return;}
    if(voiced)this.events.speech(s.id,s.target,frame.at);
    if(this.live(s.target)){
      this.emitLive(s,frame.at-s.last>=1500);
      if(frame.at-s.last>=1500){this.events.end(s.id,s.target,s.last+1500);this.segment=null;}
      return;
    }
    this.receiveChunked(s,frame.at);
  }
  private receiveChunked(s:Segment<T>,at:number){
    const quiet=at-s.last,duration=s.frames.reduce((n,f)=>n+f.ms,0);
    if(quiet>=1500){
      if(s.hasAudio)this.emit(s,true);
      this.events.end(s.id,s.target,s.last+1500);this.segment=null;
    }else if(s.hasAudio&&(duration>=28000||(this.phraseDrafts(s.target)&&duration>=2000&&quiet>=250)))this.emit(s,false);
  }
  private confirm(s:Segment<T>,frame:Frame){
    if(s.voiced>=100){s.active=true;this.events.start(s.id,s.target,s.started);this.events.speech(s.id,s.target,s.last);if(this.live(s.target))this.emitLive(s,false);}
    else if(frame.at-s.last>=200){this.segment=null;this.before=[];}
  }
  private emit(s:Segment<T>,final:boolean){
    const frames=s.frames.filter(f=>f.at<=s.last+150).map(f=>f.samples);
    s.frames=[];s.hasAudio=false;this.events.chunk(s.id,s.target,frames,final);
  }
  private emitLive(s:Segment<T>,final:boolean){
    const frames=s.frames.map(f=>f.samples);s.frames=[];
    this.events.stream!(s.id,s.target,frames,final);
  }
  finish(at:number){
    const s=this.segment;this.segment=null;this.before=[];
    if(!s?.active)return;
    if(this.live(s.target))this.emitLive(s,true);
    else if(s.hasAudio)this.emit(s,true);
    this.events.end(s.id,s.target,at);
  }
}
