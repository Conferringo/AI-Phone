import {
  createReadStream,
  createWriteStream,
  openSync,
  closeSync,
  writeSync,
  fstatSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  rmSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { createHash } from "node:crypto";
import { Readable } from "node:stream";
import { createGunzip, createGzip } from "node:zlib";
import { ROOT, hash, digest, output, run } from "./process";
import {
  tarEntries,
  tarHeader,
  writeTarStream,
  releaseTools,
  entryBytes,
  type ArchiveFile,
  type TarEntry,
} from "./archive";

export const imageRoles = ["workspace", "desktop", "postgres", "redis"];
type Image = {
  ref: string;
  source_digest: string;
  export_digest: string;
  config_digest: string;
  size: number;
};
type Blob = {
  sha256: string;
  size: number;
  rawSha256: string;
  rawSize: number;
  gzip: boolean;
};
const blob = (identity: string) =>
  "blobs/sha256/" + identity.replace(/^sha256:/, "");
export const imageRef = (role: string, identity: string) =>
  "ai-phone-bundle/" + role + ":sha-" + identity.replace(/^sha256:/, "");
function reference(descriptor: any) {
  return (descriptor.annotations?.["io.containerd.image.name"] || "").replace(
    /^docker.io\//,
    "",
  );
}
async function scan(file: string) {
  const objects = new Map<string, any>(),
    files = new Map<string, Blob>();
  for await (const entry of tarEntries(file, "gzip")) {
    if (entry.type === "5") continue;
    if (entry.type !== "0") throw Error("Unexpected Docker archive entry");
    const name = entry.name.replace(/^\.\//, ""),
      measured = createHash("sha256");
    let size = 0,
      first: Buffer | undefined;
    const chunks: Buffer[] = [];
    async function* tapped() {
      for await (const data of entry.body) {
        const bytes = Buffer.from(data);
        first ||= bytes;
        size += bytes.length;
        measured.update(bytes);
        if (entry.size <= 8 * 1024 * 1024) chunks.push(bytes);
        yield bytes;
      }
    }
    const iterator = tapped()[Symbol.asyncIterator](),
      initial = await iterator.next();
    const gzip =
      !!initial.value && initial.value[0] === 0x1f && initial.value[1] === 0x8b;
    const input = (async function* () {
      if (!initial.done) yield initial.value;
      for (
        let next = await iterator.next();
        !next.done;
        next = await iterator.next()
      )
        yield next.value;
    })();
    let rawSize = 0;
    const raw = createHash("sha256");
    for await (const bytes of gzip
      ? Readable.from(input).pipe(createGunzip())
      : input) {
      rawSize += bytes.length;
      raw.update(bytes);
    }
    if (size !== entry.size) throw Error("Docker member changed");
    const sha256 = measured.digest("hex");
    files.set(name, {
      sha256,
      size,
      rawSha256: raw.digest("hex"),
      rawSize,
      gzip,
    });
    if (chunks.length) {
      const bytes = Buffer.concat(chunks);
      if (bytes[0] === 0x7b || bytes[0] === 0x5b)
        objects.set(name, JSON.parse(bytes.toString()));
    }
  }
  return { objects, files };
}
function plan(
  records: Record<string, Image>,
  objects: Map<string, any>,
  files: Map<string, Blob>,
) {
  const index = objects.get("index.json"),
    docker = objects.get("manifest.json");
  if (!Array.isArray(docker)) throw Error("Docker image inventory missing");
  const graphs = new Map<string, any>(),
    reserved = new Set<string>(),
    converted = new Map<
      string,
      { path: string; size: number; sha256: string }
    >(),
    metadata = new Map<string, Buffer>();
  const preserve = (descriptor: any) => {
    const name = blob(descriptor.digest);
    if (reserved.has(name)) return;
    reserved.add(name);
    const object = objects.get(name);
    if (!object) return;
    for (const child of [
      ...(object.manifests || []),
      ...(object.layers || []),
      ...(object.config ? [object.config] : []),
      ...(object.subject ? [object.subject] : []),
    ])
      preserve(child);
  };
  if (index) {
    if (index.schemaVersion !== 2) throw Error("OCI index invalid");
    for (const descriptor of index.manifests)
      if (
        !["workspace", "desktop"].some(
          (role) => reference(descriptor) === records[role]!.ref,
        )
      )
        preserve(descriptor);
    if (index.subject) preserve(index.subject);
  }
  for (const role of imageRoles) {
    const record = records[role]!,
      row = docker.find((row: any) => row.RepoTags?.includes(record.ref));
    if (!row) throw Error("Image export role missing: " + role);
    const config = objects.get(row.Config);
    if (config?.architecture === undefined || config.os !== "linux")
      throw Error("Docker config missing");
    record.config_digest = "sha256:" + files.get(row.Config)!.sha256;
    const descriptor = index?.manifests.find(
      (d: any) => reference(d) === record.ref,
    );
    if (descriptor) {
      record.export_digest = descriptor.digest;
      graphs.set(role, objects.get(blob(descriptor.digest)));
    } else record.export_digest = record.config_digest;
  }
  if (!index) return { records, metadata, converted, skip: new Set<string>() };
  const skip = new Set<string>(["index.json", "manifest.json"]);
  for (const role of ["workspace", "desktop"]) {
    const record = records[role]!,
      graph = structuredClone(graphs.get(role)),
      row = docker.find((row: any) => row.RepoTags?.includes(record.ref)),
      config = objects.get(blob(record.config_digest));
    if (!graph || graph.layers.length !== config.rootfs.diff_ids.length)
      throw Error("Local OCI graph incomplete");
    const oldGraph = blob(record.export_digest);
    if (!reserved.has(oldGraph)) skip.add(oldGraph);
    graph.layers = graph.layers.map((layer: any, n: number) => {
      const name = blob(layer.digest),
        source = files.get(name)!;
      if (!source || "sha256:" + source.rawSha256 !== config.rootfs.diff_ids[n])
        throw Error("OCI layer content differs from config");
      if (!source.gzip || reserved.has(name)) return layer;
      const path = blob("sha256:" + source.rawSha256);
      converted.set(name, {
        path,
        size: source.rawSize,
        sha256: source.rawSha256,
      });
      skip.add(name);
      return {
        ...layer,
        mediaType: "application/vnd.oci.image.layer.v1.tar",
        digest: "sha256:" + source.rawSha256,
        size: source.rawSize,
      };
    });
    const bytes = Buffer.from(JSON.stringify(graph)),
      identity = "sha256:" + hash(bytes),
      ref = imageRef(role, identity);
    metadata.set(blob(identity), bytes);
    for (const descriptor of index.manifests)
      if (reference(descriptor) === record.ref) {
        descriptor.digest = identity;
        descriptor.size = bytes.length;
        descriptor.annotations = {
          ...descriptor.annotations,
          "io.containerd.image.name": "docker.io/" + ref,
          "org.opencontainers.image.ref.name": ref
            .split(":")
            .slice(1)
            .join(":"),
        };
      }
    row.RepoTags = [ref];
    row.Layers = graph.layers.map((layer: any) => blob(layer.digest));
    record.export_digest = identity;
    record.ref = ref;
  }
  metadata.set("index.json", Buffer.from(JSON.stringify(index)));
  metadata.set("manifest.json", Buffer.from(JSON.stringify(docker)));
  return { records, metadata, converted, skip };
}
export async function buildImages(
  root: string,
  sources: Record<string, string>,
  destination: string,
) {
  if (
    imageRoles.some((role) => !sources[role]) ||
    Object.keys(sources).length !== 4
  )
    throw Error("Exactly four factory image roles required");
  mkdirSync(dirname(destination), { recursive: true });
  const architecture = JSON.parse(
    output(["docker", "info", "--format", "{{json .}}"]),
  ).Architecture;
  const arch =
    architecture === "aarch64"
      ? "arm64"
      : architecture === "x86_64"
        ? "amd64"
        : architecture;
  if (!["amd64", "arm64"].includes(arch))
    throw Error("Unsupported Docker architecture");
  const records: Record<string, Image> = {};
  for (const role of imageRoles) {
    const record = JSON.parse(
      output(["docker", "image", "inspect", sources[role]!]),
    )[0];
    if (record.Os !== "linux" || record.Architecture !== arch)
      throw Error("Factory image architecture mismatch");
    const selected = JSON.parse(
        output([
          "docker",
          "image",
          "inspect",
          record.Id,
          "--platform",
          "linux/" + arch,
        ]),
      )[0],
      ref = imageRef(role, selected.Id);
    run(["docker", "tag", record.Id, ref]);
    records[role] = {
      ref,
      source_digest: record.Id,
      export_digest: selected.Id,
      config_digest: "",
      size: record.Size,
    };
  }
  const temporary = mkdtempSync(join(dirname(destination), ".images-")),
    raw = join(temporary, "export.tar.gz"),
    payload = join(temporary, "images.payload");
  try {
    const child = spawn(
        "docker",
        [
          "image",
          "save",
          "--platform",
          "linux/" + arch,
          ...Object.values(records).map((row) => row.ref),
        ],
        { stdio: ["ignore", "pipe", "inherit"] },
      ),
      finished = once(child, "close"),
      file = createWriteStream(raw),
      closed = once(file, "close");
    child.stdout!.pipe(createGzip({ level: 1 })).pipe(file);
    await closed;
    if ((await finished)[0] !== 0) throw Error("Docker export failed");
    const catalog = await scan(raw),
      conversion = plan(records, catalog.objects, catalog.files);
    async function* members(): AsyncGenerator<ArchiveFile> {
      const emitted = new Set<string>();
      for await (const entry of tarEntries(raw, "gzip", root)) {
        const name = entry.name.replace(/^\.\//, "");
        if (entry.type === "5") continue;
        const transform = conversion.converted.get(name);
        if (transform && !emitted.has(transform.path)) {
          emitted.add(transform.path);
          yield {
            path: transform.path,
            mode: 0o644,
            size: transform.size,
            body: Readable.from(entry.body).pipe(createGunzip()),
          };
        } else if (!conversion.skip.has(name) && !emitted.has(name)) {
          emitted.add(name);
          yield { path: name, mode: 0o644, size: entry.size, body: entry.body };
        }
      }
      for (const [path, data] of conversion.metadata)
        yield { path, data, mode: 0o644 };
    }
    const fd = openSync(payload, "wx");
    let result: Record<string, unknown>;
    try {
      writeSync(fd, Buffer.alloc(512));
      const encoder = spawn(
          releaseTools(root),
          ["encode-xz", arch, "9", temporary],
          { stdio: ["pipe", fd, "inherit"] },
        ),
        done = once(encoder, "close");
      await writeTarStream(encoder.stdin!, members());
      if ((await done)[0] !== 0) throw Error("Image compression failed");
      const size = fstatSync(fd).size - 512;
      const measured = createHash("sha256");
      for await (const chunk of createReadStream(payload, {
        start: 512,
        end: 511 + size,
      }))
        measured.update(chunk);
      const manifest = {
        format: 2,
        os: "linux",
        architecture: arch,
        redistribution: "review-pending",
        images: conversion.records,
        archive: {
          path: "images.tar.xz",
          codec: "xz",
          size,
          sha256: measured.digest("hex"),
        },
      };
      writeSync(fd, tarHeader("images.tar.xz", size), 0, 512, 0);
      let position = 512 + size;
      const pad = Buffer.alloc((512 - (size % 512)) % 512);
      writeSync(fd, pad, 0, pad.length, position);
      position += pad.length;
      const bytes = Buffer.from(JSON.stringify(manifest)),
        head = tarHeader("IMAGES-MANIFEST.json", bytes.length);
      for (const chunk of [
        head,
        bytes,
        Buffer.alloc((512 - (bytes.length % 512)) % 512),
        Buffer.alloc(1024),
      ]) {
        writeSync(fd, chunk, 0, chunk.length, position);
        position += chunk.length;
      }
      result = {
        ...manifest,
        bytes: position,
      };
    } finally {
      closeSync(fd);
    }
    renameSync(payload, destination);
    return { ...result, archive_sha256: await digest(destination) };
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}
export async function imageManifest(file: string, root = ROOT) {
  for await (const entry of tarEntries(file, "raw", root))
    if (entry.name === "IMAGES-MANIFEST.json")
      return JSON.parse((await entryBytes(entry)).toString());
  throw Error("Images manifest missing");
}
