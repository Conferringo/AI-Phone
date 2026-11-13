import {
  existsSync,
  readFileSync,
  readdirSync,
  lstatSync,
  mkdirSync,
  openSync,
  readSync,
  closeSync,
  mkdtempSync,
  rmSync,
} from "node:fs";
import { join, relative } from "node:path";
import { createHash } from "node:crypto";
import { create, toBinary, fromBinary } from "@bufbuild/protobuf";
import {
  NativeAndroidPrepareSchema,
  NativeAndroidPreparedSchema,
} from "../../packages/protocol/src/gen/ai_phone/v1/native_android_pb";
import {
  ROOT,
  hash,
  digest,
  download,
  hostKey,
  output,
  run,
  nativeProgram,
} from "./process";
import { json, save, canonical, type FileRecord } from "./inventory";
import { Zip, type ZipEntry } from "./zip";
import {
  writeTar,
  tarEntries,
  entryBytes,
  releaseTools,
  type ArchiveFile,
} from "./archive";

const abis: Record<string, string> = {
  "macos-arm64": "arm64-v8a",
  "linux-x64": "x86_64",
  "windows-x64": "x86_64",
};
export function androidPolicy(root = ROOT) {
  const value = json(join(root, "config/android-factory.json")),
    host = hostKey(),
    selected = value.hosts[host];
  if (
    value.format !== 1 ||
    value.api !== 35 ||
    value.license_id !== "android-sdk-license" ||
    selected?.abi !== abis[host]
  )
    throw Error("Unsupported Android factory policy");
  const pins = {
    ...selected.archives,
    "system-image": value.images[selected.abi],
  };
  if (Object.values(pins).some((pin: any) => pin.channel !== "stable"))
    throw Error("Stable SDK archives required");
  if (
    host === "macos-arm64" &&
    (pins.emulator.host_os !== "macosx" ||
      pins.emulator.host_arch !== "aarch64" ||
      pins["platform-tools"].host_os !== "macosx" ||
      pins["system-image"].abi !== "arm64-v8a")
  )
    throw Error("Native ARM64 Android factory required");
  return pins;
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
export function androidPath(name: string, abi: string) {
  canonical(name);
  if (
    ![
      "sdk/emulator/",
      "sdk/platform-tools/",
      `sdk/system-images/android-35/default/${abi}/`,
    ].some((prefix) => name.startsWith(prefix)) ||
    name
      .split("/")
      .some(
        (part) =>
          [
            ".env",
            "snapshots",
            "userdata-qemu.img",
            "userdata-qemu.img.qcow2",
          ].includes(part) || part.startsWith("._"),
      )
  )
    throw Error("Invalid Android component path");
  return name;
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
function sdkFiles(sdk: string, abi: string) {
  const files: Record<string, string> = {};
  function walk(folder: string) {
    for (const entry of readdirSync(folder, { withFileTypes: true })) {
      if (entry.name.startsWith("._") || entry.name === ".DS_Store") continue;
      const file = join(folder, entry.name);
      if (entry.isSymbolicLink()) throw Error("SDK runtime contains an alias");
      if (entry.isDirectory()) walk(file);
      else if (entry.isFile())
        files[
          androidPath("sdk/" + relative(sdk, file).split("\\").join("/"), abi)
        ] = file;
    }
  }
  for (const component of ["emulator", "platform-tools"])
    walk(join(sdk, component));
  return files;
}
async function buildAndroidRuntime(
  root: string,
  sdk: string,
  imageArchive: string,
  destination: string,
  expectedImage: string,
  preset = 6,
) {
  if (![3, 6, 9].includes(preset)) throw Error("Unsupported XZ preset");
  const abi = abis[hostKey()]!,
    tools = sdkFiles(sdk, abi);
  if (hostKey() === "windows-x64") {
    const selected = JSON.parse(
      output([releaseTools(root), "windows-sdk-files"], {
        input: JSON.stringify(Object.keys(tools)),
      }),
    );
    for (const name of Object.keys(tools))
      if (!selected.includes(name)) delete tools[name];
  }
  const records: FileRecord[] = [],
    sources = new Map<string, ArchiveFile>();
  for (const [name, file] of Object.entries(tools)) {
    const info = lstatSync(file),
      mode =
        process.platform === "win32"
          ? /\.(exe|cmd|bat)$/i.test(name)
            ? 0o755
            : 0o644
          : info.mode & 0o111
            ? 0o755
            : 0o644;
    records.push({
      path: name,
      size: info.size,
      sha256: await digest(file),
      mode,
    });
    sources.set(name, { path: name, file, size: info.size, mode });
  }
  const image = new Zip(imageArchive);
  for (const entry of image.entries) {
    if (entry.directory) continue;
    if (!entry.name.startsWith(abi + "/"))
      throw Error("Unexpected Android image ABI");
    const name = androidPath(
        `sdk/system-images/android-35/default/${abi}/` +
          entry.name.slice(abi.length + 1),
        abi,
      ),
      measured = createHash("sha256");
    for await (const chunk of image.body(entry)) measured.update(chunk);
    const mode = entry.mode & 0o111 ? 0o755 : 0o644;
    records.push({
      path: name,
      size: entry.size,
      sha256: measured.digest("hex"),
      mode,
    });
    sources.set(name, {
      path: name,
      size: entry.size,
      mode,
      body: image.body(entry),
    });
  }
  const manifest = {
    format: 1,
    host: hostKey(),
    api: 35,
    abi,
    redistribution: "review-pending",
    image_archive_sha256: expectedImage,
    files: records,
  };
  await writeTar(
    destination,
    [
      {
        path: "ANDROID-MANIFEST.json",
        mode: 0o644,
        data: Buffer.from(JSON.stringify(manifest)),
      },
      ...sources.values(),
    ],
    {
      codec: "xz",
      architecture: abi === "x86_64" ? "amd64" : "arm64",
      preset,
      root,
    },
  );
  return {
    host: manifest.host,
    abi,
    files: records.length,
    bytes: records.reduce((n, row) => n + row.size, 0),
    archive_sha256: await digest(destination),
    redistribution: manifest.redistribution,
  };
}
export async function buildAndroid(root: string, work: string, preset = 6) {
  mkdirSync(work, { recursive: true });
  const pins = androidPolicy(root),
    archives: Record<string, string> = {};
  for (const [role, pin] of Object.entries(pins))
    archives[role] = await pinnedDownload(
      pin as any,
      join(work, role + ".zip"),
    );
  const temporary = mkdtempSync(join(work, ".sdk-"));
  try {
    const sdk = join(temporary, "sdk");
    mkdirSync(sdk);
    for (const role of ["emulator", "platform-tools"]) {
      await new Zip(archives[role]!).extract(sdk, role);
      const properties = readFileSync(
        join(sdk, role, "source.properties"),
        "utf8",
      );
      const revision = properties
        .match(/^Pkg.Revision\s*=\s*(.+)$/m)?.[1]
        ?.trim();
      if (revision !== pins[role].version) throw Error("SDK revision mismatch");
    }
    if (hostKey() === "macos-arm64") {
      nativeArm64(join(sdk, "emulator/emulator"));
      nativeArm64(join(sdk, "platform-tools/adb"));
    }
    const file = join(work, "android.payload"),
      receipt = await buildAndroidRuntime(
        root,
        sdk,
        archives["system-image"]!,
        file,
        pins["system-image"].sha256,
        preset,
      );
    return { file, receipt };
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
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
