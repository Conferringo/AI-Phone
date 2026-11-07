import {
  existsSync,
  readdirSync,
  mkdtempSync,
  rmSync,
  readFileSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { createServer } from "node:net";
import { once } from "node:events";
import { ROOT, hash } from "./process";
import { webCommand } from "./web-runtime";

export function isolatedEnvironment(
  root: string,
  original = process.env,
): NodeJS.ProcessEnv {
  root = resolve(root);
  if (
    existsSync(join(root, ".env")) ||
    (existsSync(join(root, "data")) && readdirSync(join(root, "data")).length)
  )
    throw Error("Use an isolated runtime without user state");
  const allowed = new Set([
    "HOME",
    "USERPROFILE",
    "SYSTEMROOT",
    "TEMP",
    "TMP",
    "TMPDIR",
    "LANG",
  ]);
  return {
    ...Object.fromEntries(
      Object.entries(original).filter(([key]) =>
        allowed.has(key.toUpperCase()),
      ),
    ),
    PATH:
      process.platform === "win32"
        ? (original.SystemRoot || "C:\\Windows") + "\\System32"
        : "/bin:/usr/bin",
    AI_PHONE_PROJECT_ROOT: root,
    BUN_RUNTIME_TRANSPILER_CACHE_PATH: "0",
  };
}
async function freePort() {
  const server = createServer();
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const port = (server.address() as { port: number }).port;
  await new Promise<void>((resolve, reject) =>
    server.close((error) => (error ? reject(error) : resolve())),
  );
  return port;
}
export async function verifyWeb(root = ROOT) {
  const command = await webCommand(root),
    folder = mkdtempSync(join(tmpdir(), "aiphone-web-")),
    port = await freePort(),
    origin = "http://127.0.0.1:" + port;
  const child = Bun.spawn(command, {
    cwd: root,
    env: {
      ...isolatedEnvironment(folder),
      PORT: String(port),
      HOSTNAME: "127.0.0.1",
      AI_PHONE_LOCAL_ENTRY: "1",
      NEXT_TELEMETRY_DISABLED: "1",
    },
    stdout: "ignore",
    stderr: "inherit",
  });
  try {
    const deadline = Date.now() + 25000;
    let html = "";
    while (!html) {
      if (child.exitCode !== null)
        throw Error("Prebuilt web exited before readiness");
      try {
        const response = await fetch(origin + "/phone");
        if (!response.ok)
          throw Error("Prebuilt page returned HTTP " + response.status);
        html = await response.text();
      } catch (error) {
        if (Date.now() >= deadline) throw error;
        await Bun.sleep(100);
      }
    }
    const scripts = [
      ...html.matchAll(/<script[^>]+src="(\/_next\/static\/[^"?]+)"/g),
    ].map((match) => match[1]!);
    if (!scripts.length) throw Error("Prebuilt page has no script assets");
    for (const url of scripts) {
      const response = await fetch(origin + url);
      if (
        !response.ok ||
        hash(new Uint8Array(await response.arrayBuffer())) !==
          hash(
            readFileSync(
              join(
                root,
                "runtime/web/apps/web/.next/static",
                url.slice("/_next/static/".length),
              ),
            ),
          )
      )
        throw Error("Prebuilt script differs from package");
    }
    const media = join(root, "runtime/web/apps/web/.next/static/media"),
      image = readdirSync(media).find((name) => name.endsWith(".png"));
    if (!image) throw Error("Prebuilt static PNG missing");
    const url = "/_next/static/media/" + image,
      response = await fetch(origin + url);
    if (
      !response.ok ||
      hash(new Uint8Array(await response.arrayBuffer())) !==
        hash(readFileSync(join(media, image)))
    )
      throw Error("Prebuilt PNG differs from package");
    if (
      (
        await fetch(
          origin + "/_next/image?url=" + encodeURIComponent(url) + "&w=64&q=75",
        )
      ).status !== 404
    )
      throw Error("Unused image optimizer remains enabled");
    return {
      web_html: 200,
      script_assets_verified: scripts.length,
      static_png_verified: true,
      image_optimizer: false,
      runtime: "bundled Bun",
      package_install_or_build: false,
      private_state_supplied: false,
    };
  } finally {
    child.kill();
    const stop = setTimeout(() => child.kill(9), 5000);
    try {
      await child.exited;
    } finally {
      clearTimeout(stop);
      rmSync(folder, { recursive: true, force: true });
    }
  }
}
