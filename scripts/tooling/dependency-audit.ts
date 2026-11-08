import { readFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { spawnSync } from "node:child_process";
import { ROOT, hash, output, bunProgram } from "./process";
import { save } from "./inventory";

export const targets = [
  "aarch64-apple-darwin",
  "aarch64-unknown-linux-gnu",
  "x86_64-unknown-linux-gnu",
  "x86_64-pc-windows-msvc",
];
type Finding = {
  package: string;
  advisory: string;
  category: string;
  title: string;
  url: string;
  active_targets: string[];
  blocking_when_active: boolean;
  mitigation?: unknown;
};
export function findings(
  report: any,
  reachable: Record<string, Set<string>>,
): Finding[] {
  return Object.entries({
    vulnerability: report.vulnerabilities?.list || [],
    ...report.warnings,
  }).flatMap(([category, rows]) =>
    (rows as any[]).map((row) => ({
      package: row.package.name + "@" + row.package.version,
      advisory: row.advisory.id,
      category,
      title: row.advisory.title,
      url: row.advisory.url,
      active_targets: Object.keys(reachable).filter((target) =>
        reachable[target]!.has(row.package.name + "@" + row.package.version),
      ),
      blocking_when_active: ["vulnerability", "unsound"].includes(category),
    })),
  );
}
function auditCommand(args: string[], root: string) {
  const result = spawnSync(args[0]!, args.slice(1), {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
  if (result.error) throw result.error;
  if (result.status !== 0 && result.status !== 1)
    throw Error("Dependency audit did not complete");
  return { passed: result.status === 0, value: JSON.parse(result.stdout) };
}
export function audit(root = ROOT) {
  const rust: Record<string, any> = {};
  for (const [name, manifest] of [
    ["server", "Cargo.toml"],
    ["client", "apps/client/src-tauri/Cargo.toml"],
  ]) {
    const lock = join(dirname(manifest!), "Cargo.lock"),
      report = auditCommand(
        ["cargo", "audit", "--file", lock, "--json"],
        root,
      ).value;
    if (!report.vulnerabilities || !report.database?.["last-commit"])
      throw Error("Advisory database missing");
    const reachable: Record<string, Set<string>> = {};
    for (const target of name === "client"
      ? targets.filter((t) => !t.includes("linux"))
      : targets) {
      const tree = output(
        [
          "cargo",
          "tree",
          "--manifest-path",
          manifest!,
          "--locked",
          "--target",
          target,
          "--prefix",
          "none",
          "--format",
          "{p}",
        ],
        { cwd: root },
      );
      reachable[target] = new Set(
        tree
          .split("\n")
          .map((row) => row.match(/^(\S+) v(\S+)/))
          .filter(Boolean)
          .map((row) => row![1] + "@" + row![2]),
      );
    }
    const rows = findings(report, reachable);
    rust[name!] = {
      lock_sha256: hash(readFileSync(join(root, lock))),
      database: report.database,
      findings: rows,
    };
  }
  const javascript = auditCommand([bunProgram(root), "audit", "--json"], root);
  return {
    format: 1,
    scope:
      "Locked dependency advisories and selected build graphs; not platform runtime acceptance",
    rust,
    javascript: javascript.value,
    javascript_passed: javascript.passed,
    blocked_targets: Object.fromEntries(
      targets.map((target) => [
        target,
        Object.entries(rust).flatMap(([name, part]) =>
          part.findings
            .filter(
              (row: Finding) =>
                row.blocking_when_active && row.active_targets.includes(target),
            )
            .map((row: Finding) => name + ":" + row.advisory),
        ),
      ]),
    ),
    blanket_ignores: [],
  };
}
const packagePattern =
  /^(?:@[a-z0-9._-]+\/)?[a-z0-9._-]+(?:@[0-9][a-zA-Z0-9.+_-]*)?$/;
const advisoryPattern =
  /^(?:RUSTSEC-\d{4}-\d{4}|GHSA-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}|NPM-\d{1,12})$/;
function identifier(value: unknown, pattern: RegExp) {
  return typeof value === "string" && value.length <= 256 && pattern.test(value)
    ? value
    : "unknown";
}
export function sanitizedReport(result: ReturnType<typeof audit>) {
  const rust = Object.fromEntries(
    Object.entries(result.rust).map(([name, part]) => [
      name,
      {
        lock_sha256: part.lock_sha256,
        database_commit: part.database["last-commit"],
        findings: part.findings.map((row: Finding) => ({
          package: identifier(row.package, packagePattern),
          advisory: identifier(row.advisory, advisoryPattern),
          category: [
            "vulnerability",
            "unsound",
            "unmaintained",
            "yanked",
            "notice",
          ].includes(row.category)
            ? row.category
            : "unknown",
          active_targets: targets.filter((t) => row.active_targets.includes(t)),
          blocking_when_active: row.blocking_when_active,
          ...(row.mitigation
            ? { mitigation: "verified-upstream-backport" }
            : {}),
        })),
      },
    ]),
  );
  const javascript = Object.entries(result.javascript).flatMap(
    ([name, items]) =>
      (items as any[]).map((row) => ({
        package: identifier(name, packagePattern),
        advisory: identifier(
          row.url?.match(
            /^https:\/\/github\.com\/advisories\/(GHSA-[a-z0-9-]+)/,
          )?.[1] || (Number.isSafeInteger(row.id) ? "NPM-" + row.id : ""),
          advisoryPattern,
        ),
        severity: ["info", "low", "moderate", "high", "critical"].includes(
          row.severity,
        )
          ? row.severity
          : "unknown",
      })),
  );
  return {
    format: 2,
    completed: true,
    javascript_passed: result.javascript_passed,
    blocked_targets: result.blocked_targets,
    rust,
    javascript,
    blanket_ignores: [],
  };
}
export function writeAudit(result: ReturnType<typeof audit>, file: string) {
  const report = sanitizedReport(result);
  save(file, report);
  console.log(
    JSON.stringify({
      javascript_passed: report.javascript_passed,
      blocked_targets: report.blocked_targets,
    }),
  );
}
