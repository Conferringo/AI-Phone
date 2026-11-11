import {
  copyFileSync,
  createReadStream,
  createWriteStream,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  realpathSync,
  renameSync,
  rmSync,
} from "node:fs";
import { createHash } from "node:crypto";
import { dirname, join, sep } from "node:path";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { entryBytes, readTar } from "./archive";
import { json, save } from "./inventory";
import { digest, output } from "./process";

export type WindowsVmInputs = {
  qemu: string;
  qemuIdentity: string;
  guest: string;
  guestIdentity: string;
};
type FileReceipt = { path: string; size: number; sha256: string };
export const windowsBackendComponents = [
  "guest-apps", "native-services", "phone-apps", "web",
];
const guestManifest = "WINDOWS-VM-DISK-MANIFEST.json";
const guestFiles = ["bzImage", "guest.qcow2", "kernel.config", "KERNEL-LICENSE.txt"];
const qemuPrograms = [
  "runtime/qemu-system-x86_64.exe",
  "runtime/qemu-img.exe",
  "runtime/share/bios-256k.bin",
  "runtime/share/linuxboot_dma.bin",
];

export async function windowsSourceIdentity(root: string) {
  const commit = output(["git", "rev-parse", "HEAD"], { cwd: root });
  if (!/^[0-9a-f]{40}$/.test(commit))
    throw Error("Complete Windows factory requires an exact source commit");
  return {
    source_commit: commit,
    source_manifest_sha256: await digest(join(root, "SOURCE-MANIFEST.json")),
  };
}

async function stageGuest(source: string, identity: string, stage: string) {
  const capsuleHash = createHash("sha256");
  async function* capsule() {
    for await (const chunk of createReadStream(source)) {
      capsuleHash.update(chunk);
      yield chunk;
    }
  }
  const files = new Map<string, FileReceipt>();
  let manifest: any;
  for await (const entry of readTar(capsule())) {
    if (entry.type !== "0" || !Number.isSafeInteger(entry.size) || entry.size <= 0)
      throw Error("Invalid Windows guest capsule member");
    if (entry.name === guestManifest && !manifest) {
      manifest = JSON.parse((await entryBytes(entry, 16384)).toString());
      continue;
    }
    if (!guestFiles.includes(entry.name) || files.has(entry.name))
      throw Error("Unexpected Windows guest capsule member");
    const measured = createHash("sha256"),
      path = "guest/" + entry.name;
    async function* member() {
      for await (const chunk of entry.body) {
        measured.update(chunk);
        yield chunk;
      }
    }
    await pipeline(
      Readable.from(member()),
      createWriteStream(join(stage, path), { flags: "wx" }),
    );
    files.set(entry.name, { path, size: entry.size, sha256: measured.digest("hex") });
  }
  if (
    capsuleHash.digest("hex") !== identity ||
    manifest?.format !== 1 ||
    manifest.host !== "windows-x64" ||
    manifest.runtime !== "qemu-linux-backend" ||
    JSON.stringify([...manifest.source_components].sort()) !==
      JSON.stringify(windowsBackendComponents) ||
    files.size !== guestFiles.length
  )
    throw Error("Invalid Windows guest capsule");
  for (const [name, row] of [
    ["bzImage", manifest.kernel],
    ["guest.qcow2", manifest.disk],
    ["kernel.config", manifest.kernel_source.config],
    ["KERNEL-LICENSE.txt", manifest.kernel_source.license],
  ] as const) {
    const actual = files.get(name)!;
    if (
      row.path !== name ||
      actual.sha256 !== row.sha256 ||
      (row.size !== undefined && actual.size !== row.size)
    )
      throw Error("Windows guest capsule changed: " + name);
  }
  return { manifest, files: [...files.values()] };
}

async function stageQemu(source: string, identity: string, stage: string) {
  source = realpathSync(source);
  const path = join(source, "windows-qemu.json");
  if ((await digest(path)) !== identity)
    throw Error("Windows QEMU factory manifest changed");
  const manifest = json(path);
  if (manifest.format !== 1 || manifest.component !== "windows-qemu-whpx")
    throw Error("Invalid Windows QEMU factory manifest");
  const files: FileReceipt[] = [],
    paths = new Set<string>();
  for (const row of manifest.files as FileReceipt[]) {
    if (row.path.startsWith("source/")) continue;
    const allowed =
      qemuPrograms.includes(row.path) ||
      /^runtime\/[\w.-]+\.dll$/i.test(row.path) ||
      /^licenses\/(?:[\w-][\w.-]*\/)*[\w-][\w.-]*$/.test(row.path);
    if (!allowed || paths.has(row.path.toLowerCase()))
      throw Error("Unexpected Windows QEMU file: " + row.path);
    paths.add(row.path.toLowerCase());
    const input = join(source, row.path), target = join(stage, row.path),
      info = lstatSync(input);
    if (
      !info.isFile() ||
      !realpathSync(input).startsWith(source + sep) ||
      info.size !== row.size ||
      (await digest(input)) !== row.sha256
    )
      throw Error("Windows QEMU file changed: " + row.path);
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(input, target);
    files.push(row);
  }
  if (
    qemuPrograms.some((name) => !paths.has(name)) ||
    !paths.has("licenses/qemu/copying")
  )
    throw Error("Windows QEMU runtime or license missing");
  return { manifest, files };
}

export async function stageWindowsVm(
  root: string,
  destination: string,
  input: WindowsVmInputs,
) {
  const stage = mkdtempSync(join(dirname(destination), ".qemu-"));
  try {
    mkdirSync(join(stage, "guest"));
    const source = await windowsSourceIdentity(root);
    const qemu = await stageQemu(input.qemu, input.qemuIdentity, stage);
    const guest = await stageGuest(input.guest, input.guestIdentity, stage);
    if (
      guest.manifest.source_commit !== source.source_commit ||
      guest.manifest.source_manifest_sha256 !== source.source_manifest_sha256
    )
      throw Error("Windows guest and installer source differ");
    const manifest = {
      format: 1,
      runtime: "qemu-whpx-backend",
      ...source,
      files: [...qemu.files, ...guest.files].sort((a, b) => a.path.localeCompare(b.path)),
      qemu_factory_manifest_sha256: input.qemuIdentity,
      guest_capsule_sha256: input.guestIdentity,
      qemu: qemu.manifest,
      guest: guest.manifest,
      windows_boot_verified: false,
      redistribution_review_complete: false,
    };
    const path = join(stage, "manifest.json");
    save(path, manifest);
    const identity = await digest(path);
    rmSync(destination, { recursive: true, force: true });
    renameSync(stage, destination);
    return identity;
  } finally {
    rmSync(stage, { recursive: true, force: true });
  }
}
