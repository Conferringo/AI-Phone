import {toBinary,type MessageShape} from '@bufbuild/protobuf';
import {ChatStateSchema} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
type Chat=MessageShape<typeof ChatStateSchema>;
export type ChatSnapshot={chat:Chat|null;order:number};

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
