import {
  lstatSync,
  mkdirSync,
  mkdtempSync,
  copyFileSync,
  createWriteStream,
} from "node:fs";
import { join, resolve, dirname, relative, basename } from "node:path";
import { pipeline } from "node:stream/promises";
import { digest, output } from "./process";
import { json, save } from "./inventory";
import { imageManifest } from "./image-capsule";
import { checkSource } from "./source";
import { capacity, checkDocker, prepareImages } from "./factory";
import { buildReleaseTools } from "./archive";

const body = "factory-images-arm64.payload",
  metadata = "factory-images-arm64.json",
  kind = "factory-container-images",
  limit = 2 * 1024 ** 3;
async function header(root: string, source: string) {
  if (!/^[0-9a-f]{40}$/.test(source))
    throw Error("Factory source identity missing");
  return {
    format: 1,
    kind,
    source_commit: source,
    source_manifest_sha256: await digest(join(root, "SOURCE-MANIFEST.json")),
    architecture: "arm64",
    redistribution: "review-pending",
    binary_publication: false,
    user_state_bundled: false,
  };
}
export async function admitImages(
  root: string,
  folder: string,
  identity: string,
  source: string,
) {
  const path = join(folder, metadata),
    file = join(folder, body),
    expected = await header(root, source);
  if (
    lstatSync(path).isSymbolicLink() ||
    lstatSync(path).size > 1024 ** 2 ||
    (await digest(path)) !== identity
  )
    throw Error("Factory image metadata differs");
  const value = json(path);
  if (
    Object.keys(value).length !== Object.keys(expected).length + 1 ||
    Object.entries(expected).some(([key, item]) => value[key] !== item)
  )
    throw Error("Factory image source differs");
  if (
    value.artifact.file !== body ||
    lstatSync(file).isSymbolicLink() ||
    !lstatSync(file).isFile() ||
    lstatSync(file).size !== value.artifact.size ||
    value.artifact.size <= 0 ||
    value.artifact.size >= limit ||
    (await digest(file)) !== value.artifact.sha256
  )
    throw Error("Factory image bytes differ");
  const manifest = await imageManifest(file, root);
  if (
    manifest.architecture !== "arm64" ||
    ["workspace", "desktop", "postgres", "redis"].some(
      (role) => !manifest.images?.[role],
    )
  )
    throw Error("Native Mac images must be ARM64");
  return { file, manifest, sha256: value.artifact.sha256 };
}
export async function stageImages(
  root: string,
  payload: string,
  source: string,
) {
  const manifest = await imageManifest(payload, root),
    size = lstatSync(payload).size;
  if (manifest.architecture !== "arm64" || size <= 0 || size >= limit)
    throw Error("Unsupported factory image handoff");
  mkdirSync(join(root, "dist"), { recursive: true });
  const folder = mkdtempSync(join(root, "dist/factory-images-")),
    target = join(folder, body);
  copyFileSync(payload, target);
  const value = {
    ...(await header(root, source)),
    artifact: { file: body, size, sha256: await digest(target) },
  };
  const path = join(folder, metadata);
  save(path, value);
  return path;
}
export async function imageUploadPlan(
  root: string,
  report: string,
  source: string,
) {
  report = resolve(root, report);
  await admitImages(root, dirname(report), await digest(report), source);
  const files = [];
  for (const file of [report, join(dirname(report), body)])
    files.push({
      path: relative(join(root, "dist"), file).split("\\").join("/"),
      name: basename(file),
      size: lstatSync(file).size,
      sha256: await digest(file),
    });
  return {
    format: 1,
    kind,
    source_commit: source,
    host: "linux-arm64",
    binary_publication: false,
    files,
  };
}
function repository() {
  const value = process.env.GITHUB_REPOSITORY || "";
  if (!/^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/.test(value))
    throw Error("Explicit factory repository required");
  return value;
}
function github(path: string, ...args: string[]) {
  return JSON.parse(
    output(["gh", "api", "repos/" + repository() + "/" + path, ...args]),
  );
}
function remote(
  value: any,
  tag: string,
  identity: string,
  source: string,
  id: number,
) {
  if (
    !new RegExp(
      "^candidate-" + source.slice(0, 12) + "-[0-9]+-[0-9a-f]{32}$",
    ).test(tag) ||
    !Number.isSafeInteger(id) ||
    id <= 0 ||
    value.id !== id ||
    value.draft !== true ||
    value.published_at !== null ||
    value.target_commitish !== source ||
    value.tag_name !== tag
  )
    throw Error("Handoff is not its owned unpublished draft");
  const assets = new Map<string, any>(
    value.assets.map((asset: any) => [asset.name, asset]),
  );
  if (
    value.assets.length !== 2 ||
    assets.size !== 2 ||
    !assets.has(body) ||
    !assets.has(metadata) ||
    assets.get(metadata).size > 1024 ** 2 ||
    assets.get(metadata).digest !== "sha256:" + identity ||
    [...assets.values()].some(
      (asset) =>
        asset.state !== "uploaded" ||
        !Number.isSafeInteger(asset.id) ||
        asset.id <= 0 ||
        !Number.isSafeInteger(asset.size) ||
        asset.size <= 0 ||
        asset.size >= limit ||
        !/^sha256:[0-9a-f]{64}$/.test(asset.digest),
    )
  )
    throw Error("Handoff asset inventory differs");
  return assets;
}
export async function fetchImages(
  root: string,
  tag: string,
  identity: string,
  source: string,
  folder: string,
  id: number,
) {
  const assets = remote(github("releases/" + id), tag, identity, source, id);
  mkdirSync(folder, { recursive: true });
  for (const name of [metadata, body]) {
    const child = Bun.spawn(
      [
        "gh",
        "api",
        "repos/" + repository() + "/releases/assets/" + assets.get(name).id,
        "-H",
        "Accept: application/octet-stream",
      ],
      { stdout: "pipe", stderr: "inherit" },
    );
    await pipeline(
      child.stdout,
      createWriteStream(join(folder, name), { flags: "wx" }),
    );
    if ((await child.exited) !== 0)
      throw Error("Factory handoff download failed");
  }
  const result = await admitImages(root, folder, identity, source);
  if ("sha256:" + result.sha256 !== assets.get(body).digest)
    throw Error("Downloaded handoff differs from its asset");
  return folder;
}
export function discardImages(
  tag: string,
  identity: string,
  source: string,
  id: number,
) {
  remote(github("releases/" + id), tag, identity, source, id);
  const refs: any[] = github("git/matching-refs/tags/" + tag),
    exact = refs.filter((ref) => ref.ref === "refs/tags/" + tag);
  if (
    exact.length > 1 ||
    exact.some(
      (ref) => ref.object?.type !== "commit" || ref.object.sha !== source,
    )
  )
    throw Error("Handoff tag source differs");
  if (exact.length)
    output([
      "gh",
      "api",
      "repos/" + repository() + "/git/refs/tags/" + tag,
      "--method",
      "DELETE",
    ]);
  output([
    "gh",
    "api",
    "repos/" + repository() + "/releases/" + id,
    "--method",
    "DELETE",
  ]);
}
export async function buildImageHandoff(
  root: string,
  work: string,
  source: string,
) {
  if (
    process.platform !== "linux" ||
    process.arch !== "arm64" ||
    process.env.GITHUB_ACTIONS !== "true" ||
    process.env.RUNNER_ENVIRONMENT !== "github-hosted" ||
    resolve(process.env.GITHUB_WORKSPACE || "") !== resolve(root)
  )
    throw Error(
      "Native image assembly requires a disposable Linux ARM64 builder",
    );
  capacity(root);
  checkSource(root, { repository: true, manifest: true });
  checkDocker();
  buildReleaseTools(root);
  mkdirSync(work, { recursive: true });
  const identity = await digest(join(root, "SOURCE-MANIFEST.json"));
  const result = await prepareImages(root, work, identity, "arm64");
  if ((await digest(join(root, "SOURCE-MANIFEST.json"))) !== identity)
    throw Error("Factory source inventory changed");
  return stageImages(root, result.file, source);
}
