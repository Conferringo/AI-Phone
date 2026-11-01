/** Maintainer-only real Xvfb oracle. Never included in a device runtime. */
import assert from "node:assert/strict";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { createHash, randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync, existsSync, openSync, readSync, closeSync } from "node:fs";
import { join } from "node:path";

const [binary, legacy, receipt] = process.argv.slice(2);
assert(binary && legacy && receipt, "usage: qualification.ts RustBinary LegacyBridge Receipt");
const nonce = process.env.AI_PHONE_GUEST_QUALIFICATION;
assert(nonce && /^[a-f0-9-]{36}$/.test(nonce));
assert.equal(readFileSync("/home/ai/.aiphone-qualification-owner", "utf8"), nonce);
assert.equal(process.env.DISPLAY, ":0");
const root = join("/home/ai/Files", `qualification-${nonce}`);
assert(!existsSync(root), "fresh fixture only");
mkdirSync(root, { mode: 0o700 });
const owned = new Set<ChildProcess>();
const spawnErrors: Error[] = [];
const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

function digest(path: string) {
  const descriptor = openSync(path, "r"), hash = createHash("sha256"), buffer = Buffer.alloc(65536);
  try { for (;;) { const count = readSync(descriptor, buffer, 0, buffer.length, null); if (!count) return hash.digest("hex"); hash.update(buffer.subarray(0, count)); } }
  finally { closeSync(descriptor); }
}
function command(program: string, args: string[], input?: Buffer, seconds = 15) {
  const result = spawnSync(program, args, { input, encoding: "buffer", timeout: seconds * 1000, maxBuffer: 16 * 1024 * 1024, env: { ...process.env, HOME: "/home/ai", DISPLAY: ":0", LANG: "C.UTF-8" } });
  if (result.error) throw result.error;
  return { code: result.status, stdout: result.stdout ?? Buffer.alloc(0), stderr: result.stderr ?? Buffer.alloc(0) };
}
function checked(program: string, args: string[]) {
  const result = command(program, args); assert.equal(result.code, 0, result.stderr.toString()); return result.stdout.toString().trim();
}
function child(program: string, args: string[]) {
  const value = spawn(program, args, { env: { ...process.env, HOME: "/home/ai", DISPLAY: ":0", LANG: "C.UTF-8" }, stdio: ["ignore", "pipe", "pipe"] });
  owned.add(value); value.once("exit", () => owned.delete(value)); value.once("error", (error) => spawnErrors.push(error)); return value;
}
async function window(name: string) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (spawnErrors.length) throw spawnErrors[0];
    const result = command("xdotool", ["search", "--name", `^${name}$`]);
    const id = result.stdout.toString().trim().split("\n")[0];
    if (result.code === 0 && /^\d+$/.test(id)) { checked("xdotool", ["windowactivate", "--sync", id]); return id; }
    await delay(50);
  }
  throw new Error("owned fixture window did not appear");
}
function png(bytes: Buffer, path: string, width: number, height: number) {
  assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a"); writeFileSync(path, bytes);
  const dimensions = checked("ffprobe", ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", path]);
  assert.equal(dimensions, `${width},${height}`);
  const x = width === 1280 ? 1100 : Math.floor(width / 2);
  const y = height === 800 ? 700 : Math.floor(height / 2);
  const pixel = command("ffmpeg", ["-v", "error", "-i", path, "-vf", `format=rgb24,crop=1:1:${x}:${y}`, "-frames:v", "1", "-threads", "1", "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"]);
  assert.equal(pixel.code, 0, pixel.stderr.toString());
  assert.deepEqual([...pixel.stdout], [31, 63, 127], "actual X11 RGB channel order");
}
function cursor(id: string, expected: number, x: number, y: number) {
  const value = JSON.parse(readFileSync("/home/ai/.aiphone/cursor.json", "utf8")); assert.equal(value.action_id, id); assert.equal(value.points.length, expected);
  const last = value.points.at(-1); assert.equal(last.x, x); assert.equal(last.y, y);
  assert(value.points.every((p: any, index: number) => index === 0 || p.at_ms >= value.points[index - 1].at_ms));
}

async function scenario(label: "legacy" | "rust") {
  const directory = join(root, label); mkdirSync(directory);
  const invoke = (operation: string, input: unknown = {}) => {
    const result = label === "legacy" ? command("/usr/bin/python3", [legacy, operation], Buffer.from(JSON.stringify(input)), operation === "run" ? 95 : 15) : command(binary, [operation], Buffer.from(JSON.stringify(input)), operation === "run" ? 95 : 15);
    assert.equal(result.code, 0, result.stdout.toString() + result.stderr.toString());
    return operation === "snapshot" ? result.stdout : JSON.parse(result.stdout.toString());
  };
  const call = (kind: string, args: object) => { const id = randomUUID(); const value = invoke("action", { kind, action_id: id, ...args }); assert.equal(value.success, true); return id; };
  checked("xsetroot", ["-solid", "#1f3f7f"]);
  const observe = invoke("observe"); assert.equal(observe.width, 1280); assert.equal(observe.height, 800); assert(observe.title);
  png(Buffer.from(observe.png, "base64"), join(directory, "observe.png"), 1280, 800);
  png(invoke("snapshot"), join(directory, "snapshot.png"), 1280, 800);
  const zoom = invoke("zoom", { png: observe.png, x: 1100, y: 700, width: 64, height: 64 }); assert.equal(zoom.success, true); assert.equal(zoom.scale, 2); png(Buffer.from(zoom.png, "base64"), join(directory, "zoom.png"), 128, 128);
  const name = `AIPhoneQualifier-${label}`; const eventWindow = child("stdbuf", ["-oL", "xev", "-name", name, "-geometry", "600x400+20+20"]); let events = "";
  eventWindow.stdout!.on("data", (bytes: Buffer) => { events += bytes.toString(); assert(events.length <= 512 * 1024); });
  eventWindow.stderr!.resume(); await window(name); await delay(100);
  const started = performance.now(); const clickId = call("click", { x: 140, y: 150, clicks: 2, button: 1 }); cursor(clickId, 2, 140, 150);
  const dragId = call("drag", { x: 160, y: 180, to_x: 250, to_y: 230 }); cursor(dragId, 13, 250, 230);
  await delay(50); const previous = (events.match(/ButtonPress event/g) ?? []).length;
  const scrollId = call("scroll", { x: 250, y: 230, amount: -99 }); cursor(scrollId, 2, 250, 230); await delay(100);
  assert.equal((events.match(/ButtonPress event/g) ?? []).length - previous, 8, "actual X11 button events");
  assert.match(events, /button 4/); eventWindow.kill("SIGTERM"); await delay(50);
  const out = join(directory, "paste.txt"); const terminalName = `AIPhonePaste-${label}`;
  const terminal = child("xterm", ["-title", terminalName, "-xrm", "XTerm*VT100*translations: #override Ctrl<Key>v: insert-selection(CLIPBOARD)", "-e", "sh", "-c", 'exec cat > "$1"', "sh", out]); terminal.stdout!.resume(); terminal.stderr!.resume(); await window(terminalName); await delay(100);
  const text = "你好，鲸鱼😀\n"; call("text", { text }); call("key", { key: "ctrl+d" });
  for (let n = 0; n < 100 && (!existsSync(out) || readFileSync(out, "utf8") !== text); n++) await delay(30);
  assert.equal(readFileSync(out, "utf8"), text); if (terminal.exitCode === null) terminal.kill("SIGTERM");
  const run = invoke("run", { script: `printf 'out'; printf 'err' >&2; exit 7` }); assert.deepEqual(run, { success: false, exit_code: 7, output: "outerr" });
  const listing = invoke("files", { path: `qualification-${nonce}/${label}` }); assert(listing.files.some((entry: any) => entry.name === "paste.txt" && entry.bytes === Buffer.byteLength(text)));
  const invalid = label === "legacy" ? command("/usr/bin/python3", [legacy, "action"], Buffer.from(JSON.stringify({ kind: "drag", x: 0, y: 0, to_x: 1280, to_y: 10 }))) : command(binary, ["action"], Buffer.from(JSON.stringify({ kind: "drag", x: 0, y: 0, to_x: 1280, to_y: 10 })));
  assert.equal(invalid.code, 1); assert.equal(JSON.parse(invalid.stdout.toString()).success, false);
  return { backend: label, cases: ["observe", "snapshot", "zoom", "click", "drag", "scroll", "text", "key", "run", "files", "invalid_endpoint"], input_bytes: Buffer.byteLength(text), cursor_ack: true, x11_scroll_events: 8, elapsed_ms: Math.round(performance.now() - started) };
}

try {
  const results = [await scenario("legacy"), await scenario("rust")];
  writeFileSync(receipt, JSON.stringify({ format: 1, fixture: "owned-xvfb", complete: true, source_commit: /^[0-9a-f]{40}$/.test(process.env.GITHUB_SHA ?? "") ? process.env.GITHUB_SHA : "", new_binary_sha256: digest(binary), old_source_sha256: digest(legacy), results, host_authority_epoch_tested: false, existing_devices_used: false, private_state_uploaded: false }, null, 2) + "\n");
} finally {
  for (const process of owned) {
    process.kill("SIGTERM"); await delay(50);
    if (process.exitCode === null && process.signalCode === null) {
      process.kill("SIGKILL");
      await Promise.race([new Promise<void>((resolve) => process.once("exit", () => resolve())), delay(2000).then(() => { throw new Error("owned fixture child did not exit"); })]);
    }
  }
}
