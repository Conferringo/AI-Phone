import { audit, writeAudit } from "./tooling/dependency-audit";
const args = process.argv.slice(2),
  file = args[args.indexOf("--output") + 1];
if (!file) throw Error("--output required");
const result = audit();
writeAudit(result, file);
const target = args.includes("--target")
  ? args[args.indexOf("--target") + 1]
  : undefined;
if (
  !result.javascript_passed ||
  (target && result.blocked_targets[target]?.length)
)
  process.exitCode = 1;
