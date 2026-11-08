import {
  mkdirSync,
  existsSync,
  copyFileSync,
  cpSync,
  appendFileSync,
  renameSync,
  unlinkSync,
  openSync,
  closeSync,
  lstatSync,
} from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { ROOT, digest, run, output } from "./process";
import { json } from "./inventory";
import { dependencyPin } from "./linux-dependencies";
import { fetchPin } from "./factory";

export async function factoryArchive(root: string, role: string) {
  const folder = process.env.AI_PHONE_FACTORY_ARCHIVES;
  if (!folder) return;
  const pin = dependencyPin(root, role),
    file = join(folder, pin.file);
  if (
    !lstatSync(file).isFile() ||
    lstatSync(file).size !== pin.size_bytes ||
    (await digest(file)) !== pin.sha256
  )
    throw Error("Fixed factory dependency differs");
  return folder;
}
export async function acquireFactoryInputs(root = ROOT) {
  if (process.env.AI_PHONE_USE_FACTORY_ASSETS !== "1")
    return { native_factory_inputs: "source" };
  const repository = process.env.GITHUB_REPOSITORY || "";
  if (
    !/^[A-Za-z0-9][A-Za-z0-9-]*\/[A-Za-z0-9][A-Za-z0-9_.-]*$/.test(repository)
  )
    throw Error("Factory input repository missing");
  const folder = join(root, ".tools/factory-native-inputs");
  mkdirSync(folder, { recursive: true, mode: 0o700 });
  for (const role of ["linux-host", "linux-ocr"]) {
    const pin = dependencyPin(root, role),
      destination = join(folder, pin.file);
    if (!existsSync(destination)) {
      const endpoint =
          "repos/" + repository + "/releases/assets/" + pin.asset_id,
        metadata = JSON.parse(output(["gh", "api", endpoint]));
      if (
        metadata.name !== pin.file ||
        metadata.size !== pin.size_bytes ||
        metadata.digest !== "sha256:" + pin.sha256
      )
        throw Error("Factory asset does not match the fixed pin");
      const temporary = join(folder, "." + randomUUID()),
        fd = openSync(temporary, "wx", 0o644);
      try {
        try {
          run(
            [
              "gh",
              "api",
              endpoint,
              "--header",
              "Accept: application/octet-stream",
            ],
            { stdio: ["ignore", fd, "inherit"] },
          );
        } finally {
          closeSync(fd);
        }
        if (
          lstatSync(temporary).size !== pin.size_bytes ||
          (await digest(temporary)) !== pin.sha256
        )
          throw Error("Factory input changed");
        renameSync(temporary, destination);
      } finally {
        if (existsSync(temporary)) unlinkSync(temporary);
      }
    } else if (
      !lstatSync(destination).isFile() ||
      lstatSync(destination).size !== pin.size_bytes ||
      (await digest(destination)) !== pin.sha256
    )
      throw Error("Factory input differs");
  }
  if (process.env.GITHUB_ENV)
    appendFileSync(
      process.env.GITHUB_ENV,
      "AI_PHONE_FACTORY_ARCHIVES=" + folder + "\n",
    );
  return { native_factory_inputs: "pinned" };
}
export async function dependencyContext(
  root: string,
  folder: string,
  role: string,
) {
  const supplied = await factoryArchive(root, role);
  if (supplied) return supplied;
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
