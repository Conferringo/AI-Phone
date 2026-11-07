import { existsSync, readFileSync, realpathSync, statSync } from "node:fs";
import { join, resolve, dirname, relative } from "node:path";
import { ROOT, hostKey, output, hash } from "./process";
import { sourceFiles, type SourceFile } from "./source";
import {
  collect,
  records,
  json,
  verifyFiles,
  links,
  publicBytes,
  secret,
} from "./inventory";

function relocateConfig(value: any, root: string) {
  for (const [owner, key] of [
    [value, "repoRoot"],
    [value, "outputFileTracingRoot"],
    [value.turbopack, "root"],
    [value.experimental, "outputFileTracingRoot"],
  ] as const) {
    if (owner && key in owner) {
      if (owner[key] !== "." && resolve(owner[key]) !== root)
        throw Error("Unexpected standalone build root");
      owner[key] = ".";
    }
  }
  return value;
}
function serverConfig(text: string) {
  const start = text.indexOf("const nextConfig = ");
  if (start < 0) throw Error("Standalone configuration missing");
  const offset = start + "const nextConfig = ".length;
  let depth = 0,
    quoted = false,
    escaped = false;
  for (let end = offset; end < text.length; end++) {
    const c = text[end];
    if (quoted) {
      if (escaped) escaped = false;
      else if (c === "\\") escaped = true;
      else if (c === '"') quoted = false;
    } else if (c === '"') quoted = true;
    else if (c === "{") depth++;
    else if (c === "}" && --depth === 0)
      return {
        offset,
        end: end + 1,
        value: JSON.parse(text.slice(offset, end + 1)),
      };
  }
  throw Error("Incomplete standalone configuration");
}
export function relocateWeb(bytes: Buffer, name: string, root: string) {
  if (name === "runtime/web/apps/web/server.js") {
    const text = bytes.toString(),
      { offset, end, value } = serverConfig(text);
    return Buffer.from(
      text.slice(0, offset) +
        JSON.stringify(relocateConfig(value, root)) +
        text.slice(end),
    );
  }
  if (name === "runtime/web/apps/web/.next/required-server-files.json") {
    const value = JSON.parse(bytes.toString());
    relocateConfig(value.config, root);
    if (resolve(value.appDir) !== join(root, "apps/web"))
      throw Error("Unexpected application root");
    value.appDir = ".";
    return Buffer.from(JSON.stringify(value));
  }
  return bytes;
}
export function prepareWeb(
  root = ROOT,
  bun = join(root, ".tools/bin/bun"),
): SourceFile[] {
  root = resolve(root);
  const app = join(root, "apps/web"),
    standalone = join(app, ".next/standalone");
  const config = serverConfig(
    readFileSync(join(standalone, "apps/web/server.js"), "utf8"),
  ).value;
  if (config.images?.unoptimized !== true)
    throw Error("Disable unused native image optimization before packaging");
  const routes = json(
    join(standalone, "apps/web/.next/server/app-paths-manifest.json"),
  );
  const expected = new Set(
    sourceFiles(root)
      .filter((f) =>
        /^apps\/web\/src\/app\/(?:.*\/)?(?:page|route)\.(?:tsx?|jsx?)$/.test(
          f.path,
        ),
      )
      .map(
        (f) =>
          "/" +
          f.path
            .slice("apps/web/src/app/".length)
            .replace(/\.(?:tsx?|jsx?)$/, ""),
      ),
  );
  const actual = Object.keys(routes).filter(
    (name) => !["/_global-error/page", "/_not-found/page"].includes(name),
  );
  if (
    actual.length !== expected.size ||
    actual.some((name) => !expected.has(name))
  )
    throw Error("Compiled routes differ from source");
  const files: SourceFile[] = [],
    aliases: { path: string; target: string }[] = [];
  for (const [folder, prefix] of [
    [standalone, "runtime/web"],
    [join(app, ".next/static"), "runtime/web/apps/web/.next/static"],
    [join(app, "public"), "runtime/web/apps/web/public"],
  ]) {
    const selected = collect(folder!, prefix!, {
      omit: (name) =>
        name.endsWith(".map") ||
        name.endsWith(".nft.json") ||
        /\/(?:node_modules\/sharp|node_modules\/@img\/sharp-|\.bun\/sharp@|\.bun\/@img\+sharp-)/.test(
          "/" + name + "/",
        ),
    });
    for (const file of selected.files) {
      const rel = file.path.slice(prefix!.length + 1);
      if (
        rel
          .split("/")
          .some(
            (p) => p.startsWith(".env") || [".git", ".tools"].includes(p),
          ) ||
        (!rel.includes("node_modules/") &&
          rel.split("/").some((p) => p === "data" || p === "user"))
      )
        throw Error("Private runtime path");
      file.data = relocateWeb(file.data, file.path, root);
      if (secret.test(file.data.toString()))
        throw Error("Credential in web runtime");
      const upstream = join(root, rel);
      const vendor =
        rel.includes("node_modules/") &&
        existsSync(upstream) &&
        readFileSync(upstream).equals(file.data);
      if (!vendor) publicBytes(file.data, file.path);
      files.push(file);
    }
    aliases.push(...selected.links);
  }
  const policy = json(join(root, "config/bun-runtime.json"));
  if (output([bun, "--version"]) !== policy.version)
    throw Error("Pinned Bun runtime required");
  files.push({
    path:
      process.platform === "win32" ? "runtime/bin/bun.exe" : "runtime/bin/bun",
    data: readFileSync(bun),
    mode: 0o755,
  });
  if (new Set(files.map((f) => f.path)).size !== files.length)
    throw Error("Duplicate web runtime file");
  const manifest = {
    format: 1,
    host: hostKey(),
    services: ["web"],
    links: aliases,
    files: records(files),
  };
  return [
    ...files,
    {
      path: "runtime/manifest.json",
      data: Buffer.from(JSON.stringify(manifest) + "\n"),
      mode: 0o644,
    },
  ];
}
export async function webCommand(root = ROOT) {
  const value = json(join(root, "runtime/manifest.json"));
  if (
    value.format !== 1 ||
    value.host !== hostKey() ||
    JSON.stringify(value.services) !== '["web"]'
  )
    throw Error("Web runtime host mismatch");
  await verifyFiles(root, value.files);
  links(root, value.links || [], "runtime/web");
  return [
    join(
      root,
      process.platform === "win32" ? "runtime/bin/bun.exe" : "runtime/bin/bun",
    ),
    join(root, "runtime/web/apps/web/server.js"),
  ];
}
