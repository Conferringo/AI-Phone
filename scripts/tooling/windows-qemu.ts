import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  renameSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { basename, dirname, join, relative, resolve } from "node:path";
import { save } from "./inventory";
import { digest, download, output, run } from "./process";

const version = "11.1.3";
const archiveName = `qemu-${version}.tar.xz`;
const sourceUrl = `https://download.qemu.org/${archiveName}`;
const sourceDigest =
  "5a8289c7940dc77d3873c81a6c29ba3afe1446fc9d36e77fa1658cdbecb84b22";
const upstreamRecipe =
  "https://github.com/msys2/MINGW-packages/blob/ea64639f4a27f7ce9187b13fdc7f14a50c4637b9/mingw-w64-qemu/PKGBUILD";
const packagePrefix = "mingw-w64-ucrt-x86_64-";
const packages = [
  "tar",
  "xz",
  ...[
    "gcc", "binutils", "pkgconf", "meson", "ninja", "python", "python-pip",
    "python-setuptools", "python-wheel", "glib2", "libslirp", "zlib",
  ].map((name) => packagePrefix + name),
];
const executables = ["qemu-system-x86_64.exe", "qemu-img.exe"];
const firmware = ["bios-256k.bin", "linuxboot_dma.bin"];
const configure = [
  "--target-list=x86_64-softmmu",
  "--without-default-features",
  "--enable-whpx",
  "--enable-slirp",
  "--enable-tools",
  "--disable-download",
  "--cc=gcc",
  "--cxx=g++",
  "--python=python",
  "--ninja=ninja",
];

export async function buildWindowsQemu(destination: string, archive?: string) {
  if (
    process.platform !== "win32" ||
    process.arch !== "x64" ||
    process.env.MSYSTEM !== "UCRT64"
  )
    throw Error("Build QEMU on Windows x64 from an MSYS2 UCRT64 shell");
  destination = resolve(destination);
  if (existsSync(destination)) throw Error("Use a new QEMU output directory");
  mkdirSync(dirname(destination), { recursive: true });

  const msys = output(["cygpath.exe", "-aw", "/"]);
  const pacman = join(msys, "usr/bin/pacman.exe");
  const objdump = join(msys, "ucrt64/bin/objdump.exe");
  const installedPackages = output([pacman, "-Q", ...packages]);
  const work = mkdtempSync(join(dirname(destination), ".qemu-build-"));
  try {
    const input = archive ? resolve(archive) : join(work, archiveName);
    if (!archive) await download(sourceUrl, input);
    if (statSync(input).size !== 141819712 || (await digest(input)) !== sourceDigest)
      throw Error("QEMU source archive differs from the pinned official release");

    const build = join(work, "build");
    run(
      [join(msys, "usr/bin/bash.exe"), "--noprofile", "--norc", "-ec", `
          export PATH=/ucrt64/bin:/usr/bin
          tar -xf "$(cygpath -u "$1")"
          shift
          mkdir build
          cd build
          ../qemu-${version}/configure "$@"
          ninja ${executables.join(" ")}
          strip ${executables.join(" ")}
        `, "qemu-factory", input, ...configure],
      { cwd: work, env: { ...process.env, MSYS: "winsymlinks:nativestrict" } },
    );

    const source = join(work, `qemu-${version}`);
    const stage = join(work, "output");
    const runtime = join(stage, "runtime");
    const sourceOutput = join(stage, "source");
    const licenses = join(stage, "licenses");
    const files = new Set<string>();
    const keep = (file: string, dest: string) => {
      mkdirSync(dirname(dest), { recursive: true });
      copyFileSync(file, dest);
      files.add(dest);
    };
    for (const name of executables) keep(join(build, name), join(runtime, name));
    for (const name of firmware)
      keep(join(source, "pc-bios", name), join(runtime, "share", name));
    for (const name of [
      "LICENSE", "COPYING", "COPYING.LIB", "pc-bios/README",
      "roms/seabios/COPYING", "roms/seabios/COPYING.LESSER",
    ])
      keep(join(source, name), join(licenses, "qemu", name));
    keep(input, join(sourceOutput, archiveName));

    const pending = [...executables];
    const dependencies = new Set<string>(executables);
    const system = new Set<string>();
    const imports: Record<string, string[]> = {};
    const providers = new Map<string, string>();
    const system32 = join(process.env.SystemRoot!, "System32");
    for (const binary of pending) {
      const info = output([objdump, "-p", join(runtime, binary)]);
      if (!info.includes("file format pei-x86-64"))
        throw Error("Expected x64 Windows binary: " + binary);
      const names = [...new Set([...info.matchAll(/DLL Name:\s*([\w.-]+\.dll)\s*$/gim)]
        .map((entry) => entry[1]!.toLowerCase()))].sort();
      imports[binary] = names;
      for (const name of names) {
        if (dependencies.has(name)) continue;
        dependencies.add(name);
        if (
          name === "winhvplatform.dll" || /^(?:api|ext)-ms-win-/.test(name) ||
          existsSync(join(system32, name))
        ) {
          system.add(name);
          continue;
        }
        const provider = output([pacman, "-Qoq", "/ucrt64/bin/" + name]);
        if (!provider.startsWith(packagePrefix))
          throw Error("Unexpected QEMU DLL provider: " + provider);
        providers.set(name, provider);
        keep(join(msys, "ucrt64/bin", name), join(runtime, name));
        pending.push(name);
      }
    }

    const runtimePackages = [...new Set(providers.values())].sort().map((name) => {
      const installed = output([pacman, "-Q", name]);
      const notices = output([pacman, "-Qlq", name]).split(/\r?\n/)
        .filter((file) => file.startsWith("/ucrt64/share/licenses/") && !file.endsWith("/"));
      for (const file of notices)
        keep(join(msys, file.slice(1)), join(licenses, name, basename(file)));
      return {
        name,
        version: installed.slice(name.length + 1),
        package: `https://packages.msys2.org/packages/${name}`,
        notices: notices.map((file) => basename(file)),
      };
    });
    writeFileSync(join(sourceOutput, "configure.args"), configure.join("\n") + "\n");
    writeFileSync(join(sourceOutput, "msys2-packages.txt"), installedPackages + "\n");
    for (const name of ["configure.args", "msys2-packages.txt"]) files.add(join(sourceOutput, name));

    const manifest = {
      format: 1,
      component: "windows-qemu-whpx",
      version,
      source: {
        url: sourceUrl,
        sha256: sourceDigest,
        upstream_msys2_recipe: upstreamRecipe,
        seabios_commit: "b52ca86e094d19b58e2304417787e96b940e39c6",
      },
      system_imports: [...system].sort(),
      host_feature: "Windows Hypervisor Platform",
      files: await Promise.all([...files].sort().map(async (file) => ({
        path: relative(stage, file).replaceAll("\\", "/"),
        size: statSync(file).size,
        sha256: await digest(file),
      }))),
      imports,
      dll_providers: Object.fromEntries(providers),
      runtime_packages: runtimePackages,
      redistribution_review_complete: false,
    };
    save(join(stage, "windows-qemu.json"), manifest);
    renameSync(stage, destination);
    return manifest;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}
