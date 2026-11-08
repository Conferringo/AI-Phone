import {
  createReadStream,
  createWriteStream,
  existsSync,
  mkdirSync,
  chmodSync,
  readFileSync,
  openSync,
  closeSync,
} from "node:fs";
import { join, dirname, resolve } from "node:path";
import { spawn } from "node:child_process";
import { once } from "node:events";
import type { Writable } from "node:stream";
import { createGunzip, createGzip } from "node:zlib";
import { ROOT, run } from "./process";
import { canonical } from "./inventory";

export function releaseTools(root = ROOT) {
  return resolve(
    root,
    process.env.CARGO_TARGET_DIR || "target",
    "debug",
    process.platform === "win32" ? "release-tools.exe" : "release-tools",
  );
}
export function buildReleaseTools(root = ROOT) {
  run(
    [
      "cargo",
      "build",
      "--locked",
      "-p",
      "ai-phone-release-tools",
      "--bin",
      "release-tools",
    ],
    { cwd: root },
  );
  return releaseTools(root);
}
class Bytes {
  private iterator: AsyncIterator<Buffer>;
  private pending = Buffer.alloc(0);
  constructor(input: AsyncIterable<Buffer>) {
    this.iterator = input[Symbol.asyncIterator]();
  }
  async read(size: number): Promise<Buffer> {
    const parts: Buffer[] = [];
    let remaining = size;
    while (remaining) {
      if (!this.pending.length) {
        const next = await this.iterator.next();
        if (next.done) throw Error("Incomplete archive");
        this.pending = Buffer.from(next.value);
      }
      const count = Math.min(remaining, this.pending.length);
      parts.push(this.pending.subarray(0, count));
      this.pending = this.pending.subarray(count);
      remaining -= count;
    }
    return Buffer.concat(parts, size);
  }
  async *chunks(size: number) {
    while (size) {
      const count = Math.min(size, 1024 * 1024);
      yield await this.read(count);
      size -= count;
    }
  }
  async finish() {
    this.pending = Buffer.alloc(0);
    while (!(await this.iterator.next()).done) {}
  }
}
export type TarEntry = {
  name: string;
  size: number;
  mode: number;
  type: string;
  link: string;
  body: AsyncIterable<Buffer>;
};
function text(header: Buffer, start: number, length: number) {
  return header
    .subarray(start, start + length)
    .toString()
    .replace(/\0.*$/s, "");
}
function number(header: Buffer, start: number, length: number) {
  const bytes = header.subarray(start, start + length);
  if (bytes[0]! & 0x80) {
    const copy = Buffer.from(bytes);
    copy[0]! &= 0x7f;
    let value = 0;
    for (const byte of copy) value = value * 256 + byte;
    return value;
  }
  return parseInt(text(header, start, length).trim() || "0", 8);
}
function pax(bytes: Buffer) {
  const result: Record<string, string> = {};
  let offset = 0;
  while (offset < bytes.length) {
    const split = bytes.indexOf(32, offset),
      length = Number(bytes.subarray(offset, split).toString());
    if (!length || offset + length > bytes.length)
      throw Error("Invalid PAX header");
    const row = bytes.subarray(split + 1, offset + length - 1).toString(),
      key = row.indexOf("=");
    result[row.slice(0, key)] = row.slice(key + 1);
    offset += length;
  }
  return result;
}
export async function* tarEntries(
  file: string,
  codec: "raw" | "gzip" | "xz" = "raw",
  root = ROOT,
): AsyncGenerator<TarEntry> {
  const descriptor = codec === "xz" ? openSync(file, "r") : undefined;
  const child =
    descriptor === undefined
      ? undefined
      : spawn(releaseTools(root), ["decode-capsule", "xz"], {
          stdio: [descriptor, "pipe", "inherit"],
        });
  if (descriptor !== undefined) closeSync(descriptor);
  const completed = child ? once(child, "close") : undefined;
  const source = child ? undefined : createReadStream(file);
  const input = child
    ? child.stdout!
    : codec === "gzip"
      ? source!.pipe(createGunzip())
      : source!;
  try {
    yield* readTar(input as AsyncIterable<Buffer>);
    if (completed) {
      const [code] = await completed;
      if (code !== 0) throw Error("Capsule decoder failed");
    }
  } finally {
    source?.destroy();
    if (child) {
      child.stdout?.destroy();
      if (child.exitCode === null) child.kill();
      await completed;
    }
  }
}
export async function* readTar(
  input: AsyncIterable<Buffer>,
): AsyncGenerator<TarEntry> {
  const bytes = new Bytes(input);
  let attributes: Record<string, string> = {},
    global: Record<string, string> = {};
  while (true) {
    const header = await bytes.read(512);
    if (header.every((value) => value === 0)) {
      break;
    }
    const expected = number(header, 148, 8),
      check = Buffer.from(header);
    check.fill(32, 148, 156);
    if (check.reduce((a, b) => a + b, 0) !== expected)
      throw Error("Invalid tar header checksum");
    const size = number(header, 124, 12),
      type = text(header, 156, 1) || "0",
      prefix = text(header, 345, 155);
    let name = (prefix ? prefix + "/" : "") + text(header, 0, 100),
      link = text(header, 157, 100);
    if (["x", "g", "L", "K"].includes(type)) {
      const data = await bytes.read(size);
      await bytes.read((512 - (size % 512)) % 512);
      if (type === "g") global = { ...global, ...pax(data) };
      else if (type === "x") attributes = { ...attributes, ...pax(data) };
      else
        attributes[type === "L" ? "path" : "linkpath"] = data
          .toString()
          .replace(/\0.*$/s, "");
      continue;
    }
    const metadata = { ...global, ...attributes };
    attributes = {};
    name = metadata.path || name;
    link = metadata.linkpath || link;
    const actualSize = metadata.size ? Number(metadata.size) : size;
    let consumed = 0;
    const body = (async function* () {
      for await (const chunk of bytes.chunks(actualSize)) {
        consumed += chunk.length;
        yield chunk;
      }
    })();
    yield {
      name,
      size: actualSize,
      mode: number(header, 100, 8),
      type,
      link,
      body,
    };
    if (consumed !== actualSize)
      for await (const _ of body) {
      }
    await bytes.read((512 - (actualSize % 512)) % 512);
  }
  await bytes.finish();
}
export function tarHeader(
  name: string,
  size: number,
  mode = 0o644,
  type = "0",
  link = "",
) {
  canonical(name);
  const header = Buffer.alloc(512);
  const split = name.lastIndexOf("/");
  if (Buffer.byteLength(name) > 100) {
    if (
      split < 0 ||
      Buffer.byteLength(name.slice(0, split)) > 155 ||
      Buffer.byteLength(name.slice(split + 1)) > 100
    )
      throw Error("Tar path too long");
    header.write(name.slice(0, split), 345, 155);
    header.write(name.slice(split + 1), 0, 100);
  } else header.write(name, 0, 100);
  for (const [offset, length, value] of [
    [100, 8, mode],
    [108, 8, 0],
    [116, 8, 0],
    [124, 12, size],
    [136, 12, 0],
  ])
    header.write(
      value!.toString(8).padStart(length! - 1, "0") + "\0",
      offset!,
      length!,
    );
  header.fill(32, 148, 156);
  header.write(type, 156, 1);
  header.write(link, 157, 100);
  header.write("ustar\0", 257, 6);
  header.write("00", 263, 2);
  header.write(
    header
      .reduce((a, b) => a + b, 0)
      .toString(8)
      .padStart(6, "0") + "\0 ",
    148,
    8,
  );
  return header;
}
export type ArchiveFile = {
  path: string;
  mode: number;
  data?: Buffer;
  file?: string;
  size?: number;
  body?: AsyncIterable<Buffer>;
  type?: string;
  link?: string;
};
export async function writeTarStream(
  target: Writable,
  files: Iterable<ArchiveFile> | AsyncIterable<ArchiveFile>,
) {
  const write = async (bytes: Buffer) => {
    if (!target.write(bytes)) await once(target, "drain");
  };
  for await (const entry of files) {
    const size = entry.data?.length ?? entry.size!;
    await write(
      tarHeader(entry.path, size, entry.mode, entry.type, entry.link),
    );
    if (entry.data) await write(entry.data);
    else if (size)
      for await (const chunk of entry.body || createReadStream(entry.file!))
        await write(Buffer.from(chunk));
    await write(Buffer.alloc((512 - (size % 512)) % 512));
  }
  await write(Buffer.alloc(1024));
  target.end();
}
export async function writeTar(
  destination: string,
  files: Iterable<ArchiveFile> | AsyncIterable<ArchiveFile>,
  {
    codec = "raw",
    architecture = "amd64",
    preset = 6,
    root = ROOT,
  }: {
    codec?: "raw" | "gzip" | "xz";
    architecture?: string;
    preset?: number;
    root?: string;
  } = {},
) {
  mkdirSync(dirname(destination), { recursive: true });
  const descriptor = codec === "xz" ? openSync(destination, "wx") : undefined;
  const child =
    descriptor === undefined
      ? undefined
      : spawn(
          releaseTools(root),
          ["encode-xz", architecture, String(preset), dirname(destination)],
          { stdio: ["pipe", descriptor, "inherit"] },
        );
  if (descriptor !== undefined) closeSync(descriptor);
  const completed = child ? once(child, "close") : undefined;
  const file = child
      ? undefined
      : createWriteStream(destination, { flags: "wx" }),
    compressed = codec === "gzip" ? createGzip({ level: 9 }) : undefined;
  if (compressed) compressed.pipe(file!);
  const target = child ? child.stdin! : compressed || file!;
  const closed = file ? once(file, "close") : undefined;
  try {
    await writeTarStream(target, files);
    if (completed) {
      const [code] = await completed;
      if (code !== 0) throw Error("Capsule encoder failed");
    } else await closed;
  } finally {
    if (!target.writableFinished) {
      target.destroy();
      child?.kill();
      if (completed) await completed;
    }
  }
}
export async function entryBytes(entry: TarEntry, maximum = 8 * 1024 * 1024) {
  if (entry.size > maximum) throw Error("Archive metadata exceeds budget");
  const chunks: Buffer[] = [];
  for await (const chunk of entry.body) chunks.push(chunk);
  return Buffer.concat(chunks, entry.size);
}
export async function extractTar(
  file: string,
  folder: string,
  codec: "raw" | "gzip" | "xz",
  root = ROOT,
  omittedLinks: Record<string, string> = {},
) {
  mkdirSync(folder, { recursive: true });
  for await (const entry of tarEntries(file, codec, root)) {
    const name = entry.name.replace(/^\.\//, "").replace(/\/$/, "");
    if (!name || name === ".") continue;
    canonical(name);
    const target = join(folder, name);
    if (entry.type === "5") {
      mkdirSync(target, { recursive: true });
      continue;
    }
    if (entry.type === "2" && omittedLinks[name] === entry.link) continue;
    if (entry.type !== "0") throw Error("Unexpected archive alias: " + name);
    mkdirSync(dirname(target), { recursive: true });
    const output = createWriteStream(target, {
      flags: "wx",
      mode: entry.mode & 0o111 ? 0o755 : 0o644,
    });
    for await (const bytes of entry.body)
      if (!output.write(bytes)) await once(output, "drain");
    output.end();
    await once(output, "close");
  }
}
