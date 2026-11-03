export type SpokenReply={id:string;text:string;ordinal:bigint;complete:boolean};
function readable(text:string){return text.replace(/```[\s\S]*?(?:```|$)/g,'').replace(/!?\[([^\]]*)\]\([^)]+\)/g,'$1').replace(/[*_`#]+/g,'');}
export function phrase(text:string,complete:boolean):string {
  const punctuation=text.search(/[。！？.!?\n]/u);
  if(punctuation>=0)return text.slice(0,punctuation+1);
  if(Array.from(text).length>=180)return Array.from(text).slice(0,180).join('');
  return complete?text:'';
}
type Pending<T>={scope:string;ordinal:bigint;target:T;text:string;offset:number;complete:boolean};
/** One audible output owner. Speaking never blocks an Agent or input capture. */
export class CopilotOutput<T>{
  private pending=new Map<string,Pending<T>>();
  private baselines=new Map<string,bigint>();
  private active:AbortController|null=null;
  private held=false;
  private running=false;
  constructor(private speak:(target:T,text:string,signal:AbortSignal)=>Promise<void>,private failure:(message:string)=>void){}
  feed(scope:string,target:T,replies:SpokenReply[]){
    if(!this.baselines.has(scope)){
      this.baselines.set(scope,replies.reduce((n,r)=>r.ordinal>n?r.ordinal:n,0n));
      for(const reply of replies){const text=readable(reply.text);this.pending.set(scope+':'+reply.id,{scope,ordinal:reply.ordinal,target,text,offset:text.length,complete:reply.complete});}
      return;
    }
    for(const reply of replies){
      const key=scope+':'+reply.id,text=readable(reply.text),old=this.pending.get(key);
      if(!old&&reply.ordinal<=this.baselines.get(scope)!)continue;
      this.pending.set(key,{scope,ordinal:reply.ordinal,target,text,complete:reply.complete,offset:old?(text.startsWith(old.text)?old.offset:text.length):0});
    }
    // Keep offsets for the supplied view, including late suffixes. Retired history
    // stays behind an ordinal fence and cannot be replayed when scrolled back in.
    const visible=new Set(replies.map(r=>scope+':'+r.id));
    for(const [key,p]of this.pending)if(p.scope===scope&&!visible.has(key)&&p.complete&&p.offset>=p.text.length){this.baselines.set(p.scope,p.ordinal>this.baselines.get(p.scope)!?p.ordinal:this.baselines.get(p.scope)!);this.pending.delete(key);}
    void this.pump();
  }
  hold(){this.held=true;this.active?.abort();for(const p of this.pending.values())p.offset=p.text.length;}
  release(){this.held=false;void this.pump();}
  close(){this.hold();this.pending.clear();this.baselines.clear();}
  private async pump(){
    if(this.running||this.held)return;this.running=true;
    try{
      for(;;){
        if(this.held)break;
        const entry=[...this.pending.entries()].find(([,p])=>phrase(p.text.slice(p.offset),p.complete));
        if(!entry)break;
        const [key,next]=entry,start=next.offset,text=phrase(next.text.slice(start),next.complete);
        if(!text.trim()){next.offset+=text.length;continue;}
        const control=new AbortController();this.active=control;
        try{await this.speak(next.target,text,control.signal);const current=this.pending.get(key);if(current&&!control.signal.aborted)current.offset=Math.max(current.offset,start+text.length);}
        catch(e){if(!control.signal.aborted){this.held=true;this.failure(e instanceof Error?e.message:'语音播放失败');break;}}
        finally{if(this.active===control)this.active=null;}
      }
    }finally{this.running=false;}
  }
}
