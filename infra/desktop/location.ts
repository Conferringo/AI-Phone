// Chromium CDP is an upstream JSON protocol. Our host/guest envelope is protobuf.
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  LocationFixSchema,
  LocationDeviceSchema,
} from "../../packages/protocol/src/gen/ai_phone/v1/location_pb";
const folder = "/home/ai/.config/ai-phone-location",
  file = folder + "/state.pb";
async function targets() {
  const r = await fetch("http://127.0.0.1:9222/json/list", {
    proxy: "",
    signal: AbortSignal.timeout(1500),
  });
  if (!r.ok) throw Error("browser_unavailable");
  return (
    (await r.json()) as {
      id: string;
      type: string;
      webSocketDebuggerUrl?: string;
    }[]
  ).filter((t) => t.type === "page" && t.webSocketDebuggerUrl);
}
async function override(
  url: string,
  fix: ReturnType<typeof fromBinary<typeof LocationFixSchema>>,
) {
  await new Promise<void>((resolve, reject) => {
    const socket = new WebSocket(url),
      timer = setTimeout(() => {
        socket.close();
        reject(Error("browser_timeout"));
      }, 2000);
    const finish = (error?: Error) => {
      clearTimeout(timer);
      socket.close();
      error ? reject(error) : resolve();
    };
    socket.onopen = () =>
      socket.send(
        JSON.stringify({
          id: 1,
          method: fix.enabled
            ? "Emulation.setGeolocationOverride"
            : "Emulation.clearGeolocationOverride",
          params: fix.enabled
            ? {
                latitude: fix.point!.latitude,
                longitude: fix.point!.longitude,
                accuracy: Math.max(1, fix.point!.accuracyM),
                speed: fix.speedMps,
              }
            : {},
        }),
      );
    socket.onmessage = (e) => {
      try {
        const value = JSON.parse(String(e.data));
        if (value.id === 1)
          finish(value.error ? Error("browser_rejected") : undefined);
      } catch {
        finish(Error("browser_response"));
      }
    };
    socket.onerror = () => finish(Error("browser_unavailable"));
  });
}
async function apply(
  fix: ReturnType<typeof fromBinary<typeof LocationFixSchema>>,
) {
  let pages;
  try {
    pages = await targets();
  } catch {
    if (!fix.enabled) return;
    throw Error("browser_unavailable");
  }
  if (fix.enabled && !pages.length) throw Error("browser_unavailable");
  for (const page of pages) await override(page.webSocketDebuggerUrl!, fix);
}
if (process.argv.includes("--watch")) {
  // Keep new tabs in sync. No network or browser calls while disabled.
  let wasEnabled = false;
  while (true) {
    try {
      const f = Bun.file(file);
      if (await f.exists()) {
        const fix = fromBinary(
          LocationFixSchema,
          new Uint8Array(await f.arrayBuffer()),
        );
        if (fix.enabled || wasEnabled) await apply(fix);
        wasEnabled = fix.enabled;
      }
    } catch {}
    await Bun.sleep(1000);
  }
} else {
  const bytes = new Uint8Array(await Bun.stdin.arrayBuffer()),
    fix = fromBinary(LocationFixSchema, bytes);
  if (
    fix.enabled &&
    (!fix.point ||
      !Number.isFinite(fix.point.latitude) ||
      Math.abs(fix.point.latitude) > 90 ||
      !Number.isFinite(fix.point.longitude) ||
      Math.abs(fix.point.longitude) > 180)
  )
    throw Error("invalid_location");
  const { mkdir, rename } = await import("node:fs/promises");
  await mkdir(folder, { recursive: true, mode: 0o700 });
  await Bun.write(file + ".tmp", bytes, { mode: 0o600 });
  await rename(file + ".tmp", file);
  await apply(fix);
  await Bun.write(
    Bun.stdout,
    toBinary(
      LocationDeviceSchema,
      create(LocationDeviceSchema, {
        kind: "desktop",
        status: fix.enabled ? "applied" : "disabled",
        appliedAtMs: BigInt(Date.now()),
      }),
    ),
  );
}
