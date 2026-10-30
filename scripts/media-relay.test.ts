import {expect, test} from 'bun:test';
import {relayMedia} from '../apps/web/src/lib/media-relay';

function source() {
  let cancelled = 0;
  let controller!: ReadableStreamDefaultController<Uint8Array>;
  const body = new ReadableStream<Uint8Array>({
    start(value) { controller = value; },
    cancel() { cancelled++; },
  }, {highWaterMark: 0});
  return {body, controller, cancellations: () => cancelled};
}

test('a request cancelled before connecting never opens a device stream', async () => {
  const abort = new AbortController();
  abort.abort();
  let connections = 0;
  let response: Response | undefined;
  try {
    response = await relayMedia(abort.signal, async () => {
      connections++;
      return new Response('frame');
    }, async () => true);
  } catch {}
  await response?.body?.cancel();
  expect(connections).toBe(0);
});

test('disconnect cancels a silent source immediately, without waiting for a frame', async () => {
  const upstream = source(), abort = new AbortController();
  const response = await relayMedia(abort.signal, async () => new Response(upstream.body), async () => true);
  const reader = response.body!.getReader();
  const pending = reader.read();
  abort.abort();
  // Cleanup even when the regression is present so this test cannot leak timers.
  const cancellations = upstream.cancellations();
  await reader.cancel();
  await pending;
  expect(cancellations).toBe(1);
});

test('media bytes and MIME type survive forwarding; EOF releases upstream', async () => {
  const upstream = source(), abort = new AbortController();
  let upstreamSignal!: AbortSignal;
  const response = await relayMedia(abort.signal, async signal => {
    upstreamSignal = signal;
    return new Response(upstream.body, {headers: {'content-type': 'audio/wav'}});
  }, async () => true);
  upstream.controller.enqueue(new Uint8Array([0, 128, 255]));
  upstream.controller.close();
  expect(new Uint8Array(await response.arrayBuffer())).toEqual(new Uint8Array([0, 128, 255]));
  expect(response.headers.get('content-type')).toBe('audio/wav');
  expect(response.headers.get('cache-control')).toBe('private,no-store');
  expect(upstreamSignal.aborted).toBe(true);
});

test('raw screen mode and downstream cancellation preserve their contracts', async () => {
  const upstream = source();
  const response = await relayMedia(new AbortController().signal,
    async () => new Response(upstream.body, {headers: {'content-type': 'multipart/x-mixed-replace; boundary=frame'}}),
    async () => true, true);
  expect(response.headers.get('content-type')).toBe('application/octet-stream');
  await response.body!.cancel();
  expect(upstream.cancellations()).toBe(1);
});

test('session revocation closes a silent stream and stops polling', async () => {
  const upstream = source();
  let checks = 0;
  const response = await relayMedia(new AbortController().signal, async () => new Response(upstream.body),
    async () => { checks++; return false; }, false, 5);
  expect(await response.body!.getReader().read()).toEqual({done: true, value: undefined});
  await Bun.sleep(20);
  expect(checks).toBe(1);
  expect(upstream.cancellations()).toBe(1);
});

test('session check errors fail closed without overlapping checks', async () => {
  for (const outcome of ['unknown', 'throw'] as const) {
    const upstream = source();
    let checks = 0;
    const response = await relayMedia(new AbortController().signal, async () => new Response(upstream.body), async () => {
      checks++;
      await Bun.sleep(15);
      if (outcome === 'throw') throw new Error('offline');
      return null;
    }, false, 2);
    expect(await response.body!.getReader().read()).toEqual({done: true, value: undefined});
    expect(checks).toBe(1);
    expect(upstream.cancellations()).toBe(1);
  }
});

test('a source that finishes connecting after cancellation is discarded', async () => {
  const abort = new AbortController(), upstream = source();
  let release!: (value: Response) => void;
  const pending = relayMedia(abort.signal, () => new Promise(resolve => { release = resolve; }), async () => true);
  abort.abort();
  release(new Response(upstream.body));
  await expect(pending).rejects.toThrow();
  expect(upstream.cancellations()).toBe(1);
});

test('an active session is checked again, then closes when revoked', async () => {
  const upstream = source();
  let checks = 0;
  const response = await relayMedia(new AbortController().signal, async () => new Response(upstream.body),
    async () => ++checks < 3, false, 3);
  expect(await response.body!.getReader().read()).toEqual({done: true, value: undefined});
  expect(checks).toBe(3);
  expect(upstream.cancellations()).toBe(1);
});

test('a session check finishing after disconnect cannot close downstream twice', async () => {
  const upstream = source(), abort = new AbortController();
  let finish!: (value: boolean) => void;
  let ready!: () => void;
  const checking = new Promise<void>(resolve => { ready = resolve; });
  const response = await relayMedia(abort.signal, async () => new Response(upstream.body), () => {
    ready();
    return new Promise(resolve => { finish = resolve; });
  }, false, 2);
  const reader = response.body!.getReader();
  await checking;
  abort.abort();
  finish(false);
  expect(await reader.read()).toEqual({done: true, value: undefined});
  await Bun.sleep(5);
  expect(upstream.cancellations()).toBe(1);
});

test('non-success responses and fetch failures never leave an active connection', async () => {
  for (const kind of ['status', 'no-body', 'network'] as const) {
    const upstream = source();
    let signal!: AbortSignal;
    const failure = relayMedia(new AbortController().signal, async value => {
      signal = value;
      if (kind === 'network') throw new Error('offline');
      if (kind === 'no-body') return new Response(null, {status: 204});
      return new Response(upstream.body, {status: 503});
    }, async () => true);
    await expect(failure).rejects.toThrow(kind === 'network' ? 'offline' : 'media_unavailable');
    expect(signal.aborted).toBe(true);
    expect(upstream.cancellations()).toBe(kind === 'status' ? 1 : 0);
  }
});

test('upstream read failure closes downstream and detaches the client listener', async () => {
  const upstream = source(), abort = new AbortController();
  let removed = 0;
  const remove = abort.signal.removeEventListener.bind(abort.signal);
  abort.signal.removeEventListener = (...args) => { removed++; remove(...args); };
  const response = await relayMedia(abort.signal, async () => new Response(upstream.body), async () => true);
  upstream.controller.error(new Error('interrupted'));
  expect(await response.body!.getReader().read()).toEqual({done: true, value: undefined});
  expect(removed).toBe(1);
  abort.abort();
  expect(removed).toBe(1);
});
