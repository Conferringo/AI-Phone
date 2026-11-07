import { lstatSync, readFileSync, realpathSync } from "node:fs";
import { join, relative, basename, resolve, sep } from "node:path";
import { ROOT, digest, output } from "./process";
import { json } from "./inventory";

const assetLimit = 2 * 1024 ** 3;
function native(root: string, ...args: string[]) {
  return JSON.parse(
    output(
      [
        "cargo",
        "run",
        "--release",
        "--locked",
        "-p",
        "ai-phone-host-runtime",
        "--bin",
        "package-volumes",
        "--",
        ...args,
      ],
      { cwd: root },
    ),
  );
}
export async function prepareDelivery(root: string, artifact: any) {
  const file = artifact.file,
    source = join(root, "dist", file);
  if (
    basename(file) !== file ||
    !artifact.delivery.complete_device_bundle ||
    lstatSync(source).size !== artifact.size
  )
    throw Error("Delivery requires a complete inspected installer");
  if (artifact.size < assetLimit)
    return {
      format: "single",
      file,
      size: artifact.size,
      sha256: artifact.sha256,
    };
  const folder = source + ".volumes",
    sealed = native(root, "split", source, folder, artifact.sha256),
    manifest = join(folder, sealed.manifest);
  const verified = native(root, "verify", manifest, sealed.manifest_sha256);
  if (
    !verified.verified ||
    verified.sha256 !== artifact.sha256 ||
    verified.size !== artifact.size
  )
    throw Error("Installer volume receipt differs");
  return {
    format: "volumes",
    manifest: relative(join(root, "dist"), manifest).split(sep).join("/"),
    manifest_sha256: sealed.manifest_sha256,
    size: verified.size,
    sha256: verified.sha256,
    parts: json(manifest).parts,
  };
}
async function fileRecord(root: string, path: string) {
  const base = realpathSync(join(root, "dist")),
    name = relative(base, realpathSync(path)).split(sep).join("/"),
    info = lstatSync(path);
  if (
    !info.isFile() ||
    name
      .split("/")
      .some((s) => !/^[A-Za-z0-9][A-Za-z0-9._-]{0,190}$/.test(s)) ||
    info.size <= 0 ||
    info.size >= assetLimit
  )
    throw Error("Invalid candidate asset");
  return {
    path: name,
    name: basename(path),
    size: info.size,
    sha256: await digest(path),
  };
}
export async function uploadPlan(report: any, source: string, root = ROOT) {
  if (
    ![1, 2].includes(report.format) ||
    report.source_commit !== source ||
    report.binary_publication !== false ||
    report.source_manifest_sha256 !==
      (await digest(join(root, "SOURCE-MANIFEST.json")))
  )
    throw Error("Candidate source differs");
  const installers =
    report.format === 1
      ? [
          {
            artifact: report.artifact,
            package_delivery: report.package_delivery,
          },
        ]
      : report.installers;
  const expected =
    report.host === "macos-arm64"
      ? "dmg"
      : report.host === "windows-x64"
        ? "exe"
        : "";
  if (
    installers.length !== 1 ||
    installers[0].artifact.file.split(".").at(-1) !== expected
  )
    throw Error("Candidate installer formats differ");
  const files: any[] = [];
  for (const { artifact, package_delivery: delivery } of installers) {
    if (
      basename(artifact.file) !== artifact.file ||
      artifact.delivery.complete_device_bundle !== true
    )
      throw Error("Incomplete inspected installer");
    const metadata = join(root, "dist", artifact.file + ".manifest.json"),
      value = json(metadata);
    if (
      value.sha256 !== artifact.sha256 ||
      value.source_commit !== source ||
      value.complete_device_bundle !== true ||
      value.local_bootstrap?.user_state_bundled !== false
    )
      throw Error("Candidate metadata differs");
    if (delivery.format === "volumes") {
      const manifest = join(root, "dist", delivery.manifest),
        record = await fileRecord(root, manifest);
      if (record.sha256 !== delivery.manifest_sha256)
        throw Error("Volume inventory changed");
      const verified = native(
        root,
        "verify",
        manifest,
        delivery.manifest_sha256,
      );
      if (
        !verified.verified ||
        verified.sha256 !== artifact.sha256 ||
        verified.size !== artifact.size
      )
        throw Error("Volume receipt differs");
      files.push(record);
      for (const part of json(manifest).parts) {
        const row = await fileRecord(root, resolve(manifest, "..", part.name));
        if (row.size !== part.size || row.sha256 !== part.sha256)
          throw Error("Candidate volume changed");
        files.push(row);
      }
    } else if (delivery.format === "single") {
      const record = await fileRecord(root, join(root, "dist", artifact.file));
      if (record.sha256 !== artifact.sha256 || record.size !== artifact.size)
        throw Error("Candidate installer changed");
      files.push(record);
    } else throw Error("Unknown candidate delivery format");
    files.push(
      await fileRecord(root, metadata),
      await fileRecord(root, join(root, "dist", artifact.file + ".sha256")),
    );
  }
  if (new Set(files.map((f) => f.name)).size !== files.length)
    throw Error("Candidate asset names are duplicated");
  return {
    format: 1,
    source_commit: source,
    host: report.host,
    binary_publication: false,
    files,
  };
}
export function verifyUploaded(plan: any, release: any) {
  if (
    release.draft !== true ||
    release.published_at !== null ||
    release.target_commitish !== plan.source_commit
  )
    throw Error("Candidate is not an unpublished source-matched draft");
  const assets = new Map(release.assets.map((r: any) => [r.name, r]));
  if (
    assets.size !== release.assets.length ||
    assets.size !== plan.files.length
  )
    throw Error("Candidate remote inventory differs");
  for (const row of plan.files) {
    const asset: any = assets.get(row.name);
    if (
      asset?.state !== "uploaded" ||
      asset.size !== row.size ||
      asset.digest !== "sha256:" + row.sha256
    )
      throw Error("Candidate remote asset differs");
  }
  return true;
}
