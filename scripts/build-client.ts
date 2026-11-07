import { join } from "node:path";
import { ROOT, run, output, bunProgram } from "./tooling/process";
import { checkSource } from "./tooling/source";
import { preparePayload, type PayloadOptions } from "./tooling/client-payload";
import { buildNative } from "./tooling/native-build";
import {
  buildClient,
  noticeManifest,
  type Platform,
} from "./tooling/client-build";

const args = process.argv.slice(2),
  option = (name: string) =>
    args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
const platform = (option("--platform") || "macos") as Platform;
if (!["macos", "windows"].includes(platform))
  throw Error("--platform macos|windows is required");
const packageOnly = args.includes("--package-only");
if (packageOnly && platform === "macos")
  throw Error("macOS requires a current application build");
checkSource(ROOT);
noticeManifest();
if (!packageOnly) {
  if (output(["rustc", "--version"]).split(" ")[1] !== "1.96.0")
    throw Error("Rust 1.96.0 is required");
  const options: PayloadOptions = {
    web: args.includes("--with-web-runtime"),
    nativeServices: args.includes("--with-service-runtime"),
    phoneApps: args.includes("--with-phone-apps"),
    containerHost: args.includes("--with-container-host"),
    guestApps: args.includes("--with-guest-apps"),
    androidRuntime: option("--android-runtime"),
    imageRuntime: option("--images-runtime"),
    linuxOcr: option("--linux-ocr"),
    wslRuntime: option("--wsl-runtime"),
  };
  if (options.web) run([bunProgram(ROOT), "run", "build"], { cwd: ROOT });
  if (options.nativeServices) await buildNative();
  await preparePayload(ROOT, options);
}
console.log(JSON.stringify(await buildClient(platform, ROOT, packageOnly)));
