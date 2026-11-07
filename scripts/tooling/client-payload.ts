import {
  existsSync,
  readFileSync,
  lstatSync,
  mkdirSync,
  copyFileSync,
  renameSync,
  rmSync,
  createWriteStream,
} from "node:fs";
import { join, dirname, basename } from "node:path";
import { once } from "node:events";
import { ROOT, hash, digest, output, nativeProgram } from "./process";
import { sourceFiles, type SourceFile } from "./source";
import { json, save, records, canonical } from "./inventory";
import { writeTar, tarEntries, entryBytes, type ArchiveFile } from "./archive";
import { prepareWeb } from "./web-runtime";
import { prepareServices } from "./native-build";
import { preparePhone, prepareGuestApps } from "./phone-apps";
import { prepareContainerHost } from "./container-host";
import { buildConfig } from "./resources";

export const componentManifests = {
  web: "runtime/manifest.json",
  "native-services": "runtime/services/manifest.json",
  "phone-apps": "runtime/phone-apps/manifest.json",
  "container-host": "runtime/container-host/manifest.json",
  "guest-apps": "runtime/guest-apps/manifest.json",
};
export type PayloadOptions = {
  web?: boolean;
  nativeServices?: boolean;
  phoneApps?: boolean;
  containerHost?: boolean;
  guestApps?: boolean;
  androidRuntime?: string;
  imageRuntime?: string;
  linuxOcr?: string;
  wslRuntime?: string;
};
export async function preparePayload(
  root = ROOT,
  options: PayloadOptions = {},
) {
  const folder = join(root, "apps/client/src-tauri/gen/host");
  mkdirSync(folder, { recursive: true });
  const selected: SourceFile[] = sourceFiles(root),
    components: string[] = [];
  const producers: [
    keyof PayloadOptions,
    keyof typeof componentManifests,
    () => SourceFile[] | Promise<SourceFile[]>,
  ][] = [
    ["web", "web", () => prepareWeb(root)],
    [
      "nativeServices",
      "native-services",
      () => prepareServices(root, options.linuxOcr),
    ],
    ["phoneApps", "phone-apps", () => preparePhone(root)],
    ["containerHost", "container-host", () => prepareContainerHost(root)],
    ["guestApps", "guest-apps", () => prepareGuestApps(root)],
  ];
  for (const [key, name, produce] of producers)
    if (options[key]) {
      components.push(name);
      selected.push(...(await produce()));
    }
  const manifest = {
      format: components.length ? 2 : 1,
      components,
      files: records(selected),
    },
    payload = join(folder, "source.payload"),
    temporary = payload + ".tmp";
  rmSync(temporary, { force: true });
  await writeTar(
    temporary,
    [
      ...selected,
      {
        path: "HOST-MANIFEST.json",
        mode: 0o644,
        data: Buffer.from(JSON.stringify(manifest)),
      },
    ],
    { codec: "xz", root },
  );
  renameSync(temporary, payload);
  const integrity: Record<string, string> = {
    "source.payload": await digest(payload),
  };
  for (const [name, source] of [
    ["android.payload", options.androidRuntime],
    ["images.payload", options.imageRuntime],
  ] as const) {
    const target = join(folder, name);
    if (!source) {
      rmSync(target, { force: true });
      continue;
    }
    if (source !== target) copyFileSync(source, target);
    integrity[name] = await digest(target);
  }
  if (options.wslRuntime) {
    const target = join(folder, "wsl");
    mkdirSync(target, { recursive: true });
    let manifest: any;
    for await (const entry of tarEntries(options.wslRuntime, "raw", root)) {
      if (entry.name === "WSL-BACKEND-MANIFEST.json")
        manifest = JSON.parse((await entryBytes(entry)).toString());
      else if (entry.name === "rootfs.tar.gz") {
        const file = join(target, "rootfs.tar.gz.tmp"),
          out = createWriteStream(file, { flags: "wx" }),
          done = once(out, "close");
        for await (const chunk of entry.body)
          if (!out.write(chunk)) await once(out, "drain");
        out.end();
        await done;
        renameSync(file, join(target, "rootfs.tar.gz"));
      } else throw Error("Unexpected WSL capsule member");
    }
    if (
      !manifest ||
      lstatSync(join(target, "rootfs.tar.gz")).size !== manifest.rootfs.size ||
      (await digest(join(target, "rootfs.tar.gz"))) !== manifest.rootfs.sha256
    )
      throw Error("WSL capsule rootfs changed");
    save(join(target, "manifest.json"), manifest);
    integrity["wsl/manifest.json"] = await digest(
      join(target, "manifest.json"),
    );
  } else rmSync(join(folder, "wsl"), { recursive: true, force: true });
  save(join(folder, "integrity.json"), integrity);
  buildConfig(root);
  return integrity;
}
export async function payloadComponents(root = ROOT) {
  const folder = join(root, "apps/client/src-tauri/gen/host"),
    payload = join(folder, "source.payload");
  if (!existsSync(payload)) return [];
  let components: string[] = [];
  for await (const entry of tarEntries(payload, "xz", root))
    if (entry.name === "HOST-MANIFEST.json")
      components =
        JSON.parse((await entryBytes(entry)).toString()).components || [];
  const integrity = json(join(folder, "integrity.json"));
  return [
    ...components,
    ...("android.payload" in integrity ? ["android-runtime"] : []),
    ...("images.payload" in integrity ? ["container-images"] : []),
    ...("wsl/manifest.json" in integrity ? ["wsl-backend"] : []),
  ];
}
export async function stageSource(archive: string, home: string, root = ROOT) {
  const result = output(
    [
      nativeProgram(root),
      "--stage-source",
      archive,
      home,
      await digest(archive),
    ],
    { cwd: root },
  );
  const offset = result.indexOf(" ");
  if (offset !== 64) throw Error("Invalid source staging receipt");
  return result.slice(offset + 1);
}
export function installerMetadata(file: string, host: string) {
  const format = file.split(".").at(-1)!.toLowerCase(),
    supported: Record<string, string> = {
      "macos-arm64": "dmg",
      "windows-x64": "exe",
    },
    info = lstatSync(file);
  if (supported[host] !== format || !info.isFile() || info.size === 0)
    throw Error("Unsupported single-file installer");
  return {
    file: basename(file),
    format,
    size: info.size,
    single_file: true,
    user_assembly_required: false,
    github_asset_eligible: info.size < 2 * 1024 ** 3,
  };
}
export function deliveryMetadata(components: string[], host: string) {
  const known = new Set([
      ...Object.keys(componentManifests),
      "android-runtime",
      "container-images",
      "wsl-backend",
    ]),
    selected = new Set(components);
  if (
    selected.size !== components.length ||
    components.some((name) => !known.has(name))
  )
    throw Error("Unknown client component");
  const required =
    host === "windows-x64"
      ? ["android-runtime", "wsl-backend"]
      : [
          ...Object.keys(componentManifests),
          "android-runtime",
          "container-images",
        ];
  const missing = required.filter((name) => !selected.has(name));
  return {
    prebuilt_components: components,
    server_included:
      selected.has("wsl-backend") ||
      ["web", "native-services"].every((name) => selected.has(name)),
    runtime_payload_complete: !missing.length,
    complete_device_bundle: !missing.length,
    missing_runtime_components: missing,
    deployment_mode: missing.length ? "component-bundle" : "embedded-local",
    server_requirement: missing.length
      ? "Complete local runtime components are required."
      : "None; the client initializes its local device runtime.",
  };
}
