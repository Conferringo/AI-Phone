import {
  existsSync,
  readFileSync,
  readdirSync,
  lstatSync,
  mkdirSync,
  copyFileSync,
  cpSync,
  chmodSync,
  mkdtempSync,
  renameSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { homedir } from "node:os";
import { ROOT, hash, hostKey, output, run, digest } from "./process";
import { json, save, collect, records, verifyFiles } from "./inventory";
import { extractTar } from "./archive";
import { prepareDependency } from "./linux-dependencies";

export function minimumMac(root = ROOT) {
  return json(join(root, "apps/client/src-tauri/tauri.conf.json")).bundle.macOS
    .minimumSystemVersion as string;
}
export function versionParts(value: string) {
  return [...value.split(".").map(Number), 0, 0].slice(0, 3);
}
export function checkMacTargets(
  root: string,
  programs: Record<string, string>,
) {
  const maximum = versionParts(minimumMac(root));
  return Object.fromEntries(
    Object.entries(programs).map(([role, file]) => {
      const matches = [
        ...output(["xcrun", "vtool", "-show-build", file]).matchAll(
          /^\s*minos (\d+\.\d+(?:\.\d+)?)\s*$/gm,
        ),
      ];
      if (matches.length !== 1) throw Error("Native minimum macOS missing");
      const version = matches[0]![1]!,
        parts = versionParts(version);
      for (let n = 0; n < 3; n++) {
        if (parts[n]! > maximum[n]!)
          throw Error("Native helper requires newer macOS");
        if (parts[n]! < maximum[n]!) break;
      }
      return [role, version];
    }),
  );
}
export function containerPolicy(root = ROOT) {
  const value = json(join(root, "config/container-host-runtime.json")),
    host = hostKey(),
    runtime = value.hosts[host],
    bootstrap = json(join(root, "config/host-bootstrap.json"));
  if (value.format !== 1 || !runtime) throw Error("Unsupported container host");
  return {
    host,
    runtime,
    lima: bootstrap.lima_hosts[host],
    ...(host === "macos-arm64" ? { minimum_macos: minimumMac(root) } : {}),
  };
}
function sorted(value: any): any {
  return Array.isArray(value)
    ? value.map(sorted)
    : value && typeof value === "object"
      ? Object.fromEntries(
          Object.keys(value)
            .sort()
            .map((key) => [key, sorted(value[key])]),
        )
      : value;
}
export function policyDigest(value: unknown) {
  return hash(JSON.stringify(sorted(value)));
}
export async function pinnedFile(
  file: string,
  pin: { sha256: string; size_bytes?: number },
) {
  if (
    lstatSync(file).isSymbolicLink() ||
    (pin.size_bytes !== undefined && lstatSync(file).size !== pin.size_bytes) ||
    (await digest(file)) !== pin.sha256
  )
    throw Error("Container host pinned file changed");
}
export async function packageInputs(folder: string, pins: any[]) {
  if (readdirSync(folder).length !== pins.length)
    throw Error("Incomplete offline engine package closure");
  for (const pin of pins) await pinnedFile(join(folder, pin.file), pin);
}
export async function verifyContainerHost(root: string, folder: string) {
  const value = json(join(folder, "manifest.json"));
  if (
    value.format !== 1 ||
    value.host !== hostKey() ||
    value.policy_sha256 !== policyDigest(containerPolicy(root))
  )
    throw Error("Container host policy changed");
  await verifyFiles(folder, value.files);
  return value;
}
export async function buildContainerHost(
  root: string,
  lima: string,
  image: string,
  packages: string,
) {
  const value = containerPolicy(root),
    output = join(root, ".tools/container-host");
  if (existsSync(output)) return verifyContainerHost(root, output);
  await pinnedFile(lima, value.lima);
  await pinnedFile(image, value.runtime.image);
  await packageInputs(packages, value.runtime.packages);
  mkdirSync(dirname(output), { recursive: true });
  const temporary = mkdtempSync(join(dirname(output), ".container-host-")),
    work = join(temporary, "component");
  mkdirSync(work);
  try {
    await extractTar(lima, join(work, "lima"), "gzip", root, {
      "share/doc/lima/templates": "../../lima/templates",
    });
    if (value.runtime.lima_build) {
      const native = join(root, ".tools/lima-native");
      await verifyLima(root, native);
      copyFileSync(join(native, "limactl"), join(work, "lima/bin/limactl"));
    }
    let targets: Record<string, string>;
    if (value.host === "linux-x64") {
      for (const file of await prepareDependency(
        root,
        join(root, ".tools/linux-host-runtime"),
        "linux-host",
      )) {
        const name = file.path.slice("runtime/container-host/native/".length),
          target = join(work, "native", name);
        mkdirSync(dirname(target), { recursive: true });
        writeFileSync(target, file.data, { mode: file.mode });
      }
      mkdirSync(join(work, "bin"));
      copyFileSync(
        join(root, "scripts/linux-host/lima.sh"),
        join(work, "bin/limactl"),
      );
      chmodSync(join(work, "bin/limactl"), 0o755);
      targets = { lima: "linux-x64" };
    } else
      targets = checkMacTargets(root, { lima: join(work, "lima/bin/limactl") });
    copyFileSync(image, join(work, "image.iso"));
    cpSync(packages, join(work, "packages"), { recursive: true });
    const files = collect(work, "").files.map((f) => ({
        ...f,
        path: f.path.slice(1),
      })),
      manifest = {
        format: 1,
        host: value.host,
        policy_sha256: policyDigest(value),
        native_targets: targets,
        files: records(files),
      };
    save(join(work, "manifest.json"), manifest);
    renameSync(work, output);
    return manifest;
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}
export async function prepareContainerHost(root = ROOT) {
  const folder = join(root, ".tools/container-host");
  await verifyContainerHost(root, folder);
  return collect(folder, "runtime/container-host").files;
}
function limaPolicy(root: string) {
  if (hostKey() !== "macos-arm64")
    throw Error("Native Lima requires Apple Silicon");
  const value = containerPolicy(root);
  return {
    ...value.runtime.lima_build,
    version: value.runtime.lima,
    minimum: minimumMac(root),
  };
}
function limaFingerprint(root: string) {
  return policyDigest({
    policy: limaPolicy(root),
    producer: hash(
      readFileSync(join(root, "scripts/tooling/container-host.ts")),
    ),
  });
}
export async function verifyLima(
  root: string,
  folder = join(root, ".tools/lima-native"),
) {
  const value = json(join(folder, "build.json")),
    policy = limaPolicy(root);
  if (
    value.format !== 1 ||
    value.fingerprint !== limaFingerprint(root) ||
    value.source_commit !== policy.source_commit ||
    value.go_version !== policy.go_version
  )
    throw Error("Native Lima source mismatch");
  await verifyFiles(folder, [value.file]);
  return value;
}
export async function buildLima(root: string, source: string, go: string) {
  const policy = limaPolicy(root),
    destination = join(root, ".tools/lima-native");
  if (existsSync(destination)) return verifyLima(root, destination);
  if (
    output(["git", "rev-parse", "HEAD"], { cwd: source }) !==
      policy.source_commit ||
    output(["git", "status", "--porcelain"], { cwd: source })
  )
    throw Error("Lima source differs from pin");
  if (
    output([go, "version"]) !==
    "go version " + policy.go_version + " darwin/arm64"
  )
    throw Error("Pinned Go compiler required");
  mkdirSync(dirname(destination), { recursive: true });
  const temporary = mkdtempSync(join(dirname(destination), ".lima-native-")),
    binary = join(temporary, "limactl"),
    cache = join(root, ".tools/container-host-factory");
  const environment = {
    ...process.env,
    GOOS: "darwin",
    GOARCH: "arm64",
    CGO_ENABLED: "1",
    GOMAXPROCS: "2",
    GOTOOLCHAIN: "local",
    GOWORK: "off",
    GOENV: "off",
    GOCACHE: join(cache, "go-cache"),
    GOMODCACHE: join(cache, "go-modules"),
    GOTMPDIR: join(cache, "go-tmp"),
    GOSUMDB: "sum.golang.org",
    GOPROXY: "https://proxy.golang.org",
    GOPRIVATE: "",
    GONOSUMDB: "",
    GONOPROXY: "",
    GOINSECURE: "",
    GOFLAGS: "",
    MACOSX_DEPLOYMENT_TARGET: policy.minimum,
    CGO_CFLAGS: "-O2 -g0 -mmacosx-version-min=" + policy.minimum,
    CGO_LDFLAGS: "-mmacosx-version-min=" + policy.minimum,
  };
  for (const name of ["GOCACHE", "GOMODCACHE", "GOTMPDIR"] as const)
    mkdirSync(environment[name], { recursive: true });
  try {
    run(
      [
        go,
        "build",
        "-mod=readonly",
        "-trimpath",
        "-p",
        "2",
        "-ldflags",
        "-s -w -X github.com/lima-vm/lima/v2/pkg/version.Version=" +
          policy.version +
          " -linkmode=external -extldflags=-mmacosx-version-min=" +
          policy.minimum,
        "-o",
        binary,
        "./cmd/limactl",
      ],
      { cwd: source, env: environment },
    );
    run([
      "codesign",
      "-f",
      "--entitlements",
      join(source, "vz.entitlements"),
      "-s",
      "-",
      binary,
    ]);
    run(["codesign", "--verify", "--strict", binary]);
    const target = checkMacTargets(root, { lima: binary }).lima;
    if (output([binary, "--version"]) !== "limactl version " + policy.version)
      throw Error("Lima version mismatch");
    const value = {
      format: 1,
      fingerprint: limaFingerprint(root),
      source_commit: policy.source_commit,
      go_version: policy.go_version,
      minimum_macos: target,
      file: {
        path: "limactl",
        size: lstatSync(binary).size,
        sha256: await digest(binary),
        mode: 0o755,
      },
    };
    save(join(temporary, "build.json"), value);
    renameSync(temporary, destination);
    return value;
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}
