import {
  mkdirSync,
  existsSync,
  lstatSync,
  readFileSync,
  writeFileSync,
  copyFileSync,
  cpSync,
  createReadStream,
  createWriteStream,
  openSync,
  readSync,
  closeSync,
  statfsSync,
} from "node:fs";
import { join, dirname } from "node:path";
import { randomUUID, createHash } from "node:crypto";
import { pipeline } from "node:stream/promises";
import { createGzip } from "node:zlib";
import { Readable } from "node:stream";
import { digest, run, output } from "./process";
import { json, save, collect, records } from "./inventory";
import { tarEntries, entryBytes } from "./archive";
import { Zip } from "./zip";
import { fetchPin } from "./factory";
import { packageInputs } from "./container-host";
import { imageManifest } from "./image-capsule";
import {
  publishWsl,
  phoneSchema,
  phoneManifest,
  phoneRequired,
  qemuBootstrapContract,
} from "./wsl-capsule";

export function wslPlan(root: string) {
  const policy = json(join(root, "config/wsl-backend.json")),
    engine = json(join(root, "config/container-host-runtime.json")).hosts[
      "linux-x64"
    ];
  if (
    policy.format !== 1 ||
    policy.host !== "windows-x64" ||
    policy.backend_host !== "linux-x64" ||
    !/^ubuntu@sha256:[0-9a-f]{64}$/.test(policy.base_image)
  )
    throw Error("Unsupported WSL backend policy");
  return {
    ...policy,
    engine_image: engine.builder_image,
    engine_packages: engine.packages,
    engine_versions: { docker: engine.docker, compose: engine.compose },
  };
}
export async function sourceInput(file: string, root: string) {
  const files: Record<string, any> = {};
  let manifest: any,
    web: any,
    services: any,
    total = 0;
  for await (const entry of tarEntries(file, "xz", root)) {
    if (entry.type === "5") continue;
    if (entry.type !== "0" || Object.hasOwn(files, entry.name))
      throw Error("Invalid source payload member");
    if (entry.name === "HOST-MANIFEST.json") {
      manifest = JSON.parse((await entryBytes(entry)).toString());
      continue;
    }
    const hash = createHash("sha256"),
      chunks: Buffer[] = [],
      metadata = [
        "runtime/manifest.json",
        "runtime/services/manifest.json",
      ].includes(entry.name);
    for await (const chunk of entry.body) {
      hash.update(chunk);
      if (metadata) chunks.push(chunk);
    }
    files[entry.name] = {
      path: entry.name,
      size: entry.size,
      mode: entry.mode,
      sha256: hash.digest("hex"),
    };
    total += entry.size;
    if (metadata) {
      const value = JSON.parse(Buffer.concat(chunks).toString());
      if (entry.name === "runtime/manifest.json") web = value;
      else services = value;
    }
  }
  const components = [...manifest.components].sort(),
    base = ["native-services", "web"],
    complete = ["guest-apps", "native-services", "phone-apps", "web"];
  if (
    ![JSON.stringify(base), JSON.stringify(complete)].includes(
      JSON.stringify(components),
    ) ||
    web?.host !== "linux-x64" ||
    services?.host !== "linux-x64" ||
    services?.vision !== "linux-tesseract-bundled" ||
    Object.keys(files).length !== manifest.files.length ||
    manifest.files.some((row: any) => {
      const actual = files[row.path];
      return (
        !actual ||
        ["size", "mode", "sha256"].some((key) => actual[key] !== row[key])
      );
    })
  )
    throw Error(
      "WSL requires matched Linux x64 backend components and offline OCR",
    );
  const bootstrap = files["runtime/services/bin/assistant"];
  if (!bootstrap || bootstrap.mode !== 0o755)
    throw Error("WSL bootstrap missing");
  return { sha256: await digest(file), components, files, total, bootstrap };
}
export async function preparePhoneTools(root: string, work: string) {
  const pin = {
      url: "https://github.com/meator/android-tools-static/releases/download/36.0.1/adb-36.0.1-linux-x64.zip",
      sha256: "1808ce1db6ae78ca6e11a52ec2d57d0dcff6e35157cdde2831c5d5973082f62a",
      size_bytes: 13_007_124,
      version: "36.0.1",
    },
    archive = await fetchPin(work, "platform-tools.zip", pin),
    folder = join(work, "sdk");
  mkdirSync(folder);
  mkdirSync(join(folder, "platform-tools"));
  const zip = new Zip(archive);
  for (const name of phoneRequired.filter((name) => name.startsWith("platform-tools/"))) {
    const entry = zip.entries.find((entry) => entry.name === name);
    if (!entry) throw Error(`Missing native phone bridge file: ${name}`);
    writeFileSync(join(folder, name), await zip.bytes(entry), {
      flag: "wx",
      mode: entry.mode & 0o111 ? 0o755 : 0o644,
    });
  }
  const revision = readFileSync(
    join(folder, "platform-tools/source.properties"),
    "utf8",
  )
    .match(/^Pkg.Revision\s*=\s*(.+)$/m)?.[1]
    ?.trim();
  if (revision !== pin.version) throw Error("Linux ADB revision differs");
  const header = Buffer.alloc(20),
    fd = openSync(join(folder, "platform-tools/adb"), "r");
  try {
    readSync(fd, header);
  } finally {
    closeSync(fd);
  }
  if (
    !header.subarray(0, 7).equals(Buffer.from([127, 69, 76, 70, 2, 1, 1])) ||
    header.readUInt16LE(18) !== 62
  )
    throw Error("Phone bridge requires Linux x64 ADB");
  const vendor = join(root, "packages/protocol/vendor/android-emulator"),
    schema = json(join(vendor, "provenance.json"));
  if (
    lstatSync(join(vendor, "emulator_controller.proto")).size !==
      schema.bytes ||
    (await digest(join(vendor, "emulator_controller.proto"))) !== schema.sha256
  )
    throw Error("Emulator protocol differs");
  mkdirSync(dirname(join(folder, phoneSchema)), { recursive: true });
  copyFileSync(
    join(vendor, "emulator_controller.proto"),
    join(folder, phoneSchema),
  );
  copyFileSync(
    join(vendor, "provenance.json"),
    join(folder, "provenance.json"),
  );
  copyFileSync(join(root, "LICENSE"), join(folder, "LICENSE"));
  const files = records(collect(folder, "").files).map((row) => ({
      ...row,
      path: row.path.replace(/^\//, ""),
    })),
    inventory = Object.fromEntries(
      files.map((row) => [
        row.path,
        { bytes: row.size, sha256: row.sha256, mode: row.mode },
      ]),
    );
  if (
    phoneRequired
      .filter((name) => name !== phoneManifest)
      .some((name) => !Object.hasOwn(inventory, name))
  )
    throw Error("Incomplete native phone bridge");
  save(join(folder, phoneManifest), {
    format: 1,
    host: "linux-x64",
    platform_tools_sha256: pin.sha256,
    schema_sha256: schema.sha256,
    redistribution: "review-pending",
    files: inventory,
  });
  return folder;
}
async function engineLicenses(folder: string, pins: any[]) {
  const result = [];
  for (const pin of pins) {
    let info: Record<string, string> | undefined;
    for await (const entry of tarEntries(join(folder, pin.file), "gzip"))
      if (entry.name === ".PKGINFO")
        info = Object.fromEntries(
          (await entryBytes(entry, 65536))
            .toString()
            .split("\n")
            .filter((line) => line.includes(" = "))
            .map((line) => {
              const offset = line.indexOf(" = ");
              return [line.slice(0, offset), line.slice(offset + 3)];
            }),
        );
    if (!info?.license || !info.pkgname)
      throw Error("Missing engine package attribution");
    result.push({
      package: info.pkgname,
      version: info.pkgver,
      license: info.license,
      source: info.url || "",
      archive_url: pin.url,
      sha256: pin.sha256,
    });
  }
  return result;
}
function engineScript(program: string) {
  return (
    '#!/bin/sh\nset -eu\nengine=/opt/aiphone/engine\nexport LD_LIBRARY_PATH="$engine/lib:$engine/usr/lib"\nexport XTABLES_LIBDIR="$engine/usr/lib/xtables"\nPATH="$engine/usr/bin:$engine/usr/sbin:$engine/bin:$engine/sbin:$PATH"\nexport PATH\nexec "$engine/usr/bin/' +
    program +
    '" "$@"\n'
  );
}
function dockerfile(plan: any) {
  return `FROM ${plan.engine_image} AS engine
COPY packages/ /packages/
RUN apk --no-network add /packages/*.apk && rm -rf /packages /var/cache/apk/*
FROM ${plan.base_image}
COPY --from=engine / /opt/aiphone/engine/
RUN find /opt/aiphone/engine/bin /opt/aiphone/engine/sbin /opt/aiphone/engine/usr /opt/aiphone/engine/lib /opt/aiphone/engine/etc /opt/aiphone/engine/var -type l -exec sh -c 'for path do target=$(readlink "$path"); case "$target" in /*) if [ -e "/opt/aiphone/engine$target" ] || [ -L "/opt/aiphone/engine$target" ]; then ln -snf "/opt/aiphone/engine$target" "$path"; fi;; esac; done' sh {} +
COPY phone-tools/ /opt/aiphone/native-phone-bridge/
COPY source.payload images.payload source.sha256 /opt/aiphone/bundle/
COPY bootstrap.program /usr/local/libexec/aiphone-bootstrap
COPY docker dockerd /usr/local/bin/
COPY wsl.conf /etc/wsl.conf
COPY engine-sources.json /usr/share/doc/aiphone-engine/source-records.json
RUN cp -L /usr/lib/os-release /tmp/aiphone-os-release && mv /tmp/aiphone-os-release /etc/os-release && chmod 644 /etc/os-release && chmod 755 /usr/local/bin/docker /usr/local/bin/dockerd && ln -s /opt/aiphone/engine/lib/ld-musl-x86_64.so.1 /lib/ld-musl-x86_64.so.1 && mkdir -p /usr/local/lib/docker/cli-plugins && ln -s /opt/aiphone/engine/usr/libexec/docker/cli-plugins/docker-compose /usr/local/lib/docker/cli-plugins/docker-compose
RUN printf '%s\\n' '/opt/aiphone/engine/lib:/opt/aiphone/engine/usr/lib' > /etc/ld-musl-x86_64.path
RUN /usr/bin/env -i PATH=/usr/bin:/bin /opt/aiphone/engine/usr/bin/runc --version
RUN /usr/local/bin/docker --version && /usr/local/bin/docker compose version && /usr/local/bin/dockerd --version
RUN /opt/aiphone/native-phone-bridge/platform-tools/adb version
RUN test "$(/usr/bin/timeout 10 /usr/local/libexec/aiphone-bootstrap --source-staging-contract)" = ai-phone-source-staging-v1 && test "$(/usr/bin/timeout 10 /usr/local/libexec/aiphone-bootstrap --wsl-backend-initializer-contract)" = ai-phone-wsl-initializer-v1
RUN test "$(/usr/bin/timeout 10 /usr/local/libexec/aiphone-bootstrap --qemu-guest-control-contract)" = ${qemuBootstrapContract}
ENTRYPOINT ["/usr/local/libexec/aiphone-bootstrap", "--initialize-wsl-backend"]
`;
}
export async function buildWsl(
  root: string,
  work: string,
  source: string,
  images: string,
  packages: string,
  phoneTools: string,
) {
  if (existsSync(work)) throw Error("Use a fresh WSL factory workspace");
  const plan = wslPlan(root),
    input = await sourceInput(source, root),
    image = await imageManifest(images, root);
  if (image.architecture !== "amd64") throw Error("WSL requires amd64 images");
  await packageInputs(packages, plan.engine_packages);
  let contextBytes =
    lstatSync(source).size + lstatSync(images).size + input.bootstrap.size + 65;
  for (const folder of [packages, phoneTools])
    for (const file of collect(folder, "").files)
      contextBytes += file.data.length;
  const rootfsBudget = contextBytes + 1536 * 1024 ** 2,
    disk = statfsSync(dirname(work));
  if (
    disk.bavail * disk.bsize <
    4 * 1024 ** 3 + contextBytes + 4 * rootfsBudget
  )
    throw Error("Insufficient WSL factory capacity");
  const context = join(work, "context");
  mkdirSync(context, { recursive: true });
  for (const [file, name] of [
    [source, "source.payload"],
    [images, "images.payload"],
  ])
    copyFileSync(file!, join(context, name!));
  await Bun.write(join(context, "source.sha256"), input.sha256 + "\n");
  for await (const entry of tarEntries(source, "xz", root))
    if (entry.name === "runtime/services/bin/assistant") {
      const target = createWriteStream(join(context, "bootstrap.program"), {
        flags: "wx",
        mode: 0o755,
      });
      await pipeline(Readable.from(entry.body), target);
    }
  cpSync(packages, join(context, "packages"), { recursive: true });
  cpSync(phoneTools, join(context, "phone-tools"), { recursive: true });
  for (const program of ["docker", "dockerd"])
    await Bun.write(join(context, program), engineScript(program));
  await Bun.write(
    join(context, "wsl.conf"),
    "[interop]\nenabled=false\nappendWindowsPath=false\n[automount]\nenabled=false\n",
  );
  save(
    join(context, "engine-sources.json"),
    await engineLicenses(packages, plan.engine_packages),
  );
  await Bun.write(join(context, "Dockerfile"), dockerfile(plan));
  const provenance = {
    policy: plan,
    source_sha256: input.sha256,
    source_commit: output(["git", "rev-parse", "HEAD"], { cwd: root }),
    source_manifest_sha256: await digest(join(root, "SOURCE-MANIFEST.json")),
    source_components: input.components,
    images_sha256: await digest(images),
    phone_tools_manifest_sha256: await digest(join(phoneTools, phoneManifest)),
    qemu_bootstrap: qemuBootstrapContract,
  };
  const nonce = randomUUID().replaceAll("-", ""),
    reference = "ai-phone-wsl-factory:" + nonce,
    name = "ai-phone-wsl-factory-" + nonce;
  run(
    [
      "docker",
      "buildx",
      "build",
      "--platform",
      "linux/amd64",
      "--network",
      "none",
      "--load",
      "--provenance=false",
      "-t",
      reference,
      context,
    ],
    { cwd: root },
  );
  const container = output([
      "docker",
      "create",
      "--name",
      name,
      "--network",
      "none",
      "--label",
      "dev.aiphone.factory=wsl-backend-factory",
      reference,
    ]),
    archive = join(work, "rootfs.tar"),
    fd = openSync(archive, "wx");
  try {
    run(["docker", "export", container], { stdio: ["ignore", fd, "inherit"] });
  } finally {
    closeSync(fd);
    run(["docker", "rm", container]);
  }
  const compressed = join(work, "rootfs.tar.gz");
  await pipeline(
    createReadStream(archive),
    createGzip({ level: 9 }),
    createWriteStream(compressed, { flags: "wx" }),
  );
  const result = await publishWsl(
    compressed,
    join(work, "wsl-backend.payload"),
    provenance,
  );
  save(join(work, "receipt.json"), result);
  return result;
}
