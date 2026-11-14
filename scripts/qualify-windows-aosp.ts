import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import { spawn, spawnSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { connect } from "node:net";
import { join, resolve } from "node:path";
import {
  NativeAndroidSessionControlSchema,
  NativeAndroidSessionReplySchema,
  NativeAndroidStartSchema,
} from "../packages/protocol/src/gen/ai_phone/v1/native_android_pb";
import { digest } from "./tooling/process";
import { prepareAndroid } from "./tooling/android-component";

const [input, destination] = process.argv.slice(2);
if (!input || !destination) throw Error("Provide the new Android capsule and clean test home");
const capsule = resolve(input), home = resolve(destination);
const unpackStarted = performance.now();
const phone = await prepareAndroid(capsule, home, await digest(capsule));
console.log("Local Android SDK prepared in", Math.round(performance.now() - unpackStarted), "ms");
const worker = spawn(join("target", "release", "host-resources.exe"), ["--android-session"], {
  stdio: ["pipe", "pipe", "inherit"],
});
function send(body: Uint8Array) {
  const header = Buffer.alloc(4);
  header.writeUInt32BE(body.length);
  worker.stdin.write(Buffer.concat([header, Buffer.from(body)]));
}
const stdout = worker.stdout[Symbol.asyncIterator]();
const finished = new Promise<number | null>((success, fail) => {
  worker.once("error", fail);
  worker.once("exit", (code) => success(code));
});
let verified = false;
try {
  const start = performance.now();
  send(toBinary(NativeAndroidStartSchema, create(NativeAndroidStartSchema, { phone })));
  let frame = Buffer.alloc(0);
  while (frame.length < 4 || frame.length < frame.readUInt32BE() + 4) {
    const chunk = await stdout.next();
    if (chunk.done) throw Error("Android session exited without a boot receipt");
    frame = Buffer.concat([frame, chunk.value]);
  }
  const { result } = fromBinary(NativeAndroidSessionReplySchema, frame.subarray(4, frame.readUInt32BE() + 4));
  if (result.case !== "booted") throw Error("Android boot failed: " + JSON.stringify(result));
  const { serial, rootVerified, width, height } = result.value;
  if (!rootVerified || serial !== "127.0.0.1:5565" || width < 1 || height < 1)
    throw Error("Native boot receipt did not prove the expected root device");
  const adbPath = join(phone.sdkRoot, "platform-tools/adb.exe");
  function adb(...args: string[]) {
    const reply = spawnSync(adbPath, ["-P", "15037", "-s", serial, ...args], { maxBuffer: 8 * 1024 * 1024 });
    if (reply.error) throw reply.error;
    if (reply.status !== 0) throw Error(`adb ${args[0]} failed: ${reply.stderr.toString().trim() || reply.stdout.toString().trim()}`);
    return reply.stdout;
  }
  if (adb("shell", "id", "-u").toString().trim() !== "0") throw Error("Independent ADB root readback failed");
  const png = Buffer.from("89504e470d0a1a0a", "hex");
  const snapshot = () => {
    const image = adb("exec-out", "screencap", "-p");
    if (!image.subarray(0, 8).equals(png)) throw Error("Android returned an invalid PNG");
    return image;
  };
  const before = snapshot();
  console.log(JSON.stringify({ acceleration: "whpx", rootVerified, serial, width, height, before: before.length, boot_ms: Math.round(performance.now() - start) }));
  adb("shell", "svc power stayon true; input keyevent KEYCODE_WAKEUP; cmd statusbar collapse; wm dismiss-keyguard; input swipe 360 1150 360 360 280");
  const audioStarted = performance.now();
  const audioPort = Number(adb("forward", "tcp:0", "localabstract:scrcpy_00000017").toString().trim());
  const recorder = spawn(adbPath, ["-P", "15037", "-s", serial, "shell", "-T",
    "exec su 2000 env CLASSPATH=/data/local/tmp/aimanity-audio.jar app_process / com.genymobile.scrcpy.Server 3.3.4 scid=00000017 audio=true audio_source=output audio_codec=raw video=false control=false tunnel_forward=true send_dummy_byte=true send_device_meta=false send_frame_meta=false cleanup=false"],
  { stdio: ["ignore", "inherit", "inherit"] });
  try {
    const deadline = performance.now() + 5000;
    for (;;) {
      const pcm = connect(audioPort, "127.0.0.1");
      pcm.setTimeout(12000, () => pcm.destroy(Error("Android audio samples timed out")));
      try {
        let prefix: Buffer = Buffer.alloc(0);
        let tail: Buffer = Buffer.alloc(0);
        let min = 0, max = 0, samples = 0;
        for await (const data of pcm) {
          let chunk: Buffer = data;
          if (prefix.length < 5) {
            chunk = Buffer.concat([prefix, chunk]);
            prefix = chunk.subarray(0, 5);
            if (prefix.length < 5) continue;
            if (!prefix.equals(Buffer.from([0, 0, 114, 97, 119]))) throw Error("Android rejected raw audio capture");
            const playback = adb("shell", "am", "start", "-W", "-a", "android.intent.action.VIEW", "-d", "file:///product/media/audio/alarms/Neon.ogg", "-t", "audio/ogg", "-n", "com.android.music/.AudioPreview").toString();
            if (!/Status: ok\b/.test(playback)) throw Error(`Android alarm did not play: ${playback.trim()}`);
            chunk = chunk.subarray(5);
          }
          if (tail.length) chunk = Buffer.concat([tail, chunk]);
          const complete = chunk.length & ~1;
          for (let index = 0; index < complete; index += 2) {
            const value = chunk.readInt16LE(index);
            min = Math.min(min, value);
            max = Math.max(max, value);
            samples++;
          }
          if (min < 0 && max > 0) {
            console.log(JSON.stringify({ androidAudio: "scrcpy-adb", rate: 48000, channels: 2, samples, min, max, elapsed_ms: Math.round(performance.now() - audioStarted) }));
            break;
          }
          if (samples >= 10 * 48000 * 2) throw Error("Android alarm produced silent PCM");
          tail = chunk.subarray(complete);
        }
        if (min < 0 && max > 0) break;
        if (prefix.length || recorder.exitCode !== null || performance.now() >= deadline)
          throw Error("Android audio capture ended before returning nonzero PCM");
      } finally {
        pcm.destroy();
      }
      await Bun.sleep(100);
    }
  } finally {
    recorder.kill();
    adb("forward", "--remove", `tcp:${audioPort}`);
    adb("shell", "am", "force-stop", "com.android.music");
  }
  const launch = adb("shell", "am", "start", "-W", "--user", "0", "-a", "android.intent.action.DIAL", "-d", "tel:", "-p", "com.android.dialer").toString();
  if (!/Status: ok\b/.test(launch)) throw Error(`Android Dialer did not launch: ${launch.trim()}`);
  const path = "/sdcard/aimanity-whpx.xml";
  const readUi = async (matches: (xml: string) => boolean) => {
    const deadline = performance.now() + 15000;
    let last = "";
    do {
      last = adb("shell", `rm -f ${path}; timeout 7 uiautomator dump ${path} 2>&1; [ ! -s ${path} ] || cat ${path}`).toString();
      if (matches(last)) return last;
      await Bun.sleep(150);
    } while (performance.now() < deadline);
    const focus = adb("shell", "dumpsys window displays").toString().split("\n")
      .filter(line => /mCurrentFocus|mFocusedApp/.test(line)).join("\n").trim();
    throw Error(`Android Dialer did not show expected input: ${focus}\n${last.trim().slice(0, 500)}`);
  };
  const one = /<node[^>]*resource-id="com\.android\.dialer:id\/one"[^>]*bounds="\[(\d+),(\d+)\]\[(\d+),(\d+)\]"/;
  const dialpad = await readUi(xml => one.test(xml));
  const button = dialpad.match(one)!;
  adb("shell", "input", "tap", String((Number(button[1]) + Number(button[3])) >> 1), String((Number(button[2]) + Number(button[4])) >> 1));
  const digits = (xml: string) => xml.match(/<node[^>]*text="([^"]*)"[^>]*resource-id="com\.android\.dialer:id\/digits"/)?.[1].replace(/\D/g, "");
  await readUi(xml => digits(xml) === "1");
  adb("shell", "input", "text", "374");
  await readUi(xml => digits(xml) === "1374");
  const after = snapshot();
  if (before.equals(after)) throw Error("The Android frame did not change after input");
  verified = true;
  console.log(JSON.stringify({ acceleration: "whpx", rootVerified, serial, width, height, before: before.length, after: after.length, tap: "1", input: "374", readback: "1374", elapsed_ms: Math.round(performance.now() - start) }));
} catch (error) {
  for (const path of [join(phone.avdHome, "..", "native", "sdk.log"), join(phone.avdHome, "cuttlefish", "kernel.log")]) {
    try {
      const lines = readFileSync(path, "utf8").split("\n");
      if (path.endsWith("kernel.log"))
        console.error(lines.filter(line => /service 'console'|\badbd\b|sys\.boot_completed|Starting kernel/.test(line)).slice(0, 30).join("\n"));
      console.error(lines.slice(-80).join("\n"));
    }
    catch {}
  }
  throw error;
} finally {
  if (worker.exitCode === null) {
    send(toBinary(NativeAndroidSessionControlSchema, create(NativeAndroidSessionControlSchema, { cancelStartup: true })));
    worker.stdin.end();
  }
  const exited = await finished;
  if (verified && exited !== 0) throw Error("Android owner did not stop cleanly: " + exited);
}
