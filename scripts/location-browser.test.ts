import { test, expect } from "bun:test";
import { create } from "@bufbuild/protobuf";
import {
  LocationFixSchema,
  GeoPointSchema,
} from "../packages/protocol/src/gen/ai_phone/v1/location_pb";
import { LocationBrowser } from "../infra/desktop/location";

test("browser location keeps its debugger session alive until disabled and reuses it for movement", async () => {
  const connections = new Map<
    object,
    { latitude: number; longitude: number } | null
  >();
  let opened = 0;
  let closed: () => void = () => {};
  const server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    fetch(request, server) {
      if (server.upgrade(request)) return;
      return new Response(null, { status: 400 });
    },
    websocket: {
      open(ws) {
        connections.set(ws, null);
        opened++;
      },
      message(ws, message) {
        const call = JSON.parse(String(message));
        connections.set(
          ws,
          call.method === "Emulation.clearGeolocationOverride"
            ? null
            : call.params,
        );
        ws.send(JSON.stringify({ id: call.id, result: {} }));
      },
      close(ws) {
        connections.delete(ws);
        closed();
      },
    },
  });
  const browser = new LocationBrowser(async () => [
    {
      id: "page",
      type: "page",
      webSocketDebuggerUrl: `ws://127.0.0.1:${server.port}`,
    },
  ]);
  try {
    const fix = create(LocationFixSchema, {
      enabled: true,
      point: create(GeoPointSchema, {
        latitude: 31,
        longitude: 121,
        accuracyM: 5,
      }),
    });
    await browser.apply(fix);
    expect(connections.size).toBe(1);
    expect([...connections.values()][0]?.latitude).toBe(31);
    await browser.apply({ ...fix, point: { ...fix.point!, latitude: 32 } });
    expect(opened).toBe(1);
    expect([...connections.values()][0]?.latitude).toBe(32);
    const disconnected = new Promise<void>((resolve) => {
      closed = resolve;
    });
    await browser.apply(create(LocationFixSchema));
    await disconnected;
    expect(connections.size).toBe(0);
  } finally {
    await browser.apply(create(LocationFixSchema));
    server.stop(true);
  }
});
