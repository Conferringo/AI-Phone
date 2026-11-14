import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join, dirname, resolve, relative, isAbsolute } from "node:path";
import { ROOT, digest, output } from "./process";
import { json } from "./inventory";

function metadata(root: string, manifest: string, noDependencies = false) {
  return JSON.parse(
    output(
      [
        "cargo",
        "metadata",
        "--manifest-path",
        join(root, manifest),
        "--locked",
        "--format-version",
        "1",
        ...(noDependencies ? ["--no-deps"] : []),
      ],
      { cwd: root },
    ),
  );
}
async function notices(folder: string) {
  const result = [];
  for (const entry of readdirSync(folder, { withFileTypes: true }).sort(
    (a, b) => a.name.localeCompare(b.name),
  ))
    if (
      entry.isFile() &&
      /^(LICENSE|LICENCE|COPYING|NOTICE|COPYRIGHT)/i.test(entry.name)
    )
      result.push({
        file: entry.name,
        sha256: await digest(join(folder, entry.name)),
      });
  return result;
}
function within(root: string, file: string) {
  const name = relative(root, file);
  if (name === ".." || name.startsWith("../") || isAbsolute(name))
    throw Error("Notice path outside its source tree");
}
async function supplements(root: string, pkg: any, folder: string) {
  const config = join(root, "config/native-notice-supplements.json");
  if (!existsSync(config)) return [];
  const key = pkg.name + "@" + pkg.version,
    match = json(config).packages.find((row: any) => row.package === key);
  if (!match) return [];
  if (pkg.repository !== match.repository)
    throw Error("Supplemental notice repository differs: " + key);
  if (match.commit) {
    if (json(join(folder, ".cargo_vcs_info.json")).git.sha1 !== match.commit)
      throw Error("Supplemental source differs: " + key);
  } else {
    const lock = Bun.TOML.parse(
        readFileSync(join(root, "apps/client/src-tauri/Cargo.lock"), "utf8"),
      ) as any,
      row = lock.package.find(
        (row: any) => row.name === pkg.name && row.version === pkg.version,
      );
    if (!row?.checksum || row.checksum !== match.archive_sha256)
      throw Error("Supplemental archive differs: " + key);
    for (const item of match.grant_files) {
      const path = resolve(folder, item.file);
      within(folder, path);
      if ((await digest(path)) !== item.sha256)
        throw Error("Supplemental grant differs: " + key);
    }
  }
  const result = [];
  for (const item of match.files) {
    const path = resolve(root, item.file);
    within(join(root, "docs/public/licenses/native"), path);
    if ((await digest(path)) !== item.sha256)
      throw Error("Supplemental notice differs: " + key);
    result.push({ path, url: item.url });
  }
  return result;
}
export async function dependencyInventory(root = ROOT) {
  const owned = new Set(metadata(root, "Cargo.toml", true).workspace_members),
    rust = new Map<string, any>(),
    locks: Record<string, string> = {
      "Cargo.lock": await digest(join(root, "Cargo.lock")),
      "bun.lock": await digest(join(root, "bun.lock")),
    };
  for (const manifest of ["Cargo.toml", "apps/client/src-tauri/Cargo.toml"]) {
    const graph = metadata(root, manifest),
      workspace = new Set([...owned, ...graph.workspace_members]);
    locks[join(dirname(manifest), "Cargo.lock")] = await digest(
      join(root, dirname(manifest), "Cargo.lock"),
    );
    for (const pkg of graph.packages) {
      if (workspace.has(pkg.id)) continue;
      const key = pkg.name + "@" + pkg.version;
      if (!rust.has(key))
        rust.set(key, {
          name: pkg.name,
          version: pkg.version,
          license: pkg.license,
          repository: pkg.repository,
          notices: await notices(dirname(pkg.manifest_path)),
          workspaces: [],
        });
      rust.get(key).workspaces.push(manifest);
    }
  }
  const lock = Bun.JSONC.parse(
      readFileSync(join(root, "bun.lock"), "utf8"),
    ) as { packages: Record<string, [string, ...unknown[]]> },
    locked = lock.packages,
    versions = new Set(Object.values(locked).map((row: any) => row[0])),
    javascript = new Map<string, any>();
  async function visit(folder: string) {
    const file = join(folder, "package.json");
    if (existsSync(file)) {
      const pkg = json(file),
        key = pkg.name + "@" + pkg.version;
      if (!pkg.private && pkg.name && pkg.version && versions.has(key)) {
        let license = pkg.license;
        if (
          license === undefined &&
          Array.isArray(pkg.licenses) &&
          pkg.licenses.length === 1
        )
          license = pkg.licenses[0].type;
        if (typeof license === "object") license = license.type;
        javascript.set(key, {
          name: pkg.name,
          version: pkg.version,
          license,
          repository:
            typeof pkg.repository === "object"
              ? pkg.repository.url
              : pkg.repository,
          notices: await notices(folder),
        });
      }
    }
    for (const entry of readdirSync(folder, { withFileTypes: true }))
      if (entry.isDirectory()) await visit(join(folder, entry.name));
  }
  await visit(join(root, "node_modules"));
  const order = (a: any, b: any) =>
      a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
    rustRows = [...rust.values()].sort(order),
    jsRows = [...javascript.values()].sort(order);
  return {
    format: 1,
    kind: "dependency-license-metadata",
    scope:
      "locked-source-build-dependencies; server and native client installed metadata, not a redistribution clearance",
    locks,
    rust: rustRows,
    javascript: jsRows,
    missing_license_metadata: {
      rust: rustRows
        .filter((row) => !row.license)
        .map((row) => row.name + "@" + row.version),
      javascript: jsRows
        .filter((row) => !row.license)
        .map((row) => row.name + "@" + row.version),
    },
  };
}
export async function rustNotices(
  root: string,
  manifest: string,
  component: string,
) {
  const graph = metadata(root, manifest),
    owned = new Set([
      ...metadata(root, "Cargo.toml", true).workspace_members,
      ...graph.workspace_members,
    ]),
    blocks = [
      "AI Phone " +
        component +
        " dependency notices\n\nExact locked build graph, including platform/build dependencies. Component licences remain independent. This collection is not a final redistribution clearance.\n",
    ],
    missing: string[] = [];
  for (const pkg of graph.packages.sort(
    (a: any, b: any) =>
      a.name.localeCompare(b.name) || a.version.localeCompare(b.version),
  )) {
    if (owned.has(pkg.id)) continue;
    const folder = dirname(pkg.manifest_path),
      paths = (await notices(folder)).map((item) => join(folder, item.file));
    if (pkg.license_file) {
      const path = resolve(folder, pkg.license_file);
      if (existsSync(path) && !paths.includes(path)) paths.push(path);
    }
    const extra = await supplements(root, pkg, folder);
    if (!paths.length && !extra.length)
      missing.push(pkg.name + "@" + pkg.version);
    blocks.push(
      "\n" +
        "=".repeat(72) +
        "\n" +
        pkg.name +
        " " +
        pkg.version +
        "\nSPDX: " +
        (pkg.license || "see licence file") +
        "\nSource: " +
        (pkg.repository || "https://crates.io/crates/" + pkg.name) +
        "\nAttribution: " +
        (pkg.authors?.join(", ") || "See retained upstream grant/source") +
        "\n",
    );
    for (const path of paths)
      blocks.push(
        "\n--- " +
          path.split(/[\\/]/).at(-1) +
          " ---\n" +
          readFileSync(path, "utf8"),
      );
    for (const item of extra)
      blocks.push(
        "\n--- " +
          item.path.split(/[\\/]/).at(-1) +
          " (pinned grant or linked full licence text) ---\n" +
          item.url +
          "\n" +
          readFileSync(item.path, "utf8"),
      );
  }
  const runtime = join(root, "config/rust-runtime-notices.json");
  if (existsSync(runtime))
    for (const pkg of json(runtime).packages) {
      blocks.push(
        "\n" +
          "=".repeat(72) +
          "\n" +
          pkg.package +
          "\nStandard-library copyright and accompanying terms; compiler tools are not bundled.\n",
      );
      for (const item of pkg.files) {
        const file = join(root, item.file);
        if ((await digest(file)) !== item.sha256)
          throw Error("Rust runtime notice differs");
        blocks.push(
          "\n--- " +
            item.file.split("/").at(-1) +
            " ---\n" +
            item.url +
            "\n" +
            readFileSync(file, "utf8"),
        );
      }
    }
  return {
    text: blocks.join("\n").replace(/\r\n/g, "\n").replace(/[\t ]+$/gm, ""),
    missing,
    packages: graph.packages
      .filter((pkg: any) => !owned.has(pkg.id))
      .map((pkg: any) => pkg.name + "@" + pkg.version),
  };
}
