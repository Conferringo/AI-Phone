import {create,fromBinary,toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ChatHistoryRequestSchema,ChatStateSchema} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
import {ApiErrorSchema} from '@ai-phone/protocol/ai_phone/v1/system_pb';
import {historyRequest} from './chat-window';
type Chat=MessageShape<typeof ChatStateSchema>;
type Cell<T>={current:T};
type Owner={conversation:Cell<string>;fetching:Cell<boolean>;latest:Cell<Chat|null>;watchOffset:Cell<number>};
type Projection={begin:()=>number;accept:(chat:Chat,order:number,before:string)=>void;error:(message:string)=>void};

function begin(owner:Owner,before:string){
 if(before)return true;
 if(owner.fetching.current)return false;
 owner.fetching.current=true;return true;
}

async function request(owner:Owner,conversationId:string,before:string,signal?:AbortSignal){
 const value=historyRequest(conversationId,owner.latest.current,[],before,owner.watchOffset.current);
 // Tail and page reads share the workspace's transient watch cursor.
 owner.watchOffset.current+=value.watchMessageIds.length;
 const response=await fetch('/api/assistant/history',{method:'POST',headers:{'content-type':'application/x-protobuf'},body:toBinary(ChatHistoryRequestSchema,create(ChatHistoryRequestSchema,value)),signal,cache:'no-store'});
 const bytes=new Uint8Array(await response.arrayBuffer());
 if(response.status===401)location.assign('/login');
 if(!response.ok)throw Error(fromBinary(ApiErrorSchema,bytes).message);
 return fromBinary(ChatStateSchema,bytes);
}

function failure(error:unknown,projection:Projection,signal?:AbortSignal){
 if(!signal?.aborted)projection.error(error instanceof Error?error.message:'读取对话失败');
}

/** Borrows the workspace's existing refs and snapshot callbacks; owns no cache. */
export async function readChatHistory(owner:Owner,projection:Projection,signal?:AbortSignal,before=''){
 if(!begin(owner,before))return;
 const conversation=owner.conversation.current,order=projection.begin();
 try{
  const chat=await request(owner,conversation,before,signal);
  if(signal?.aborted||conversation!==owner.conversation.current)return;
  if(!chat.conversationId||(conversation&&chat.conversationId!==conversation))throw Error('读取对话失败');
  owner.conversation.current=chat.conversationId;
  projection.accept(chat,order,before);projection.error('');
  return chat;
 }catch(error){failure(error,projection,signal);}
 finally{if(!before)owner.fetching.current=false;}
}
