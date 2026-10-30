type Authorize = () => Promise<boolean | null>;

/** Owns the reader, downstream controller and session check for one response. */
class MediaSession {
  readonly body: ReadableStream<Uint8Array>;
  private output!: ReadableStreamDefaultController<Uint8Array>;
  private timer?: ReturnType<typeof setInterval>;
  private stopped = false;
  private checking = false;

  constructor(
    private readonly reader: ReadableStreamDefaultReader<Uint8Array>,
    private readonly abort: AbortController,
    private readonly dispose: () => void,
    private readonly authorize: Authorize,
    checkIntervalMs: number,
  ) {
    this.body = new ReadableStream<Uint8Array>({
      start: output => {
        this.output = output;
        this.timer = setInterval(() => { void this.checkSession(); }, checkIntervalMs);
        abort.signal.addEventListener('abort', this.disconnect);
      },
      pull: () => this.forward(),
      cancel: () => this.stop(false),
    });
  }

  private readonly disconnect = () => this.stop();

  private stop(close = true) {
    if (this.stopped) return;
    this.stopped = true;
    clearInterval(this.timer);
    this.abort.signal.removeEventListener('abort', this.disconnect);
    this.dispose();
    this.abort.abort();
    void this.reader.cancel().catch(() => {});
    if (close) this.output.close();
  }

  private async forward() {
    try {
      const next = await this.reader.read();
      if (this.stopped) return;
      if (next.done) this.stop();
      else this.output.enqueue(next.value);
    } catch { this.stop(); }
  }

  private async checkSession() {
    if (this.stopped || this.checking) return;
    this.checking = true;
    try {
      if (await this.authorize() !== true) this.stop();
    } catch { this.stop(); }
    finally { this.checking = false; }
  }
}

/** Transport only: each route retains its own authentication and device lookup. */
export async function relayMedia(
  signal: AbortSignal,
  connect: (signal: AbortSignal) => Promise<Response>,
  authorize: Authorize,
  rawScreen = false,
  checkIntervalMs = 5000,
) {
  signal.throwIfAborted();
  const abort = new AbortController();
  const disconnect = () => abort.abort();
  const dispose = () => signal.removeEventListener('abort', disconnect);
  signal.addEventListener('abort', disconnect);
  const deadline = setTimeout(disconnect, 5000);
  let upstream: Response | undefined;
  try {
    upstream = await connect(abort.signal);
    abort.signal.throwIfAborted();
    if (!upstream.ok || !upstream.body) throw new Error('media_unavailable');
    const session = new MediaSession(upstream.body.getReader(), abort, dispose, authorize, checkIntervalMs);
    return new Response(session.body, {headers: {
      'content-type': rawScreen ? 'application/octet-stream' : (upstream.headers.get('content-type') || 'application/octet-stream'),
      'cache-control': 'private,no-store',
      'x-content-type-options': 'nosniff',
      'x-accel-buffering': 'no',
    }});
  } catch (error) {
    dispose();
    abort.abort();
    void upstream?.body?.cancel().catch(() => {});
    throw error;
  } finally { clearTimeout(deadline); }
}
