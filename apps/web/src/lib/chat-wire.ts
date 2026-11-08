import {create,toBinary} from '@bufbuild/protobuf';
import {ChatSendRequestSchema} from '@ai-phone/protocol/ai_phone/v1/assistant_pb';
import type {OutboxInput} from './chat-outbox';

/** All durable sends, including retries, use the same protobuf mapping. */
export function encodeChatSend(item:OutboxInput){
  return toBinary(ChatSendRequestSchema,create(ChatSendRequestSchema,{
    conversationId:item.conversationId,deviceId:item.deviceId,text:item.text,
    attachmentIds:item.attachments.map(file=>file.id),requestId:item.id,voice:item.voice,
  }));
}
