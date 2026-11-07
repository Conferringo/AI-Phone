import { ROOT } from "./tooling/process";
import { macosFactory } from "./tooling/macos-factory";
const args = process.argv.slice(2),
  option = (name: string) =>
    args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
if (!option("--compiler-sdk")) throw Error("--compiler-sdk required");
if (!!option("--image-candidate") !== !!option("--image-metadata-sha256"))
  throw Error("Both image candidate and identity required");
console.log(
  JSON.stringify(
    await macosFactory(
      ROOT,
      option("--compiler-sdk")!,
      option("--image-candidate"),
      option("--image-metadata-sha256"),
    ),
  ),
);
