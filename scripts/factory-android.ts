import { appendFileSync, createWriteStream } from "node:fs";
import { join } from "node:path";
import { pipeline } from "node:stream/promises";
import { hostKey, output } from "./tooling/process";

const {
  GITHUB_REPOSITORY, GITHUB_ENV, GH_TOKEN, RUNNER_TEMP,
  AIMANITY_ANDROID_DRAFT_ID, AIMANITY_ANDROID_PAYLOAD_SHA256,
} = process.env;
if (!GITHUB_REPOSITORY || !GITHUB_ENV || !GH_TOKEN || !RUNNER_TEMP ||
    !/^[1-9][0-9]*$/.test(AIMANITY_ANDROID_DRAFT_ID ?? "") ||
    !/^[a-f0-9]{64}$/.test(AIMANITY_ANDROID_PAYLOAD_SHA256 ?? ""))
  throw Error("Provide the factory repository, draft release ID and Android payload SHA-256");

const repository = `repos/${GITHUB_REPOSITORY}`;
if (output(["gh", "api", repository, "--jq", ".private"]) !== "true")
  throw Error("Use a private repository for the installer factory");

type DraftRelease = {
  draft: boolean;
  published_at: string | null;
  assets: { id: number; name: string; state: string; digest: string | null }[];
};
const release: DraftRelease = JSON.parse(output([
  "gh", "api", `${repository}/releases/${AIMANITY_ANDROID_DRAFT_ID}`,
]));
if (release.draft !== true || release.published_at !== null)
  throw Error("Use an unpublished Android payload in the private factory repository");

const name = `android-${hostKey()}.payload`;
const asset = release.assets.find((entry) => entry.name === name);
if (asset?.state !== "uploaded" || asset.digest !== `sha256:${AIMANITY_ANDROID_PAYLOAD_SHA256}`)
  throw Error(`The draft does not contain ${name} at the requested SHA-256`);

const file = join(RUNNER_TEMP, name);
const download = Bun.spawn(
  ["gh", "api", `${repository}/releases/assets/${asset.id}`, "-H", "Accept: application/octet-stream"],
  { stdout: "pipe", stderr: "inherit" },
);
const [, status] = await Promise.all([
  pipeline(download.stdout, createWriteStream(file, { flags: "wx" })),
  download.exited,
]);
if (status !== 0) throw Error(`Could not download ${name}`);
appendFileSync(GITHUB_ENV, `AIMANITY_ANDROID_PAYLOAD=${file}\n`);
