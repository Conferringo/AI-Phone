import { ROOT } from "./tooling/process";
import { json } from "./tooling/inventory";
import { stageWindowsBackend, windowsFactory } from "./tooling/windows-factory";
const args = process.argv.slice(2),
  option = (name: string) => {
    const offset = args.indexOf(name), value = args[offset + 1];
    if (offset === -1 || !value || value.startsWith("--"))
      throw Error(name + " is required");
    return value;
  },
  root = args.includes("--root") ? option("--root") : ROOT;
const result =
  args[0] === "stage-backend"
    ? await stageWindowsBackend(
        root,
        json(option("--report")),
        option("--backend"),
      )
    : args[0] === "build"
      ? await windowsFactory(root, {
          runtime: "wsl",
          folder: option("--backend"),
          identity: option("--identity"),
          backendSource: args.includes("--backend-source") ? option("--backend-source") : root,
        }, args.includes("--sign-release"))
      : args[0] === "build-qemu"
        ? await windowsFactory(root, {
            runtime: "qemu",
            vm: {
              qemu: option("--qemu"),
              qemuIdentity: option("--qemu-identity"),
              guest: option("--guest"),
              guestIdentity: option("--guest-identity"),
            },
          }, args.includes("--sign-release"))
        : (() => {
            throw Error("Expected stage-backend, build or build-qemu");
          })();
console.log(JSON.stringify(result));
