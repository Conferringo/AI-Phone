import {
  readdirSync,
  readFileSync,
  writeFileSync,
  statSync,
  lstatSync,
  mkdirSync,
  chmodSync,
  existsSync,
  renameSync,
  mkdtempSync,
  rmSync,
} from "node:fs";
import {
  resolve,
  relative,
  dirname,
  join,
  basename,
  extname,
  isAbsolute,
} from "node:path";
import { ROOT, hash, output, run } from "./process";
const directories = [
  "apps",
  "crates",
  "packages",
  "scripts",
  "infra",
  "config",
  "skills",
  "docs/public",
  ".github",
];
const files = [
  "ai-phone",
  "Start AI Phone.command",
  "README.md",
  "README.en.md",
  "Cargo.toml",
  "Cargo.lock",
  "package.json",
  "bun.lock",
  "buf.yaml",
  "buf.gen.yaml",
  "compose.yaml",
  "compose.dev.yaml",
  "compose.desktop.yaml",
  "compose.kafka.yaml",
  ".env.example",
  ".gitignore",
  ".dockerignore",
  ".gitattributes",
  "CONTRIBUTING.md",
  "SECURITY.md",
  "DISCLAIMER.md",
  "PRIVACY.md",
  "THIRD_PARTY_NOTICES.md",
  "CHANGELOG.md",
  "LICENSE",
  "NOTICE",
];
const omitted = new Set([
  ".git",
  ".next",
  "node_modules",
  "target",
  "data",
  ".tools",
  "evidence",
  "dist",
  ".AppleDouble",
  "__MACOSX",
]);
const privateNames = new Set([
  "agent.md",
  "agents.md",
  "agents.override.md",
  "claude.md",
  "gemini.md",
  "copilot-instructions.md",
  ".cursorrules",
  "contributor-agents.md",
]);
const privateParts = new Set([".agents", ".claude", ".codex"]);
const omittedNames = new Set([".DS_Store", "Thumbs.db", "desktop.ini"]);
const omittedSuffixes = new Set([
  ".payload",
  ".log",
  ".dump",
  ".apk",
  ".jks",
  ".keystore",
  ".tsbuildinfo",
  ".qcow2",
  ".img",
  ".tar",
  ".gz",
  ".tgz",
  ".zip",
  ".xz",
  ".zst",
  ".sqlite",
  ".db",
  ".key",
]);
const omittedTrees = ["apps/client/src-tauri/gen"];
const secret =
  /(?<![A-Za-z0-9_-])(?:sk-[A-Za-z0-9_-]{24,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|gh[pousr]_[A-Za-z0-9]{30,})/;
export type SourceFile = { path: string; data: Buffer; mode: number };
export function privateMetadata(name: string) {
  const parts = name.split("/");
  return (
    privateNames.has(parts.at(-1)!.toLowerCase()) ||
    parts.some((p) => privateParts.has(p.toLowerCase()))
  );
}
function walk(root: string, folder: string, result: string[]) {
  for (const entry of readdirSync(join(root, folder), {
    withFileTypes: true,
  })) {
    const name = `${folder}/${entry.name}`;
    if (
      omitted.has(entry.name) ||
      privateMetadata(name) ||
      omittedTrees.some((t) => name === t || name.startsWith(t + "/"))
    )
      continue;
    if (entry.isSymbolicLink()) throw Error(`Source alias: ${name}`);
    if (entry.isDirectory()) walk(root, name, result);
    else result.push(name);
  }
}
export function sourceFiles(root = ROOT): SourceFile[] {
  const names = files.filter((n) => existsSync(join(root, n)));
  for (const d of directories)
    if (existsSync(join(root, d))) {
      if (lstatSync(join(root, d)).isSymbolicLink())
        throw Error("Source alias: " + d);
      walk(root, d, names);
    }
  const compare = (a: string, b: string) => {
    const aa = a.split("/"),
      bb = b.split("/");
    for (let i = 0; i < Math.min(aa.length, bb.length); i++) {
      if (aa[i] !== bb[i]) return aa[i]! < bb[i]! ? -1 : 1;
    }
    return aa.length - bb.length;
  };
  return [...new Set(names)]
    .sort(compare)
    .filter(
      (name) =>
        !privateMetadata(name) &&
        !omittedNames.has(basename(name)) &&
        !omittedSuffixes.has(extname(name)) &&
        !name.split("/").some((p) => omitted.has(p)) &&
        !basename(name).startsWith("._") &&
        (!basename(name).startsWith(".env") || name === ".env.example") &&
        !/^scripts\/verify-.*\.ts$/.test(name),
    )
    .map((path) => {
      const full = join(root, path),
        stat = lstatSync(full);
      if (
        !stat.isFile() ||
        stat.isSymbolicLink() ||
        stat.size > 16 * 1024 * 1024
      )
        throw Error(`Invalid source: ${path}`);
      const data = readFileSync(full);
      if (secret.test(data.toString()))
        throw Error(`Credential in source: ${path}`);
      return { path, data, mode: stat.mode & 0o111 ? 0o755 : 0o644 };
    });
}
export function records(selected: SourceFile[]) {
  return selected.map(({ path, data }) => ({
    path,
    size: data.length,
    sha256: hash(data),
  }));
}
export function sourceManifest(
  root = ROOT,
  selected = sourceFiles(root),
  draft = false,
) {
  const policy = JSON.parse(
    readFileSync(join(root, "config/release-policy.json"), "utf8"),
  );
  const preview = policy.source_preview ?? {},
    review = preview.remote_review_authorized === true;
  return {
    format: 1,
    version: policy.version ?? null,
    license: preview.license ?? null,
    remote_publication_authorized:
      !draft && preview.remote_publication_authorized === true,
    remote_review_authorized: review,
    kind: draft
      ? review
        ? "private-source-review-draft"
        : "local-review-draft"
      : "source-preview",
    product_ready: Object.values(policy.publication_gates).every(
      (v) => v === true,
    ),
    product_gates: policy.publication_gates,
    files: records(selected),
  };
}
export function checkSource(
  root = ROOT,
  { repository = false, manifest = false, draft = false } = {},
) {
  const selected = sourceFiles(root),
    names = new Set(selected.map((f) => f.path));
  const provenance = join(root, "config/source-assets.json");
  if (existsSync(provenance)) {
    for (const asset of JSON.parse(readFileSync(provenance, "utf8")).assets) {
      const refs = [
        ...Object.entries(asset.sha256 || {}),
        ...(asset.references || []).map(
          (r: { license_file: string; license_sha256: string }) => [
            r.license_file,
            r.license_sha256,
          ],
        ),
      ];
      for (const [name, expected] of refs)
        if (
          !names.has(String(name)) ||
          hash(readFileSync(join(root, String(name)))) !== expected
        )
          throw Error("Source asset provenance mismatch: " + name);
    }
  }
  for (const file of [
    "native-notice-supplements.json",
    "rust-runtime-notices.json",
  ]) {
    const path = join(root, "config", file);
    if (existsSync(path))
      for (const pkg of JSON.parse(readFileSync(path, "utf8")).packages)
        for (const item of pkg.files)
          if (
            !names.has(item.file) ||
            hash(readFileSync(join(root, item.file))) !== item.sha256
          )
            throw Error("Upstream notice mismatch: " + item.file);
  }
  for (const required of [
    "README.md",
    "README.en.md",
    "SECURITY.md",
    "DISCLAIMER.md",
    "PRIVACY.md",
    "THIRD_PARTY_NOTICES.md",
    ".github/workflows/source.yml",
    ...(draft ? [] : ["LICENSE"]),
  ])
    if (!names.has(required))
      throw Error(`Required source missing: ${required}`);
  for (const f of selected) {
    if (f.path === "scripts/tooling/source.ts") continue;
    if (
      /\/Users\/[A-Za-z0-9._-]+\//.test(f.data.toString()) ||
      /https?:\/\/(?:172\.(?:1[6-9]|2[0-9]|3[01])\.|192\.168\.)/.test(
        f.data.toString(),
      )
    )
      throw Error(`Private origin in source: ${f.path}`);
  }
  if (repository) {
    const indexed = output(
      [
        "git",
        "--no-optional-locks",
        "-c",
        "core.fsmonitor=false",
        "ls-files",
        "-z",
      ],
      { cwd: root },
    )
      .split("\0")
      .filter(Boolean);
    for (const name of indexed)
      if (name !== "SOURCE-MANIFEST.json" && !names.has(name))
        throw Error(`Outside source allowlist: ${name}`);
  }
  if (
    manifest ||
    (repository && existsSync(join(root, "SOURCE-MANIFEST.json")))
  ) {
    const path = join(root, "SOURCE-MANIFEST.json");
    if (lstatSync(path).isSymbolicLink())
      throw Error("Source manifest must be a regular file");
    const value = JSON.parse(
      readFileSync(join(root, "SOURCE-MANIFEST.json"), "utf8"),
    );
    if (
      value.format !== 1 ||
      JSON.stringify(value.files) !== JSON.stringify(records(selected))
    )
      throw Error("Source manifest mismatch");
  }
  return {
    files: selected.length,
    private_state_included: false,
    license_present: true,
    source_scan_passed: true,
  };
}
export function refreshManifest(root = ROOT) {
  if (
    existsSync(join(root, "SOURCE-MANIFEST.json")) &&
    lstatSync(join(root, "SOURCE-MANIFEST.json")).isSymbolicLink()
  )
    throw Error("Source manifest must be a regular file");
  const selected = sourceFiles(root);
  writeFileSync(
    join(root, "SOURCE-MANIFEST.json"),
    JSON.stringify(sourceManifest(root, selected), null, 2) + "\n",
  );
  return { source_files: selected.length };
}
function tar(files: SourceFile[], prefix: string) {
  const blocks: Buffer[] = [];
  for (const file of files) {
    const name = `${prefix}/${file.path}`;
    const header = Buffer.alloc(512);
    const split = name.lastIndexOf("/");
    if (Buffer.byteLength(name) > 100) {
      if (
        Buffer.byteLength(name.slice(0, split)) > 155 ||
        Buffer.byteLength(name.slice(split + 1)) > 100
      )
        throw Error("Tar path too long");
      header.write(name.slice(0, split), 345, 155);
      header.write(name.slice(split + 1), 0, 100);
    } else header.write(name, 0, 100);
    const octal = (value: number, offset: number, length: number) =>
      header.write(
        value.toString(8).padStart(length - 1, "0") + "\0",
        offset,
        length,
      );
    octal(file.mode, 100, 8);
    octal(0, 108, 8);
    octal(0, 116, 8);
    octal(file.data.length, 124, 12);
    octal(0, 136, 12);
    header.fill(32, 148, 156);
    header[156] = 48;
    header.write("ustar\0", 257, 6);
    header.write("00", 263, 2);
    octal(
      header.reduce((a, b) => a + b, 0),
      148,
      8,
    );
    blocks.push(
      header,
      file.data,
      Buffer.alloc((512 - (file.data.length % 512)) % 512),
    );
  }
  blocks.push(Buffer.alloc(1024));
  return Buffer.concat(blocks);
}
export function createSourcePackage(
  root: string,
  destination: string,
  publicRelease = false,
) {
  const policy = JSON.parse(
      readFileSync(join(root, "config/release-policy.json"), "utf8"),
    ),
    blocked = Object.entries(policy.publication_gates)
      .filter(([, passed]) => passed !== true)
      .map(([name]) => name);
  if (publicRelease && blocked.length)
    throw Error(`Publication blocked: ${blocked.join(", ")}`);
  const selected = sourceFiles(root),
    manifest = {
      version: policy.version,
      license: policy.source_preview.license,
      remote_publication_authorized: false,
      kind: publicRelease ? "public-source-preview" : "internal-source-preview",
      publication_ready: blocked.length === 0,
      blocked_gates: blocked,
      files: records(selected),
    };
  selected.push({
    path: "RELEASE-MANIFEST.json",
    data: Buffer.from(JSON.stringify(manifest, null, 2) + "\n"),
    mode: 0o644,
  });
  if (blocked.length)
    selected.push({
      path: "NOT-FOR-PUBLICATION.txt",
      data: Buffer.from(
        "Internal review bundle. See RELEASE-MANIFEST.json. No user state included.\n",
      ),
      mode: 0o644,
    });
  mkdirSync(destination, { recursive: true });
  const file = join(destination, `ai-phone-${policy.version}-source.tar.gz`);
  writeFileSync(
    file,
    Bun.gzipSync(tar(selected, `ai-phone-${policy.version}`)),
  );
  writeFileSync(
    file + ".sha256",
    hash(readFileSync(file)) + "  " + basename(file) + "\n",
  );
  writeFileSync(
    file + ".manifest.json",
    JSON.stringify(manifest, null, 2) + "\n",
  );
  return file;
}
export function exportSource(
  root: string,
  destination: string,
  { draft = false, git = false } = {},
) {
  root = resolve(root);
  destination = resolve(destination);
  if (existsSync(destination)) throw Error("Source export already exists");
  if (
    destination.startsWith(root + "/") &&
    !destination.startsWith(join(root, "dist") + "/") &&
    !destination.startsWith(join(root, "data/backups") + "/")
  )
    throw Error("Export outside source or into dist");
  const selected = sourceFiles(root);
  checkSource(root, { draft });
  mkdirSync(dirname(destination), { recursive: true });
  const temporary = mkdtempSync(join(dirname(destination), ".source-export-")),
    work = join(temporary, "source");
  try {
    mkdirSync(work);
    for (const f of selected) {
      const path = join(work, f.path);
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, f.data, { mode: f.mode });
    }
    const value = sourceManifest(root, selected, draft);
    writeFileSync(
      join(work, "SOURCE-MANIFEST.json"),
      JSON.stringify(value, null, 2) + "\n",
    );
    const marker = value.remote_review_authorized
      ? "REVIEW-ONLY.txt"
      : "LOCAL-REVIEW-ONLY.txt";
    if (draft)
      writeFileSync(
        join(work, marker),
        "Review candidate. Not a product release. See SOURCE-MANIFEST.json.\n",
      );
    if (git) {
      run(["git", "init", "--initial-branch=master"], { cwd: work });
      run(
        [
          "git",
          "add",
          "--",
          ...selected.map((f) => f.path),
          "SOURCE-MANIFEST.json",
          ...(draft ? [marker] : []),
        ],
        { cwd: work },
      );
      run(["git", "commit", "-m", "Initial AI Phone source snapshot"], {
        cwd: work,
      });
    }
    renameSync(work, destination);
    return {
      destination,
      files: selected.length,
      git_initialized: git,
      remote_created: false,
      draft,
    };
  } finally {
    rmSync(temporary, { recursive: true, force: true });
  }
}
