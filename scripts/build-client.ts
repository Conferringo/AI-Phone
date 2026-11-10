import { join, resolve } from "node:path";
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
const platform = (option("--platform") || "macos") as Platform,
  root = option("--root") ? resolve(option("--root")!) : ROOT;
if (!["macos", "windows"].includes(platform))
  throw Error("--platform macos|windows is required");
const packageOnly = args.includes("--package-only");
const developmentDmg = args.includes("--development-dmg");
if (packageOnly && platform === "macos")
  throw Error("macOS requires a current application build");
if (developmentDmg && platform !== "macos")
  throw Error("--development-dmg requires macOS");
checkSource(root);
noticeManifest(root);
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
    windowsVm: option("--qemu") ? {
      qemu: option("--qemu")!,
      qemuIdentity: option("--qemu-identity")!,
      guest: option("--guest")!,
      guestIdentity: option("--guest-identity")!,
    } : undefined,
  };
  if (options.web) run([bunProgram(root), "run", "build"], { cwd: root });
  if (options.nativeServices) await buildNative(root);
  await preparePayload(root, options);
}
console.log(JSON.stringify(await buildClient(platform, root, packageOnly, developmentDmg)));
