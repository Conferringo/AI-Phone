import {
  cpSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
} from "node:fs";
import { join } from "node:path";
import {
  nativeClosure,
  packageRecords,
  archiveDependency,
} from "../tooling/linux-dependencies";
const root = "/out";
mkdirSync(root);
const programs = Object.fromEntries(
  ["qemu-system-x86_64", "qemu-img", "ssh", "ssh-keygen", "scp"].map((name) => [
    name,
    "/usr/bin/" + name,
  ]),
);
const modules = readdirSync("/usr/lib/x86_64-linux-gnu/qemu")
  .filter((name) => name.endsWith(".so"))
  .map((name) => join("/usr/lib/x86_64-linux-gnu/qemu", name));
const files = nativeClosure(root, programs, modules);
for (const name of ["qemu", "seabios"])
  cpSync("/usr/share/" + name, join(root, "share", name), {
    recursive: true,
    dereference: true,
  });
for (const folder of ["/usr/share/seabios", "/usr/lib/ipxe/qemu"])
  for (const name of readdirSync(folder, { withFileTypes: true }))
    if (name.isFile())
      copyFileSync(
        join(folder, name.name),
        join(root, "share/qemu", name.name),
      );
copyFileSync(
  "/usr/share/OVMF/OVMF_CODE_4M.fd",
  join(root, "share/qemu/edk2-x86_64-code.fd"),
);
packageRecords(root, files);
mkdirSync("/export");
await archiveDependency(root, "/export/linux-x64-host.tar.gz");
