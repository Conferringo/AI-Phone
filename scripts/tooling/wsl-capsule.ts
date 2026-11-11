import { createReadStream, lstatSync, linkSync, unlinkSync } from "node:fs";
import { join, dirname, posix } from "node:path";
import { createHash, randomUUID } from "node:crypto";
import { Readable } from "node:stream";
import { createGunzip } from "node:zlib";
import { tarEntries, readTar, entryBytes, writeTar } from "./archive";

export const wslBody = "rootfs.tar.gz",
  wslManifest = "WSL-BACKEND-MANIFEST.json",
  qemuBootstrapContract = "ai-phone-qemu-bootstrap-v2";
export const phoneRoot = "opt/aiphone/native-phone-bridge/",
  phoneSchema = "emulator/lib/emulator_controller.proto",
  phoneManifest = "PHONE-TOOLS-MANIFEST.json";
export const phoneRequired = [
  "platform-tools/adb",
  "platform-tools/source.properties",
  "platform-tools/NOTICE.txt",
  phoneSchema,
  phoneManifest,
  "provenance.json",
  "LICENSE",
];
const required = new Set([
  "etc/os-release",
  "etc/wsl.conf",
  "opt/aiphone/bundle/source.payload",
  "opt/aiphone/bundle/images.payload",
  "opt/aiphone/bundle/source.sha256",
  "usr/local/libexec/aiphone-bootstrap",
  "usr/local/bin/docker",
  "usr/local/bin/dockerd",
  ...phoneRequired.map((name) => phoneRoot + name),
]);
async function inspectRootfs(source: AsyncIterable<Buffer>) {
  const names = new Set<string>(),
    links = new Set<string>(),
    files: Record<string, string> = {},
    licenses: string[] = [];
  let total = 0;
  const stream = Readable.from(source),
    decoded = stream.pipe(createGunzip());
  stream.on("error", (error) => decoded.destroy(error));
  try {
    for await (const entry of readTar(decoded)) {
      const name = entry.name.replace(/^\.\//, "").replace(/\/$/, "");
      if (name === ".") continue;
      if (
        posix.isAbsolute(name) ||
        posix.normalize(name) !== name ||
        /[\\\x00-\x1f]/.test(name)
      )
        throw Error("Invalid WSL rootfs path: " + name);
      total += entry.size;
      if (
        names.has(name) ||
        names.size >= 100000 ||
        total > 16 * 1024 ** 3 ||
        !["0", "1", "2", "5"].includes(entry.type)
      )
        throw Error("Invalid WSL rootfs inventory");
      names.add(name);
      if (entry.type !== "5") {
        const parts = name.split("/"),
          shell =
            ((parts[0] === "root" && parts.length === 2) ||
              (parts[0] === "home" && parts.length === 3)) &&
            [".bashrc", ".profile", ".bash_logout"].includes(parts.at(-1)!);
        if (
          parts.some(
            (p) =>
              [
                ".git",
                ".ssh",
                ".env",
                "authorized_keys",
                "id_rsa",
                "limactl",
              ].includes(p) || p.startsWith("qemu-system-"),
          ) ||
          (parts.includes("emulator") && name !== phoneRoot + phoneSchema) ||
          name.startsWith("var/lib/aiphone/") ||
          ((name.startsWith("home/") || name.startsWith("root/")) && !shell)
        )
          throw Error("Private state or foreign runtime in WSL rootfs");
      }
      if (entry.type === "1" || entry.type === "2") {
        const target = posix.normalize(
          posix.join(entry.type === "2" ? posix.dirname(name) : "", entry.link),
        );
        if (
          !entry.link ||
          /[\x00-\x1f]/.test(entry.link) ||
          target === ".." ||
          target.startsWith("../")
        )
          throw Error("Invalid WSL rootfs link");
        links.add(name);
      }
      if (entry.type === "0" && required.has(name)) {
        const hash = createHash("sha256");
        for await (const chunk of entry.body) hash.update(chunk);
        files[name] = hash.digest("hex");
      }
      if (
        entry.type === "0" &&
        name.startsWith("usr/share/doc/") &&
        name.endsWith("/copyright")
      )
        licenses.push(name);
    }
  } finally {
    decoded.destroy();
    stream.destroy();
  }
  for (const name of names) {
    let parent = posix.dirname(name);
    while (parent !== "." && parent !== "/") {
      if (links.has(parent)) throw Error("WSL rootfs writes through an alias");
      parent = posix.dirname(parent);
    }
  }
  if (Object.keys(files).length !== required.size || !licenses.length)
    throw Error("Incomplete WSL rootfs or license inventory");
  return {
    entries: names.size,
    uncompressed_bytes: total,
    required_files: files,
    base_license_files: licenses.sort(),
  };
}
function checkManifest(value: any) {
  if (
    value.format !== 1 ||
    value.host !== "windows-x64" ||
    value.backend_host !== "linux-x64" ||
    value.runtime !== "wsl2-backend" ||
    value.redistribution !== "review-pending" ||
    value.windows_boot_verified !== false ||
    value.complete_device_bundle !== false ||
    value.rootfs.path !== wslBody
  )
    throw Error("Unsupported WSL backend component");
}
function same(left: unknown, right: unknown): boolean {
  if (left === right) return true;
  if (!left || !right || typeof left !== "object" || typeof right !== "object")
    return false;
  const a = Object.keys(left),
    b = Object.keys(right);
  return (
    a.length === b.length &&
    a.every(
      (key) =>
        Object.hasOwn(right, key) &&
        same((left as any)[key], (right as any)[key]),
    )
  );
}
export async function verifyWsl(payload: string, root: string) {
  const names = new Set<string>();
  let manifest: any, inventory: any, body: any;
  for await (const entry of tarEntries(payload, "raw", root)) {
    if (
      names.has(entry.name) ||
      entry.type !== "0" ||
      ![wslBody, wslManifest].includes(entry.name)
    )
      throw Error("Invalid WSL capsule members");
    names.add(entry.name);
    if (entry.name === wslManifest)
      manifest = JSON.parse(
        (await entryBytes(entry, 4 * 1024 ** 2)).toString(),
      );
    else {
      if (!entry.size || entry.size > 16 * 1024 ** 3)
        throw Error("Invalid WSL rootfs size");
      const hash = createHash("sha256");
      async function* measured() {
        for await (const chunk of entry.body) {
          hash.update(chunk);
          yield chunk;
        }
      }
      inventory = await inspectRootfs(measured());
      body = { path: wslBody, size: entry.size, sha256: hash.digest("hex") };
    }
  }
  if (names.size !== 2 || !manifest) throw Error("Incomplete WSL capsule");
  checkManifest(manifest);
  if (!same(body, manifest.rootfs) || !same(inventory, manifest.inventory))
    throw Error("WSL capsule integrity differs");
  return manifest;
}
export async function publishWsl(
  rootfs: string,
  destination: string,
  provenance: unknown,
) {
  const measured = createHash("sha256");
  async function* source() {
    for await (const chunk of createReadStream(rootfs)) {
      measured.update(chunk);
      yield chunk;
    }
  }
  const inventory = await inspectRootfs(source()),
    value = {
      format: 1,
      host: "windows-x64",
      backend_host: "linux-x64",
      runtime: "wsl2-backend",
      rootfs: {
        path: wslBody,
        size: lstatSync(rootfs).size,
        sha256: measured.digest("hex"),
      },
      inventory,
      provenance,
      redistribution: "review-pending",
      windows_boot_verified: false,
      complete_device_bundle: false,
    };
  const temporary = join(dirname(destination), ".wsl-" + randomUUID());
  try {
    await writeTar(temporary, [
      { path: wslBody, file: rootfs, size: value.rootfs.size, mode: 0o644 },
      {
        path: wslManifest,
        data: Buffer.from(JSON.stringify(value)),
        mode: 0o644,
      },
    ]);
    linkSync(temporary, destination);
  } finally {
    unlinkSync(temporary);
  }
  return value;
}
