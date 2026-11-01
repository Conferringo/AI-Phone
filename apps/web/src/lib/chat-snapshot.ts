import {toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ChatStateSchema,type ChatMessage} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
type Chat=MessageShape<typeof ChatStateSchema>;
export type ChatSnapshot={chat:Chat|null;order:number};
export type AcceptedInput={conversationId:string;deviceId:string;message:ChatMessage};

/** Project only the verified immutable user receipt, never the receipt's old run or assistant state. */
export function acceptInputReceipt(previous:ChatSnapshot,receipt:AcceptedInput,order:number):ChatSnapshot{
  const chat=previous.chat,message=receipt.message;
  if(!chat||chat.conversationId!==receipt.conversationId||chat.deviceId!==receipt.deviceId||message.role!=='user')return previous;
  const watermark=Math.max(previous.order,order);
  if(chat.messages.some(item=>item.id===message.id||item.requestId===message.requestId))return{chat,order:watermark};
  const messages=[...chat.messages,structuredClone(message)].sort((a,b)=>Number(a.ordinal-b.ordinal));
  return{chat:{...chat,messages,revision:chat.revision>message.ordinal?chat.revision:message.ordinal},order:watermark};
}

/** Request order resolves equal-revision polls; durable sequence protects accepted messages. */
export function acceptSnapshot(previous:ChatSnapshot,next:Chat,order:number):ChatSnapshot{
  const old=previous.chat;
  if(old?.conversationId===next.conversationId){
    if(next.revision<old.revision||(next.revision===old.revision&&order<previous.order))return previous;
    const before=toBinary(ChatStateSchema,old),after=toBinary(ChatStateSchema,next);
    if(before.length===after.length&&before.every((value,index)=>value===after[index]))return{chat:old,order:Math.max(previous.order,order)};
  }
  return{chat:next,order:Math.max(previous.order,order)};
}
