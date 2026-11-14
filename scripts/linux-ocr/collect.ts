import { copyFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import {
  nativeClosure,
  packageRecords,
  archiveDependency,
} from "../tooling/linux-dependencies";
const root = "/out";
mkdirSync(root);
const files = nativeClosure(root, { tesseract: "/usr/bin/tesseract" });
mkdirSync(join(root, "tessdata/configs"), { recursive: true });
for (const language of ["eng", "chi_sim"])
  copyFileSync(
    "/usr/share/tesseract-ocr/5/tessdata/" + language + ".traineddata",
    join(root, "tessdata", language + ".traineddata"),
  );
copyFileSync(
  "/usr/share/tesseract-ocr/5/tessdata/configs/tsv",
  join(root, "tessdata/configs/tsv"),
);
packageRecords(root, files);
mkdirSync("/export");
await archiveDependency(root, "/export/linux-x64-ocr.tar.gz");
