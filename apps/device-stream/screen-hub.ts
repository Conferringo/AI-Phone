import type {ServerResponse} from 'node:http';
export type RawFrame={format:{width:number;height:number;rotation?:{rotation?:string|number}};image:Buffer;timestampUs?:string|number;seq?:number;width?:number;height?:number};
export type JpegFrame={jpeg:Buffer;packet:Buffer;capturedAt:number;verifiedAt:number;sequence:number};
type Viewer={response:ServerResponse;blocked:boolean;blockedAt:number;pending:JpegFrame|null;drain:()=>void;close:()=>void};
/** A bounded latest-frame queue shared by viewers. Each slow viewer retains one replacement. */
export class ScreenHub{
 private viewers=new Map<ServerResponse,Viewer>();private pending:{raw:RawFrame;rotation:number;epoch:number;verifiedAt:number}|null=null;private encoding=false;private timer:ReturnType<typeof setTimeout>|null=null;private lastEncodedAt=0;private epoch=0;private latest:JpegFrame|null=null;
 private received=0;private encoded=0;private coalesced=0;private expired=0;private failures=0;
 constructor(private encode:(frame:RawFrame,rotation:number)=>Promise<Buffer>,private empty:()=>void,private intervalMs=1000/30){}
 get size(){return this.viewers.size;}
 get cached(){return this.latest;}
 get generation(){return this.epoch;}
 stats(){return{viewers:this.size,epoch:this.epoch,receivedFrames:this.received,encodedFrames:this.encoded,coalescedFrames:this.coalesced,discardedOldGeneration:this.expired,encodingFailures:this.failures,blockedViewers:[...this.viewers.values()].filter(v=>v.blocked).length,verifiedAgeMs:this.latest?Date.now()-this.latest.verifiedAt:null,captureAgeMs:this.latest?Math.max(0,Date.now()-this.latest.capturedAt):null,sequence:this.latest?.sequence||0};}
 add(response:ServerResponse){
  const viewer:Viewer={response,blocked:false,blockedAt:0,pending:null,drain:()=>{viewer.blocked=false;viewer.blockedAt=0;const latest=viewer.pending;viewer.pending=null;if(latest)this.send(viewer,latest);},close:()=>{if(!this.viewers.delete(response))return;response.off('drain',viewer.drain);response.off('close',viewer.close);response.off('error',viewer.close);if(!this.size)this.empty();}};
  this.viewers.set(response,viewer);response.on('drain',viewer.drain);response.once('close',viewer.close);response.once('error',viewer.close);response.socket?.setNoDelay(true);
  // Old cache is never a new connection's first picture. The capture watchdog refreshes it.
  if(this.latest&&Date.now()-this.latest.verifiedAt<=1500)this.send(viewer,this.latest);
 }
 offer(raw:RawFrame,rotation:number,verifiedAt=Date.now()){
  this.received++;if(this.pending)this.coalesced++;this.pending={raw,rotation,epoch:this.epoch,verifiedAt};this.schedule();
 }
 heartbeat(){if(!this.latest)return;const verifiedAt=Date.now(),previous=this.latest;const head=Buffer.from(`--phone\r\nContent-Type: image/jpeg\r\nContent-Length: ${previous.jpeg.length}\r\nX-Capture-Time: ${previous.capturedAt}\r\nX-Verified-Time: ${verifiedAt}\r\nX-Frame-Sequence: ${previous.sequence}\r\n\r\n`);const fresh={...previous,verifiedAt,packet:Buffer.concat([head,previous.jpeg,Buffer.from('\r\n')])};this.latest=fresh;this.broadcast(fresh);}
 advance(preserveVerified=false){this.epoch++;this.pending=null;if(this.timer){clearTimeout(this.timer);this.timer=null;}if(!preserveVerified)this.latest=null;}
 close(){this.advance();for(const viewer of [...this.viewers.values()])viewer.response.destroy();}
 sweep(){const now=Date.now();for(const viewer of [...this.viewers.values()])if(viewer.blocked&&now-viewer.blockedAt>2500)viewer.response.destroy();}
 private schedule(){if(this.encoding||this.timer||!this.pending||!this.size)return;const wait=Math.max(0,Math.ceil(this.intervalMs-(Date.now()-this.lastEncodedAt)));if(wait){this.timer=setTimeout(()=>{this.timer=null;this.schedule();},wait);return;}void this.pump();}
 private async pump(){
  const item=this.pending;if(!item||!this.size)return;this.pending=null;this.encoding=true;
  try{const jpeg=await this.encode(item.raw,item.rotation);if(item.epoch!==this.epoch){this.expired++;return;}const capturedAt=Math.round(Number(item.raw.timestampUs||0)/1000)||item.verifiedAt,sequence=++this.encoded;const head=Buffer.from(`--phone\r\nContent-Type: image/jpeg\r\nContent-Length: ${jpeg.length}\r\nX-Capture-Time: ${capturedAt}\r\nX-Verified-Time: ${item.verifiedAt}\r\nX-Frame-Sequence: ${sequence}\r\n\r\n`);this.latest={jpeg,packet:Buffer.concat([head,jpeg,Buffer.from('\r\n')]),capturedAt,verifiedAt:item.verifiedAt,sequence};this.lastEncodedAt=Date.now();this.broadcast(this.latest);}
  catch{this.failures++;}
  finally{this.encoding=false;this.schedule();}
 }
 private broadcast(frame:JpegFrame){for(const viewer of this.viewers.values())this.send(viewer,frame);}
 private send(viewer:Viewer,frame:JpegFrame){
  if(viewer.response.destroyed||viewer.response.writableEnded)return;
  if(viewer.blocked){viewer.pending=frame;return;}
  try{if(!viewer.response.write(frame.packet)){viewer.blocked=true;viewer.blockedAt=Date.now();}}
  catch{viewer.response.destroy();}
 }
}
