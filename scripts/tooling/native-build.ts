import { readFileSync, existsSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { homedir } from "node:os";
import { ROOT, run, output, hash, hostKey } from "./process";
import { sourceFiles, type SourceFile } from "./source";
import { json, save, records, verifyFiles, publicBytes } from "./inventory";
import { prepareDependency } from "./linux-dependencies";

export const roles = {
  assistant: "assistant",
  runner: "phone-runner",
  vision: "vision",
} as const;
export const contracts = {
  device_setup: "ai-phone-device-setup-v2",
  service_supervision: "ai-phone-linux-supervision-v2",
  phone_input_setup: "ai-phone-input-setup-v2",
  database_prepare: "ai-phone-database-prepare-v1",
  host_profile: "ai-phone-host-profile-v1",
  managed_engine: "ai-phone-managed-engine-v1",
  image_activation: "ai-phone-image-activation-v1",
  runtime_environment: "ai-phone-runtime-environment-v1",
  guest_app_resources: "ai-phone-guest-app-resources-v1",
};
export const fixture = "crates/phone-runner/fixtures/ocr-probe.png";
export function sourceFingerprint(root = ROOT) {
  return hash(
    JSON.stringify(
      sourceFiles(root)
        .filter(
          (f) =>
            f.path.startsWith("crates/") ||
            f.path.startsWith("packages/protocol/proto/") ||
            [
              "Cargo.toml",
              "Cargo.lock",
              "scripts/tooling/native-build.ts",
              "apps/client/src-tauri/tauri.conf.json",
              "config/ocr-models.json",
            ].includes(f.path),
        )
        .map((f) => [f.path, hash(f.data)]),
    ),
  );
}
export function versions(root = ROOT) {
  return Object.fromEntries(
    Object.entries(roles).map(([role, folder]) => [
      role,
      (
        Bun.TOML.parse(
          readFileSync(join(root, "crates", folder, "Cargo.toml"), "utf8"),
        ) as { package: { name: string; version: string } }
      ).package,
    ]),
  ) as Record<keyof typeof roles, { name: string; version: string }>;
}
export function binary(root: string, role: keyof typeof roles) {
  return join(root, "target/release", versions(root)[role].name);
}
export async function cachedBuild(root = ROOT) {
  const file = join(root, "target/release/native-service-build.json");
  if (!existsSync(file)) return null;
  const receipt = json(file);
  if (
    receipt.host !== hostKey() ||
    receipt.source_sha256 !== sourceFingerprint(root)
  )
    return null;
  await verifyFiles(
    root,
    Object.values(receipt.artifacts).map((row: any) => ({
      ...row,
      mode: 0o755,
    })),
  );
  return receipt;
}
export function versionCheck(program: string, name: string, version: string) {
  if (output([program, "--version"]) !== `${name} ${version}`)
    throw Error("Native version mismatch: " + name);
  return { program: name, version, state_created: false };
}
export function visionCheck(program: string, root = ROOT) {
  const png = readFileSync(join(root, fixture));
  const value = JSON.parse(
    String(
      run([program], {
        input: png,
        stdio: ["pipe", "pipe", "pipe"],
        encoding: "utf8",
        env: {
          ...process.env,
          AI_PHONE_TESSERACT: process.env.AI_PHONE_TEST_TESSERACT,
          TESSDATA_PREFIX: process.env.AI_PHONE_TEST_TESSDATA,
        },
      }).stdout,
    ),
  );
  if (
    value.width !== png.readUInt32BE(16) ||
    value.height !== png.readUInt32BE(20) ||
    value.grid.length !== 2048 ||
    !value.regions.some(
      (r: any) =>
        r.confidence >= 0.8 && r.text.replaceAll(/\s/g, "") === "7319",
    )
  )
    throw Error("OCR fixture recognition failed");
  return {
    fixture_text: "7319",
    width: value.width,
    height: value.height,
    grid: 2048,
  };
}
export async function buildNative(root = ROOT) {
  const cached = await cachedBuild(root);
  if (cached) return cached;
  const compiler = output(["rustc", "--version"]);
  if (compiler.split(" ")[1] !== "1.96.0")
    throw Error("Pinned Rust 1.96.0 required for runtime notices");
  const packages = versions(root),
    env = {
      ...process.env,
      CARGO_BUILD_JOBS: "2",
      CARGO_INCREMENTAL: "0",
      CARGO_TARGET_DIR: join(root, "target"),
      CARGO_PROFILE_RELEASE_DEBUG: "0",
      CARGO_PROFILE_RELEASE_STRIP: "debuginfo",
      CARGO_ENCODED_RUSTFLAGS:
        "--remap-path-prefix=" + homedir() + "=/build-user",
    };
  run(
    [
      "cargo",
      "build",
      "--release",
      "--locked",
      "--features",
      "ai-phone-vision/native",
      ...Object.values(packages).flatMap((p) => ["-p", p.name]),
    ],
    { cwd: root, env },
  );
  const artifacts: Record<string, unknown> = {},
    checks: Record<string, unknown> = {};
  for (const [role, pkg] of Object.entries(packages)) {
    const path = binary(root, role as keyof typeof roles);
    checks[role] = versionCheck(path, pkg.name, pkg.version);
    const data = publicBytes(readFileSync(path), role);
    artifacts[role] = {
      path: "target/release/" + pkg.name,
      size: data.length,
      sha256: hash(data),
    };
  }
  for (const [name, contract] of Object.entries(contracts)) {
    const flag = "--" + name.replaceAll("_", "-") + "-contract";
    if (output([binary(root, "assistant"), flag]) !== contract)
      throw Error("Missing native contract: " + name);
    checks[name] = contract;
  }
  checks.vision_version = checks.vision;
  checks.vision = visionCheck(binary(root, "vision"), root);
  const receipt = {
    format: 1,
    host: hostKey(),
    source_sha256: sourceFingerprint(root),
    compiler,
    artifacts,
    checks,
    developer_paths_remapped: true,
  };
  save(join(root, "target/release/native-service-build.json"), receipt);
  return receipt;
}
export async function prepareServices(root = ROOT, linuxOcr?: string) {
  const receipt = await cachedBuild(root);
  if (!receipt) throw Error("Build source-matched native services first");
  const files: SourceFile[] = [];
  for (const role of Object.keys(roles)) {
    const path = receipt.artifacts[role].path;
    files.push({
      path: "runtime/services/bin/" + role,
      data: publicBytes(readFileSync(join(root, path)), role),
      mode: 0o755,
    });
  }
  const bridge = process.env.AI_PHONE_RELEASE_DESKTOP_BRIDGE;
  if (bridge) {
    const data = publicBytes(readFileSync(bridge), "desktop-bridge");
    const machines: Record<string, number> = { "macos-arm64": 183, "linux-x64": 62 };
    const machine = machines[hostKey()];
    if (
      !machine ||
      data.length < 64 ||
      data.subarray(0, 6).toString("hex") !== "7f454c460201" ||
      data.readUInt16LE(18) !== machine
    )
      throw Error("Desktop bridge must be a matching Linux ELF");
    files.push({
      path: "runtime/services/bin/desktop-bridge",
      data,
      mode: 0o755,
    });
  }
  files.push({
    path: "runtime/services/fixtures/ocr.png",
    data: readFileSync(join(root, fixture)),
    mode: 0o644,
  });
  if (linuxOcr)
    files.push(...(await prepareDependency(root, linuxOcr, "linux-ocr")));
  files.push({
    path: "runtime/services/manifest.json",
    data: Buffer.from(
      JSON.stringify({
        format: 1,
        host: hostKey(),
        source_sha256: receipt.source_sha256,
        compiler: receipt.compiler,
        versions: Object.fromEntries(
          ["assistant", "runner"].map((role) => [
            role,
            receipt.checks[role].version,
          ]),
        ),
        vision: linuxOcr ? "linux-tesseract-bundled" : "container-tesseract",
        files: records(files),
        ...contracts,
      }),
    ),
    mode: 0o644,
  });
  return files;
}
