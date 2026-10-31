/** Recorded-event core. Capture/provider adapters must normalize audio time to one
 * monotonic clock; transcript arrival never moves the silence deadline. No audio
 * or credentials are stored here, and no model/device action is dispatched here. */
import type {OutboxInput} from './chat-outbox';
export type VoiceBinding = {conversationId: string; deviceId: string; sessionId: string};
export type VoicePatch = {
  utteranceId: string;
  revision: number;
  literal: string;
  final: boolean;
  /** A semantic edit must explicitly name the draft it replaces. Ordinary
   * negation/quoted words never trigger local string-deletion heuristics. */
  correction?: {baseRevision: number; text: string};
};
export type VoiceCommit = VoiceBinding & {
  requestId: string;
  utteranceId: string;
  revision: number;
  literal: string;
  text: string;
  correctsRequestId?: string;
};

export function voiceOutboxInput(commit: VoiceCommit): OutboxInput {
  return {
    id: commit.requestId, conversationId: commit.conversationId,
    deviceId: commit.deviceId, text: commit.text, attachments: [],
    voice: {sessionId: commit.sessionId, utteranceId: commit.utteranceId,
      revision: commit.revision, literal: commit.literal,
      correctsRequestId: commit.correctsRequestId ?? ''},
  };
}
type Utterance = {
  id: string;
  lastAudio: number;
  ended: boolean;
  revision: number;
  literal: string;
  text: string;
  final: boolean;
  accepted?: VoiceCommit;
  pending?: VoiceCommit;
};
export type VoiceDraft = {
  utteranceId: string;
  revision: number;
  literal: string;
  text: string;
  status: 'draft' | 'awaiting-transcript' | 'handoff-pending' | 'accepted';
};

const SILENCE_MS = 1500;
const CAPACITY = 32;
const MAX_TEXT_BYTES = 48000;

function audioTime(value: number) {
  if (!Number.isFinite(value) || value < 0) throw Error('Invalid audio timestamp');
}
function staleAudio(slot: Utterance | undefined, atMs: number) {
  return slot ? slot.ended || atMs < slot.lastAudio : false;
}
function boundedText(value: string) {
  if (typeof value !== 'string') throw Error('Invalid transcript text');
  if (new TextEncoder().encode(value).length > MAX_TEXT_BYTES) throw Error('Transcript is too large');
  return value.trim();
}
function validatePatch(patch: VoicePatch, slot: Utterance) {
  if (!Number.isSafeInteger(patch.revision) || patch.revision <= 0 || patch.revision > 0xffffffff) throw Error('Invalid transcript revision');
  boundedText(patch.literal);
  if (!patch.correction) return;
  if (patch.correction.baseRevision !== slot.revision) throw Error('Correction revision conflict');
  if (!boundedText(patch.correction.text)) throw Error('Empty semantic correction');
}
function settled(slot: Utterance) {
  return slot.ended && slot.final && !slot.pending && slot.accepted?.text === slot.text;
}
function draftStatus(slot: Utterance): VoiceDraft['status'] {
  if (slot.pending) return 'handoff-pending';
  if (slot.accepted?.text === slot.text) return 'accepted';
  if (!slot.ended) return 'draft';
  return slot.text ? 'handoff-pending' : 'awaiting-transcript';
}

/** One capture lifetime bound to one conversation. `accept` is a synchronous,
 * durable handoff, NOT a network acknowledgement or proof of task completion.
 * Failed handoffs freeze the same receipt/body until explicit retry. A provider
 * adapter must keep correction metadata when wiring the eventual voice API. */
export class CopilotInput {
  private readonly binding: VoiceBinding;
  private slots: Utterance[] = [];
  private enabled = true;
  private flushing = false;
  private failure = '';

  constructor(binding: VoiceBinding, private accept: (commit: VoiceCommit) => void, private receipt = () => crypto.randomUUID()) {
    if (!binding.conversationId || !binding.deviceId || !binding.sessionId) throw Error('A conversation, device and capture session are required');
    this.binding = {...binding};
  }

  get error() {return this.failure;}
  get deadline(): number | null {
    if (!this.enabled) return null;
    const active = this.slots.find(slot => !slot.ended);
    return active ? active.lastAudio + SILENCE_MS : null;
  }
  get drafts(): VoiceDraft[] {
    return this.slots.map(slot => ({utteranceId: slot.id, revision: slot.revision, literal: slot.literal, text: slot.text, status: draftStatus(slot)}));
  }

  /** Called for local voice activity, not every received transcript packet.
   * The adapter begins a fresh utterance ID after an endpoint. */
  speech(utteranceId: string, atMs: number): boolean {
    if (!this.enabled) return false;
    audioTime(atMs);
    if (!utteranceId || utteranceId.length > 128) throw Error('Invalid utterance ID');
    let slot = this.slots.find(item => item.id === utteranceId);
    if (staleAudio(slot, atMs)) return false;
    this.advance(atMs);
    if (slot?.ended) return false;
    if (!slot) slot = this.begin(utteranceId, atMs);
    slot.lastAudio = atMs;
    return true;
  }

  private begin(id: string, atMs: number): Utterance {
    if (this.slots.some(slot => !slot.ended)) throw Error('Another utterance is active');
    if (this.slots.length === CAPACITY) {
      const oldest = this.slots.findIndex(settled);
      if (oldest < 0) throw Error('Voice input capacity reached; retain transcripts and pause capture');
      this.slots.splice(oldest, 1);
    }
    const slot: Utterance = {id, lastAudio: atMs, ended: false, revision: 0, literal: '', text: '', final: false};
    this.slots.push(slot);
    return slot;
  }

  transcript(patch: VoicePatch): boolean {
    if (!this.enabled) return false;
    const slot = this.slots.find(item => item.id === patch.utteranceId);
    if (!slot) return false; // Retired segment or an old provider session.
    if (patch.revision <= slot.revision && patch.revision > 0) return false;
    validatePatch(patch, slot);
    slot.revision = patch.revision;
    slot.literal = patch.literal;
    slot.text = boundedText(patch.correction?.text ?? patch.literal);
    slot.final = patch.final;
    this.flush();
    return true;
  }

  /** The capture owner schedules this at `deadline`; finals cannot extend it. */
  advance(now: number) {
    audioTime(now);
    if (!this.enabled) return;
    for (const slot of this.slots) {
      if (slot.lastAudio + SILENCE_MS <= now) slot.ended = true;
    }
    this.flush();
  }

  retry() {
    if (!this.enabled) return;
    this.failure = '';
    this.flush();
  }

  stop() {
    this.enabled = false;
    // Keep text for explicit draft recovery; accepted Agent work is independent.
  }

  private prepare(slot: Utterance): VoiceCommit {
    const result: VoiceCommit = {...this.binding, requestId: this.receipt(), utteranceId: slot.id, revision: slot.revision, literal: slot.literal, text: slot.text};
    if (slot.accepted) result.correctsRequestId = slot.accepted.requestId;
    return result;
  }

  private deliver(slot: Utterance): boolean {
    slot.pending ??= this.prepare(slot);
    try {
      this.accept(structuredClone(slot.pending));
      slot.accepted = slot.pending;
      slot.pending = undefined;
      return true;
    } catch (error) {
      this.failure = error instanceof Error && error.message ? error.message : 'Voice handoff failed';
      return false;
    }
  }

  private flushSlot(slot: Utterance): boolean {
    if (!slot.ended) return false;
    while (slot.pending || (slot.text && slot.accepted?.text !== slot.text)) {
      if (!this.enabled) return false;
      if (!this.deliver(slot)) return false;
    }
    return Boolean(slot.text);
  }

  private flush() {
    if (this.flushing || this.failure || !this.enabled) return;
    this.flushing = true;
    try {
      for (const slot of this.slots) {
        if (!this.flushSlot(slot)) break;
      }
    } finally {
      this.flushing = false;
    }
  }
}
