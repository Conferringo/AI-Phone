import { qualifyWsl } from "./tooling/wsl-qualification";

const args = process.argv.slice(2);
const ocrIndex = args.indexOf("--linux-ocr");
const linuxOcr = ocrIndex < 0 ? undefined : args[ocrIndex + 1];
if (ocrIndex >= 0 && (!linuxOcr || linuxOcr.startsWith("--")))
  throw Error("--linux-ocr requires a directory");

console.log(
  JSON.stringify(
    await qualifyWsl(undefined, args.includes("--with-device-apps"), linuxOcr),
  ),
);
