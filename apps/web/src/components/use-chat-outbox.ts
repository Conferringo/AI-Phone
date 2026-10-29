"use client";
import {useCallback,useEffect,useRef,useState} from 'react';
import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ChatHistoryRequestSchema,ChatSendRequestSchema,ChatStateSchema} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {ChatOutbox,type OutboxEntry,type OutboxInput} from '@/lib/chat-outbox';

type Chat=MessageShape<typeof ChatStateSchema>;
export function useChatOutbox(conversationId:string,deviceId:string,accepted:(chat:Chat,order:number)=>void,beginHistory:()=>number){
  const queue=useRef<ChatOutbox<{chat:Chat;order:number}>|null>(null);
  const [entries,setEntries]=useState<OutboxEntry[]>([]),[error,setError]=useState(''),[ready,setReady]=useState(false);
  useEffect(()=>{
    if(!conversationId)return;
    const key=`ai-phone:outbox:${deviceId}:${conversationId}`;
    try{
      const value=new ChatOutbox({read:()=>localStorage.getItem(key),write:value=>localStorage.setItem(key,value)},async (item,signal)=>{
        const response=await fetch('/api/assistant/send',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(ChatSendRequestSchema,create(ChatSendRequestSchema,{conversationId:item.conversationId,deviceId:item.deviceId,text:item.text,attachmentIds:item.attachments.map(file=>file.id),requestId:item.id})),signal:AbortSignal.any([signal,AbortSignal.timeout(30000)])});
        const bytes=new Uint8Array(await response.arrayBuffer());
        if(!response.ok)throw Error(fromBinary(ApiErrorSchema,bytes).message);
        // Send returns the original run receipt on retry. Read the current conversation
        // separately so an old receipt cannot replace a newer task's status in the UI.
        const order=beginHistory();
        const history=await fetch('/api/assistant/history',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(ChatHistoryRequestSchema,create(ChatHistoryRequestSchema,{conversationId:item.conversationId,deviceId:item.deviceId})),signal:AbortSignal.any([signal,AbortSignal.timeout(10000)])});
        const current=new Uint8Array(await history.arrayBuffer());
        if(!history.ok)throw Error(fromBinary(ApiErrorSchema,current).message);
        const state=fromBinary(ChatStateSchema,current);
        if(!state.messages.some(message=>message.requestId===item.id))throw Error('消息回执尚未确认，请重试');
        return {chat:state,order};
      },result=>accepted(result.chat,result.order),setEntries);
      queue.current=value;setEntries(value.entries);setError('');setReady(true);value.start();
      return()=>{value.dispose();queue.current=null;};
    }catch(value){setError(value instanceof Error?value.message:'待发送消息无法读取');setReady(false);}
  },[conversationId,deviceId,accepted,beginHistory]);
  const submit=useCallback((item:OutboxInput)=>{
    if(!queue.current)throw Error('对话正在连接，请稍后重试');
    queue.current.enqueue(item);
  },[]);
  const retry=useCallback(()=>{try{queue.current?.retry();setError('');}catch(value){setError(value instanceof Error?value.message:'重试失败');}},[]);
  return{entries,error,ready,submit,retry};
}
