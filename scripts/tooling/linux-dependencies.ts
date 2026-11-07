import {
  existsSync,
  readFileSync,
  mkdirSync,
  copyFileSync,
  chmodSync,
  readdirSync,
  statSync,
  mkdtempSync,
  renameSync,
  rmSync,
} from "node:fs";
import { join, basename, dirname } from "node:path";
import { ROOT, hash, digest, output, hostKey } from "./process";
import {
  json,
  save,
  collect,
  records,
  verifyFiles,
  type FileRecord,
} from "./inventory";
import { extractTar, writeTar } from "./archive";

export type DependencyProfile = {
  config: string;
  source: string[];
  roots: string[];
  required: string[];
  installFiles: [string, string, number][];
  prefix: string;
};
export const profiles: Record<string, DependencyProfile> = {
  "linux-host": {
    config: "config/linux-host-runtime.json",
    source: [
      "config/linux-host-runtime.json",
      "config/linux-factory.json",
      "scripts/tooling/linux-dependencies.ts",
      "scripts/linux-host/Dockerfile",
      "scripts/linux-host/collect.ts",
      "scripts/linux-host/launch.sh",
      "scripts/linux-host/lima.sh",
    ],
    roots: [
      "bin",
      "lib",
      "modules",
      "share",
      "licenses",
      "source-records.json",
    ],
    required: [
      "bin/qemu-system-x86_64",
      "bin/qemu-img",
      "bin/ssh",
      "bin/ssh-keygen",
      "bin/scp",
      "lib/ld-linux-x86-64.so.2",
      "share/qemu/edk2-x86_64-code.fd",
      "share/qemu/bios-256k.bin",
      "source-records.json",
      "licenses/qemu-system-x86.copyright",
    ],
    installFiles: [
      "qemu-system-x86_64",
      "qemu-img",
      "ssh",
      "ssh-keygen",
      "scp",
    ].map((name) => [
      "launchers/" + name,
      "scripts/linux-host/launch.sh",
      0o755,
    ]),
    prefix: "runtime/container-host/native/",
  },
  "linux-ocr": {
    config: "config/linux-ocr.json",
    source: [
      "config/linux-ocr.json",
      "config/linux-factory.json",
      "scripts/tooling/linux-dependencies.ts",
      "scripts/linux-ocr/Dockerfile",
      "scripts/linux-ocr/collect.ts",
    ],
    roots: ["bin", "lib", "tessdata", "licenses", "source-records.json"],
    required: [
      "bin/tesseract",
      "lib/ld-linux-x86-64.so.2",
      "lib/libtesseract.so.5",
      "tessdata/eng.traineddata",
      "tessdata/chi_sim.traineddata",
      "tessdata/configs/tsv",
      "source-records.json",
      "licenses/tesseract-ocr.copyright",
    ],
    installFiles: [],
    prefix: "runtime/services/ocr/",
  },
};
export function dependencyFingerprint(
  root: string,
  profile: DependencyProfile,
) {
  return hash(
    JSON.stringify(
      profile.source.map((name) => [
        name,
        hash(readFileSync(join(root, name))),
      ]),
    ),
  );
}
export function dependencyPin(root: string, role: string) {
  const value = json(join(root, profiles[role]!.config));
  if (value.format !== 1 || value.host !== "linux-x64")
    throw Error("Unsupported Linux dependency pin");
  return value.archive;
}
export async function verifyDependency(
  root: string,
  folder: string,
  role: string,
) {
  const profile = profiles[role]!,
    value = json(join(folder, "manifest.json"));
  if (
    value.format !== 1 ||
    value.host !== "linux-x64" ||
    value.source_sha256 !== dependencyFingerprint(root, profile) ||
    value.archive_sha256 !== dependencyPin(root, role).sha256
  )
    throw Error("Linux component identity changed");
  await verifyFiles(folder, value.files);
  return value;
}
export async function buildDependency(
  root: string,
  archive: string,
  destination: string,
  role: string,
) {
  if (existsSync(destination)) return verifyDependency(root, destination, role);
  const profile = profiles[role]!,
    pin = dependencyPin(root, role);
  if (
    statSync(archive).size !== pin.size_bytes ||
    (await digest(archive)) !== pin.sha256
  )
    throw Error("Linux dependency archive mismatch");
  mkdirSync(dirname(destination), { recursive: true });
  const work = mkdtempSync(join(dirname(destination), ".dependency-"));
  try {
    await extractTar(archive, work, "gzip", root);
    const files = collect(work, "").files;
    const names = new Set(files.map((f) => f.path.slice(1)));
    if (
      profile.required.some((name) => !names.has(name)) ||
      [...names].some((name) => !profile.roots.includes(name.split("/")[0]!))
    )
      throw Error("Linux dependency closure incomplete");
    for (const [target, source, mode] of profile.installFiles) {
      mkdirSync(dirname(join(work, target)), { recursive: true });
      copyFileSync(join(root, source), join(work, target));
      chmodSync(join(work, target), mode);
    }
    const inventory = collect(work, "").files.map((f) => ({
      ...f,
      path: f.path.slice(1),
    }));
    const value = {
      format: 1,
      host: "linux-x64",
      source_sha256: dependencyFingerprint(root, profile),
      archive_sha256: pin.sha256,
      files: records(inventory),
    };
    save(join(work, "manifest.json"), value);
    renameSync(work, destination);
    return value;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}
export async function prepareDependency(
  root: string,
  folder: string,
  role: string,
) {
  await verifyDependency(root, folder, role);
  return collect(folder, profiles[role]!.prefix.replace(/\/$/, "")).files;
}
export function nativeClosure(
  root: string,
  programs: Record<string, string>,
  modules: string[] = [],
) {
  const files: Record<string, { target: string; sha256: string }> = {};
  const explicit = new Set(Object.values(programs)),
    plugins = new Set(modules);
  for (const name of [...explicit, ...plugins]) {
    const shared = output(["ldd", name])
      .split("\n")
      .flatMap((line) => {
        const match = line.match(/=>\s+(\/\S+)/) || line.match(/^\s*(\/\S+)/);
        return match ? [match[1]!] : [];
      });
    for (const file of [name, ...shared]) {
      const group = explicit.has(file)
          ? "bin"
          : plugins.has(file)
            ? "modules"
            : "lib",
        target = group + "/" + basename(file),
        data = readFileSync(file),
        identity = hash(data),
        destination = join(root, target);
      if (
        existsSync(destination) &&
        hash(readFileSync(destination)) !== identity
      )
        throw Error("Conflicting native library basename");
      mkdirSync(dirname(destination), { recursive: true });
      copyFileSync(file, destination);
      chmodSync(destination, 0o755);
      files[file] = { target, sha256: identity };
    }
  }
  return files;
}
export function packageRecords(root: string, files: unknown) {
  const packages = output([
    "dpkg-query",
    "-W",
    "-f=${Package} ${Version}\n",
  ]).split("\n");
  mkdirSync(join(root, "licenses"), { recursive: true });
  for (const row of packages) {
    const name = row.split(" ")[0]!,
      source = join("/usr/share/doc", name, "copyright");
    if (existsSync(source))
      copyFileSync(source, join(root, "licenses", name + ".copyright"));
  }
  save(join(root, "source-records.json"), { native_files: files, packages });
}
export async function archiveDependency(folder: string, destination: string) {
  const files = collect(folder, "").files.map((f) => ({
    ...f,
    path: f.path.slice(1),
  }));
  await writeTar(destination, files, { codec: "gzip" });
}
