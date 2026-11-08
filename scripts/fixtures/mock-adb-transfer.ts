import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  copyFileSync,
  mkdirSync,
  existsSync,
  renameSync,
} from "node:fs";
import { join, dirname, resolve } from "node:path";
import { createHash } from "node:crypto";
const root = resolve(import.meta.dir, ".."),
  args = process.argv.slice(2),
  guest = (path: string) => join(root, "guest", path.replace(/^\//, ""));
function words(text: string) {
  const values: string[] = [];
  let word = "",
    quote = "",
    escaped = false,
    started = false;
  for (const char of text) {
    if (escaped) {
      word += char;
      escaped = false;
      started = true;
    } else if (char === "\\" && quote !== "'") {
      escaped = true;
      started = true;
    } else if (quote) {
      if (char === quote) quote = "";
      else word += char;
    } else if (char === "'" || char === '"') {
      quote = char;
      started = true;
    } else if (/\s/.test(char)) {
      if (started) {
        values.push(word);
        word = "";
        started = false;
      }
    } else {
      word += char;
      started = true;
    }
  }
  if (quote || escaped) throw Error("Invalid fixture shell request");
  if (started) values.push(word);
  return values;
}
if (args[0] === "push") {
  const file = guest(args[2]!);
  mkdirSync(dirname(file), { recursive: true });
  copyFileSync(args[1]!, file);
} else if (args[0] === "exec-out") {
  writeFileSync(join(root, "copy-started"), "started");
  Bun.sleepSync(350);
  process.stdout.write("fixture document");
} else if (args[0] === "shell") {
  const command = words(args[1]!),
    last = command.at(-1)!;
  switch (command[0]) {
    case "readlink":
      console.log(last);
      break;
    case "stat":
      console.log(Buffer.byteLength("fixture document"));
      break;
    case "mkdir":
      mkdirSync(guest(last), { recursive: true });
      break;
    case "sha256sum":
      console.log(
        createHash("sha256")
          .update(readFileSync(guest(last)))
          .digest("hex"),
      );
      break;
    case "test": {
      const source = guest(command.at(-2)!),
        target = guest(last);
      if (existsSync(target)) process.exit(1);
      renameSync(source, target);
      appendFileSync(join(root, "published"), "publish\n");
      if (existsSync(join(root, "slow-publication"))) Bun.sleepSync(350);
      if (existsSync(join(root, "fail-after-publish"))) process.exit(3);
      break;
    }
    case "am":
      break;
    default:
      throw Error("Unexpected fixture shell request");
  }
} else throw Error("Unexpected fixture transport request");
