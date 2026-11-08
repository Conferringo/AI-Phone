import { existsSync, mkdirSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { ROOT, run } from "./process";

function files(folder: string, extension: string): string[] {
  return readdirSync(folder, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(extension))
    .map((entry) => join(entry.parentPath, entry.name));
}

export function buildApk(source: string, name: string, folder: string) {
  const sdk = process.env.ANDROID_HOME,
    java = process.env.JAVA_HOME;
  if (!sdk || !java) throw Error("ANDROID_HOME and JAVA_HOME are required");
  const out = join(ROOT, "data/android", folder),
    tools = join(sdk, "build-tools/35.0.0");
  const jdk = join(java, "bin"),
    platform = join(sdk, "platforms/android-35/android.jar");
  for (const name of ["gen", "classes", "dex"])
    mkdirSync(join(out, name), { recursive: true });
  run([
    join(tools, "aapt2"),
    "compile",
    "--dir",
    join(source, "res"),
    "-o",
    join(out, "resources.zip"),
  ]);
  run([
    join(tools, "aapt2"),
    "link",
    "-o",
    join(out, "base.apk"),
    "-I",
    platform,
    "--manifest",
    join(source, "AndroidManifest.xml"),
    "--java",
    join(out, "gen"),
    "--min-sdk-version",
    "26",
    "--target-sdk-version",
    "35",
    join(out, "resources.zip"),
  ]);
  run([
    join(jdk, "javac"),
    "--release",
    "8",
    "-classpath",
    platform,
    "-d",
    join(out, "classes"),
    ...files(join(source, "src"), ".java"),
    ...files(join(out, "gen"), ".java"),
  ]);
  run([
    join(tools, "d8"),
    "--lib",
    platform,
    "--min-api",
    "26",
    "--output",
    join(out, "dex"),
    ...files(join(out, "classes"), ".class"),
  ]);
  run([
    join(jdk, "jar"),
    "uf",
    join(out, "base.apk"),
    "-C",
    join(out, "dex"),
    "classes.dex",
  ]);
  run([
    join(tools, "zipalign"),
    "-f",
    "4",
    join(out, "base.apk"),
    join(out, "aligned.apk"),
  ]);
  const key = join(out, "debug.jks");
  if (!existsSync(key))
    run([
      join(jdk, "keytool"),
      "-genkeypair",
      "-keystore",
      key,
      "-storepass",
      "android",
      "-keypass",
      "android",
      "-alias",
      "androiddebugkey",
      "-keyalg",
      "RSA",
      "-keysize",
      "2048",
      "-validity",
      "3650",
      "-dname",
      `CN=AI Phone ${name},O=Local Development,C=CN`,
    ]);
  const apk = join(out, `ai-phone-${name}.apk`);
  run([
    join(tools, "apksigner"),
    "sign",
    "--ks",
    key,
    "--ks-pass",
    "pass:android",
    "--out",
    apk,
    join(out, "aligned.apk"),
  ]);
  run([join(tools, "apksigner"), "verify", apk]);
  return apk;
}
