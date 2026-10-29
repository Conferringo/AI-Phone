import type{RawFrame}from'./screen-hub';
export const sameImage=(a:RawFrame|undefined,b:RawFrame)=>!!a&&a.format.width===b.format.width&&a.format.height===b.format.height&&a.image.equals(b.image);
/** Distinguish a quiet scene from a stuck stream by checking actual current pixels. */
export class CaptureWatchdog{
 streamAt=0;raw:RawFrame|undefined;streamRaw:RawFrame|undefined;busy=false;failures=0;restarts=0;private mismatches=0;private recoveredAt=0;
 constructor(private dep:{size:()=>number;generation:()=>number;verifiedAt:()=>number|undefined;capture:()=>Promise<RawFrame>;offer:(frame:RawFrame)=>void;heartbeat:()=>void;restart:()=>void;close:()=>void;now?:()=>number}){}
 private now(){return this.dep.now?.()??Date.now();}
 accept(frame:RawFrame){this.streamAt=this.now();this.streamRaw=this.raw=frame;this.mismatches=0;this.failures=0;this.dep.offer(frame);}
 reset(){this.raw=this.streamRaw=undefined;this.streamAt=0;this.mismatches=0;}
 async tick(force=false){
  if(this.busy||!this.dep.size())return;const began=this.now(),verified=this.dep.verifiedAt();if(!force&&verified!==undefined&&began-this.streamAt<1000&&began-verified<1200)return;
  this.busy=true;const generation=this.dep.generation();
  try{const frame=await this.dep.capture();if(!this.dep.size()||this.dep.generation()!==generation||this.streamAt>began)return;this.failures=0;const unchanged=sameImage(this.raw,frame);this.mismatches=this.streamRaw&&!sameImage(this.streamRaw,frame)?this.mismatches+1:0;
   if(this.mismatches>=2&&this.now()-this.recoveredAt>5000){this.recoveredAt=this.now();this.restarts++;this.dep.restart();this.raw=frame;this.dep.offer(frame);}
   else if(unchanged&&verified!==undefined)this.dep.heartbeat();
   else{this.raw=frame;this.dep.offer(frame);}
  }catch{if(this.dep.generation()===generation&&++this.failures>=2)this.dep.close();}
  finally{this.busy=false;}
 }
}
