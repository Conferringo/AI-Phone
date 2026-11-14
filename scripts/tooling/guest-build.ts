import {
  existsSync,
  mkdirSync,
  copyFileSync,
  chmodSync,
  openSync,
  readSync,
  closeSync,
  lstatSync,
} from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { run, output, hash, digest } from "./process";
import { sourceFiles } from "./source";
import { json, save } from "./inventory";
export const guestTarget = "aarch64-unknown-linux-musl",
  guestBinary = "build/target/release/desktop-bridge";
function sourceIdentity(root: string) {
  return hash(
    JSON.stringify(
      sourceFiles(root)
        .filter(
          (f) =>
            [
              "Cargo.toml",
              "Cargo.lock",
              "scripts/tooling/guest-build.ts",
            ].includes(f.path) ||
            [
              "crates/guest-runtime/",
              "crates/api/",
              "packages/protocol/proto/",
            ].some((prefix) => f.path.startsWith(prefix)),
        )
        .map((f) => [f.path, hash(f.data)]),
    ),
  );
}
function staticArmElf(file: string) {
  const fd = openSync(file, "r");
  try {
    const header = Buffer.alloc(64);
    if (
      readSync(fd, header) !== 64 ||
      header.subarray(0, 6).toString("hex") !== "7f454c460201" ||
      header.readUInt16LE(18) !== 183
    )
      throw Error("Guest bridge is not Linux ARM64");
    const offset = Number(header.readBigUInt64LE(32)),
      width = header.readUInt16LE(54),
      count = header.readUInt16LE(56);
    if (
      width !== 56 ||
      count < 1 ||
      count > 128 ||
      offset < 64 ||
      offset + width * count > lstatSync(file).size
    )
      throw Error("Guest ELF program headers invalid");
    const table = Buffer.alloc(width * count);
    if (readSync(fd, table, 0, table.length, offset) !== table.length)
      throw Error("Guest ELF table incomplete");
    for (let i = 0; i < count; i++)
      if (table.readUInt32LE(i * width) === 3)
        throw Error("Guest bridge requires an external interpreter");
  } finally {
    closeSync(fd);
  }
}
export async function verifyGuest(
  root: string,
  folder: string,
  architecture: string,
) {
  const manifest = json(join(folder, "manifest.json")),
    file = join(folder, guestBinary);
  if (
    architecture !== "arm64" ||
    lstatSync(folder).isSymbolicLink() ||
    manifest.target !== guestTarget ||
    manifest.source_sha256 !== sourceIdentity(root) ||
    lstatSync(file).isSymbolicLink() ||
    (await digest(file)) !== manifest.sha256
  )
    throw Error("Guest runtime source or bytes differ");
  staticArmElf(file);
  return manifest;
}
export async function buildGuest(root: string, work: string) {
  if (existsSync(work)) throw Error("Use a fresh guest compiler directory");
  if (output(["rustc", "--version"]).split(" ")[1] !== "1.96.0")
    throw Error("Guest factory requires Rust 1.96.0");
  const sysroot = output(["rustc", "--print", "sysroot"]),
    linker = join(sysroot, "lib/rustlib/aarch64-apple-darwin/bin/rust-lld");
  if (
    !existsSync(linker) ||
    !existsSync(join(sysroot, "lib/rustlib", guestTarget, "lib"))
  )
    throw Error("Linux ARM musl target and rust-lld are required");
  mkdirSync(work, { recursive: true });
  const target = join(work, "target");
  run(
    [
      "cargo",
      "build",
      "--release",
      "--locked",
      "--package",
      "ai-phone-guest-runtime",
      "--bin",
      "desktop-bridge",
      "--target",
      guestTarget,
    ],
    {
      cwd: root,
      env: {
        ...process.env,
        CARGO_TARGET_DIR: target,
        CARGO_BUILD_JOBS: "2",
        CARGO_INCREMENTAL: "0",
        CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER: linker,
        CARGO_ENCODED_RUSTFLAGS: [
          "-Clinker-flavor=ld.lld",
          "--remap-path-prefix=" + homedir() + "=/build-user",
        ].join("\x1f"),
      },
    },
  );
  const binary = join(target, guestTarget, "release/desktop-bridge");
  staticArmElf(binary);
  const context = join(work, "context"),
    destination = join(context, guestBinary);
  mkdirSync(join(context, "build/target/release"), { recursive: true });
  copyFileSync(binary, destination);
  chmodSync(destination, 0o755);
  save(join(context, "manifest.json"), {
    format: 1,
    target: guestTarget,
    source_sha256: sourceIdentity(root),
    sha256: await digest(destination),
  });
  await verifyGuest(root, context, "arm64");
  return context;
}
