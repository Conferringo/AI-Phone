import { constants, copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fetchPin, type Pin } from "./factory";
import { json } from "./inventory";
import { ROOT } from "./process";

export async function prepareSpeechModel(root = ROOT) {
  const pin = json<Pin & { file: string }>(join(root, "config/speech-model.json"));
  const source = await fetchPin(join(root, ".tools/speech"), pin.file, pin);
  const resource = "speech/" + pin.file;
  const target = join(root, "apps/client/src-tauri/gen/host", resource);
  mkdirSync(dirname(target), { recursive: true });
  copyFileSync(source, target, constants.COPYFILE_FICLONE);
  return { resource, path: target, sha256: pin.sha256 };
}
