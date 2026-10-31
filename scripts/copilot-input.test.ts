import {describe, expect, test} from 'bun:test';
import {CopilotInput, type VoiceCommit} from '../apps/web/src/lib/copilot-input';
import {ChatOutbox} from '../apps/web/src/lib/chat-outbox';

function fixture(accept?: (commit: VoiceCommit) => void) {
  const sent: VoiceCommit[] = [];
  let id = 0;
  const voice = new CopilotInput({conversationId: 'conversation', deviceId: 'phone', sessionId: 'capture'}, commit => {
    accept?.(commit);
    sent.push(structuredClone(commit));
  }, () => `receipt-${++id}`);
  return {voice, sent};
}
const patch = (utteranceId: string, revision: number, literal: string, final = false) => ({utteranceId, revision, literal, final});

describe('Copilot continuous input contract', () => {
  test('uses audio silence, not a delayed transcript, for the 1500ms endpoint', () => {
    const {voice, sent} = fixture();
    voice.speech('first', 0);
    voice.speech('first', 400);
    voice.transcript(patch('first', 1, '打开微信'));
    expect(voice.deadline).toBe(1900);
    voice.advance(1899);
    expect(sent).toEqual([]);
    voice.advance(1900);
    expect(sent.map(item => item.text)).toEqual(['打开微信']);
    expect(voice.deadline).toBeNull();
    voice.advance(9000);
    expect(sent).toHaveLength(1);
  });

  test('a final transcript does not send before audio silence', () => {
    const {voice, sent} = fixture();
    voice.speech('a', 100);
    voice.transcript(patch('a', 1, '不要发送，只打开', true));
    voice.advance(1599);
    expect(sent).toEqual([]);
    voice.advance(1600);
    expect(sent[0].text).toBe('不要发送，只打开');
  });

  test('multiple hypotheses revise one draft; stale and duplicate packets cannot revert it', () => {
    const {voice, sent} = fixture();
    voice.speech('a', 0);
    expect(voice.transcript(patch('a', 1, '打开'))).toBe(true);
    expect(voice.transcript(patch('a', 1, '重复首包'))).toBe(false);
    voice.transcript(patch('a', 3, '打开通讯录', true));
    expect(voice.transcript(patch('a', 2, '打开微信'))).toBe(false);
    expect(voice.transcript(patch('a', 3, '重复包'))).toBe(false);
    expect(voice.drafts[0].literal).toBe('打开通讯录');
    voice.advance(1500);
    expect(sent.map(item => item.text)).toEqual(['打开通讯录']);
  });

  test('explicit semantic corrections preserve literal speech; ordinary negation is not rewritten', () => {
    const {voice, sent} = fixture();
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '处理第一份表格'));
    voice.transcript({...patch('a', 2, '处理第一份表格，不是，说错了，第二份'), correction: {baseRevision: 1, text: '处理第二份表格'}});
    expect(voice.drafts[0].literal).toContain('说错了');
    expect(voice.drafts[0].text).toBe('处理第二份表格');
    voice.advance(1500);
    voice.speech('b', 1600);
    voice.transcript(patch('b', 1, '原文是“不是，说错了”，不要替换引用'));
    voice.advance(3100);
    expect(sent.map(item => item.text)).toEqual(['处理第二份表格', '原文是“不是，说错了”，不要替换引用']);
  });

  test('a correction based on a different draft cannot overwrite the current one', () => {
    const {voice} = fixture();
    voice.speech('a', 0);
    voice.transcript(patch('a', 2, '保留这个文件'));
    expect(() => voice.transcript({...patch('a', 3, '不是，另一个'), correction: {baseRevision: 1, text: '删除文件'}})).toThrow('revision');
    expect(voice.drafts[0].text).toBe('保留这个文件');
    expect(voice.drafts[0].revision).toBe(2);
  });

  test('a late final transcript becomes a new linked correction in the same conversation', () => {
    const {voice, sent} = fixture();
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '处理第一份'));
    voice.advance(1500);
    voice.speech('b', 1600);
    voice.transcript(patch('b', 1, '保存一份副本'));
    voice.transcript(patch('a', 2, '处理第二份', true));
    voice.transcript(patch('a', 2, '处理第二份', true));
    voice.advance(3100);
    expect(sent.map(item => item.text)).toEqual(['处理第一份', '处理第二份', '保存一份副本']);
    expect(sent[1].correctsRequestId).toBe(sent[0].requestId);
    expect(sent[1].requestId).not.toBe(sent[0].requestId);
    expect(sent.every(item => item.conversationId === 'conversation' && item.deviceId === 'phone')).toBe(true);
    expect(sent[2].correctsRequestId).toBeUndefined();
  });

  test('silence without recognition waits visibly and preserves audio order when recognition arrives late', () => {
    const {voice, sent} = fixture();
    voice.speech('a', 0);
    voice.advance(1500);
    expect(voice.drafts[0].status).toBe('awaiting-transcript');
    voice.speech('b', 1600);
    voice.transcript(patch('b', 1, '第二段', true));
    voice.advance(3100);
    expect(sent).toEqual([]);
    voice.transcript(patch('a', 1, '第一段', true));
    expect(sent.map(item => item.text)).toEqual(['第一段', '第二段']);
  });

  test('failed handoff freezes its body and receipt; new recognition becomes a correction after retry', () => {
    let fail = true;
    const attempts: VoiceCommit[] = [];
    const {voice, sent} = fixture(commit => {attempts.push(structuredClone(commit)); if (fail) throw Error('disk full');});
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '第一份'));
    voice.advance(1500);
    expect(voice.error).toBe('disk full');
    voice.transcript(patch('a', 2, '第二份', true));
    expect(attempts).toHaveLength(1);
    fail = false;
    voice.retry();
    expect(attempts[1]).toEqual(attempts[0]);
    expect(sent.map(item => item.text)).toEqual(['第一份', '第二份']);
    expect(sent[1].correctsRequestId).toBe(sent[0].requestId);
    expect(voice.error).toBe('');
  });

  test('stop cancels endpointing and late results, retains text, and never cancels accepted work', () => {
    const {voice, sent} = fixture();
    voice.speech('sent', 0);
    voice.transcript(patch('sent', 1, '已接受的任务'));
    voice.advance(1500);
    voice.speech('draft', 1600);
    voice.transcript(patch('draft', 1, '尚未发送'));
    voice.stop();
    voice.advance(10000);
    voice.retry();
    expect(voice.transcript(patch('draft', 2, '迟到结果'))).toBe(false);
    expect(voice.speech('late', 11000)).toBe(false);
    expect(voice.deadline).toBeNull();
    expect(sent.map(item => item.text)).toEqual(['已接受的任务']);
    expect(voice.drafts[1].text).toBe('尚未发送');
  });

  test('bounded unresolved input refuses new capture without dropping existing speech', () => {
    const {voice, sent} = fixture();
    for (let i = 0; i < 32; i++) {voice.speech(`u${i}`, i * 1600); voice.advance(i * 1600 + 1500);}
    expect(() => voice.speech('overflow', 52000)).toThrow('capacity');
    expect(voice.drafts).toHaveLength(32);
    expect(sent).toEqual([]);
    voice.transcript(patch('u0', 1, '第一段仍然在', true));
    expect(sent[0].text).toBe('第一段仍然在');
  });

  test('completed final segments are bounded while continuing capture; retired packets are ignored', () => {
    const {voice, sent} = fixture();
    for (let i = 0; i < 300; i++) {
      voice.speech(`u${i}`, i * 1600);
      voice.transcript(patch(`u${i}`, 1, `第${i}段`, true));
      voice.advance(i * 1600 + 1500);
    }
    expect(sent).toHaveLength(300);
    expect(voice.drafts.length).toBeLessThanOrEqual(32);
    expect(voice.transcript(patch('u0', 2, '过期结果', true))).toBe(false);
  });

  test('rejects invalid audio, oversized Unicode, invalid revisions and concurrent utterances', () => {
    const {voice} = fixture();
    for (const time of [NaN, Infinity, -1]) expect(() => voice.speech('a', time)).toThrow();
    for (const id of ['', 'a'.repeat(129)]) expect(() => voice.speech(id, 0)).toThrow('ID');
    expect(voice.speech('a', 0)).toBe(true);
    expect(voice.speech('a', 0)).toBe(true);
    expect(() => voice.speech('b', 100)).toThrow('active');
    expect(() => voice.transcript(patch('a', 0, 'invalid'))).toThrow();
    expect(() => voice.transcript(patch('a', 1, '😀'.repeat(12001)))).toThrow('large');
    expect(voice.drafts[0].text).toBe('');
    voice.transcript(patch('a', 1, '不要丢掉这段'));
    voice.speech('a', 400);
    expect(voice.speech('a', 200)).toBe(false);
    expect(voice.deadline).toBe(1900);
    expect(() => voice.advance(NaN)).toThrow();
  });

  test('binding and transcript byte limits are exact; caller-owned bindings cannot retarget capture', () => {
    for (const binding of [{conversationId: '', deviceId: 'phone', sessionId: 'capture'}, {conversationId: 'c', deviceId: '', sessionId: 'capture'}, {conversationId: 'c', deviceId: 'phone', sessionId: ''}]) {
      expect(() => new CopilotInput(binding, () => {})).toThrow('required');
    }
    const binding = {conversationId: 'original', deviceId: 'phone', sessionId: 'capture'};
    const sent: VoiceCommit[] = [];
    const voice = new CopilotInput(binding, commit => {sent.push(commit);});
    binding.conversationId = 'other';
    const id = 'a'.repeat(128);
    expect(voice.speech(id, 0)).toBe(true);
    const exact = '😀'.repeat(12000);
    voice.transcript(patch(id, 0xffffffff, exact, true));
    expect(() => voice.transcript(patch(id, 0x100000000, exact, true))).toThrow('revision');
    voice.advance(1500);
    expect(sent[0].text).toBe(exact);
    expect(sent[0].conversationId).toBe('original');
  });

  test('unrecognized final segments and nonfinal accepted segments cannot be evicted as settled', () => {
    const empty = fixture().voice;
    for (let i = 0; i < 32; i++) {
      empty.speech(`u${i}`, i * 1600);
      empty.transcript(patch(`u${i}`, 1, '', true));
      empty.advance(i * 1600 + 1500);
    }
    expect(() => empty.speech('overflow', 52000)).toThrow('capacity');
    expect(empty.drafts[0].utteranceId).toBe('u0');
    const {voice, sent} = fixture();
    voice.speech('unfinished', 0);
    voice.transcript(patch('unfinished', 1, '仍待最终结果'));
    voice.advance(1500);
    for (let i = 1; i < 34; i++) {
      voice.speech(`done${i}`, i * 1600);
      voice.transcript(patch(`done${i}`, 1, '已完成', true));
      voice.advance(i * 1600 + 1500);
    }
    expect(voice.transcript(patch('unfinished', 2, '最终修正', true))).toBe(true);
    expect(sent.at(-1)?.correctsRequestId).toBe(sent[0].requestId);
  });

  test('a failed handoff blocks later ready segments until explicit retry', () => {
    let fail = true;
    const attempts: string[] = [];
    const {voice, sent} = fixture(commit => {attempts.push(commit.text); if (fail) throw 'offline';});
    voice.speech('a', 0);
    voice.advance(1500);
    voice.speech('b', 1600);
    voice.transcript(patch('b', 1, '第二段', true));
    voice.advance(3100);
    voice.transcript(patch('a', 1, '第一段', true));
    expect(attempts).toEqual(['第一段']);
    expect(voice.error).toBe('Voice handoff failed');
    fail = false;
    voice.retry();
    expect(sent.map(item => item.text)).toEqual(['第一段', '第二段']);
  });

  test('draft snapshots and transport callbacks cannot mutate the owned state', () => {
    const {voice, sent} = fixture(commit => {commit.text = 'mutated';});
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '原文', true));
    voice.drafts[0].text = 'changed';
    expect(voice.drafts[0].text).toBe('原文');
    voice.advance(1500);
    voice.transcript(patch('a', 2, '原文', true));
    expect(sent).toHaveLength(1);
  });

  test('an empty failure message still blocks automatic retries', () => {
    let attempts = 0;
    const {voice} = fixture(() => {attempts++; throw Error('');});
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '保留'));
    voice.advance(1500);
    voice.advance(1600);
    expect(voice.error).toBe('Voice handoff failed');
    expect(attempts).toBe(1);
  });

  test('an empty late hypothesis cannot strand a frozen handoff receipt', () => {
    let fail = true;
    const {voice, sent} = fixture(() => {if (fail) throw Error('full');});
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '待确认文本'));
    voice.advance(1500);
    voice.transcript(patch('a', 2, '', true));
    fail = false;
    voice.retry();
    expect(sent.map(item => item.text)).toEqual(['待确认文本']);
    expect(voice.drafts[0].status).toBe('awaiting-transcript');
  });

  test('stop during a handoff prevents dispatching a newer reentrant revision', () => {
    let voice!: CopilotInput;
    const sent: VoiceCommit[] = [];
    voice = new CopilotInput({conversationId: 'c', deviceId: 'd', sessionId: 'capture'}, commit => {
      sent.push(commit);
      voice.transcript(patch('a', 2, '保留为草稿', true));
      voice.stop();
    });
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '第一段'));
    voice.advance(1500);
    expect(sent).toHaveLength(1);
    expect(voice.drafts[0].text).toBe('保留为草稿');
  });

  test('an elapsed endpoint is processed when the next utterance arrives before the timer callback', () => {
    const {voice, sent} = fixture();
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '第一段', true));
    voice.speech('b', 1600);
    expect(sent.map(item => item.text)).toEqual(['第一段']);
    expect(voice.speech('a', 1700)).toBe(false);
    voice.transcript(patch('b', 1, '第二段', true));
    voice.advance(3100);
    expect(sent.map(item => item.text)).toEqual(['第一段', '第二段']);
  });

  test('audio arriving after the same segment has expired asks the capture adapter for a fresh ID', () => {
    const {voice, sent} = fixture();
    voice.speech('a', 0);
    voice.transcript(patch('a', 1, '已经分段', true));
    expect(voice.speech('a', 1500)).toBe(false);
    expect(sent.map(item => item.text)).toEqual(['已经分段']);
    expect(voice.speech('b', 1500)).toBe(true);
    expect(voice.deadline).toBe(3000);
  });

  test('hands off consecutive utterances to the existing durable outbox while an earlier send is unresolved', async () => {
    let persisted = '[]';
    const requests: string[] = [], acknowledgements: string[] = [];
    const resolvers: Array<() => void> = [];
    const outbox = new ChatOutbox({read: () => persisted, write: value => {persisted = value;}}, async item => {
      requests.push(item.id);
      await new Promise<void>(resolve => resolvers.push(resolve));
      return item.id;
    }, id => {acknowledgements.push(id);});
    const {voice, sent} = fixture(commit => outbox.enqueue({id: commit.requestId, text: commit.text, conversationId: commit.conversationId, deviceId: commit.deviceId, attachments: []}));
    for (let i = 0; i < 3; i++) {
      voice.speech(`u${i}`, i * 1600);
      voice.transcript(patch(`u${i}`, 1, `补充${i}`, true));
      voice.advance(i * 1600 + 1500);
    }
    expect(JSON.parse(persisted).map((item: {text: string}) => item.text)).toEqual(['补充0', '补充1', '补充2']);
    expect(requests).toEqual([sent[0].requestId]);
    for (let i = 0; i < 3; i++) {resolvers.shift()!(); await new Promise(resolve => setTimeout(resolve, 0));}
    expect(acknowledgements).toEqual(sent.map(item => item.requestId));
    expect(outbox.entries).toEqual([]);
    outbox.dispose();
  });
});
