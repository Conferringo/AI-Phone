import { resolve } from "node:path";
import { ROOT } from "./tooling/process";
import { dependencyContext } from "./tooling/factory-inputs";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!,
  role = option("--role"),
  folder = resolve(option("--work"));
if (!["linux-host", "linux-ocr"].includes(role))
  throw Error("--role must be linux-host or linux-ocr");
await dependencyContext(ROOT, folder, role);
