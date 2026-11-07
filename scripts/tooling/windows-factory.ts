import {
  mkdirSync,
  copyFileSync,
  readdirSync,
  lstatSync,
  rmSync,
  unlinkSync,
} from "node:fs";
import { join, resolve, basename } from "node:path";
import { digest, hostKey, bunProgram, run } from "./process";
import { json, save } from "./inventory";
import { checkSource } from "./source";
import { verifyWsl } from "./wsl-capsule";
import { buildAndroid, prepareAndroid } from "./android-component";
import { deliveryMetadata, installerMetadata } from "./client-payload";
import { prepareDelivery } from "./package-delivery";
import { buildReleaseTools } from "./archive";

const components = ["guest-apps", "native-services", "phone-apps", "web"],
  body = "wsl-backend.payload",
  metadata = "windows-backend.json";
async function sourceIdentity(root: string) {
  const commit = process.env.GITHUB_SHA || "";
  if (!/^[0-9a-f]{40}$/.test(commit))
    throw Error("Complete Windows factory requires an exact source commit");
  return {
    source_commit: commit,
    source_manifest_sha256: await digest(join(root, "SOURCE-MANIFEST.json")),
  };
}
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
  const source = await sourceIdentity(root),
    runtime = report.runtime;
  if (
    report.source_commit !== source.source_commit ||
    report.passed !== true ||
    runtime?.passed !== true ||
    runtime.phases.length !== 2 ||
    runtime.phases.some(
      (phase: any) =>
        !Object.keys(phase.checks).length ||
        Object.values(phase.checks).some((v) => v !== true),
    ) ||
    Object.keys(runtime.cleanup).length !== 3 ||
    ["container", "image", "volume"].some((k) => runtime.cleanup[k] !== true)
  )
    throw Error("Backend startup, restart and owned cleanup must qualify");
  const component = await verifyWsl(payload, root);
  backendComponents(component);
  if (
    component.rootfs.sha256 !== report.component.rootfs.sha256 ||
    component.provenance.source_sha256 !==
      report.component.provenance.source_sha256
  )
    throw Error("Qualified backend changed");
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
  return { payload, component };
}
export async function windowsFactory(
  root: string,
  folder: string,
  identity: string,
) {
  if (hostKey() !== "windows-x64")
    throw Error("Complete Windows assembly requires Windows x64");
  checkSource(root, { repository: true, manifest: true });
  const source = await sourceIdentity(root),
    backend = await admitBackend(root, resolve(folder), identity),
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
  const android = await buildAndroid(root, join(work, "android"), 9),
    prepared = join(work, "android-qualification");
  await prepareAndroid(android.file, prepared, root);
  rmSync(prepared, { recursive: true });
  for (const role of ["emulator", "platform-tools", "system-image"])
    unlinkSync(join(work, "android", role + ".zip"));
  run(
    [
      bunProgram(root),
      join(root, "scripts/build-client.ts"),
      "--platform",
      "windows",
      "--android-runtime",
      android.file,
      "--wsl-runtime",
      backend.payload,
    ],
    { cwd: root },
  );
  const version = json(join(root, "config/release-policy.json")).version,
    file = join(root, "dist", `AI-Phone-${version}-windows-x64-setup.exe`),
    manifest = json(file + ".manifest.json"),
    delivery = deliveryMetadata(manifest.prebuilt_components, "windows-x64");
  if (
    !delivery.complete_device_bundle ||
    manifest.complete_device_bundle !== true ||
    manifest.sha256 !== (await digest(file)) ||
    !installerMetadata(file, "windows-x64").github_asset_eligible
  )
    throw Error("Windows installer incomplete or over delivery limit");
  const artifact = {
    file: basename(file),
    size: lstatSync(file).size,
    sha256: manifest.sha256,
    delivery,
  };
  if (JSON.stringify(await sourceIdentity(root)) !== JSON.stringify(source))
    throw Error("Source inventory changed during Windows assembly");
  const report = {
    format: 1,
    host: "windows-x64",
    ...source,
    artifact,
    package_delivery: await prepareDelivery(root, artifact),
    android: {
      ...android.receipt,
      programs_prepared: true,
      guest_boot: "not-executed",
    },
    backend_rootfs: backend.component.rootfs,
    physical_installation: "not-tested",
    windows_boot_verified: false,
    binary_publication: false,
  };
  save(join(root, "dist/windows-full-factory-report.json"), report);
  return report;
}
