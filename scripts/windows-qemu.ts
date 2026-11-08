import { buildWindowsQemu } from "./tooling/windows-qemu";

const [destination, archive, ...rest] = process.argv.slice(2);
if (!destination || rest.length)
  throw Error(
    "Usage: scripts/windows-qemu.ts <new-output-dir> [qemu-11.1.3.tar.xz]",
  );

console.log(JSON.stringify(await buildWindowsQemu(destination, archive)));
