import {
  closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, readSync,
  rmSync, statSync, truncateSync, writeFileSync, writeSync,
} from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { crc32 } from "node:zlib";
import { createHash } from "node:crypto";
import { run } from "./tooling/process";

const SEC = 512, MIB = 2 ** 20;
const align = (n: number, block: number) => Math.ceil(n / block) * block;
const sha256 = (bytes: Uint8Array) => createHash("sha256").update(bytes).digest();
const id = (text: string) => createHash("md5").update(text).digest();
const u64 = (bytes: Buffer, value: number, at: number) => bytes.writeBigUInt64LE(BigInt(value), at);

function bootImages(parts: string, out: string) {
  const boot = readFileSync(join(parts, "boot.img"));
  const generic = readFileSync(join(parts, "init_boot.img"));
  const vendor = readFileSync(join(parts, "vendor_boot.img"));
  if (!boot.subarray(0, 8).equals(Buffer.from("ANDROID!")) || boot.readUInt32LE(40) !== 4
    || !generic.subarray(0, 8).equals(Buffer.from("ANDROID!")) || generic.readUInt32LE(40) !== 4
    || !vendor.subarray(0, 8).equals(Buffer.from("VNDRBOOT")) || vendor.readUInt32LE(8) !== 4)
    throw Error("Expected Cuttlefish Android boot image headers v4");
  const kernel = boot.subarray(4096, 4096 + boot.readUInt32LE(8));
  if (kernel.toString("ascii", 514, 518) !== "HdrS") throw Error("Expected Linux bzImage");
  writeFileSync(join(out, "bzImage"), kernel, { flag: "wx" });

  const page = vendor.readUInt32LE(12);
  const ramdisk = align(vendor.readUInt32LE(2096), page);
  const table = ramdisk + align(vendor.readUInt32LE(24), page) + align(vendor.readUInt32LE(2100), page);
  const tableSize = vendor.readUInt32LE(2112);
  const bootconfigStart = table + align(tableSize, page);
  const official = vendor.toString("utf8", bootconfigStart, bootconfigStart + vendor.readUInt32LE(2124)).trim();
  const ramdisks: Buffer[] = [];
  const entrySize = vendor.readUInt32LE(2120);
  for (let offset = table; offset < table + tableSize; offset += entrySize) {
    const start = ramdisk + vendor.readUInt32LE(offset + 4);
    if (vendor.readUInt32LE(offset + 8) !== 2)
      ramdisks.push(vendor.subarray(start, start + vendor.readUInt32LE(offset)));
  }
  ramdisks.push(generic.subarray(4096, 4096 + generic.readUInt32LE(12)));

  const rootVbmeta = readFileSync(join(parts, "vbmeta.img"));
  const size = (b: Buffer) => 256 + Number(b.readBigUInt64BE(12) + b.readBigUInt64BE(20));
  const chain: Buffer[] = [rootVbmeta.subarray(0, size(rootVbmeta))];
  let descriptor = 256 + Number(rootVbmeta.readBigUInt64BE(12) + rootVbmeta.readBigUInt64BE(96));
  const descriptorsEnd = descriptor + Number(rootVbmeta.readBigUInt64BE(104));
  while (descriptor < descriptorsEnd) {
    if (rootVbmeta.readBigUInt64BE(descriptor) === 4n) {
      const name = rootVbmeta.toString("ascii", descriptor + 92, descriptor + 92 + rootVbmeta.readUInt32BE(descriptor + 20));
      const candidate = readFileSync(join(parts, name + ".img"));
      if (candidate.toString("ascii", 0, 4) === "AVB0") {
        chain.push(candidate.subarray(0, size(candidate)));
      } else {
        const footer = candidate.length - 64;
        if (candidate.toString("ascii", footer, footer + 4) !== "AVBf")
          throw Error("Missing AVB footer in " + name);
        const start = Number(candidate.readBigUInt64BE(footer + 20));
        chain.push(candidate.subarray(start, start + Number(candidate.readBigUInt64BE(footer + 28))));
      }
    }
    descriptor += 16 + Number(rootVbmeta.readBigUInt64BE(descriptor + 8));
  }
  const vbmeta = Buffer.concat(chain);
  const keys = {
    boot_devices: "pci0000:00/0000:00:05.0,pci0000:00/0000:00:06.0",
    slot_suffix: "_a",
    force_normal_boot: 1,
    serialno: "AIMANITY35",
    verifiedbootstate: "orange",
    "vbmeta.device_state": "unlocked",
    "vbmeta.avb_version": "1.3",
    "vbmeta.hash_alg": "sha256",
    "vbmeta.size": vbmeta.length,
    "vbmeta.digest": sha256(vbmeta).toString("hex"),
    "vbmeta.invalidate_on_error": "yes",
    veritymode: "enforcing",
    console: "hvc1",
    serialconsole: 1,
    fstab_suffix: "cf.f2fs.hctr2",
    lcd_density: 320,
    setupwizard_mode: "DISABLED",
    enable_bootanimation: 0,
    enable_confirmationui: 0,
    hw_timeout_multiplier: 3,
    "hypervisor.vm.supported": 0,
    "hypervisor.protected_vm.supported": 0,
    "cpuvulkan.version": 4202496,
    "opengles.version": 196609,
    "hardware.gralloc": "minigbm",
    "hardware.egl": "angle",
    "hardware.vulkan": "pastel",
    "hardware.hwcomposer": "drm_hwcomposer",
    "hardware.hwcomposer.mode": "client",
    "hardware.hwcomposer.display_finder_mode": "drm",
    "hardware.hwcomposer.display_framebuffer_format": "bgra",
    "vendor.apex.com.android.hardware.gatekeeper": "com.android.hardware.gatekeeper.nonsecure",
    "vendor.apex.com.android.hardware.keymint": "com.android.hardware.keymint.rust_nonsecure",
    "vendor.apex.com.android.hardware.strongbox": "none",
    "vendor.apex.com.android.hardware.weaver": "none",
    "vendor.apex.com.android.hardware.graphics.composer": "com.android.hardware.graphics.composer.drm_hwcomposer",
    vsock_lights_port: 6900,
    vsock_lights_cid: 3,
    vsock_camera_port: 7000,
    vsock_camera_cid: 3,
    modem_simulator_ports: 9600,
    openthread_node_id: 1,
    "audio.tinyalsa.ignore_output": "false",
    "audio.tinyalsa.simulate_input": "false",
  };
  const prop = Buffer.from("ro.vendor.disable_rename_eth0=1\nservice.adb.tcp.port=5555\nro.adb.secure=1\n");
  const add = (...files: [string, Buffer, number][]) => {
    const all: Buffer[] = [];
    for (const [index, [name, data, mode]] of [...files, ["TRAILER!!!", Buffer.alloc(0), 0] as const].entries()) {
      const fields = [index + 1, mode, 0, 0, 1, 0, data.length, 0, 0, 0, 0, name.length + 1, 0];
      const header = Buffer.from("070701" + fields.map((field) => field.toString(16).padStart(8, "0")).join("") + name + "\0");
      all.push(header, Buffer.alloc(align(header.length, 4) - header.length), data, Buffer.alloc(align(data.length, 4) - data.length));
    }
    return Buffer.concat(all);
  };
  const ramdiskProps = add(
    ["system", Buffer.alloc(0), 0o040755],
    ["system/etc", Buffer.alloc(0), 0o040755],
    ["system/etc/ramdisk", Buffer.alloc(0), 0o040755],
    ["system/etc/ramdisk/build.prop", prop, 0o100644],
    ["force_debuggable", Buffer.alloc(0), 0o100644],
    ["adb_debug.prop", prop, 0o100644],
  );
  const text = Buffer.from(official + "\n" + Object.entries(keys).map(([key, value]) => "androidboot." + key + "=" + value).join("\n") + "\n");
  const body = Buffer.alloc(align(text.length + 1, 4));
  text.copy(body);
  const tail = Buffer.alloc(20);
  tail.writeUInt32LE(body.length);
  tail.writeUInt32LE(body.reduce((sum, byte) => (sum + byte) >>> 0, 0), 4);
  tail.write("#BOOTCONFIG\n", 8);
  writeFileSync(join(out, "initrd.img"), Buffer.concat([ramdiskProps, ...ramdisks, body, tail]), { flag: "wx" });
}

function sparseSize(path: string): number {
  const fd = openSync(path, "r");
  try {
    const header = Buffer.alloc(28);
    readSync(fd, header, 0, 28, 0);
    if (header.readUInt32LE(0) !== 0xed26ff3a) throw Error("Expected Android sparse partition");
    return header.readUInt32LE(12) * header.readUInt32LE(16);
  } finally {
    closeSync(fd);
  }
}

function copyPartition(fd: number, at: number, partition: string, sparse: boolean) {
  const source = openSync(partition, "r");
  const buffer = Buffer.alloc(4 * MIB);
  const zero = Buffer.alloc(buffer.length);
  const chunk = Buffer.alloc(12);
  try {
    const copy = (size: number, offset: number, pattern?: Buffer) => {
      if (pattern) buffer.fill(pattern);
      for (let step = 0; step < size;) {
        const limit = Math.min(size - step, buffer.length);
        const count = pattern ? limit : readSync(source, buffer, 0, limit, offset + step);
        if (count !== limit) throw Error("Truncated partition " + partition);
        if (Buffer.compare(buffer.subarray(0, count), zero.subarray(0, count)) !== 0)
          writeSync(fd, buffer, 0, count, at);
        step += count;
        at += count;
      }
    };
    if (!sparse) {
      copy(statSync(partition).size, 0);
      return;
    }
    const header = Buffer.alloc(28);
    readSync(source, header, 0, 28, 0);
    const block = header.readUInt32LE(12);
    const count = header.readUInt32LE(20);
    const chunkBytes = header.readUInt16LE(10);
    let offset = header.readUInt16LE(8);
    for (let index = 0; index < count; index++) {
      if (readSync(source, chunk, 0, 12, offset) !== 12) throw Error("Truncated sparse chunk");
      const kind = chunk.readUInt16LE(0);
      const size = chunk.readUInt32LE(4) * block;
      const encoded = chunk.readUInt32LE(8);
      const data = offset + chunkBytes;
      if (kind === 0xcac1) copy(size, data);
      else if (kind === 0xcac2) {
        const pattern = Buffer.alloc(4);
        if (readSync(source, pattern, 0, 4, data) !== 4) throw Error("Truncated sparse fill");
        if (pattern.readUInt32LE(0) === 0) at += size;
        else copy(size, data, pattern);
      } else if (kind === 0xcac3) at += size;
      else if (kind !== 0xcac4) throw Error("Unknown Android sparse chunk");
      offset += encoded;
    }
  } finally {
    closeSync(source);
  }
}

type Partition = { label: string; size: number; file?: string; sparse?: boolean };

function disk(parts: Partition[], raw: string, target: string, qemuImg: string) {
  const layout = Buffer.alloc(128 * 128);
  let next = MIB / SEC;
  const positions = parts.map((part, index) => {
    const start = next, size = align(part.size, SEC) / SEC;
    const entry = layout.subarray(index * 128, (index + 1) * 128);
    Buffer.from("a2a0d0ebe5b9334487c068b6b72699c7", "hex").copy(entry);
    id(part.label + ":" + basename(target)).copy(entry, 16);
    u64(entry, start, 32);
    u64(entry, start + size - 1, 40);
    entry.write(part.label, 56, "utf16le");
    next = align(start + size, MIB / SEC);
    return start * SEC;
  });
  const end = next + (MIB / SEC) - 1;
  const mbr = Buffer.alloc(SEC);
  mbr[450] = 0xee;
  mbr.writeUInt32LE(1, 454);
  mbr.writeUInt32LE(end, 458);
  mbr.writeUInt16LE(0xaa55, 510);
  const header = (mine: number, other: number, entries: number) => {
    const bytes = Buffer.alloc(SEC);
    bytes.write("EFI PART");
    bytes.writeUInt32LE(0x00010000, 8);
    bytes.writeUInt32LE(92, 12);
    u64(bytes, mine, 24);
    u64(bytes, other, 32);
    u64(bytes, 34, 40);
    u64(bytes, end - 33, 48);
    id(parts.map((part) => part.label).join(",") + end).copy(bytes, 56);
    u64(bytes, entries, 72);
    bytes.writeUInt32LE(128, 80);
    bytes.writeUInt32LE(128, 84);
    bytes.writeUInt32LE(crc32(layout), 88);
    bytes.writeUInt32LE(crc32(bytes.subarray(0, 92)), 16);
    return bytes;
  };
  const fd = openSync(raw, "wx");
  try {
    writeSync(fd, mbr, 0, SEC, 0);
    writeSync(fd, header(1, end, 2), 0, SEC, SEC);
    writeSync(fd, layout, 0, layout.length, 2 * SEC);
    for (const [index, part] of parts.entries())
      if (part.file) copyPartition(fd, positions[index]!, part.file, Boolean(part.sparse));
    writeSync(fd, layout, 0, layout.length, (end - 32) * SEC);
    writeSync(fd, header(end, 1, end - 32), 0, SEC, end * SEC);
  } finally {
    closeSync(fd);
  }
  truncateSync(raw, (end + 1) * SEC);
  run([qemuImg, "convert", "-f", "raw", "-O", "qcow2", "-S", "4k", raw, target]);
  rmSync(raw);
}

const [images, output, qemuImg] = process.argv.slice(2);
if (!images || !output || !qemuImg || process.argv.length !== 5 || existsSync(output))
  throw Error("Usage: windows-cuttlefish.ts <extracted Android CI image> <new output> <qemu-img executable>");
const src = resolve(images), dst = resolve(output);
mkdirSync(dirname(dst), { recursive: true });
mkdirSync(dst);
const work = mkdtempSync(join(dirname(dst), ".cuttlefish-"));
try {
  const image = (name: string) => join(src, name + ".img");
  bootImages(src, dst);
  const os = ["boot", "init_boot", "vendor_boot", "vbmeta", "vbmeta_system", "vbmeta_system_dlkm", "vbmeta_vendor_dlkm"]
    .flatMap((name): Partition[] => {
      const file = image(name), size = statSync(file).size;
      return [{ label: name + "_a", file, size }, { label: name + "_b", size }];
    });
  os.push({ label: "super", file: image("super"), size: sparseSize(image("super")), sparse: true });
  const raw = join(work, "disk.raw");
  disk(os, raw, join(dst, "disk.qcow2"), qemuImg);
  for (const gb of [16, 24, 32])
    disk([
      { label: "misc", size: MIB }, { label: "frp", size: MIB }, { label: "metadata", size: 64 * MIB },
      { label: "userdata", size: gb * 1024 * MIB },
    ], raw, join(dst, "userdata-" + gb + ".qcow2"), qemuImg);
} finally {
  rmSync(work, { recursive: true, force: true });
}
