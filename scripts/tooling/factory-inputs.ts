import { mkdirSync, copyFileSync, cpSync } from "node:fs";
import { join } from "node:path";
import { run } from "./process";
import { json } from "./inventory";
import { buildDependency, dependencyArchive } from "./linux-dependencies";
import { fetchPin } from "./factory";

export async function dependencyContext(
  root: string,
  folder: string,
  role: string,
) {
  mkdirSync(folder, { recursive: true });
  copyFileSync(
    join(root, "scripts", role, "Dockerfile"),
    join(folder, "Dockerfile"),
  );
  copyFileSync(
    join(root, "scripts", role, "collect.ts"),
    join(folder, "collect.ts"),
  );
  const policy = json(join(root, "config/linux-factory.json"));
  await fetchPin(folder, "bootstrap-ca.deb", policy.ca_certificates);
  await Bun.write(
    join(folder, "ubuntu.sources"),
    "Types: deb\nURIs: https://snapshot.ubuntu.com/ubuntu/" +
      policy.snapshot +
      "\nSuites: noble noble-updates noble-security\nComponents: main universe\nArchitectures: amd64\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\nSnapshot: no\n",
  );
  for (const name of ["tooling", "linux-ocr", "linux-host"]) {
    cpSync(join(root, "scripts", name), join(folder, "scripts", name), {
      recursive: true,
    });
  }
  const destination = join(folder, "output");
  run(
    [
      "docker",
      "buildx",
      "build",
      "--platform",
      "linux/amd64",
      "--target",
      "export",
      "--output",
      "type=local,dest=" + destination,
      folder,
    ],
    { cwd: root },
  );
  return destination;
}

export async function buildFactoryDependency(
  root: string,
  work: string,
  destination: string,
  role: string,
) {
  const archive = dependencyArchive(root, role);
  const output = await dependencyContext(root, work, role);
  return buildDependency(root, join(output, archive), destination, role);
}
