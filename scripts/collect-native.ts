import { mkdirSync, copyFileSync, writeFileSync } from "node:fs";
import { join, dirname, resolve } from "node:path";
import { ROOT, run } from "./tooling/process";
import { json } from "./tooling/inventory";
import { pinnedDownload } from "./tooling/android-component";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!,
  role = option("--role"),
  folder = resolve(option("--work"));
if (!["linux-host", "linux-ocr"].includes(role))
  throw Error("--role must be linux-host or linux-ocr");
mkdirSync(folder, { recursive: true });
copyFileSync(
  join(ROOT, "scripts", role, "Dockerfile"),
  join(folder, "Dockerfile"),
);
for (const file of [
  role + "/collect.ts",
  "tooling/linux-dependencies.ts",
  "tooling/process.ts",
  "tooling/archive.ts",
  "tooling/inventory.ts",
]) {
  const target = join(folder, "scripts", file);
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(join(ROOT, "scripts", file), target);
}
const policy = json(join(ROOT, "config/linux-factory.json"));
await pinnedDownload(policy.ca_certificates, join(folder, "bootstrap-ca.deb"));
writeFileSync(
  join(folder, "ubuntu.sources"),
  `Types: deb\nURIs: https://snapshot.ubuntu.com/ubuntu/${policy.snapshot}\nSuites: noble noble-updates noble-security\nComponents: main universe\nArchitectures: amd64\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\nSnapshot: no\n`,
);
run([
  "docker",
  "buildx",
  "build",
  "--platform",
  "linux/amd64",
  "--target",
  "export",
  "--output",
  "type=local,dest=" + join(folder, "output"),
  folder,
]);
