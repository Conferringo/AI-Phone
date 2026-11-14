import {
  existsSync,
  readdirSync,
  lstatSync,
  mkdirSync,
  openSync,
  readSync,
  closeSync,
  rmSync,
} from "node:fs";
import { join, dirname, resolve } from "node:path";
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import {
  NativeAndroidPrepareSchema,
  NativeAndroidPreparedSchema,
} from "../../packages/protocol/src/gen/ai_phone/v1/native_android_pb";
import {
  ROOT,
  digest,
  download,
  hostKey,
  run,
  nativeProgram,
} from "./process";
import { canonical, json, type FileRecord } from "./inventory";
import {
  writeTar,
  writeTarStream,
  tarEntries,
  entryBytes,
  type ArchiveFile,
} from "./archive";
import { spawn } from "node:child_process";
import { once } from "node:events";

const abis: Record<string, string> = {
  "macos-arm64": "arm64-v8a",
  "linux-x64": "x86_64",
  "windows-x64": "x86_64",
};
type AndroidManifest = {
  host: string;
  abi: string;
  engine: string;
  files: FileRecord[];
};

function androidReceipt(manifest: AndroidManifest, archiveIdentity: string) {
  return {
    host: manifest.host,
    abi: manifest.abi,
    engine: manifest.engine,
    files: manifest.files.length,
    bytes: manifest.files.reduce((total, row) => total + row.size, 0),
    archive_sha256: archiveIdentity,
    redistribution: "review-pending",
  };
}
export async function pinnedDownload(
  pin: { url: string; sha256: string; size_bytes?: number },
  file: string,
) {
  if (!existsSync(file)) await download(pin.url, file);
  if (
    (pin.size_bytes !== undefined && lstatSync(file).size !== pin.size_bytes) ||
    (await digest(file)) !== pin.sha256
  )
    throw Error("Pinned download mismatch: " + file.split(/[\\/]/).at(-1));
  return file;
}
export function nativeArm64(file: string) {
  const fd = openSync(file, "r");
  try {
    const header = Buffer.alloc(8);
    readSync(fd, header, 0, 8, 0);
    const thin = (value: Buffer) =>
      value.readUInt32LE(0) === 0xfeedfacf &&
      value.readUInt32LE(4) === 0x0100000c;
    if (thin(header)) return;
    const magic = header.readUInt32BE(0),
      little = [0xbebafeca, 0xbfbafeca].includes(magic),
      wide = [0xcafebabf, 0xbfbafeca].includes(magic);
    if (![0xcafebabe, 0xbebafeca, 0xcafebabf, 0xbfbafeca].includes(magic))
      throw Error("ARM64 Mach-O required");
    const uint = (value: Buffer, offset: number) =>
        little ? value.readUInt32LE(offset) : value.readUInt32BE(offset),
      count = uint(header, 4),
      width = wide ? 32 : 20,
      table = Buffer.alloc(count * width);
    if (count < 1 || count > 64) throw Error("Invalid Mach-O slices");
    readSync(fd, table, 0, table.length, 8);
    for (let start = 0; start < table.length; start += width)
      if (uint(table, start) === 0x0100000c) {
        const offset = wide
          ? Number(
              little
                ? table.readBigUInt64LE(start + 8)
                : table.readBigUInt64BE(start + 8),
            )
          : uint(table, start + 8);
        readSync(fd, header, 0, 8, offset);
        if (thin(header)) return;
      }
    throw Error("Native ARM64 slice missing");
  } finally {
    closeSync(fd);
  }
}
// Maintainer input: the extracted, version-pinned AOSP and native host tools.
export async function sealAndroidSdk(
  sdk: string,
  file: string,
  preset = 6,
  root = ROOT,
  host = hostKey(),
) {
  if (host !== "macos-arm64" && host !== "windows-x64")
    throw Error("Native desktop capsule requires macOS arm64 or Windows x64");
  const [engine, directory, api] = host === "macos-arm64"
    ? ["avo", "avo", 37] as const
    : ["whpx", "cuttlefish", 35] as const;
  const abi = abis[host];
  if (engine === "avo") {
    nativeArm64(join(sdk, "avo/avo-vmm"));
    nativeArm64(join(sdk, "platform-tools/adb"));
  }
  const files: { path: string; file: string; mode: number; size: number }[] = [];
  function walk(folder: string) {
    for (const entry of readdirSync(join(sdk, folder), { withFileTypes: true })) {
      const name = `${folder}/${entry.name}`;
      if (entry.isDirectory()) walk(name);
      else if (entry.isFile()) {
        const path = canonical(`sdk/${name}`), input = join(sdk, name);
        const info = lstatSync(input);
        const executable = /\.exe$/i.test(name) || name === "platform-tools/adb" ||
          name === "avo/avo-vmm" || (process.platform !== "win32" && (info.mode & 0o111) !== 0);
        files.push({ path, file: input, mode: executable ? 0o755 : 0o644, size: info.size });
      } else throw Error("Android capsule may only contain regular files: " + name);
    }
  }
  walk(directory);
  walk("platform-tools");
  const records = await Promise.all(files.map(async (row) => ({
    path: row.path, size: row.size, mode: row.mode, sha256: await digest(row.file),
  })));
  const manifest = { format: 1, engine, host, api, abi, files: records };
  const contents: ArchiveFile[] = [
    { path: "ANDROID-MANIFEST.json", mode: 0o644, data: Buffer.from(JSON.stringify(manifest)) },
    ...files,
  ];
  const xz = process.env.AIMANITY_FACTORY_XZ;
  if (xz) {
    mkdirSync(dirname(file), { recursive: true });
    const fd = openSync(file, "wx");
    const compression = engine === "whpx" && preset === 6
      ? ["-T2", "--block-size=128MiB", "--lzma2=preset=7,dict=64MiB"]
      : [`-${preset}`, "-T6", "--block-size=64MiB"];
    const child = spawn(xz, [...compression, "--check=crc64"], {
      stdio: ["pipe", fd, "inherit"],
    });
    closeSync(fd);
    const done = once(child, "close");
    try {
      await Promise.all([writeTarStream(child.stdin!, contents), done]);
      if ((await done)[0] !== 0) throw Error("Android capsule compression failed");
    } finally {
      child.kill();
    }
  } else {
    await writeTar(file, contents, { codec: "xz", architecture: engine === "avo" ? "arm64" : "amd64", preset, root });
  }
  return {
    file,
    receipt: androidReceipt(manifest, await digest(file)),
  };
}

type FactoryAndroidInput =
  | { kind: "sdk"; path: string }
  | { kind: "sealed"; file: string; identity: string };

export function factoryAndroidInput(): FactoryAndroidInput {
  const sdk = process.env.AIMANITY_ANDROID_SDK_INPUT;
  const file = process.env.AIMANITY_ANDROID_PAYLOAD;
  const identity = process.env.AIMANITY_ANDROID_PAYLOAD_SHA256;
  if (sdk && !file && !identity) return { kind: "sdk", path: resolve(sdk) };
  if (!sdk && file && identity && /^[a-f0-9]{64}$/.test(identity))
    return { kind: "sealed", file: resolve(file), identity };
  throw Error("Provide a staged Android SDK or a sealed Android payload with its SHA-256");
}

export async function prepareFactoryAndroid(
  input: FactoryAndroidInput,
  work: string,
  preset: number,
  root: string,
) {
  const { file, receipt: { archive_sha256: identity } } = input.kind === "sdk"
    ? await sealAndroidSdk(input.path, join(work, "android.payload"), preset, root)
    : { file: input.file, receipt: { archive_sha256: input.identity } };
  const qualification = join(work, "android-qualification");
  const prepared = await prepareAndroid(file, qualification, identity, root);
  const manifest = json<AndroidManifest>(join(prepared.sdkRoot, "..", "ANDROID-MANIFEST.json"));
  rmSync(qualification, { recursive: true });
  if (manifest.engine !== (hostKey() === "macos-arm64" ? "avo" : "whpx"))
    throw Error("The Android payload requires the current native engine");
  return { file, receipt: androidReceipt(manifest, identity) };
}

export async function prepareAndroid(
  payload: string,
  home: string,
  payloadSha256: string,
  root = ROOT,
) {
  const request = create(NativeAndroidPrepareSchema, {
    version: 1,
    payloadPath: payload,
    payloadSha256,
    cpus: 2,
    memoryMb: 2048,
    storageGb: 16,
  });
  const reply = run([nativeProgram(root), "--prepare-android", home], {
    cwd: root,
    input: Buffer.from(toBinary(NativeAndroidPrepareSchema, request)),
    stdio: ["pipe", "pipe", "pipe"],
  });
  return fromBinary(NativeAndroidPreparedSchema, Buffer.from(reply.stdout!));
}
export async function androidManifest(payload: string, root = ROOT) {
  for await (const entry of tarEntries(payload, "xz", root)) {
    if (entry.name !== "ANDROID-MANIFEST.json")
      throw Error("Android capsule manifest must be first");
    return JSON.parse((await entryBytes(entry)).toString());
  }
  throw Error("Android capsule manifest missing");
}
