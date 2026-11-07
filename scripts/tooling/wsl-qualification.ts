import {
  existsSync,
  mkdirSync,
  lstatSync,
  statfsSync,
  rmSync,
  openSync,
  closeSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import { ROOT, hostKey, run, output, digest, bunProgram } from "./process";
import { json, save } from "./inventory";
import { releaseTools, buildReleaseTools } from "./archive";
import { checkDocker, prepareApps, prepareImages, fetchPin } from "./factory";
import { dependencyContext } from "./factory-inputs";
import { buildDependency } from "./linux-dependencies";
import { buildNative } from "./native-build";
import { preparePayload } from "./client-payload";
import { checkSource } from "./source";
import {
  buildWsl,
  wslPlan,
  preparePhoneTools,
  sourceInput,
} from "./wsl-factory";
import { verifyWsl } from "./wsl-capsule";
import { imageManifest } from "./image-capsule";

const label = "dev.aiphone.qualification",
  checks = [
    "assistant",
    "web",
    "channels",
    "runner_endpoint",
    "database_roles",
    "redis",
    "desktop_frame",
    "work_file",
    "database_marker",
    "desktop_registered",
    "desktop_media",
  ];
function disposable(root: string) {
  const expected = {
    GITHUB_ACTIONS: "true",
    RUNNER_ENVIRONMENT: "github-hosted",
    RUNNER_OS: "Linux",
    AI_PHONE_DISPOSABLE_QUALIFICATION: "1",
  };
  if (
    Object.entries(expected).some(
      ([key, value]) => process.env[key] !== value,
    ) ||
    resolve(process.env.GITHUB_WORKSPACE || "") !== resolve(root)
  )
    throw Error(
      "Qualification requires this disposable GitHub Linux workspace",
    );
}
function free(folder: string) {
  const info = statfsSync(folder);
  return info.bavail * info.bsize;
}
function docker(args: string[], timeout = 180000) {
  return output(["docker", ...args], { timeout });
}
function owned(kind: string, id: string, nonce: string) {
  const row = JSON.parse(docker([kind, "inspect", id]))[0],
    labels = kind === "volume" ? row.Labels : row.Config.Labels;
  if ((kind === "volume" ? row.Name : row.Id) !== id || labels[label] !== nonce)
    throw Error("Qualification resource owner changed");
}
async function runtime(
  root: string,
  work: string,
  source: string,
  images: string,
) {
  disposable(root);
  const component = await verifyWsl(join(work, "wsl-backend.payload"), root),
    input = await sourceInput(source, root),
    image = await imageManifest(images, root),
    imageDigest = await digest(images);
  if (
    input.sha256 !== component.provenance.source_sha256 ||
    imageDigest !== component.provenance.images_sha256 ||
    (await digest(join(work, "rootfs.tar.gz"))) !== component.rootfs.sha256
  )
    throw Error("Qualification inputs changed");
  const required =
    4 * 1024 ** 3 +
    2 * component.inventory.uncompressed_bytes +
    input.total +
    2 *
      Object.values(image.images).reduce(
        (total: number, row: any) => total + row.size,
        0,
      ) +
    1024 ** 3;
  if (free(work) < required)
    throw Error("Insufficient runtime qualification capacity");
  const nonce = randomUUID().replaceAll("-", ""),
    name = "ai-phone-wsl-q-" + nonce,
    resources: Record<string, string> = {},
    receipt: any = {
      format: 1,
      inputs: {
        rootfs_sha256: component.rootfs.sha256,
        source_sha256: input.sha256,
        images_sha256: imageDigest,
        required_free_bytes: required,
      },
      phases: [],
      cleanup: {},
      stage: "materialize",
      passed: false,
      windows_boot_verified: false,
      phone_ready: false,
    };
  try {
    resources.image = docker(
      [
        "image",
        "import",
        "--change",
        "LABEL " + label + "=" + nonce,
        join(work, "rootfs.tar.gz"),
        name + ":fixture",
      ],
      600000,
    );
    owned("image", resources.image, nonce);
    resources.volume = docker([
      "volume",
      "create",
      "--label",
      label + "=" + nonce,
      name,
    ]);
    owned("volume", resources.volume, nonce);
    resources.container = docker([
      "create",
      "--name",
      name,
      "--label",
      label + "=" + nonce,
      "--privileged",
      "--network=none",
      "--cgroupns=private",
      "--mount",
      "type=volume,source=" + resources.volume + ",target=/var/lib/aiphone",
      "--tmpfs",
      "/run:rw,mode=755",
      "--tmpfs",
      "/tmp:rw,mode=1777",
      "--env",
      "AI_PHONE_WSL_QUALIFICATION_NONCE=" + nonce,
      "--env",
      "container=docker",
      resources.image,
      "/usr/local/libexec/aiphone-qualification",
      "wsl-qualify",
    ]);
    owned("container", resources.container, nonce);
    docker([
      "cp",
      releaseTools(root),
      resources.container + ":/usr/local/libexec/aiphone-qualification",
    ]);
    for (const phase of [1, 2]) {
      receipt.stage = "phase_" + phase;
      owned("container", resources.container, nonce);
      const fd = openSync(
        join(work, `worker-phase-${phase}.private.log`),
        "wx",
        0o600,
      );
      let result;
      try {
        result = spawnSync(
          "docker",
          ["start", "--attach", resources.container],
          { stdio: ["ignore", fd, fd], timeout: 900000 },
        );
      } finally {
        closeSync(fd);
      }
      const file = join(work, `worker-phase-${phase}.json`);
      docker([
        "cp",
        resources.container + ":/var/lib/aiphone/qualification-result.json",
        file,
      ]);
      if (lstatSync(file).size > 65536)
        throw Error("Runtime receipt exceeds budget");
      const value = json(file);
      receipt.phases.push(value);
      if (
        result.error ||
        result.status !== 0 ||
        value.format !== 1 ||
        value.nonce !== nonce ||
        value.phase !== phase ||
        value.status !== "passed" ||
        value.stage !== "stopped" ||
        value.windows_boot_verified !== false ||
        value.phone_ready !== false ||
        Object.keys(value.checks).length !== checks.length ||
        checks.some((key) => value.checks[key] !== true) ||
        !value.frame.width ||
        !value.frame.height ||
        !/^[0-9a-f]{64}$/.test(value.frame.sha256)
      )
        throw Error("Backend qualification did not pass");
    }
    receipt.passed = true;
    receipt.stage = "complete";
    return receipt;
  } finally {
    for (const kind of ["container", "image", "volume"])
      if (resources[kind]) {
        try {
          owned(kind, resources[kind]!, nonce);
          docker([
            kind,
            "rm",
            ...(kind === "container" ? ["--force"] : []),
            resources[kind]!,
          ]);
          receipt.cleanup[kind] = true;
        } catch (error) {
          receipt.cleanup[kind] = false;
          receipt.passed = false;
          receipt.stage = "cleanup";
        }
      }
    save(join(work, "runtime-receipt.json"), receipt);
    if (Object.values(receipt.cleanup).some((value) => value !== true))
      throw Error("Owned qualification cleanup incomplete");
  }
}
async function retireCompilerCache(root: string, source: string) {
  disposable(root);
  const input = await sourceInput(source, root),
    receipt = json(join(root, "target/release/native-service-build.json"));
  for (const [role, row] of Object.entries(receipt.artifacts) as [
    string,
    any,
  ][]) {
    const actual = input.files["runtime/services/bin/" + role];
    if (!actual || actual.sha256 !== row.sha256 || actual.size !== row.size)
      throw Error("Sealed native binary differs before cache retirement");
  }
  const retired: string[] = [],
    before = free(root);
  for (const profile of ["release", "debug"])
    for (const member of ["deps", "build", ".fingerprint", "incremental"]) {
      const folder = join(root, "target", profile, member);
      if (!existsSync(folder)) continue;
      if (
        lstatSync(folder).isSymbolicLink() ||
        !lstatSync(folder).isDirectory()
      )
        throw Error("Compiler cache owner differs");
      rmSync(folder, { recursive: true });
      retired.push("target/" + profile + "/" + member);
    }
  return {
    scope: "disposable-native-build-intermediates",
    retired,
    free_before: before,
    free_after: free(root),
    native_programs_preserved: true,
    source_payload_preserved: true,
    docker_images_removed: false,
    capacity_guard_changed: false,
  };
}
function retireImageCache(root: string) {
  disposable(root);
  const endpoint = process.env.DOCKER_HOST || "",
    home = process.env.HOME || "";
  if (
    !endpoint.startsWith("unix://" + home + "/setup-docker-action/run-") ||
    !endpoint.endsWith("/docker.sock")
  )
    throw Error("Image cache requires the disposable action socket");
  const env = { ...process.env };
  for (const key of [
    "DOCKER_CONTEXT",
    "DOCKER_TLS",
    "DOCKER_TLS_VERIFY",
    "DOCKER_CERT_PATH",
  ])
    delete env[key];
  const before = free(root);
  run(["docker", "--host", endpoint, "builder", "prune", "--all", "--force"], {
    env,
    stdio: "ignore",
  });
  return {
    scope: "disposable-action-image-build-cache",
    free_before: before,
    free_after: free(root),
    image_payload_preserved: true,
    docker_images_removed: false,
    capacity_guard_changed: false,
  };
}
export async function qualifyWsl(root = ROOT, deviceApps = false) {
  disposable(root);
  if (hostKey() !== "linux-x64" || free(root) < 4 * 1024 ** 3)
    throw Error("WSL qualification needs Linux x64 and free capacity");
  checkDocker();
  checkSource(root);
  const work = join(root, ".tools/wsl-backend-qualification");
  mkdirSync(work);
  const report: any = {
    format: 1,
    stage: "assemble",
    passed: false,
    source_commit: process.env.GITHUB_SHA || "",
    windows_boot_verified: false,
    phone_ready: false,
    complete_device_bundle: false,
  };
  try {
    if (deviceApps) prepareApps(root, { ...process.env });
    const phone = await preparePhoneTools(root, join(work, "phone-tools")),
      context = await dependencyContext(
        root,
        join(work, "ocr-context"),
        "linux-ocr",
      ),
      ocr = join(root, ".tools/linux-ocr-runtime");
    await buildDependency(
      root,
      join(context, "linux-x64-ocr.tar.gz"),
      ocr,
      "linux-ocr",
    );
    buildReleaseTools(root);
    await buildNative(root);
    run([bunProgram(root), "run", "build"], { cwd: root });
    await preparePayload(root, {
      web: true,
      nativeServices: true,
      phoneApps: deviceApps,
      guestApps: deviceApps,
      linuxOcr: ocr,
    });
    const source = join(root, "apps/client/src-tauri/gen/host/source.payload"),
      retired: any = await retireCompilerCache(root, source);
    report.cache_retirement = retired;
    const images = (
      await prepareImages(
        root,
        work,
        await digest(join(root, "SOURCE-MANIFEST.json")),
        "amd64",
      )
    ).file;
    retired.image_build_cache = retireImageCache(root);
    save(join(work, "cache-retirement.json"), retired);
    const packages = join(work, "packages");
    mkdirSync(packages);
    for (const pin of wslPlan(root).engine_packages)
      await fetchPin(packages, pin.file, pin);
    const assembled = join(work, "assembled");
    report.component = await buildWsl(
      root,
      assembled,
      source,
      images,
      packages,
      phone,
    );
    report.stage = "runtime";
    report.runtime = await runtime(root, assembled, source, images);
    report.passed = true;
    report.stage = "complete";
    return report;
  } finally {
    const runtimeReceipt = join(work, "assembled/runtime-receipt.json");
    if (existsSync(runtimeReceipt)) report.runtime = json(runtimeReceipt);
    save(join(root, "dist/wsl-backend-qualification.json"), report);
  }
}
