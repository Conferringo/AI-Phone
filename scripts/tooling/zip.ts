import {
  openSync,
  readSync,
  closeSync,
  statSync,
  createReadStream,
  createWriteStream,
  mkdirSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { createInflateRaw } from "node:zlib";
import { once } from "node:events";
import { canonical } from "./inventory";

export type ZipEntry = {
  name: string;
  size: number;
  compressedSize: number;
  method: number;
  offset: number;
  mode: number;
  directory: boolean;
};
export class Zip {
  readonly entries: ZipEntry[] = [];
  constructor(readonly file: string) {
    const fd = openSync(file, "r");
    try {
      const size = statSync(file).size,
        tail = Buffer.alloc(Math.min(size, 65557));
      readSync(fd, tail, 0, tail.length, size - tail.length);
      const end = tail.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
      if (end < 0) throw Error("ZIP directory missing");
      const count = tail.readUInt16LE(end + 10),
        length = tail.readUInt32LE(end + 12),
        position = tail.readUInt32LE(end + 16);
      if (length > 16 * 1024 * 1024 || position + length > size)
        throw Error("Invalid ZIP directory");
      const data = Buffer.alloc(length);
      readSync(fd, data, 0, length, position);
      let cursor = 0;
      for (let n = 0; n < count; n++) {
        if (data.readUInt32LE(cursor) !== 0x02014b50)
          throw Error("Invalid ZIP member");
        const flags = data.readUInt16LE(cursor + 8),
          method = data.readUInt16LE(cursor + 10),
          compressedSize = data.readUInt32LE(cursor + 20),
          size = data.readUInt32LE(cursor + 24),
          nameLength = data.readUInt16LE(cursor + 28),
          extraLength = data.readUInt16LE(cursor + 30),
          commentLength = data.readUInt16LE(cursor + 32),
          mode = data.readUInt32LE(cursor + 38) >>> 16,
          offset = data.readUInt32LE(cursor + 42),
          name = data
            .subarray(cursor + 46, cursor + 46 + nameLength)
            .toString();
        if (
          flags & 1 ||
          ![0, 8].includes(method) ||
          size === 0xffffffff ||
          compressedSize === 0xffffffff ||
          offset === 0xffffffff
        )
          throw Error("Unsupported SDK ZIP format");
        const directory = name.endsWith("/");
        canonical(name.replace(/\/$/, ""));
        if ((mode & 0o170000) === 0o120000)
          throw Error("SDK ZIP contains an alias");
        this.entries.push({
          name,
          size,
          compressedSize,
          method,
          offset,
          mode,
          directory,
        });
        cursor += 46 + nameLength + extraLength + commentLength;
      }
      if (
        cursor !== length ||
        new Set(this.entries.map((e) => e.name)).size !== count
      )
        throw Error("ZIP directory inventory mismatch");
    } finally {
      closeSync(fd);
    }
  }
  body(entry: ZipEntry): AsyncIterable<Buffer> {
    const fd = openSync(this.file, "r"),
      header = Buffer.alloc(30);
    try {
      readSync(fd, header, 0, 30, entry.offset);
    } finally {
      closeSync(fd);
    }
    if (header.readUInt32LE(0) !== 0x04034b50)
      throw Error("ZIP local header missing");
    const start =
      entry.offset + 30 + header.readUInt16LE(26) + header.readUInt16LE(28);
    if (entry.compressedSize === 0) return (async function* () {})();
    const source = createReadStream(this.file, {
      start,
      end: start + entry.compressedSize - 1,
    });
    return entry.method === 8 ? source.pipe(createInflateRaw()) : source;
  }
  async bytes(entry: ZipEntry, maximum = 32 * 1024 * 1024) {
    if (entry.size > maximum) throw Error("ZIP metadata exceeds budget");
    const chunks: Buffer[] = [];
    let size = 0;
    for await (const chunk of this.body(entry)) {
      size += chunk.length;
      chunks.push(chunk);
    }
    if (size !== entry.size) throw Error("ZIP member size mismatch");
    return Buffer.concat(chunks, size);
  }
  async extract(folder: string, prefix: string) {
    for (const entry of this.entries) {
      if (!entry.name.startsWith(prefix + "/") && entry.name !== prefix + "/")
        throw Error("Unexpected SDK ZIP component");
      const target = join(folder, entry.name);
      if (entry.directory) {
        mkdirSync(target, { recursive: true });
        continue;
      }
      mkdirSync(dirname(target), { recursive: true });
      const destination = createWriteStream(target, {
          flags: "wx",
          mode: entry.mode & 0o111 ? 0o755 : 0o644,
        }),
        done = once(destination, "close");
      let size = 0;
      for await (const chunk of this.body(entry)) {
        size += chunk.length;
        if (!destination.write(chunk)) await once(destination, "drain");
      }
      destination.end();
      await done;
      if (size !== entry.size) throw Error("ZIP member size mismatch");
    }
  }
}
