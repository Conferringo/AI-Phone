import { ROOT } from "./tooling/process";
import {
  checkSource,
  refreshManifest,
  createSourcePackage,
  exportSource,
} from "./tooling/source";
const [operation, ...args] = process.argv.slice(2);
switch (operation) {
  case "check":
    console.log(
      JSON.stringify(
        checkSource(ROOT, {
          repository: args.includes("--repository"),
          manifest: args.includes("--manifest"),
        }),
      ),
    );
    break;
  case "manifest":
    console.log(JSON.stringify(refreshManifest()));
    break;
  case "package":
    console.log(createSourcePackage(ROOT, args[0]!, args.includes("--public")));
    break;
  case "export":
    console.log(
      JSON.stringify(
        exportSource(ROOT, args[0]!, {
          draft: args.includes("--draft"),
          git: args.includes("--git"),
        }),
      ),
    );
    break;
  default:
    throw Error("Expected source check|manifest|package|export");
}
