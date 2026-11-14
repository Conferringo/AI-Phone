import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { pinnedDownload } from "./tooling/android-component";
import { ROOT } from "./tooling/process";

const [directory] = process.argv.slice(2);
if (!directory) throw Error("Provide the Windows Android SDK's cuttlefish directory");
await pinnedDownload({
  url: "https://github.com/Genymobile/scrcpy/releases/download/v3.3.4/scrcpy-server-v3.3.4",
  size_bytes: 90980,
  sha256: "8588238c9a5a00aa542906b6ec7e6d5541d9ffb9b5d0f6e1bc0e365e2303079e",
}, join(directory, "aimanity-audio.jar"));
const licenses = join(directory, "licenses");
mkdirSync(licenses, { recursive: true });
copyFileSync(join(ROOT, "LICENSE"), join(licenses, "scrcpy-Apache-2.0.txt"));
