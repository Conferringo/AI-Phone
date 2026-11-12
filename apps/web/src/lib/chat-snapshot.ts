import {equals,type MessageShape} from '@bufbuild/protobuf';
import {ChatStateSchema,type ChatMessage} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
import {chatEquality,mergeMessageWindow} from './chat-window';
type Chat=MessageShape<typeof ChatStateSchema>;
export type HistoryBoundary={beforeMessageId:string;ordinal:bigint;hasOlder:boolean;order:number};
export type SnapshotVersion={revision:bigint;order:number};
export type ChatSnapshot={chat:Chat|null;order:number;messageVersions?:ReadonlyMap<string,SnapshotVersion>;headerOrder?:number;history?:HistoryBoundary};
export type AcceptedInput={conversationId:string;message:ChatMessage};

/** Project only the verified immutable user receipt, never the receipt's old run or assistant state. */
export function acceptInputReceipt(previous:ChatSnapshot,receipt:AcceptedInput,order:number):ChatSnapshot{
  const chat=previous.chat,message=receipt.message;
  if(!chat||chat.conversationId!==receipt.conversationId||message.role!=='user')return previous;
  const watermark=Math.max(previous.order,order);
  if(chat.messages.some(item=>item.id===message.id||item.requestId===message.requestId))return{...previous,chat,order:watermark};
  const messages=[...chat.messages,structuredClone(message)].sort((a,b)=>Number(a.ordinal-b.ordinal));
  const versions=previous.messageVersions?{messageVersions:new Map(previous.messageVersions).set(message.id,{revision:chat.revision>message.ordinal?chat.revision:message.ordinal,order})}:{};
  return{...previous,...versions,chat:{...chat,messages,revision:chat.revision>message.ordinal?chat.revision:message.ordinal},order:watermark};
}

/** Request order resolves equal-revision polls; durable sequence protects accepted messages. */
export function acceptSnapshot(previous:ChatSnapshot,next:Chat,order:number,beforeMessageId=''):ChatSnapshot{
  if(next.messageWindow)return mergeMessageWindow(previous,next,order,beforeMessageId);
  const old=previous.chat;
  if(old?.conversationId===next.conversationId){
    if(next.revision<old.revision||(next.revision===old.revision&&order<previous.order))return previous;
    if(equals(ChatStateSchema,old,next,chatEquality))return{chat:old,order:Math.max(previous.order,order)};
  }
  return{chat:next,order:Math.max(previous.order,order)};
}
