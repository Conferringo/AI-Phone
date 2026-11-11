import { mkdtempSync, readFileSync, renameSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { hash, run } from "./process";

export const temporaryMacSignature = { signing: "ad-hoc" } as const;

export type InstallerSignature =
  | { signing: "unsigned" }
  | typeof temporaryMacSignature
  | ReturnType<typeof signWindowsInstaller>;

export function signMacArtifact(file: string) {
  run(["codesign", "--force", "--sign", "-", "--timestamp=none", file]);
}

export function verifyMacArtifact(file: string) {
  run(["codesign", "--verify", "--deep", "--strict", "--verbose=2", file]);
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
