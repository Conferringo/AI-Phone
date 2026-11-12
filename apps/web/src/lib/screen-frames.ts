const MAX_FRAME_BYTES = 1_000_000;
const MAX_BUFFER_BYTES = 2_000_000;

export class ScreenFrames {
  private readonly decoder = new TextDecoder();
  private buffer = new Uint8Array();
  private used = 0;
  private bodyStart = 0;
  private bodyLength = 0;

  push(chunk: Uint8Array): Uint8Array<ArrayBuffer> | null {
    const used = this.used + chunk.length;
    if (used > MAX_BUFFER_BYTES) throw new Error('frame_limit');

    let buffer = this.buffer;
    if (used > buffer.length) {
      const capacity = Math.min(MAX_BUFFER_BYTES, Math.max(used, buffer.length * 2));
      const expanded = new Uint8Array(capacity);
      expanded.set(buffer.subarray(0, this.used));
      buffer = this.buffer = expanded;
    }
    buffer.set(chunk, this.used);
    this.used = used;

    let consumed = 0;
    let frameStart = 0;
    let {bodyStart, bodyLength} = this;
    for (;;) {
      if (!bodyLength) {
        let separator = consumed;
        while (separator + 3 < used && (
          buffer[separator] !== 13 || buffer[separator + 1] !== 10 ||
          buffer[separator + 2] !== 13 || buffer[separator + 3] !== 10
        )) separator++;
        if (separator + 3 >= used) break;

        const header = this.decoder.decode(buffer.subarray(consumed, separator));
        bodyLength = Number(header.match(/Content-Length:\s*(\d+)/i)?.[1]);
        if (!bodyLength || bodyLength > MAX_FRAME_BYTES) throw new Error('frame_header');
        bodyStart = separator + 4;
      }
      if (used - bodyStart < bodyLength) break;
      frameStart = bodyStart;
      consumed = bodyStart + bodyLength;
      bodyLength = 0;
    }

    this.bodyStart = bodyLength ? bodyStart - consumed : 0;
    this.bodyLength = bodyLength;
    if (!consumed) return null;
    const newest = buffer.slice(frameStart, consumed);
    buffer.copyWithin(0, consumed, used);
    this.used = used - consumed;
    return newest;
  }
}
