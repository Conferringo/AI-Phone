import { buildWindowsGuestDisk, buildWindowsGuestKernel } from "./tooling/windows-guest-disk";

const args = process.argv.slice(2);
if (args[0] === "kernel" && args.length === 2) {
  console.log(JSON.stringify(await buildWindowsGuestKernel(args[1]!)));
} else if (args.length === 6) {
  const [capsule, identity, kernel, modules, receipt, destination] = args;
  console.log(JSON.stringify(await buildWindowsGuestDisk(
    capsule!, identity!, kernel!, modules!, receipt!, destination!,
  )));
} else {
  throw Error(
    "Usage: scripts/windows-guest-disk.ts kernel <output-dir> or <sealed-backend-with-qemu-broker.payload> <capsule-sha256> <bzImage> <modules.tar.gz> <kernel-receipt.json> <output.payload>",
  );
}
