import {test,expect} from 'bun:test';
import {create} from '@bufbuild/protobuf';
import {ChatStateSchema} from '../packages/protocol/src/gen/ai_phone/v1/assistant_pb';
import {acceptSnapshot} from '../apps/web/src/lib/chat-snapshot';
const state=(revision:bigint,text:string,status='running')=>create(ChatStateSchema,{conversationId:'same',revision,runStatus:status,messages:[{id:'reply',role:'assistant',text}]});
test('first history load initializes an empty conversation snapshot',()=>{
 const initial=state(1n,'first');expect(acceptSnapshot({chat:null,order:0},initial,1)).toEqual({chat:initial,order:1});
});
test('a late send acknowledgement cannot blank a newer completed reply at the same message ordinal',()=>{
 const old={chat:state(8n,'Done','succeeded'),order:3};
 expect(acceptSnapshot(old,state(8n,''),2)).toBe(old);
});
test('a late poll cannot remove an accepted user message even if its request began later',()=>{
 const old={chat:state(9n,'new message'),order:2};expect(acceptSnapshot(old,state(8n,'old'),3)).toBe(old);
});
test('new durable messages win and equal content keeps its render identity',()=>{
 const old={chat:state(8n,'old'),order:3};const next=state(9n,'new');
 const result=acceptSnapshot(old,next,2);expect(result.chat).toBe(next);expect(result.order).toBe(3);
 const same=acceptSnapshot(result,state(9n,'new'),4);expect(same.chat).toBe(next);expect(same.order).toBe(4);
});
