import {
  readFileSync,
  writeFileSync,
  mkdirSync,
  readdirSync,
  lstatSync,
  realpathSync,
  mkdtempSync,
  rmSync,
  linkSync,
  symlinkSync,
  renameSync,
  existsSync,
} from "node:fs";
import { join, relative, resolve, basename, dirname, sep } from "node:path";
import { homedir, tmpdir } from "node:os";
import { ROOT, run, output, digest, hash, bunProgram } from "./process";
import { json, save } from "./inventory";
import { buildWindowsInstaller, unpackWindowsInstaller } from "./windows-installer";
import {
  signMacArtifact,
  signWindowsInstaller,
  temporaryMacSignature,
  verifyMacArtifact,
  type InstallerSignature,
} from "./installer-signing";
import { resourceMap, fileReceipt, verifyCompiledInventory } from "./resources";
import {
  payloadComponents,
  deliveryMetadata,
  installerMetadata,
} from "./client-payload";

export type Platform = "macos" | "windows";
type Receipt = { path: string; size: number; sha256: string };
function filesIn(base: string) {
  const result: Record<string, string> = {};
  const walk = (folder: string) => {
    for (const item of readdirSync(folder, { withFileTypes: true })) {
      const file = join(folder, item.name);
      if (item.isDirectory()) walk(file);
      else if (item.isSymbolicLink()) {
        const target = realpathSync(file);
        if (!target.startsWith(resolve(base) + sep))
          throw Error("Installer link leaves payload");
        if (lstatSync(target).isFile())
          result[relative(base, file).split(sep).join("/")] = file;
      } else if (item.isFile())
        result[relative(base, file).split(sep).join("/")] = file;
    }
  };
  walk(base);
  return result;
}
export function noticeManifest(root = ROOT) {
  if (
    json(join(root, "docs/public/dependency-licenses.json"))
      .native_client_missing_notice_files?.length !== 0
  )
    throw Error("Native dependency notices are incomplete");
  return {
    rust_missing_files: [],
    texts: {
      "native-client-licenses.txt": hash(
        readFileSync(join(root, "docs/public/native-client-licenses.txt")),
      ),
    },
  };
}
async function inspectPayload(files: Record<string, string>, root: string) {
  const native = join(root, "apps/client/src-tauri"),
    resources = resourceMap(root),
    vmManifest = "gen/host/qemu/manifest.json",
    vmFiles = new Map<string, Receipt>(
      vmManifest in resources
        ? json<{ files: Receipt[] }>(join(native, vmManifest)).files.map(
            (row) => ["host/qemu/" + row.path, row],
          )
        : [],
    );
  const expected = new Map<string, Awaited<ReturnType<typeof fileReceipt>>>();
  for (const [source, destination] of Object.entries(resources))
    expected.set(
      destination,
      vmFiles.get(destination) ?? await fileReceipt(join(native, source), root),
    );
  const seen = new Set<string>(),
    result: Receipt[] = [];
  for (const [name, file] of Object.entries(files).sort(([a], [b]) =>
    a.localeCompare(b),
  )) {
    const destination = [...expected.keys()].find(
      (s) => name === s || name.endsWith("/" + s),
    );
    if (
      !destination &&
      /(?:^|\/)host\/(?:(?:wsl|qemu)\/|windows\/wsl\.msi$)/i.test(name)
    )
      throw Error("Unexpected Windows backend installer resource");
    const receipt = await fileReceipt(file, root);
    if (destination) {
      const required = expected.get(destination)!;
      if (receipt.sha256 !== required.sha256 || receipt.size !== required.size)
        throw Error("Stale installer resource: " + destination);
      seen.add(destination);
    }
    result.push({ path: name, ...receipt });
  }
  if (seen.size !== expected.size)
    throw Error("Installer resources are incomplete");
  return result;
}
export async function inspectPackage(file: string, root = ROOT) {
  const folder = mkdtempSync(join(tmpdir(), "aiphone-package-"));
  try {
    await unpackWindowsInstaller(root, file, folder);
    const files = filesIn(folder),
      program = Object.entries(files).find(([name]) =>
        name.endsWith("ai-phone-client.exe"),
      );
    if (!program) throw Error("Windows client executable is missing");
    const header = readFileSync(program[1]),
      pe = header.readUInt32LE(60);
    if (
      header.subarray(0, 2).toString() !== "MZ" ||
      header.subarray(pe, pe + 4).toString() !== "PE\0\0" ||
      header.readUInt16LE(pe + 4) !== 0x8664
    )
      throw Error("Unexpected Windows client architecture");
    const integrity = json(join(root, "apps/client/src-tauri/gen/host/integrity.json"));
    if ("qemu/manifest.json" in integrity)
      await verifyCompiledInventory(program[1], integrity);
    return { files: await inspectPayload(files, root) };
  } finally {
    rmSync(folder, { recursive: true, force: true });
  }
}
export async function inspectMacBundle(bundle: string, root = ROOT) {
  const native = join(root, "apps/client/src-tauri"),
    config = json(join(native, "tauri.conf.json"));
  const info = JSON.parse(
    output([
      "plutil",
      "-convert",
      "json",
      "-o",
      "-",
      join(bundle, "Contents/Info.plist"),
    ]),
  );
  const version = json(join(root, "config/release-policy.json")).version;
  if (
    config.version !== version ||
    info.CFBundleIdentifier !== config.identifier ||
    info.CFBundleShortVersionString !== version ||
    info.LSMinimumSystemVersion !== config.bundle.macOS.minimumSystemVersion
  )
    throw Error("Unexpected application metadata");
  const executable = info.CFBundleExecutable;
  if (typeof executable !== "string" || basename(executable) !== executable)
    throw Error("Invalid application executable");
  const integrity = join(native, "gen/host/integrity.json");
  if (existsSync(integrity))
    await verifyCompiledInventory(
      join(bundle, "Contents/MacOS", executable),
      json(integrity),
    );
  const allowed = new Set([
    "Contents/Info.plist",
    "Contents/MacOS/" + executable,
    "Contents/Resources/icon.icns",
    ...Object.values(resourceMap(root)).map((s) => "Contents/Resources/" + s),
  ]);
  const files = filesIn(bundle);
  const signature = "Contents/_CodeSignature/CodeResources";
  if (Object.hasOwn(files, signature)) allowed.add(signature);
  if (
    Object.keys(files).length !== allowed.size ||
    Object.keys(files).some((name) => !allowed.has(name))
  )
    throw Error("Unexpected application files");
  const inspected = await inspectPayload(files, root);
  return {
    bundle_id: config.identifier,
    version,
    minimum_macos: info.LSMinimumSystemVersion,
    files: inspected,
  };
}
export function nativeEnvironment(): NodeJS.ProcessEnv {
  const home = homedir();
  const nativePrefixMap = `-ffile-prefix-map=${home}=/build-user`;
  return {
    ...process.env,
    CI: "true",
    CARGO_BUILD_JOBS: process.env.CARGO_BUILD_JOBS || "2",
    CARGO_INCREMENTAL: "0",
    CARGO_HOME: process.env.CARGO_HOME || join(home, ".cargo"),
    ...(process.platform === "darwin" ? {
      CFLAGS: [process.env.CFLAGS, nativePrefixMap].filter(Boolean).join(" "),
      CXXFLAGS: [process.env.CXXFLAGS, nativePrefixMap].filter(Boolean).join(" "),
    } : {}),
    CARGO_ENCODED_RUSTFLAGS: [
      process.env.CARGO_ENCODED_RUSTFLAGS,
      `--remap-path-prefix=${home}=/build-user`,
    ]
      .filter(Boolean)
      .join("\x1f"),
  };
}
export function windowsCargo(operation: "build" | "test", root = ROOT) {
  const env = nativeEnvironment();
  env.CARGO_ENCODED_RUSTFLAGS = [
    env.CARGO_ENCODED_RUSTFLAGS,
    "-Ctarget-feature=+crt-static",
  ].join("\x1f");
  run(
    [
      "cargo",
      operation,
      "--manifest-path",
      join(root, "apps/client/src-tauri/Cargo.toml"),
      "--release",
      "--target",
      "x86_64-pc-windows-msvc",
      "--features",
      "tauri/custom-protocol",
      ...(operation === "build" ? ["--bins"] : []),
      "--locked",
    ],
    { cwd: root, env },
  );
}
function buildDmg(
  application: string,
  destination: string,
  { developmentDmg, temporarySigning }: { developmentDmg: boolean; temporarySigning: boolean },
) {
  const temporary = mkdtempSync(join(dirname(destination), ".dmg-"));
  try {
    const contents = join(temporary, "contents");
    mkdirSync(contents);
    const copy = (from: string, to: string) => {
      mkdirSync(to);
      for (const entry of readdirSync(from, { withFileTypes: true })) {
        if (entry.isDirectory())
          copy(join(from, entry.name), join(to, entry.name));
        else linkSync(join(from, entry.name), join(to, entry.name));
      }
    };
    copy(application, join(contents, "AI Phone.app"));
    symlinkSync("/Applications", join(contents, "Applications"));
    const image = join(temporary, "client.dmg");
    run([
      "hdiutil",
      "create",
      "-srcfolder",
      contents,
      "-volname",
      "AI Phone",
      "-format",
      ...(developmentDmg ? ["UDZO", "-imagekey", "zlib-level=1"] : ["ULMO"]),
      image,
    ]);
    if (temporarySigning) {
      signMacArtifact(image);
      verifyMacArtifact(image);
    }
    run(["hdiutil", "verify", image]);
    if (temporarySigning) {
      const mount = join(temporary, "readback");
      mkdirSync(mount);
      run([
        "hdiutil", "attach", "-readonly", "-nobrowse", "-noverify",
        "-owners", "off", "-mountpoint", mount, image,
      ]);
      try {
        verifyMacArtifact(join(mount, "AI Phone.app"));
      } finally {
        run(["hdiutil", "detach", mount]);
      }
    }
    renameSync(image, destination);
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}
export async function recordInstaller(
  file: string,
  platform: Platform,
  arch: string,
  inspection: unknown,
  root = ROOT,
  signature: InstallerSignature = { signing: "unsigned" },
) {
  const sha256 = await digest(file),
    host = platform + "-" + arch;
  const delivery = deliveryMetadata(await payloadComponents(root), host);
  const manifest = {
    version: json(join(root, "config/release-policy.json")).version,
    kind: "native-client",
    os: platform,
    arch,
    sha256,
    ...signature,
    runtime_acceptance: "pending",
    dependencies_license_review_complete: false,
    remote_upload: false,
    third_party_notices: noticeManifest(root),
    source_commit: output(["git", "rev-parse", "HEAD"], { cwd: root }),
    builder_commit: output(["git", "rev-parse", "HEAD"], { cwd: ROOT }),
    source_locks: Object.fromEntries(
      await Promise.all(
        ["bun.lock", "apps/client/src-tauri/Cargo.lock"].map(async (name) => [
          name,
          await digest(join(root, name)),
        ]),
      ),
    ),
    source_payload_included: true,
    local_bootstrap: {
      implemented_on: [
        ...Object.keys(
          json(join(root, "config/host-bootstrap.json")).hosts,
        ).filter((host) => host === "macos-arm64"),
        ...(platform === "windows" && delivery.complete_device_bundle
          ? ["windows-x64"]
          : []),
      ].sort(),
      fresh_install_acceptance: "pending",
      user_state_bundled: false,
    },
    payload: inspection,
    ...delivery,
    installer_delivery: installerMetadata(file, host),
    ...(platform === "macos"
      ? { developer_id_signed: false, notarized: false }
      : {}),
  };
  writeFileSync(file + ".sha256", sha256 + "  " + basename(file) + "\n");
  save(file + ".manifest.json", manifest);
  return manifest;
}
export async function buildClient(
  platform: Platform,
  root = ROOT,
  { packageOnly = false, developmentDmg = false, temporarySigning = false } = {},
) {
  const version = json(join(root, "config/release-policy.json")).version,
    native = join(root, "apps/client/src-tauri"),
    dist = join(root, "dist"),
    env = nativeEnvironment();
  mkdirSync(dist, { recursive: true });
  const command = [
    bunProgram(root),
    "run",
    "--cwd",
    join(root, "apps/client"),
    "build",
    "--config",
    "src-tauri/gen/host/resources.json",
  ];
  let source: string, destination: string, arch: string;
  if (platform === "macos") {
    if (process.platform !== "darwin" || process.arch !== "arm64")
      throw Error("macOS packaging requires Apple Silicon");
    run(
      [...command, "--bundles", "app", "--ci", "--no-sign", "--", "--locked"],
      { cwd: root, env },
    );
    source = join(native, "target/release/bundle/macos/AI Phone.app");
    if (temporarySigning) signMacArtifact(source);
    const inspection = await inspectMacBundle(source, root);
    destination = join(dist, `AImanity-${version}-macos-arm64.dmg`);
    buildDmg(source, destination, { developmentDmg, temporarySigning });
    return {
      artifact: destination,
      ...(await recordInstaller(
        destination,
        platform,
        "arm64",
        inspection,
        root,
        temporarySigning ? temporaryMacSignature : undefined,
      )),
    };
  }
  if (!packageOnly && process.platform !== "win32")
    throw Error("Build Windows clients on Windows");
  if (!packageOnly) windowsCargo("build", root);
  arch = "x64";
  source = join(
    native,
    "target/x86_64-pc-windows-msvc/release/ai-phone-client.exe",
  );
  destination = join(dist, `AImanity-${version}-windows-x64-setup.exe`);
  await buildWindowsInstaller(root, source, destination);
  const signature = temporarySigning ? signWindowsInstaller(destination) : undefined;
  return {
    artifact: destination,
    ...(await recordInstaller(
      destination,
      platform,
      arch,
      await inspectPackage(destination, root),
      root,
      signature,
    )),
  };
}
