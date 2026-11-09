import { createPrivateKey, createPublicKey, sign } from "node:crypto";
import { stat, writeFile } from "node:fs/promises";
import { basename, join } from "node:path";
import { ROOT, digest } from "./tooling/process";

const [keyFile, macFile, windowsFile, output] = process.argv.slice(2);
if (!keyFile || !macFile || !windowsFile || !output || process.argv.length !== 6) {
  throw Error("Usage: sign-client-update.ts <private-key> <mac.dmg> <windows.exe> <update.json>");
}
const policy = await Bun.file(join(ROOT, "config/client-update.json")).json();
const { version, release_name: name, release_notes: notes } =
  await Bun.file(join(ROOT, "config/release-policy.json")).json();
if (typeof version !== "string" || !/^\d+\.\d+\.\d+$/.test(version) ||
    basename(output) !== `AI-Phone-${version}-update.json`) {
  throw Error("Output must match the stable release version");
}
if (typeof name !== "string" || !name.trim() || typeof notes !== "string" || !notes.trim()) {
  throw Error("The release policy must supply a version name and release notes");
}
const privateKey = createPrivateKey(await Bun.file(keyFile).text());
const publicKey = createPublicKey(privateKey);
const rawKey = publicKey.export({ format: "der", type: "spki" }).subarray(-32);
if (publicKey.asymmetricKeyType !== "ed25519" || rawKey.toString("base64") !== policy.public_key) {
  throw Error("Signing key does not match the client update public key");
}

async function artifact(path: string, os: "macos" | "windows", arch: string, suffix: string) {
  if (basename(path) !== `AI-Phone-${version}-${suffix}`) throw Error("Wrong installer filename");
  const report = await Bun.file(path + ".manifest.json").json();
  if (report.version !== version || report.os !== os || report.arch !== arch ||
      !report.complete_device_bundle || !report.runtime_payload_complete) {
    throw Error("Installer receipt does not match this release");
  }
  if (os === "windows") {
    const files = new Map<string, string>(report.payload.files.map((file: {path: string; sha256: string}) => [file.path, file.sha256]));
    if (files.get("host/qemu/guest/guest.qcow2") !== policy.windows_base_sha256 ||
        !files.has("host/qemu/guest/release-init.cpio.gz") ||
        !files.has("host/qemu/guest/release.ext4") ||
        typeof report.payload.qemu_upgrade_fixture !== "string" ||
        !report.payload.qemu_upgrade_fixture.includes("prior overlay data")) {
      throw Error("Windows installer must preserve the published QEMU base and include the qualified release layer");
    }
  }
  const { size } = await stat(path);
  if (size < 1 || size >= 2 ** 31) throw Error("Installer exceeds the release asset limit");
  const sha256 = await digest(path);
  if (sha256 !== report.sha256) throw Error("Installer differs from its build receipt");
  return { size, sha256 };
}

const [macos_arm64, windows_x64] = await Promise.all([
  artifact(macFile, "macos", "arm64", "macos-arm64.dmg"),
  artifact(windowsFile, "windows", "x64", "windows-x64-setup.exe"),
]);
const manifest = JSON.stringify({ version, name: name.trim(), notes: notes.trim(), macos_arm64, windows_x64 });
const signature = sign(null, Buffer.from(manifest), privateKey).toString("base64");
const envelope = JSON.stringify({ manifest, signature }) + "\n";
if (Buffer.byteLength(envelope) > 16 * 1024) throw Error("Update manifest exceeds the client limit");
await writeFile(output, envelope, { flag: "wx" });
