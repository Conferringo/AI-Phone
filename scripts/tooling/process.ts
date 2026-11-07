import { spawnSync, type SpawnSyncOptions } from "node:child_process";
import {
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  renameSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { createHash, randomUUID } from "node:crypto";
import { dirname, resolve } from "node:path";
import { pipeline } from "node:stream/promises";
export const ROOT = resolve(import.meta.dir, "../..");
export function bunProgram(root = ROOT) {
  return resolve(
    root,
    ".tools/bin",
    process.platform === "win32" ? "bun.exe" : "bun",
  );
}
export function run(args: readonly string[], options: SpawnSyncOptions = {}) {
  const result = spawnSync(args[0]!, args.slice(1), {
    cwd: ROOT,
    stdio: "inherit",
    maxBuffer: 64 * 1024 * 1024,
    ...options,
  });
  if (result.error) throw result.error;
  if (result.status !== 0)
    throw Error(`Command failed (${result.status}): ${args[0]}`);
  return result;
}
export function output(
  args: readonly string[],
  options: SpawnSyncOptions = {},
) {
  return String(
    run(args, {
      ...options,
      stdio: ["ignore", "pipe", "pipe"],
      encoding: "utf8",
    }).stdout,
  ).trim();
}
export async function digest(file: string) {
  const hash = createHash("sha256");
  for await (const chunk of createReadStream(file)) hash.update(chunk);
  return hash.digest("hex");
}
export function hash(bytes: string | Uint8Array) {
  return createHash("sha256").update(bytes).digest("hex");
}
export async function download(url: string, file: string) {
  const response = await fetch(url);
  if (!response.ok || !response.body)
    throw Error(`Download HTTP ${response.status}`);
  mkdirSync(dirname(file), { recursive: true });
  await pipeline(response.body, createWriteStream(file, { flags: "wx" }));
}
export function atomicJson(file: string, value: unknown) {
  mkdirSync(dirname(file), { recursive: true, mode: 0o700 });
  const temporary = resolve(dirname(file), `.${randomUUID()}.tmp`);
  writeFileSync(temporary, JSON.stringify(value, null, 2) + "\n", {
    mode: 0o600,
  });
  renameSync(temporary, file);
}
export function hostKey() {
  const systems: Record<string, string> = {
    darwin: "macos",
    linux: "linux",
    win32: "windows",
  };
  const architectures: Record<string, string> = { arm64: "arm64", x64: "x64" };
  const os = systems[process.platform],
    arch = architectures[process.arch];
  if (!os || !arch) throw Error("Unsupported host");
  return `${os}-${arch}`;
}
export function nativeProgram(root = ROOT) {
  if (process.platform === "win32")
    return resolve(
      root,
      process.env.CARGO_TARGET_DIR || "target",
      "release",
      "host-resources.exe",
    );
  const bundled = resolve(root, "runtime/services/bin/assistant");
  return existsSync(resolve(root, "runtime/services"))
    ? bundled
    : resolve(
        root,
        process.env.CARGO_TARGET_DIR || "target",
        "release",
        "ai-phone-assistant",
      );
}
export function nativeJson(args: string[], root = ROOT) {
  return JSON.parse(output([nativeProgram(root), ...args], { cwd: root }));
}
export function regular(file: string) {
  const stat = statSync(file);
  if (!stat.isFile()) throw Error(`Not a regular file: ${file}`);
  return stat;
}
