import { ROOT } from "./tooling/process";
import { preparePayload } from "./tooling/client-payload";
import { verifyWeb } from "./tooling/verify";
import { verifyNative, nativeDatabaseReceipt } from "./tooling/native-check";
import { stageSource } from "./tooling/client-payload";
import { mkdtempSync, rmSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";
const args = process.argv.slice(2),
  option = (name: string) =>
    args.includes(name) ? args[args.indexOf(name) + 1] : undefined;
console.log(
  JSON.stringify(
    await preparePayload(ROOT, {
      web: args.includes("--with-web-runtime"),
      nativeServices: args.includes("--with-service-runtime"),
      phoneApps: args.includes("--with-phone-apps"),
      containerHost: args.includes("--with-container-host"),
      guestApps: args.includes("--with-guest-apps"),
      androidRuntime: option("--android-runtime"),
      imageRuntime: option("--images-runtime"),
      linuxOcr: option("--linux-ocr"),
      wslRuntime: option("--wsl-runtime"),
    }),
  ),
);
if (
  args.some((arg) =>
    [
      "--verify-web-runtime",
      "--verify-service-runtime",
      "--pg-container",
    ].includes(arg),
  )
) {
  const home = mkdtempSync(join(tmpdir(), "aiphone-payload-check-"));
  try {
    const staged = await stageSource(
      join(ROOT, "apps/client/src-tauri/gen/host/source.payload"),
      home,
      ROOT,
    );
    if (args.includes("--verify-web-runtime"))
      console.log(JSON.stringify(await verifyWeb(staged)));
    if (args.includes("--verify-service-runtime"))
      console.log(JSON.stringify(verifyNative(staged)));
    if (args.includes("--pg-container"))
      console.log(
        JSON.stringify(
          nativeDatabaseReceipt(
            staged,
            option("--pg-container")!,
            Number(option("--pg-port")),
          ),
        ),
      );
  } finally {
    rmSync(home, { recursive: true });
  }
}
