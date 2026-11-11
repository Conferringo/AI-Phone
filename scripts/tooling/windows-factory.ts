import {
  mkdirSync,
  copyFileSync,
  readdirSync,
  lstatSync,
  rmSync,
  unlinkSync,
} from "node:fs";
import { join, resolve, basename } from "node:path";
import { ROOT, digest, hostKey, bunProgram, run, output } from "./process";
import { json, save } from "./inventory";
import { checkSource } from "./source";
import { verifyWsl } from "./wsl-capsule";
import { buildAndroid, prepareAndroid } from "./android-component";
import { deliveryMetadata, installerMetadata } from "./client-payload";
import { buildReleaseTools } from "./archive";
import {
  windowsBackendComponents as components,
  windowsSourceIdentity as sourceIdentity,
  type WindowsVmInputs,
} from "./windows-vm-payload";

const body = "wsl-backend.payload",
  metadata = "windows-backend.json";
function backendComponents(value: any) {
  if (
    JSON.stringify([...value.provenance.source_components].sort()) !==
    JSON.stringify(components)
  )
    throw Error("Windows backend requires both offline phone app groups");
}
async function backendRecord(file: string) {
  const info = lstatSync(file);
  if (!info.isFile() || !info.size || info.size >= 2 * 1024 ** 3)
    throw Error("Invalid backend handoff");
  return {
    path: "windows-backend/" + body,
    name: body,
    size: info.size,
    sha256: await digest(file),
  };
}
export async function stageWindowsBackend(
  root: string,
  report: any,
  payload: string,
) {
  const source = await sourceIdentity(root);
  if (
    report.source_commit !== source.source_commit ||
    report.stage !== "assembled"
  )
    throw Error("Backend assembly source differs");
  const component = await verifyWsl(payload, root);
  backendComponents(component);
  if (
    component.rootfs.sha256 !== report.component.rootfs.sha256 ||
    component.provenance.source_sha256 !==
      report.component.provenance.source_sha256
  )
    throw Error("Assembled backend changed");
  const folder = join(root, "dist/windows-backend");
  mkdirSync(folder);
  const file = join(folder, body);
  copyFileSync(payload, file);
  const path = join(folder, metadata);
  save(path, {
    format: 1,
    ...source,
    binary_publication: false,
    user_state_bundled: false,
    backend_components: components,
    runtime_acceptance: report.runtime_acceptance,
    artifact: await backendRecord(file),
  });
  return { metadata: path, sha256: await digest(path) };
}
async function admitBackend(root: string, folder: string, expected: string) {
  const source = await sourceIdentity(root),
    file = join(folder, metadata);
  if (
    lstatSync(folder).isSymbolicLink() ||
    !lstatSync(file).isFile() ||
    lstatSync(file).size > 16384 ||
    (await digest(file)) !== expected ||
    JSON.stringify(readdirSync(folder).sort()) !==
      JSON.stringify([metadata, body].sort())
  )
    throw Error("Backend handoff inventory changed");
  const value = json(file);
  if (
    value.format !== 1 ||
    value.source_commit !== source.source_commit ||
    value.source_manifest_sha256 !== source.source_manifest_sha256 ||
    value.binary_publication !== false ||
    value.user_state_bundled !== false ||
    JSON.stringify(value.backend_components) !== JSON.stringify(components)
  )
    throw Error("Backend source differs");
  const payload = join(folder, body),
    record = await backendRecord(payload);
  if (JSON.stringify(record) !== JSON.stringify(value.artifact))
    throw Error("Backend handoff bytes changed");
  const component = await verifyWsl(payload, root);
  backendComponents(component);
  return { payload, component, source, runtime_acceptance: value.runtime_acceptance };
}
export async function windowsFactory(
  root: string,
  input:
    | { runtime: "wsl"; folder: string; identity: string; backendSource: string }
    | { runtime: "qemu"; vm: WindowsVmInputs },
) {
  if (hostKey() !== "windows-x64")
    throw Error("Complete Windows assembly requires Windows x64");
  checkSource(root, { repository: true, manifest: true });
  const source = await sourceIdentity(root),
    backend = input.runtime === "wsl"
      ? await admitBackend(input.backendSource, resolve(input.folder), input.identity)
      : undefined,
    runtimeArgs = input.runtime === "qemu"
      ? [
          "--qemu", input.vm.qemu,
          "--qemu-identity", input.vm.qemuIdentity,
          "--guest", input.vm.guest,
          "--guest-identity", input.vm.guestIdentity,
        ]
      : ["--wsl-runtime", backend!.payload],
    work = join(root, ".tools/complete-windows");
  mkdirSync(work);
  buildReleaseTools(root);
  run(
    [
      "cargo",
      "build",
      "--release",
      "--locked",
      "-p",
      "ai-phone-host-runtime",
      "--bin",
      "host-resources",
    ],
    { cwd: root },
  );
  const android = await buildAndroid(root, join(work, "android"), 6),
    prepared = join(work, "android-qualification");
  await prepareAndroid(android.file, prepared, root);
  rmSync(prepared, { recursive: true });
  for (const role of ["emulator", "platform-tools", "system-image"])
    unlinkSync(join(work, "android", role + ".zip"));
  run(
    [
      bunProgram(ROOT),
      join(ROOT, "scripts/build-client.ts"),
      "--root",
      root,
      "--platform",
      "windows",
      "--sign-temporary",
      "--android-runtime",
      android.file,
      ...runtimeArgs,
    ],
    { cwd: root },
  );
  const version = json(join(root, "config/release-policy.json")).version,
    file = join(root, "dist", `AImanity-${version}-windows-x64-setup.exe`),
    manifest = json(file + ".manifest.json"),
    delivery = deliveryMetadata(manifest.prebuilt_components, "windows-x64");
  if (
    !delivery.complete_device_bundle ||
    manifest.signing !== "authenticode-self-signed" ||
    manifest.complete_device_bundle !== true ||
    manifest.sha256 !== (await digest(file))
  )
    throw Error("Windows installer incomplete");
  const artifact = {
    file: basename(file),
    size: lstatSync(file).size,
    sha256: manifest.sha256,
    delivery,
  };
  if (JSON.stringify(await sourceIdentity(root)) !== JSON.stringify(source))
    throw Error("Source inventory changed during Windows assembly");
  const backendIdentity = {
    backend_source_commit: (backend?.source ?? source).source_commit,
    backend_source_manifest_sha256: (backend?.source ?? source).source_manifest_sha256,
  };
  save(file + ".manifest.json", { ...manifest, ...backendIdentity });
  const report = {
    format: 1,
    host: "windows-x64",
    ...source,
    ...backendIdentity,
    builder_commit: output(["git", "rev-parse", "HEAD"], { cwd: ROOT }),
    artifact,
    package_delivery: {
      format: "single",
      file: artifact.file,
      size: artifact.size,
      sha256: artifact.sha256,
    },
    android: {
      ...android.receipt,
      programs_prepared: true,
      guest_boot: "not-executed",
    },
    ...(backend ? {
      backend_runtime: "wsl2-backend",
      backend_rootfs: backend.component.rootfs,
      backend_runtime_acceptance: backend.runtime_acceptance,
    } : {
      backend_runtime: "qemu-whpx-backend",
      backend_vm: json(join(root, "apps/client/src-tauri/gen/host/qemu/manifest.json")),
      backend_runtime_acceptance: "not-tested",
    }),
    physical_installation: "not-tested",
    windows_boot_verified: false,
    binary_publication: false,
  };
  save(join(root, "dist/windows-full-factory-report.json"), report);
  if (!installerMetadata(file, "windows-x64").github_asset_eligible)
    throw Error(
      `Complete Windows EXE is ${artifact.size} bytes; GitHub release assets require less than 2 GiB`,
    );
  return report;
}
