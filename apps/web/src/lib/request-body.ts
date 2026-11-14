import { assistantError } from './assistant-server';

/** Enforce the budget while reading: a missing/false length must not bypass it. */
export async function readRequestBody(request: Request, limit: number, timeoutMs = 30_000): Promise<Uint8Array<ArrayBuffer> | Response> {
  const declared = request.headers.get('content-length');
  if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > limit)) {
    void request.body?.cancel().catch(() => {});
    return assistantError(declared.match(/^\d+$/) ? 413 : 400, 'body_rejected', '请求内容过大或长度无效');
  }
  const signal = AbortSignal.any([request.signal, AbortSignal.timeout(timeoutMs)]);
  if (!request.body) return new Uint8Array();
  const reader = request.body.getReader();
  const cancel = () => { void reader.cancel().catch(() => {}); };
  signal.addEventListener('abort', cancel, { once: true });
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    if (signal.aborted) { cancel(); signal.throwIfAborted(); }
    while (true) {
      const { done, value } = await reader.read();
      signal.throwIfAborted();
      if (done) break;
      length += value.byteLength;
      if (length > limit) {
        cancel();
        return assistantError(413, 'body_rejected', '请求内容过大');
      }
      chunks.push(value);
    }
    const body = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) { body.set(chunk, offset); offset += chunk.byteLength; }
    return body;
  } catch {
    cancel();
    return assistantError(signal.aborted ? 408 : 400, 'body_incomplete', '请求未完整接收，请重试');
  } finally {
    signal.removeEventListener('abort', cancel);
    reader.releaseLock();
  }
}
