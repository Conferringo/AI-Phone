import { mkdirSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { ROOT, digest } from "./tooling/process";
import { save } from "./tooling/inventory";
import {
  dependencyInventory,
  rustNotices,
} from "./tooling/dependency-inventory";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!,
  result: any = await dependencyInventory();
if (args.includes("--client-notices-output")) {
  const value = await rustNotices(
      ROOT,
      "apps/client/src-tauri/Cargo.toml",
      "native client",
    ),
    file = option("--client-notices-output");
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, value.text);
  result.native_client_missing_notice_files = value.missing;
}
if (args.includes("--service-notices-output")) {
  const value = await rustNotices(ROOT, "Cargo.toml", "native services"),
    file = option("--service-notices-output");
  mkdirSync(dirname(file), { recursive: true });
  writeFileSync(file, value.text);
  save(option("--service-inventory-output"), {
    format: 1,
    kind: "native-service-licenses",
    scope:
      "Locked native service build graph, including target and build dependencies",
    lock_sha256: await digest("Cargo.lock"),
    packages: value.packages,
    notices_file: file,
    notices_sha256: await digest(file),
    missing_notice_files: value.missing,
  });
}
save(option("--output"), result);
console.log(
  JSON.stringify({
    rust: result.rust.length,
    javascript: result.javascript.length,
    missing_license_metadata: result.missing_license_metadata,
  }),
);
