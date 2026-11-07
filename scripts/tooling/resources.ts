import {
  createReadStream,
  readFileSync,
  existsSync,
  readdirSync,
  statSync,
} from "node:fs";
import { join, basename } from "node:path";
import { createHash } from "node:crypto";
import { ROOT, hash } from "./process";
import { json, save, secret, canonical } from "./inventory";

const WINDOW = 4096;
type Vector = { size: number; sha256: string };
export function maskVectors(data: Buffer, vectors: Vector[]) {
  const result = Buffer.from(data);
  for (const prefix of ["-----BEGIN ", "sk-"]) {
    let cursor = 0;
    while ((cursor = data.indexOf(prefix, cursor)) >= 0) {
      const start = cursor;
      cursor += prefix.length;
      for (const row of vectors) {
        const end = start + row.size;
        if (
          end <= data.length &&
          data[end - 1] === 0 &&
          hash(data.subarray(start, end)) === row.sha256
        ) {
          result.fill(32, start, end);
          break;
        }
      }
    }
  }
  return result;
}
export async function inspectStream(
  source: AsyncIterable<Uint8Array>,
  file: string,
  vectors: Vector[],
  publicBuildDigest?: string,
) {
  const measured = createHash("sha256");
  let size = 0,
    tail = Buffer.alloc(0),
    buildPath = false;
  function inspect(bytes: Buffer, boundary: number) {
    const masked = maskVectors(bytes, vectors);
    const compact = Buffer.from(masked.filter((byte) => byte !== 0));
    const compactBoundary =
      boundary -
      masked.subarray(0, boundary).filter((byte) => byte === 0).length;
    for (const [sample, end] of [
      [masked, boundary],
      [compact, compactBoundary],
    ] as const) {
      const match = secret.exec(sample.toString("latin1"));
      if (match && match.index < end)
        throw Error("Private material in bundle: " + basename(file));
    }
    buildPath ||= /\/Users\/[A-Za-z0-9._-]+\//.test(bytes.toString());
  }
  for await (const chunk of source) {
    const bytes = Buffer.from(chunk),
      sample = Buffer.concat([tail, bytes]);
    inspect(sample, Math.max(0, sample.length - WINDOW));
    measured.update(bytes);
    size += bytes.length;
    tail = Buffer.from(sample.subarray(-WINDOW));
  }
  inspect(tail, tail.length);
  const sha256 = measured.digest("hex");
  if (
    (publicBuildDigest && sha256 !== publicBuildDigest) ||
    (buildPath && sha256 !== publicBuildDigest)
  )
    throw Error("Unapproved build path in bundle: " + basename(file));
  return { size, sha256 };
}
export function fileReceipt(
  file: string,
  root = ROOT,
  publicBuildDigest?: string,
) {
  return inspectStream(
    createReadStream(file),
    file,
    json(join(root, "config/public-crypto-vectors.json")).vectors,
    publicBuildDigest,
  );
}
export function resourceMap(root = ROOT) {
  const native = join(root, "apps/client/src-tauri"),
    resources = {
      ...json(join(native, "tauri.conf.json")).bundle.resources,
    } as Record<string, string>,
    integrity = join(native, "gen/host/integrity.json");
  if (existsSync(integrity)) {
    const expected = json(integrity);
    for (const name of ["android.payload", "images.payload"])
      if (name in expected) resources["gen/host/" + name] = "host/" + name;
    for (const group of ["wsl"])
      if (group + "/manifest.json" in expected) {
        const folder = join(native, "gen/host", group),
          manifest = json(join(folder, "manifest.json"));
        if (
          hash(readFileSync(join(folder, "manifest.json"))) !==
          expected[group + "/manifest.json"]
        )
          throw Error("Resource manifest changed");
        const names = ["manifest.json", "rootfs.tar.gz"];
        for (const name of names) {
          canonical(name);
          resources[`gen/host/${group}/${name}`] = `host/${group}/${name}`;
        }
      }
  }
  return resources;
}
export function buildConfig(root = ROOT) {
  const resources = resourceMap(root);
  for (const group of ["wsl"]) {
    const members = Object.keys(resources).filter((name) =>
      name.startsWith("gen/host/" + group + "/"),
    );
    for (const name of members) delete resources[name];
    if (members.length) resources["gen/host/" + group] = "host/" + group;
  }
  const target = join(root, "apps/client/src-tauri/gen/host/resources.json");
  save(target, { bundle: { resources } });
  return target;
}
export async function verifyCompiledInventory(
  binary: string,
  expected: Record<string, string>,
) {
  const missing = new Set(Object.values(expected));
  let tail = Buffer.alloc(0);
  for await (const chunk of createReadStream(binary)) {
    const sample = Buffer.concat([tail, Buffer.from(chunk)]);
    for (const value of missing)
      if (sample.includes(Buffer.from(value))) missing.delete(value);
    tail = Buffer.from(sample.subarray(-64));
  }
  if (missing.size)
    throw Error("Compiled resources differ from prepared payload");
}
