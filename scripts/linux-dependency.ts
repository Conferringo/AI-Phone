import { ROOT } from "./tooling/process";
import { buildFactoryDependency } from "./tooling/factory-inputs";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!;
console.log(
  JSON.stringify(
    await buildFactoryDependency(
      ROOT,
      option("--work"),
      option("--destination"),
      option("--role"),
    ),
  ),
);
