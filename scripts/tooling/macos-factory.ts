import {
  existsSync,
  mkdirSync,
  copyFileSync,
  chmodSync,
  unlinkSync,
  rmSync,
  lstatSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { run, output, digest, bunProgram, hostKey } from "./process";
import { json, save } from "./inventory";
import { extractTar, buildReleaseTools } from "./archive";
import { buildNative } from "./native-build";
import {
  containerPolicy,
  buildContainerHost,
  buildLima,
  verifyLima,
} from "./container-host";
import { buildAndroid, prepareAndroid } from "./android-component";
import { guestBinary, buildGuest, verifyGuest } from "./guest-build";
import {
  capacity,
  fetchPin,
  checkDocker,
  compilerEnvironment,
  prepareApps,
  prepareImages,
} from "./factory";
import { admitImages } from "./image-handoff";
import { prepareDelivery } from "./package-delivery";
import { deliveryMetadata, installerMetadata } from "./client-payload";
import { checkSource } from "./source";

async function host(root: string, work: string) {
  const policy = containerPolicy(root),
    native = join(root, ".tools/lima-native"),
    pin = policy.runtime.lima_build;
  if (existsSync(native)) await verifyLima(root, native);
  else {
    const go = await fetchPin(work, "go.tar.gz", pin.go_archive),
      compiler = join(work, "go-compiler"),
      source = join(work, "lima-source");
    await extractTar(go, compiler, "gzip", root);
    mkdirSync(source);
    run(["git", "init", source], { cwd: root });
    run(["git", "remote", "add", "origin", pin.source_url], { cwd: source });
    run(
      ["git", "fetch", "--no-tags", "--depth=1", "origin", pin.source_commit],
      { cwd: source },
    );
    run(["git", "checkout", "--detach", "FETCH_HEAD"], { cwd: source });
    await buildLima(root, source, join(compiler, "go/bin/go"));
  }
  const image = await fetchPin(work, "alpine.iso", policy.runtime.image),
    lima = await fetchPin(work, "lima.tar.gz", policy.lima),
    packages = join(work, "apks");
  mkdirSync(packages);
  for (const pin of policy.runtime.packages)
    await fetchPin(packages, pin.file, pin);
  return buildContainerHost(root, lima, image, packages);
}
async function guest(root: string, work: string) {
  const provided = process.env.AI_PHONE_FACTORY_GUEST_RUNTIME;
  if (!provided) return buildGuest(root, join(work, "guest-native"));
  const receipt = await verifyGuest(root, provided, "arm64"),
    context = join(work, "guest-context"),
    file = join(context, guestBinary);
  mkdirSync(join(context, "build/target/release"), { recursive: true });
  copyFileSync(join(provided, guestBinary), file);
  chmodSync(file, 0o755);
  save(join(context, "manifest.json"), {
    format: 1,
    target: receipt.target,
    source_sha256: receipt.source_sha256,
    sha256: receipt.sha256,
  });
  return context;
}
export async function macosFactory(
  root: string,
  sdk: string,
  candidate?: string,
  identity?: string,
) {
  if (hostKey() !== "macos-arm64")
    throw Error("Complete Mac assembly requires Apple Silicon");
  capacity(root);
  checkSource(root);
  const env = compilerEnvironment(sdk),
    work = join(root, ".tools/complete-macos");
  if (existsSync(work)) throw Error("Use a fresh factory checkout");
  mkdirSync(work, { recursive: true });
  const source = await digest(join(root, "SOURCE-MANIFEST.json"));
  let prepared: string | undefined, context: string | undefined;
  if (candidate) {
    prepared = (
      await admitImages(
        root,
        candidate,
        identity!,
        process.env.GITHUB_SHA ||
          output(["git", "rev-parse", "HEAD"], { cwd: root }),
      )
    ).file;
  } else {
    checkDocker();
    const arch = JSON.parse(
      output(["docker", "info", "--format", "{{json .}}"]),
    ).Architecture;
    if (!["arm64", "aarch64"].includes(arch))
      throw Error("Mac factory requires an ARM64 image engine");
    context = await guest(root, work);
  }
  buildReleaseTools(root);
  await buildNative(root);
  const engine = await host(root, work),
    android = await buildAndroid(root, join(work, "android"), 9),
    qualification = join(work, "android-qualification");
  await prepareAndroid(android.file, qualification, root);
  rmSync(qualification, { recursive: true });
  for (const role of ["emulator", "platform-tools", "system-image"])
    unlinkSync(join(work, "android", role + ".zip"));
  const apps = prepareApps(root, env),
    images =
      prepared ||
      (await prepareImages(root, work, source, "arm64", context)).file;
  run(
    [
      bunProgram(root),
      join(root, "scripts/build-client.ts"),
      "--platform",
      "macos",
      "--with-web-runtime",
      "--with-service-runtime",
      "--with-phone-apps",
      "--with-container-host",
      "--with-guest-apps",
      "--android-runtime",
      android.file,
      "--images-runtime",
      images,
    ],
    { cwd: root, env },
  );
  const version = json(join(root, "config/release-policy.json")).version,
    file = join(root, "dist", `AI-Phone-${version}-macos-arm64.dmg`),
    manifest = json(file + ".manifest.json"),
    delivery = deliveryMetadata(manifest.prebuilt_components, "macos-arm64");
  if (
    !delivery.complete_device_bundle ||
    manifest.sha256 !== (await digest(file)) ||
    !installerMetadata(file, "macos-arm64").github_asset_eligible
  )
    throw Error("Mac installer incomplete or over delivery limit");
  const artifact = {
    file: file.split("/").at(-1)!,
    size: lstatSync(file).size,
    sha256: manifest.sha256,
    delivery,
  };
  if ((await digest(join(root, "SOURCE-MANIFEST.json"))) !== source)
    throw Error("Factory source changed");
  const report = {
    format: 1,
    host: "macos-arm64",
    source_commit:
      process.env.GITHUB_SHA ||
      output(["git", "rev-parse", "HEAD"], { cwd: root }),
    source_manifest_sha256: source,
    artifact,
    package_delivery: await prepareDelivery(root, artifact),
    container_host: engine,
    android: {
      ...android.receipt,
      programs_prepared: true,
      guest_boot: "not-executed",
    },
    image_capsule_bytes: lstatSync(images).size,
    phone_apps_source_sha256: apps.phone.source_sha256,
    physical_installation: "not-tested",
    binary_publication: false,
  };
  save(join(root, "dist/macos-full-factory-report.json"), report);
  return report;
}
