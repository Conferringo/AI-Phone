import {
  existsSync,
  readFileSync,
  mkdirSync,
  copyFileSync,
  chmodSync,
  mkdtempSync,
  rmSync,
} from "node:fs";
import { join, basename } from "node:path";
import { ROOT, output, run, hash, download } from "./process";
import { sourceFiles, type SourceFile } from "./source";
import {
  json,
  save,
  verifyFiles,
  records,
  collect,
  publicBytes,
} from "./inventory";
import { buildApk } from "./android-apk";

const packages = {
  input: "dev.aiphone.input",
  home: "dev.aiphone.home",
} as const;
export const probeSource =
  "scripts/android-input-probe/NotificationAccess.java";
export function phoneFingerprint(root = ROOT) {
  return hash(
    JSON.stringify(
      sourceFiles(root)
        .filter(
          (f) =>
            f.path.startsWith("apps/android-input/") ||
            f.path.startsWith("apps/android-home/") ||
            [
              "scripts/tooling/phone-apps.ts",
              "scripts/tooling/android-apk.ts",
              probeSource,
            ].includes(f.path),
        )
        .map((f) => [f.path, hash(f.data)]),
    ),
  );
}
export function inspectApk(file: string, java = process.env.JAVA_HOME!) {
  const listing = output([join(java, "bin/jar"), "tvf", file]);
  const rows = listing.split("\n").filter(Boolean);
  if (
    rows.length > 10000 ||
    rows.reduce((total, row) => total + Number(row.trim().split(/\s+/)[0]), 0) >
      256 * 1024 * 1024
  )
    throw Error("APK review budget exceeded");
  return rows;
}
export function checkApk(file: string, packageName: string, signer?: string) {
  const tools = join(process.env.ANDROID_HOME!, "build-tools/35.0.0");
  inspectApk(file);
  const certs = [
    ...output([
      join(tools, "apksigner"),
      "verify",
      "--print-certs",
      file,
    ]).matchAll(/Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})/g),
  ].map((m) => m[1]);
  if (certs.length !== 1 || (signer && certs[0] !== signer))
    throw Error("APK signer mismatch");
  if (
    !output([join(tools, "aapt2"), "dump", "badging", file]).startsWith(
      `package: name='${packageName}'`,
    )
  )
    throw Error("APK package mismatch");
  return { package: packageName, signer_sha256: certs[0] };
}
export function compileProbe(root = ROOT, work = join(root, "data/android")) {
  const folder = join(work, "notification-probe"),
    classes = join(folder, "classes"),
    dex = join(folder, "dex"),
    sdk = process.env.ANDROID_HOME!,
    jdk = join(process.env.JAVA_HOME!, "bin"),
    platform = join(sdk, "platforms/android-35/android.jar");
  mkdirSync(classes, { recursive: true });
  mkdirSync(dex, { recursive: true });
  run([
    join(jdk, "javac"),
    "--release",
    "8",
    "-classpath",
    platform,
    "-d",
    classes,
    join(root, probeSource),
  ]);
  run([
    join(sdk, "build-tools/35.0.0/d8"),
    "--lib",
    platform,
    "--min-api",
    "26",
    "--output",
    dex,
    join(classes, "dev/aiphone/input/probe/NotificationAccess.class"),
  ]);
  const file = join(folder, "notification-access.jar");
  run([join(jdk, "jar"), "cMf", file, "-C", dex, "classes.dex"]);
  return file;
}
export async function verifyPhone(root: string, folder: string) {
  const value = json(join(folder, "manifest.json"));
  if (value.format !== 1 || value.source_sha256 !== phoneFingerprint(root))
    throw Error("Phone helpers differ from source");
  if (
    JSON.stringify(Object.keys(value.apps).sort()) !==
    JSON.stringify(Object.keys(packages).sort())
  )
    throw Error("Phone helper missing");
  await verifyFiles(
    folder,
    value.files.map((row: any) => ({ ...row, path: basename(row.path) })),
  );
  return value;
}
export async function buildPhone(root = ROOT) {
  const destination = join(root, ".tools/phone-apps");
  if (existsSync(destination)) return verifyPhone(root, destination);
  mkdirSync(destination, { recursive: true });
  const apps: Record<string, unknown> = {};
  for (const [role, packageName] of Object.entries(packages)) {
    const apk = buildApk(join(root, "apps", "android-" + role), role, role);
    apps[role] = checkApk(apk, packageName);
    copyFileSync(apk, join(destination, role + ".apk"));
  }
  copyFileSync(
    compileProbe(root),
    join(destination, "notification-access.jar"),
  );
  const files = collect(destination, "runtime/phone-apps").files;
  const value = {
    format: 1,
    source_sha256: phoneFingerprint(root),
    apps,
    files: records(files),
  };
  save(join(destination, "manifest.json"), value);
  return verifyPhone(root, destination);
}
export async function preparePhone(root = ROOT) {
  const folder = join(root, ".tools/phone-apps");
  await verifyPhone(root, folder);
  return collect(folder, "runtime/phone-apps").files;
}
export function installerApk(root: string, role: keyof typeof packages) {
  const bundled = join(root, "runtime/phone-apps", role + ".apk");
  return existsSync(bundled)
    ? bundled
    : buildApk(join(root, "apps", "android-" + role), role, role);
}
export function notificationHelper(root = ROOT) {
  const bundled = join(root, "runtime/phone-apps/notification-access.jar");
  return existsSync(bundled) ? bundled : compileProbe(root);
}
export function guestPins(root = ROOT) {
  const value = json(join(root, "config/clash-apps.json"));
  return {
    "arm64-v8a": value.android,
    x86_64: value.alternatives["android-x86_64"],
  };
}
export function guestFingerprint(root = ROOT) {
  return hash(
    JSON.stringify(
      ["config/clash-apps.json", "scripts/tooling/phone-apps.ts"].map(
        (name) => [name, hash(readFileSync(join(root, name)))],
      ),
    ),
  );
}
export async function buildGuestApps(root = ROOT) {
  const destination = join(root, ".tools/guest-apps");
  mkdirSync(destination, { recursive: true });
  const apps: Record<string, unknown> = {},
    files: SourceFile[] = [];
  for (const [abi, pin] of Object.entries(guestPins(root))) {
    const file = join(root, ".tools/clash-apps/android", pin.file);
    if (!existsSync(file)) await download(pin.url, file);
    const bytes = readFileSync(file);
    if (bytes.length !== pin.size_bytes || hash(bytes) !== pin.sha256)
      throw Error("Clash APK pin mismatch");
    apps[abi] = {
      package: "com.github.metacubex.clash.meta",
      version: pin.version,
      signer_sha256: pin.signer_sha256,
    };
    checkApk(file, "com.github.metacubex.clash.meta", pin.signer_sha256);
    const name = "clash-" + abi + ".apk";
    copyFileSync(file, join(destination, name));
    files.push({
      path: "runtime/guest-apps/" + name,
      data: bytes,
      mode: 0o644,
    });
  }
  const value = {
    format: 1,
    source_sha256: guestFingerprint(root),
    apps,
    files: records(files),
  };
  save(join(destination, "manifest.json"), value);
  return value;
}
export async function prepareGuestApps(root = ROOT) {
  const folder = join(root, ".tools/guest-apps"),
    value = json(join(folder, "manifest.json"));
  if (value.source_sha256 !== guestFingerprint(root))
    throw Error("Guest app source changed");
  await verifyFiles(
    folder,
    value.files.map((row: any) => ({ ...row, path: basename(row.path) })),
  );
  const abi = process.arch === "arm64" ? "arm64-v8a" : "x86_64";
  const selected = value.files.filter((row: any) =>
    row.path.endsWith("clash-" + abi + ".apk"),
  );
  const manifest = {
    ...value,
    apps: { [abi]: value.apps[abi] },
    files: selected,
  };
  return [
    ...selected.map((row: any) => ({
      path: row.path,
      data: readFileSync(join(folder, basename(row.path))),
      mode: row.mode,
    })),
    {
      path: "runtime/guest-apps/manifest.json",
      data: Buffer.from(JSON.stringify(manifest)),
      mode: 0o644,
    },
  ];
}
