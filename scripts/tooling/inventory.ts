import {
  createReadStream,
  existsSync,
  lstatSync,
  readFileSync,
  readdirSync,
  statSync,
  mkdirSync,
  writeFileSync,
  symlinkSync,
  realpathSync,
} from "node:fs";
import {
  join,
  dirname,
  relative,
  resolve,
  isAbsolute,
  extname,
} from "node:path";
import { homedir } from "node:os";
import { createHash } from "node:crypto";
import { hash } from "./process";
import type { SourceFile } from "./source";

export type FileRecord = {
  path: string;
  size: number;
  sha256: string;
  mode: number;
};
export type LinkRecord = { path: string; target: string };
export const secret =
  /(?<![A-Za-z0-9_-])(?:sk-[A-Za-z0-9_-]{24,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|gh[pousr]_[A-Za-z0-9]{30,})/;
export function json<T = any>(file: string): T {
  return JSON.parse(readFileSync(file, "utf8"));
}
export function save(file: string, value: unknown) {
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, JSON.stringify(value, null, 2) + "\n");
}
export function canonical(name: string) {
  if (
    !name ||
    isAbsolute(name) ||
    /[\\:\x00-\x1f]/.test(name) ||
    name.split("/").some((p) => !p || p === "." || p === "..")
  )
    throw Error("Invalid payload path: " + name);
  return name;
}
export function records(files: SourceFile[]): FileRecord[] {
  return files.map((f) => ({
    path: f.path,
    size: f.data.length,
    sha256: hash(f.data),
    mode: f.mode,
  }));
}
export function checkRecord(row: FileRecord) {
  canonical(row.path);
  if (
    !Number.isSafeInteger(row.size) ||
    row.size < 0 ||
    !/^([a-f0-9]{64})$/.test(row.sha256) ||
    ![0o644, 0o755].includes(row.mode)
  )
    throw Error("Invalid file inventory");
  return row;
}
export async function verifyFiles(root: string, rows: FileRecord[]) {
  const names = new Set<string>();
  for (const row of rows) {
    checkRecord(row);
    if (names.has(row.path)) throw Error("Duplicate inventory path");
    names.add(row.path);
    const file = join(root, row.path),
      info = lstatSync(file);
    if (!info.isFile() || info.isSymbolicLink() || info.size !== row.size)
      throw Error("Payload inventory changed: " + row.path);
    const h = createHash("sha256");
    for await (const bytes of createReadStream(file)) h.update(bytes);
    if (h.digest("hex") !== row.sha256)
      throw Error("Payload checksum changed: " + row.path);
  }
  return rows;
}
export function links(
  root: string,
  rows: LinkRecord[],
  component: string,
  create = false,
) {
  const base = resolve(root, component);
  for (const row of rows) {
    canonical(row.path);
    if (
      !row.path.startsWith(component + "/") ||
      isAbsolute(row.target) ||
      /[\\:\x00-\x1f]/.test(row.target)
    )
      throw Error("Invalid runtime link");
    const file = join(root, row.path),
      target = resolve(dirname(file), row.target);
    if (!target.startsWith(base + "/") || !existsSync(target))
      throw Error("Runtime link escapes or is missing");
    if (existsSync(file)) {
      if (realpathSync(file) !== realpathSync(target))
        throw Error("Runtime link changed");
    } else if (create) {
      mkdirSync(dirname(file), { recursive: true });
      symlinkSync(
        row.target,
        file,
        statSync(target).isDirectory() ? "dir" : "file",
      );
    } else throw Error("Runtime link missing");
  }
}
export function collect(
  folder: string,
  prefix: string,
  { omit = (name: string): boolean => false } = {},
): { files: SourceFile[]; links: LinkRecord[] } {
  const files: SourceFile[] = [],
    links: LinkRecord[] = [];
  if (!existsSync(folder)) return { files, links };
  const base = realpathSync(folder);
  function walk(directory: string) {
    for (const entry of readdirSync(directory, { withFileTypes: true }).sort(
      (a, b) => a.name.localeCompare(b.name),
    )) {
      const file = join(directory, entry.name),
        name = relative(folder, file).split("\\").join("/");
      if (omit(name)) continue;
      canonical(name);
      if (entry.isSymbolicLink()) {
        const target = realpathSync(file);
        if (!target.startsWith(base + "/")) throw Error("Runtime link escapes");
        links.push({
          path: prefix + "/" + name,
          target: relative(dirname(file), target).split("\\").join("/"),
        });
      } else if (entry.isDirectory()) walk(file);
      else if (entry.isFile())
        files.push({
          path: prefix + "/" + name,
          data: readFileSync(file),
          mode: statSync(file).mode & 0o111 ? 0o755 : 0o644,
        });
      else throw Error("Unsupported component file");
    }
  }
  walk(folder);
  return { files, links };
}
export function publicBytes(bytes: Buffer, name: string) {
  if (
    secret.test(bytes.toString()) ||
    secret.test(bytes.toString().replaceAll("\0", "")) ||
    bytes.includes(Buffer.from(homedir() + "/"))
  )
    throw Error("Private material in artifact: " + name);
  return bytes;
}
