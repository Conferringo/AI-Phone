// CDP uses its upstream JSON protocol. Device commands/receipts are protobuf.
import {
  create,
  fromBinary,
  toBinary,
  type MessageShape,
} from "@bufbuild/protobuf";
import {
  LocationFixSchema,
  LocationDeviceSchema,
} from "../../packages/protocol/src/gen/ai_phone/v1/location_pb";
import { mkdir, rename, writeFile } from "node:fs/promises";
type Fix = MessageShape<typeof LocationFixSchema>;
type Page = { id: string; type: string; webSocketDebuggerUrl?: string };
const folder = "/home/ai/.config/ai-phone-location",
  file = folder + "/state.pb",
  endpoint = "http://127.0.0.1:17867/fix";
function receipt(status: string, error = "") {
  return toBinary(
    LocationDeviceSchema,
    create(LocationDeviceSchema, {
      kind: "desktop",
      status,
      error,
      appliedAtMs: BigInt(Date.now()),
    }),
  );
}
function bounded(value: number, low: number, high: number) {
  return Number.isFinite(value) && value >= low && value <= high;
}
function validPoint(point: Fix["point"]) {
  return (
    !!point &&
    bounded(point.latitude, -90, 90) &&
    bounded(point.longitude, -180, 180) &&
    bounded(point.accuracyM, 0, 100000)
  );
}
function checked(bytes: Uint8Array): Fix {
  if (bytes.length > 4096) throw Error("invalid_location");
  const fix = fromBinary(LocationFixSchema, bytes);
  if (fix.enabled && !validPoint(fix.point)) throw Error("invalid_location");
  if (!bounded(fix.speedMps, 0, 500 / 3.6)) throw Error("invalid_location");
  return fix;
}
async function targets(): Promise<Page[]> {
  const r = await fetch("http://127.0.0.1:9222/json/list", {
    proxy: "",
    signal: AbortSignal.timeout(1500),
  });
  if (!r.ok) throw Error("browser_unavailable");
  return ((await r.json()) as Page[]).filter(
    (t) => t.type === "page" && t.webSocketDebuggerUrl,
  );
}
class Session {
  private socket: WebSocket;
  private sequence = 0;
  private pending = new Map<
    number,
    {
      resolve: () => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();
  readonly opened: Promise<void>;
  constructor(url: string) {
    this.socket = new WebSocket(url);
    this.opened = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.close();
        reject(Error("browser_timeout"));
      }, 2500);
      this.socket.onopen = () => {
        clearTimeout(timer);
        resolve();
      };
      this.socket.onerror = () => {
        clearTimeout(timer);
        reject(Error("browser_unavailable"));
      };
    });
    this.socket.onmessage = (e) => {
      const result = JSON.parse(String(e.data)),
        call = this.pending.get(result.id);
      if (!call) return;
      clearTimeout(call.timer);
      this.pending.delete(result.id);
      result.error ? call.reject(Error("browser_rejected")) : call.resolve();
    };
    this.socket.onclose = () => {
      for (const call of this.pending.values()) {
        clearTimeout(call.timer);
        call.reject(Error("browser_closed"));
      }
      this.pending.clear();
    };
  }
  get closed() {
    return this.socket.readyState >= WebSocket.CLOSING;
  }
  async command(method: string, params: Record<string, number> = {}) {
    await this.opened;
    const id = ++this.sequence;
    return new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(Error("browser_timeout"));
      }, 2500);
      this.pending.set(id, { resolve, reject, timer });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }
  close() {
    this.socket.close();
  }
}
export class LocationBrowser {
  constructor(private listTargets: () => Promise<Page[]> = targets) {}
  private sessions = new Map<string, Session>();
  async apply(fix: Fix) {
    if (!fix.enabled) {
      await this.clear();
      return;
    }
    const pages = await this.listTargets();
    if (!pages.length) throw Error("browser_unavailable");
    const alive = new Set(pages.map((p) => p.id));
    for (const [id, session] of this.sessions)
      if (!alive.has(id) || session.closed) {
        session.close();
        this.sessions.delete(id);
      }
    for (const page of pages) {
      let session = this.sessions.get(page.id);
      if (!session) {
        session = new Session(page.webSocketDebuggerUrl!);
        this.sessions.set(page.id, session);
      }
      await session.command("Emulation.setGeolocationOverride", {
        latitude: fix.point!.latitude,
        longitude: fix.point!.longitude,
        accuracy: Math.max(1, fix.point!.accuracyM),
        speed: fix.speedMps,
      });
    }
  }
  private async clear() {
    for (const session of this.sessions.values()) {
      await session
        .command("Emulation.clearGeolocationOverride")
        .catch(() => {});
      session.close();
    }
    this.sessions.clear();
  }
}
function message(error: unknown) {
  return error instanceof Error && error.message === "invalid_location"
    ? "位置坐标无效"
    : "请打开电脑浏览器后重试，定位仅应用于内置浏览器";
}
async function remember(bytes: Uint8Array) {
  await mkdir(folder, { recursive: true, mode: 0o700 });
  await writeFile(file + ".tmp", bytes, { mode: 0o600 });
  await rename(file + ".tmp", file);
}
async function daemon() {
  const browser = new LocationBrowser();
  let fix = create(LocationFixSchema),
    chain = Promise.resolve(),
    pending = 0;
  const serial = (work: () => Promise<void>) => {
    pending++;
    const result = chain.then(work).finally(() => {
      pending--;
    });
    chain = result.catch(() => {});
    return result;
  };
  const saved = Bun.file(file);
  if (await saved.exists())
    fix = checked(new Uint8Array(await saved.arrayBuffer()));
  Bun.serve({
    hostname: "127.0.0.1",
    port: 17867,
    async fetch(request) {
      if (request.method !== "POST" || new URL(request.url).pathname !== "/fix")
        return new Response(null, { status: 404 });
      try {
        const bytes = new Uint8Array(await request.arrayBuffer()),
          next = checked(bytes);
        await serial(async () => {
          await remember(bytes);
          fix = next;
          await browser.apply(fix);
        });
        return new Response(receipt(fix.enabled ? "applied" : "disabled"), {
          headers: { "content-type": "application/x-protobuf" },
        });
      } catch (error) {
        return new Response(receipt("failed", message(error)), {
          headers: { "content-type": "application/x-protobuf" },
        });
      }
    },
  });
  // Long-lived sessions also apply to newly opened tabs. Disabled means no browser requests.
  setInterval(() => {
    if (fix.enabled && pending === 0)
      void serial(() => browser.apply(fix)).catch(() => {});
  }, 1000);
}
async function client() {
  try {
    const bytes = new Uint8Array(await Bun.stdin.arrayBuffer());
    checked(bytes);
    const response = await fetch(endpoint, {
      method: "POST",
      proxy: "",
      headers: { "content-type": "application/x-protobuf" },
      body: bytes,
      signal: AbortSignal.timeout(8000),
    });
    if (!response.ok) throw Error("location_daemon");
    await Bun.write(Bun.stdout, new Uint8Array(await response.arrayBuffer()));
  } catch (error) {
    await Bun.write(Bun.stdout, receipt("failed", message(error)));
  }
}
if (import.meta.main) {
  if (process.argv.includes("--watch")) await daemon();
  else await client();
}
