"use client";
import {useCallback,useEffect,useLayoutEffect,useRef,useState} from 'react';
import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ChatHistoryRequestSchema,ChatStateSchema} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {ChatOutbox,type OutboxEntry,type OutboxInput} from '@/lib/chat-outbox';
import {encodeChatSend} from '@/lib/chat-wire';
import type {AcceptedInput} from '@/lib/chat-snapshot';
import {historyRequest,receiptBatch} from '../lib/chat-window';

type Chat=MessageShape<typeof ChatStateSchema>;
type Binding=Pick<Chat,'conversationId'|'deviceId'>;
type ChatProjection={
  accepted:(chat:Chat,order:number)=>void;
  beginHistory:()=>number;
  acknowledged:(receipt:AcceptedInput,order:number)=>void;
  changed:(entries:OutboxEntry[])=>void;
  error:(message:string)=>void;
};
function sameBinding(value:Binding|null|undefined,conversationId:string,deviceId:string){
  return value?.conversationId===conversationId&&value.deviceId===deviceId;
}
function failureMessage(value:unknown,fallback:string){
  return value instanceof Error&&value.message?value.message:fallback;
}
async function chatResponse(response:Response,status:number):Promise<Chat>{
  const mediaType=response.headers.get('content-type')?.split(';',1)[0].trim().toLowerCase();
  if(mediaType!=='application/x-protobuf')throw Error('服务返回了无效的消息格式');
  const bytes=new Uint8Array(await response.arrayBuffer());
  if(response.status!==status){
    if(!response.ok)throw Error(fromBinary(ApiErrorSchema,bytes).message||'请求失败');
    throw Error('服务返回了无效的消息状态');
  }
  return fromBinary(ChatStateSchema,bytes);
}
function inputReceipt(chat:Chat,item:OutboxInput):AcceptedInput{
  const messages=chat.messages.filter(message=>message.requestId===item.id&&message.role==='user');
  const message=messages[0];
  if(!sameBinding(chat,item.conversationId,item.deviceId)||messages.length!==1||!message?.id||message.ordinal<=0n)throw Error('消息回执尚未确认，请重试');
  return{conversationId:item.conversationId,deviceId:item.deviceId,message};
}

/** One committed device/conversation owns transport, cancellation and projection. */
class BoundChatOutbox implements Binding{
  readonly box:ChatOutbox<AcceptedInput>;
  private disposed=false;
  private projection:AbortController|null=null;
  private receiptOffset=0;
  constructor(readonly conversationId:string,readonly deviceId:string,private callbacks:ChatProjection){
    const key=`ai-phone:outbox:${deviceId}:${conversationId}`;
    this.box=new ChatOutbox({read:()=>localStorage.getItem(key),write:value=>localStorage.setItem(key,value)},this.send.bind(this),this.acknowledge.bind(this),this.changed.bind(this));
  }
  start(){
    this.box.start();
    if(this.box.entries.some(item=>item.status==='accepted'))void this.refresh();
  }
  submit(item:OutboxInput){
    if(!sameBinding(item,this.conversationId,this.deviceId))throw Error('待发送消息不属于当前对话');
    this.box.enqueue(item);
  }
  retry(){
    try{this.callbacks.error('');this.box.retry();void this.refresh();}
    catch(value){this.callbacks.error(failureMessage(value,'重试失败'));}
  }
  dispose(){
    this.disposed=true;this.projection?.abort();this.box.dispose();
  }
  private active(control?:AbortController){return !this.disposed&&!control?.signal.aborted;}
  private changed(entries:OutboxEntry[]){if(this.active())this.callbacks.changed(entries);}
  private async send(item:OutboxInput,signal:AbortSignal):Promise<AcceptedInput>{
    if(!sameBinding(item,this.conversationId,this.deviceId))throw Error('待发送消息不属于当前对话');
    const response=await fetch('/api/assistant/send',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:encodeChatSend(item),signal:AbortSignal.any([signal,AbortSignal.timeout(30000)])});
    // The POST confirms this durable receipt; its original run status is not a UI snapshot.
    return inputReceipt(await chatResponse(response,202),item);
  }
  private acknowledge(receipt:AcceptedInput){
    if(!this.active())return;
    try{this.callbacks.acknowledged(receipt,this.callbacks.beginHistory());}finally{void this.refresh();}
  }
  private acceptedCount(){return this.box.entries.filter(item=>item.status==='accepted').length;}
  private async history(signal:AbortSignal){
    const batch=receiptBatch(this.box.entries,this.receiptOffset);this.receiptOffset=batch.next;
    const body=toBinary(ChatHistoryRequestSchema,create(ChatHistoryRequestSchema,historyRequest(this.conversationId,this.deviceId,null,batch.ids)));
    const response=await fetch('/api/assistant/history',{method:'POST',headers:{'content-type':'application/x-protobuf'},body,signal:AbortSignal.any([signal,AbortSignal.timeout(10000)])});
    return{chat:await chatResponse(response,200),count:batch.ids.length};
  }
  private projectHistory(chat:Chat,order:number){
    if(!sameBinding(chat,this.conversationId,this.deviceId))throw Error('对话状态与当前设备不一致');
    this.callbacks.accepted(chat,order);
    this.box.projected(new Set(chat.messages.filter(message=>message.role==='user'&&message.requestId&&message.id&&message.ordinal>0n).map(message=>message.requestId)));
    this.callbacks.error('');
  }
  private async recoverReceipts(control:AbortController){
    let remaining=this.acceptedCount();
    do{
      const before=this.acceptedCount(),order=this.callbacks.beginHistory();
      const {chat,count}=await this.history(control.signal);
      if(!this.active(control))return;
      this.projectHistory(chat,order);
      const after=this.acceptedCount();
      // Progress permits another sweep; missing canonical receipts get one
      // bounded sweep, never an automatic send or an unbounded retry loop.
      remaining=after<before?after:remaining-count;
    }while(remaining>0&&this.active(control));
  }
  private async refresh(){
    if(!this.active())return;
    this.projection?.abort();
    const control=new AbortController();this.projection=control;
    try{
      await this.recoverReceipts(control);
    }catch(value){
      if(this.active(control))this.callbacks.error(failureMessage(value,'读取对话失败'));
    }finally{
      if(this.projection===control)this.projection=null;
    }
  }
}

export function useChatOutbox(conversationId:string,deviceId:string,accepted:(chat:Chat,order:number)=>void,beginHistory:()=>number,acknowledged:(receipt:AcceptedInput,order:number)=>void){
  const owner=useRef<BoundChatOutbox|null>(null);
  const [entries,setEntries]=useState<OutboxEntry[]>([]),[error,setError]=useState(''),[ready,setReady]=useState(false);
  const disconnect=useCallback(()=>{
    const current=owner.current;
    if(sameBinding(current,conversationId,deviceId)){current!.dispose();owner.current=null;}
  },[conversationId,deviceId]);
  // Fence the old binding at commit, before late passive responses can project it.
  useLayoutEffect(()=>disconnect,[disconnect]);
  useEffect(()=>{
    setEntries([]);setError('');setReady(false);
    if(!conversationId)return;
    try{
      const current=new BoundChatOutbox(conversationId,deviceId,{accepted,beginHistory,acknowledged,changed:setEntries,error:setError});
      owner.current=current;setEntries(current.box.entries);setReady(true);current.start();
      return disconnect;
    }catch(value){setError(failureMessage(value,'待发送消息无法读取'));setReady(false);}
  },[conversationId,deviceId,accepted,beginHistory,acknowledged,disconnect]);
  const submit=useCallback((item:OutboxInput)=>{
    const current=owner.current;
    if(!sameBinding(current,conversationId,deviceId))throw Error('对话正在连接，请稍后重试');
    current!.submit(item);
  },[conversationId,deviceId]);
  const retry=useCallback(()=>{
    const current=owner.current;
    if(sameBinding(current,conversationId,deviceId))current!.retry();
  },[conversationId,deviceId]);
  const bound=sameBinding(owner.current,conversationId,deviceId);
  return{entries:bound?entries:[],error,ready:bound&&ready,submit,retry};
}
