import {describe, expect, test} from 'bun:test';
import {fromBinary, type MessageShape} from '@bufbuild/protobuf';
import {ChatSendRequestSchema} from '../packages/protocol/src/gen/ai_phone/v1/assistant_pb';
import {CopilotInput, voiceOutboxInput} from '../apps/web/src/lib/copilot-input';
import {ChatOutbox} from '../apps/web/src/lib/chat-outbox';
import {encodeChatSend} from '../apps/web/src/lib/chat-wire';

const binding = {conversationId: 'conversation', deviceId: 'phone', sessionId: '156ff8ba-b04b-4172-85f5-712b9608b274'};
describe('Copilot durable protobuf handoff', () => {
  test('late final corrections keep their literal speech, scope and target through storage reload and protobuf', async () => {
    let saved = '[]', receipt = 0;
    const first = new ChatOutbox({read: () => saved, write: value => {saved = value;}}, async () => new Promise(() => {}), () => {});
    const voice = new CopilotInput(binding, commit => first.enqueue(voiceOutboxInput(commit)), () => `request-${++receipt}`);
    voice.speech('one', 0);
    voice.transcript({utteranceId: 'one', revision: 1, literal: '处理第一份', final: false});
    voice.advance(1500);
    voice.transcript({utteranceId: 'one', revision: 2, literal: '不是，说错了，第二份', final: true, correction: {baseRevision: 1, text: '处理第二份'}});
    first.dispose();
    const wire: MessageShape<typeof ChatSendRequestSchema>[] = [];
    const next = new ChatOutbox({read: () => saved, write: value => {saved = value;}}, async item => {
      wire.push(fromBinary(ChatSendRequestSchema, encodeChatSend(item)));
    }, () => {});
    next.start();
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(wire).toHaveLength(2);
    expect(wire.map(item => item.text)).toEqual(['处理第一份', '处理第二份']);
    expect(wire[1].voice).toMatchObject({sessionId: binding.sessionId, utteranceId: 'one', revision: 2, literal: '不是，说错了，第二份', correctsRequestId: 'request-1'});
    expect(wire.every(item => item.deviceId === 'phone' && item.conversationId === 'conversation')).toBe(true);
    expect(next.entries).toEqual([]);
    next.dispose();
  });

  test('legacy text outbox entries retain their wire form without a voice source', () => {
    const message = fromBinary(ChatSendRequestSchema, encodeChatSend({id: 'text', conversationId: 'c', deviceId: 'd', text: 'typed', attachments: []}));
    expect(message.voice).toBeUndefined();
    expect(message.requestId).toBe('text');
  });

  test('the maximum protobuf revision survives durable reload unchanged', async () => {
    const item = {id: 'one', conversationId: 'c', deviceId: 'd', text: 'keep', attachments: [], voice: {sessionId: binding.sessionId, utteranceId: 'u', revision: 0xffffffff, literal: 'keep', correctsRequestId: ''}};
    const box = new ChatOutbox({read: () => JSON.stringify([item]), write: () => {}}, async saved => {
      expect(fromBinary(ChatSendRequestSchema, encodeChatSend(saved)).voice?.revision).toBe(0xffffffff);
    }, () => {});
    box.start();
    await new Promise(resolve => setTimeout(resolve, 0));
    expect(box.entries).toEqual([]);
    expect(() => box.enqueue({...item, voice: {...item.voice, revision: 0}})).toThrow('格式');
    box.dispose();
  });

  test('malformed stored voice metadata is reported without dropping pending text', () => {
    const entry = {id: 'one', conversationId: 'c', deviceId: 'd', text: 'keep me', attachments: [], voice: {sessionId: binding.sessionId, utteranceId: 'u', revision: 1, literal: 'keep', correctsRequestId: ''}};
    for (const voice of [null, {}, {...entry.voice, revision: 0}, {...entry.voice, revision: 0x100000000}, {...entry.voice, literal: 4}, {...entry.voice, correctsRequestId: null}]) {
      const saved = JSON.stringify([{...entry, voice}]);
      expect(() => new ChatOutbox({read: () => saved, write: () => {throw Error('must not overwrite');}}, async () => {}, () => {})).toThrow();
    }
  });
});
