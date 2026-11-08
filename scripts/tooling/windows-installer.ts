import { existsSync, mkdirSync, writeFileSync, linkSync, lstatSync } from "node:fs";
import { join, dirname, basename } from "node:path";
import { run } from "./process";
import { json } from "./inventory";
import { fetchPin } from "./factory";
import { resourceMap } from "./resources";
import { Zip } from "./zip";

async function unpacker(root: string) {
  const folder = join(root, ".tools/windows-installer"),
    pin = json(join(root, "config/windows-installer.json")).unpacker,
    archive = await fetchPin(folder, "innounp.zip", pin),
    program = join(folder, "innounp.exe");
  if (!existsSync(program)) {
    const zip = new Zip(archive),
      member = zip.entries.find((entry) => entry.name === "innounp.exe")!,
      executable = await zip.bytes(member),
      characteristics = executable.readUInt32LE(60) + 22;
    executable.writeUInt16LE(
      executable.readUInt16LE(characteristics) | 0x20,
      characteristics,
    );
    writeFileSync(program, executable, { flag: "wx" });
  }
  return program;
}

export async function unpackWindowsInstaller(
  root: string,
  installer: string,
  folder: string,
) {
  run(
    [
      await unpacker(root),
      "-x", "-b", "-q", "-a", "-c{app}",
      "-d" + folder, installer, "{app}\\*",
    ],
    { stdio: ["ignore", "inherit", "inherit"] },
  );
}

export async function buildWindowsInstaller(
  root: string,
  binary: string,
  destination: string,
) {
  const folder = join(root, ".tools/windows-installer"),
    policy = json(join(root, "config/windows-installer.json")),
    archive = await fetchPin(folder, "inno-setup.exe", policy.compiler),
    compilerFolder = join(folder, "compiler"),
    compiler = join(compilerFolder, "ISCC.exe");
  if (!existsSync(compiler))
    await unpackWindowsInstaller(root, archive, compilerFolder);
  const stage = join(folder, "application");
  mkdirSync(stage);
  linkSync(binary, join(stage, "ai-phone-client.exe"));
  for (const [input, output] of Object.entries(resourceMap(root))) {
    const target = join(stage, output);
    mkdirSync(dirname(target), { recursive: true });
    linkSync(join(root, "apps/client/src-tauri", input), target);
  }
  const version = json(join(root, "config/release-policy.json")).version;
  run([
    compiler,
    "/Qp",
    "/DAppVersion=" + version,
    "/DNumericVersion=" + version.split("-")[0] + ".0",
    "/DAppSource=" + stage,
    "/DProjectRoot=" + root,
    "/O" + dirname(destination),
    "/F" + basename(destination, ".exe"),
    join(root, "apps/client/src-tauri/windows/installer.iss"),
  ]);
  console.log(JSON.stringify({ installer: basename(destination), bytes: lstatSync(destination).size }));
}
