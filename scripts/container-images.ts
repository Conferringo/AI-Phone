import { ROOT } from "./tooling/process";
import { json } from "./tooling/inventory";
import { buildImages } from "./tooling/image-capsule";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!;
console.log(
  JSON.stringify(
    await buildImages(ROOT, json(option("--sources")), option("--output")),
  ),
);
