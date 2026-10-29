export type OutboxAttachment={id:string;name:string;mediaType:string;sizeBytes:string};
export type OutboxInput={id:string;conversationId:string;deviceId:string;text:string;attachments:OutboxAttachment[]};
export type OutboxEntry=OutboxInput&{status:'queued'|'sending'|'failed';error:string};
type Storage={read:()=>string|null;write:(value:string)=>void};

/** One owner for receipt IDs, FIFO delivery and acknowledgement recovery. Drafts live outside it. */
export class ChatOutbox<T> {
  entries:OutboxEntry[];
  private busy=false;
  private disposed=false;
  private abort=new AbortController();
  constructor(private storage:Storage,private send:(input:OutboxInput,signal:AbortSignal)=>Promise<T>,private accepted:(result:T)=>void,private changed:(entries:OutboxEntry[])=>void=()=>{}){
    const value:unknown=JSON.parse(storage.read()||'[]');
    if(!Array.isArray(value)||value.some(item=>!validInput(item)))throw Error('待发送消息无法读取，请保留本机数据并重试');
    this.entries=value.map(item=>({...item,status:'queued',error:''}));
  }
  enqueue(input:OutboxInput){
    if(this.disposed)throw Error('对话正在切换，请重试');
    if(this.entries.some(item=>item.id===input.id))throw Error('重复消息标识');
    this.commit([...this.entries,{...structuredClone(input),status:'queued',error:''}]);
    this.start();
  }
  start(){if(!this.busy&&!this.disposed&&this.entries[0]?.status==='queued')void this.drain();}
  retry(){
    this.commit(this.entries.map(item=>({...item,status:'queued',error:''})));
    this.start();
  }
  dispose(){this.disposed=true;this.abort.abort();}
  private commit(entries:OutboxEntry[]){
    // Save before clearing the composer or removing an acknowledged item.
    this.storage.write(JSON.stringify(entries));
    this.entries=entries;this.changed(entries);
  }
  private async drain(){
    this.busy=true;
    try{
      while(!this.disposed&&this.entries[0]?.status==='queued'){
        const current=this.entries[0];
        try{
          this.commit([{...current,status:'sending'},...this.entries.slice(1)]);
          const result=await this.send(current,this.abort.signal);
          if(this.disposed)return;
          this.accepted(result);
          this.commit(this.entries.filter(item=>item.id!==current.id));
        }catch(error){
          if(this.disposed)return;
          // Persistence might itself have failed: keep the in-memory receipt too.
          this.entries=this.entries.map(item=>item.id===current.id?{...item,status:'failed',error:error instanceof Error?error.message:'发送失败'}:item);
          this.changed(this.entries);
          break;
        }
      }
    }finally{this.busy=false;}
  }
}
function validInput(value:unknown):value is OutboxInput{
  if(!value||typeof value!=='object')return false;
  const item=value as OutboxInput;
  return ['id','conversationId','deviceId','text'].every(key=>typeof item[key as keyof OutboxInput]==='string')&&Array.isArray(item.attachments)&&item.attachments.every(file=>file&&['id','name','mediaType','sizeBytes'].every(key=>typeof file[key as keyof OutboxAttachment]==='string'));
}
