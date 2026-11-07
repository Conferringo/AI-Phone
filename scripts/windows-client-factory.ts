import { ROOT } from "./tooling/process";
import { json } from "./tooling/inventory";
import { stageWindowsBackend, windowsFactory } from "./tooling/windows-factory";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!,
  root = args.includes("--root") ? option("--root") : ROOT;
const result =
  args[0] === "stage-backend"
    ? await stageWindowsBackend(
        root,
        json(option("--report")),
        option("--backend"),
      )
    : args[0] === "build"
      ? await windowsFactory(root, option("--backend"), option("--identity"))
      : (() => {
          throw Error("Expected stage-backend or build");
        })();
console.log(JSON.stringify(result));
