import { ROOT } from "./tooling/process";
import { buildDependency } from "./tooling/linux-dependencies";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!;
console.log(
  JSON.stringify(
    await buildDependency(
      ROOT,
      option("--archive"),
      option("--destination"),
      option("--role"),
    ),
  ),
);
