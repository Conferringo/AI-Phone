import {
  copyFileSync,
  createWriteStream,
  existsSync,
  linkSync,
  lstatSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  realpathSync,
  renameSync,
  rmSync,
  statfsSync,
  symlinkSync,
  truncateSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve, sep } from "node:path";
import { createHash } from "node:crypto";
import { availableParallelism } from "node:os";
import { Readable } from "node:stream";
import { pipeline } from "node:stream/promises";
import { tarEntries, writeTar } from "./archive";
import { canonical, json, save } from "./inventory";
import { digest, hash, output, ROOT, run } from "./process";
import { qemuBootstrapContract, verifyWsl, wslBody } from "./wsl-capsule";

const gib = 1024 ** 3;
const sha256 = /^[a-f0-9]{64}$/;
const busybox = "/opt/aiphone/engine/bin/busybox";
const guestBroker = "/usr/local/libexec/aiphone-bootstrap --qemu-guest-control";
const kernelArgs = "root=/dev/vda rw net.ifnames=0";
const linuxRepository = "https://github.com/microsoft/WSL2-Linux-Kernel";
const linuxCommit = "d504d40ab83839cb61942a812a99d183c42227b9";

type KernelReceipt = {
  source: string;
  source_commit: string;
  kernel_release: string;
  kernel_sha256: string;
  config_sha256: string;
  license_sha256: string;
  modules_sha256: string;
};

export async function buildWindowsGuestKernel(destination: string): Promise<KernelReceipt> {
  if (process.platform !== "linux" || !["x64", "arm64"].includes(process.arch))
    throw Error("Guest kernel assembly requires a Linux x64 or ARM64 factory");
  destination = resolve(destination);
  if (existsSync(destination)) throw Error("Use a new guest kernel destination");
  const work = mkdtempSync(join(realpathSync(dirname(destination)), ".guest-kernel-"));
  try {
    const source = join(work, "linux"), build = join(work, "build");
    const outputDirectory = join(work, "output"), moduleRoot = join(work, "modules");
    for (const folder of [source, build, outputDirectory, moduleRoot]) mkdirSync(folder);
    run(["git", "init", "-q", source]);
    run(["git", "-C", source, "fetch", "--depth=1", linuxRepository, linuxCommit]);
    run(["git", "-C", source, "checkout", "--detach", "FETCH_HEAD"]);
    if (output(["git", "-C", source, "rev-parse", "HEAD"]) !== linuxCommit)
      throw Error("Guest kernel source commit differs");
    const config = join(build, ".config");
    copyFileSync(join(source, "Microsoft/config-wsl"), config);
    const configTool = join(source, "scripts/config");
    run([
      configTool, "--file", config, "--enable", "VIRTIO_BLK", "--enable", "FW_CFG_SYSFS",
      "--disable", "DEBUG_INFO_BTF", "--disable", "DEBUG_INFO_DWARF_TOOLCHAIN_DEFAULT",
      "--enable", "DEBUG_INFO_NONE",
      "--set-str", "LOCALVERSION", "-aiphone-vm",
    ]);
    const make = [
      "make", "-C", source, "O=" + build, "ARCH=x86_64",
      ...(process.arch === "arm64" ? ["CROSS_COMPILE=x86_64-linux-gnu-"] : []),
    ];
    run([...make, "olddefconfig"]);
    for (const name of [
      "X86_64", "DEVTMPFS_MOUNT", "EXT4_FS", "VIRTIO_PCI", "VIRTIO_BLK", "VIRTIO_NET",
      "VIRTIO_CONSOLE", "FW_CFG_SYSFS",
    ]) {
      if (output([configTool, "--file", config, "--state", name]) !== "y")
        throw Error("Guest kernel must build in " + name);
    }
    run([...make, "-j" + availableParallelism(), "bzImage", "modules"]);
    const release = output([...make, "-s", "kernelrelease"]);
    run([...make, "INSTALL_MOD_PATH=" + moduleRoot, "INSTALL_MOD_STRIP=1", "modules_install"]);
    const modules = join(moduleRoot, "lib/modules");
    for (const name of ["build", "source"])
      rmSync(join(modules, release, name), { force: true });
    run([
      "tar", "--numeric-owner", "--owner=0", "--group=0", "--sort=name",
      "-czf", join(outputDirectory, "modules.tar.gz"), "-C", modules, release,
    ]);
    copyFileSync(join(build, "arch/x86/boot/bzImage"), join(outputDirectory, "bzImage"));
    const kernelConfig = join(outputDirectory, "kernel.config");
    const kernelLicense = join(outputDirectory, "KERNEL-LICENSE.txt");
    copyFileSync(config, kernelConfig);
    writeFileSync(
      kernelLicense,
      ["COPYING", "LICENSES/preferred/GPL-2.0", "LICENSES/exceptions/Linux-syscall-note"]
        .map((name) => readFileSync(join(source, name), "utf8"))
        .join("\n\n"),
    );
    const receipt = {
      source: linuxRepository + "/tree/" + linuxCommit,
      source_commit: linuxCommit,
      kernel_release: release,
      kernel_sha256: await digest(join(outputDirectory, "bzImage")),
      config_sha256: await digest(kernelConfig),
      license_sha256: await digest(kernelLicense),
      modules_sha256: await digest(join(outputDirectory, "modules.tar.gz")),
    };
    save(join(outputDirectory, "kernel-receipt.json"), receipt);
    renameSync(outputDirectory, destination);
    return receipt;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}

async function extractRootfs(capsule: string, archive: string, expected: string) {
  const measured = createHash("sha256");
  for await (const entry of tarEntries(capsule)) {
    if (entry.name !== wslBody) continue;
    async function* body() {
      for await (const chunk of entry.body) {
        measured.update(chunk);
        yield chunk;
      }
    }
    await pipeline(
      Readable.from(body()),
      createWriteStream(archive, { flags: "wx" }),
    );
  }
  if (measured.digest("hex") !== expected)
    throw Error("Guest root filesystem differs from the sealed capsule");
}

async function moduleSize(archive: string, release: string) {
  let bytes = 0;
  let index = false;
  for await (const entry of tarEntries(archive, "gzip")) {
    const name = canonical(entry.name.replace(/\/$/, ""));
    if (
      (name !== release && !name.startsWith(release + "/")) ||
      !["0", "5"].includes(entry.type)
    )
      throw Error("Kernel modules must contain only the named release tree");
    bytes += entry.size;
    index ||= entry.type === "0" && name === release + "/modules.dep";
  }
  if (!index) throw Error("Kernel module dependency index is missing");
  return bytes;
}

function guestPath(tree: string, directory: string) {
  const path = realpathSync(join(tree, directory));
  if (!path.startsWith(tree + sep))
    throw Error("Guest directory leaves the staged root filesystem");
  return path;
}

function addGuestInit(tree: string) {
  if (!lstatSync(join(tree, busybox.slice(1))).isFile())
    throw Error("Sealed Linux backend is missing its BusyBox init");
  const sbin = guestPath(tree, "sbin");
  symlinkSync(busybox, join(sbin, "init"));
  symlinkSync("/opt/aiphone/engine/sbin/modprobe", join(sbin, "modprobe"));
  const etc = guestPath(tree, "etc");
  for (const [name, value] of [
    ["inittab", `::once:${guestBroker}\n`],
    ["resolv.conf", "nameserver 10.0.2.3\n"],
  ] as const) {
    const target = join(etc, name);
    rmSync(target, { force: true });
    writeFileSync(target, value, { flag: "wx", mode: 0o644 });
  }
}

export async function buildWindowsGuestDisk(
  capsule: string,
  identity: string,
  kernel: string,
  modules: string,
  receipt: string,
  destination: string,
) {
  if (process.platform !== "linux" || !["x64", "arm64"].includes(process.arch) || process.getuid?.() !== 0)
    throw Error("Guest disk assembly requires root on a disposable Linux factory");
  destination = resolve(destination);
  if (existsSync(destination)) throw Error("Use a new guest capsule destination");
  const kernelSource = json<KernelReceipt>(receipt);
  if (
    !/^https:\/\//.test(kernelSource.source) ||
    !/^[a-f0-9]{40}$/.test(kernelSource.source_commit) ||
    !/^[a-zA-Z0-9][a-zA-Z0-9._+-]{0,127}$/.test(kernelSource.kernel_release) ||
    !sha256.test(identity) ||
    !sha256.test(kernelSource.kernel_sha256) ||
    !sha256.test(kernelSource.config_sha256) ||
    !sha256.test(kernelSource.license_sha256) ||
    !sha256.test(kernelSource.modules_sha256)
  )
    throw Error("Missing immutable guest kernel and module identities");
  const kernelData = readFileSync(kernel);
  const configData = readFileSync(join(dirname(receipt), "kernel.config"));
  const licenseData = readFileSync(join(dirname(receipt), "KERNEL-LICENSE.txt"));
  const kernelDigest = hash(kernelData);
  const [capsuleDigest, modulesDigest] = await Promise.all([
    digest(capsule), digest(modules),
  ]);
  if (
    capsuleDigest !== identity ||
    kernelDigest !== kernelSource.kernel_sha256 ||
    hash(configData) !== kernelSource.config_sha256 ||
    hash(licenseData) !== kernelSource.license_sha256 ||
    modulesDigest !== kernelSource.modules_sha256
  )
    throw Error("Guest disk input bytes differ from their source receipts");
  const component = await verifyWsl(capsule, ROOT);
  if (component.provenance?.qemu_bootstrap !== qemuBootstrapContract)
    throw Error("Rebuild the sealed Linux backend with the QEMU bootstrap command");
  const expanded = component.inventory.uncompressed_bytes +
    await moduleSize(modules, kernelSource.kernel_release);
  const diskSize = Math.max(16 * gib, Math.ceil((expanded + 8 * gib) / gib) * gib);
  const parent = dirname(destination);
  const free = statfsSync(parent);
  if (free.bavail * free.bsize < component.rootfs.size + 4 * expanded + 2 * gib)
    throw Error("Insufficient space to assemble the guest disk");
  const work = mkdtempSync(join(realpathSync(parent), ".windows-guest-"));
  try {
    const tree = join(work, "root"), archive = join(work, "rootfs.tar.gz");
    mkdirSync(tree);
    await extractRootfs(capsule, archive, component.rootfs.sha256);
    const extract = (file: string, directory: string) => run([
      "tar", "--extract", "--file", file, "--directory", directory,
      "--numeric-owner", "--same-owner", "--same-permissions", "--xattrs", "--acls",
    ]);
    extract(archive, tree);
    rmSync(archive);
    const moduleTree = join(guestPath(tree, "lib"), "modules");
    mkdirSync(moduleTree, { recursive: true });
    extract(modules, moduleTree);
    addGuestInit(tree);
    const raw = join(work, "root.ext4"), qcow = join(work, "guest.qcow2");
    writeFileSync(raw, "", { flag: "wx" });
    truncateSync(raw, diskSize);
    run(["mke2fs", "-q", "-F", "-t", "ext4", "-L", "AI_PHONE_VM", "-d", tree, raw]);
    rmSync(tree, { recursive: true });
    run(["qemu-img", "convert", "-f", "raw", "-O", "qcow2", "-c", raw, qcow]);
    rmSync(raw);
    const manifest = {
      format: 1,
      host: "windows-x64",
      runtime: "qemu-linux-backend",
      source_commit: component.provenance.source_commit,
      source_manifest_sha256: component.provenance.source_manifest_sha256,
      source_components: component.provenance.source_components,
      backend_capsule_sha256: identity,
      rootfs_sha256: component.rootfs.sha256,
      kernel_source: {
        source: kernelSource.source,
        commit: kernelSource.source_commit,
        release: kernelSource.kernel_release,
        modules_sha256: modulesDigest,
        config: { path: "kernel.config", sha256: kernelSource.config_sha256 },
        license: { path: "KERNEL-LICENSE.txt", sha256: kernelSource.license_sha256 },
      },
      kernel: { path: "bzImage", size: kernelData.length, sha256: kernelDigest },
      disk: {
        path: "guest.qcow2",
        size: lstatSync(qcow).size,
        virtual_size: diskSize,
        sha256: await digest(qcow),
      },
      kernel_args: kernelArgs,
      windows_boot_verified: false,
    };
    const staged = join(work, "guest.payload");
    await writeTar(staged, [
      { path: manifest.kernel.path, data: kernelData, mode: 0o644 },
      { path: manifest.kernel_source.config.path, data: configData, mode: 0o644 },
      { path: manifest.kernel_source.license.path, data: licenseData, mode: 0o644 },
      { path: manifest.disk.path, file: qcow, size: manifest.disk.size, mode: 0o644 },
      { path: "WINDOWS-VM-DISK-MANIFEST.json", data: Buffer.from(JSON.stringify(manifest)), mode: 0o644 },
    ]);
    linkSync(staged, destination);
    return manifest;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}
