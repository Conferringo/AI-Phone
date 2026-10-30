import { test, expect } from 'bun:test';
import { readRequestBody } from '../apps/web/src/lib/request-body';

function streaming(chunks: number[], headers: HeadersInit = {}) {
  let pulled = 0, cancelled = false;
  const stream = new ReadableStream<Uint8Array>({
    pull(controller) {
      const size = chunks[pulled++];
      if (size === undefined) controller.close();
      else controller.enqueue(new Uint8Array(size).fill(7));
    },
    cancel() { cancelled = true; },
  }, { highWaterMark: 0 });
  return { request: new Request('http://localhost/body', { method: 'POST', body: stream, headers, duplex: 'half' } as RequestInit),
    stats: () => ({ pulled, cancelled }) };
}

test('chunked and misleading length bodies stop reading at the budget', async () => {
  for (const headers of [{}, { 'content-length': '1' }]) {
    const fixture = streaming([4, 4, 4, 4_000_000], headers);
    const result = await readRequestBody(fixture.request, 8);
    expect(result).toBeInstanceOf(Response);
    expect((result as Response).status).toBe(413);
    expect(fixture.stats()).toEqual({ pulled: 3, cancelled: true });
  }
});
test('oversized and malformed declared sizes never consume the body', async () => {
  for (const value of ['9000', '-1', 'not-a-number']) {
    const fixture = streaming([100], { 'content-length': value });
    const result = await readRequestBody(fixture.request, 8);
    expect(result).toBeInstanceOf(Response);
    expect(fixture.stats()).toEqual({ pulled: 0, cancelled: true });
  }
});
test('exact boundary and empty protobuf requests retain their bytes', async () => {
  expect(await readRequestBody(streaming([3, 5]).request, 8)).toEqual(new Uint8Array(8).fill(7));
  expect(await readRequestBody(new Request('http://localhost/body', { method: 'POST' }), 8)).toEqual(new Uint8Array());
});
test('a stalled body is cancelled on deadline', async () => {
  const fixture = streaming([]);
  let cancelled = false;
  const body = new ReadableStream({ cancel() { cancelled = true; } });
  const request = new Request(fixture.request.url, { method: 'POST', body, duplex: 'half' } as RequestInit);
  const result = await readRequestBody(request, 8, 15);
  expect((result as Response).status).toBe(408);
  expect(cancelled).toBe(true);
});
test('client disconnect cancels an in-flight body without forwarding partial bytes', async () => {
  const abort = new AbortController();
  let cancelled = false;
  const body = new ReadableStream({ start(c) { c.enqueue(new Uint8Array(2)); }, cancel() { cancelled = true; } });
  const request = new Request('http://localhost/body', { method: 'POST', body, signal: abort.signal, duplex: 'half' } as RequestInit);
  const result = readRequestBody(request, 8);
  abort.abort();
  expect(((await result) as Response).status).toBe(408);
  expect(cancelled).toBe(true);
});
