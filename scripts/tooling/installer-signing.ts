import { mkdtempSync, readFileSync, renameSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { hash, output, run } from "./process";

export const temporaryMacSignature = { signing: "ad-hoc" } as const;
export const releaseMacSignature = {
  signing: "developer-id", developer_id_signed: true, notarized: true,
} as const;
export const releaseWindowsSignature = {
  signing: "authenticode-trusted",
  authenticode: { file_digest: "sha256", cryptographically_verified: true, timestamped: true },
} as const;

export type InstallerSignature =
  | { signing: "unsigned" }
  | typeof temporaryMacSignature
  | typeof releaseMacSignature
  | typeof releaseWindowsSignature
  | ReturnType<typeof signWindowsInstaller>;

export function macReleaseSigning() {
  const identity = process.env.AIMANITY_MAC_SIGNING_IDENTITY?.trim();
  const profile = process.env.AIMANITY_MAC_NOTARY_PROFILE?.trim();
  if (!identity?.startsWith("Developer ID Application: ") || !profile)
    throw Error("Developer ID Application identity and notarytool keychain profile are required");
  return { identity, profile };
}

export function signMacArtifact(file: string, identity?: string) {
  run(["codesign", "--force", "--sign", identity ?? "-",
    ...(identity ? ["--timestamp", ...(file.endsWith(".app") ? ["--options", "runtime"] : [])] : ["--timestamp=none"]),
    file,
  ]);
}

export function verifyMacArtifact(file: string) {
  run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", file]);
}

export function notarizeMacImage(image: string, profile: string) {
  const receipt = JSON.parse(output(["xcrun", "notarytool", "submit", image,
    "--keychain-profile", profile, "--wait", "--output-format", "json"]));
  if (receipt.status !== "Accepted")
    throw Error("Apple notarization was not accepted" + (receipt.id ? ` (submission ${receipt.id})` : ""));
  run(["xcrun", "stapler", "staple", image]);
  run(["xcrun", "stapler", "validate", image]);
  run(["spctl", "--assess", "--type", "open", "--context", "context:primary-signature",
    "--verbose=2", image]);
}

export function windowsReleaseSigning() {
  if (process.platform !== "win32")
    throw Error("Trusted Windows signing requires the Windows certificate store and SignTool");
  const thumbprint = process.env.AIMANITY_WINDOWS_CERT_THUMBPRINT?.trim();
  const timestampUrl = process.env.AIMANITY_WINDOWS_TIMESTAMP_URL?.trim();
  if (!thumbprint || !/^[a-f0-9]{40}$/i.test(thumbprint) || !timestampUrl)
    throw Error("Windows certificate thumbprint and RFC 3161 timestamp URL are required");
  return { thumbprint, timestampUrl };
}

export function signWindowsRelease(file: string, { thumbprint, timestampUrl }: ReturnType<typeof windowsReleaseSigning>) {
  run(["signtool.exe", "sign", "/sha1", thumbprint, "/fd", "SHA256",
    "/tr", timestampUrl, "/td", "SHA256", file]);
  run(["signtool.exe", "verify", "/pa", "/tw", file]);
}

export function signWindowsInstaller(installer: string) {
  const work = mkdtempSync(join(dirname(installer), ".authenticode-"));
  try {
    const key = join(work, "key.pem"),
      cert = join(work, "certificate.pem"),
      der = join(work, "certificate.der"),
      signed = join(work, "setup.exe");
    run([
      "openssl", "req", "-x509", "-newkey", "rsa:3072", "-sha256", "-noenc",
      "-days", "30", "-subj", "/CN=AImanity Temporary Signing",
      "-addext", "basicConstraints=critical,CA:FALSE",
      "-addext", "keyUsage=critical,digitalSignature",
      "-addext", "extendedKeyUsage=codeSigning",
      "-keyout", key, "-out", cert,
    ]);
    run([
      "osslsigncode", "sign", "-certs", cert, "-key", key, "-h", "sha256",
      "-n", "AImanity", "-in", installer, "-out", signed,
    ]);
    rmSync(key);
    run(["openssl", "x509", "-in", cert, "-outform", "DER", "-out", der]);
    run(["osslsigncode", "verify", "-CAfile", cert, "-in", signed]);
    const fingerprint = hash(readFileSync(der));
    renameSync(signed, installer);
    return {
      signing: "authenticode-self-signed",
      authenticode: {
        certificate_sha256: fingerprint,
        file_digest: "sha256",
        cryptographically_verified: true,
        timestamped: false,
      },
    } as const;
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
}
