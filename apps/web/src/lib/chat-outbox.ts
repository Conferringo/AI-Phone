export type OutboxAttachment={id:string;name:string;mediaType:string;sizeBytes:string};
export type OutboxVoice={sessionId:string;utteranceId:string;revision:number;literal:string;correctsRequestId:string};
export type OutboxInput={id:string;conversationId:string;deviceId:string;text:string;attachments:OutboxAttachment[];voice?:OutboxVoice};
export type OutboxEntry=OutboxInput&{status:'queued'|'sending'|'failed'|'accepted';error:string};
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
    this.entries=value.map(item=>({...item,status:item.status==='accepted'?'accepted':'queued',error:''}));
  }
  enqueue(input:OutboxInput){
    if(this.disposed)throw Error('对话正在切换，请重试');
    if(!validInput(input))throw Error('待发送消息格式无效');
    if(this.entries.some(item=>item.id===input.id))throw Error('重复消息标识');
    this.commit([...this.entries,{...structuredClone(input),status:'queued',error:''}]);
    this.start();
  }
  start(){if(!this.busy&&!this.disposed&&this.nextDelivery()?.status==='queued')void this.drain();}
  retry(){
    this.commit(this.entries.map(item=>({...item,status:item.status==='accepted'?'accepted':'queued',error:''})));
    this.start();
  }
  /** A successful canonical projection, not a send retry, retires accepted input. */
  projected(requestIds:ReadonlySet<string>){
    const next=this.entries.filter(item=>item.status!=='accepted'||!requestIds.has(item.id));
    if(next.length!==this.entries.length)this.commit(next);
  }
  dispose(){this.disposed=true;this.abort.abort();}
  private nextDelivery(){return this.entries.find(item=>item.status!=='accepted');}
  private commit(entries:OutboxEntry[]){
    // Save before clearing the composer or removing an acknowledged item.
    this.storage.write(JSON.stringify(entries));
    this.entries=entries;this.changed(entries);
  }
  private recordError(id:string,status:'failed'|'accepted',error:unknown){
    const message=error instanceof Error&&error.message?error.message:status==='accepted'?'读取对话失败':'发送失败';
    this.entries=this.entries.map(item=>item.id===id?{...item,status,error:message}:item);
    this.changed(this.entries);
  }
  private confirm(id:string){
    // Keep known acceptance in memory even when this durable write fails. The old
    // disk input still retains the same ID for conservative idempotent recovery.
    this.entries=this.entries.map(item=>item.id===id?{...item,status:'accepted',error:''}:item);
    this.storage.write(JSON.stringify(this.entries));this.changed(this.entries);
  }
  private async deliver(current:OutboxEntry):Promise<boolean>{
    let result:T;
    try{
      this.commit(this.entries.map(item=>item.id===current.id?{...item,status:'sending'}:item));
      result=await this.send(current,this.abort.signal);
    }catch(error){
      if(!this.disposed)this.recordError(current.id,'failed',error);
      return false;
    }
    if(this.disposed)return false;
    try{this.confirm(current.id);}catch(error){this.recordError(current.id,'accepted',error);return false;}
    try{this.accepted(result);this.projected(new Set([current.id]));}
    catch(error){this.recordError(current.id,'accepted',error);}
    return true;
  }
  private async drain(){
    this.busy=true;
    try{
      let current=this.nextDelivery();
      while(!this.disposed&&current?.status==='queued'){
        if(!await this.deliver(current))break;
        current=this.nextDelivery();
      }
    }finally{this.busy=false;}
  }
}
function validInput(value:unknown):value is OutboxInput{
  if(!value||typeof value!=='object')return false;
  const item=value as OutboxInput;
  return ['id','conversationId','deviceId','text'].every(key=>typeof item[key as keyof OutboxInput]==='string')&&Array.isArray(item.attachments)&&item.attachments.every(file=>file&&['id','name','mediaType','sizeBytes'].every(key=>typeof file[key as keyof OutboxAttachment]==='string'))&&(item.voice===undefined||validVoice(item.voice));
}
function validVoice(value:OutboxVoice):boolean{
  return !!value&&['sessionId','utteranceId','literal','correctsRequestId'].every(key=>typeof value[key as keyof OutboxVoice]==='string')&&Number.isInteger(value.revision)&&value.revision>0&&value.revision<=0xffffffff;
}
