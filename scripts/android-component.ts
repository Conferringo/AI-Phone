import { ROOT } from "./tooling/process";
import { save } from "./tooling/inventory";
import {
  prepareAndroid,
  sealAndroidSdk,
} from "./tooling/android-component";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!,
  root = args.includes("--root") ? option("--root") : ROOT;
const preset = args.includes("--compression-preset") ? Number(option("--compression-preset")) : 6;
const target = args.includes("--target") ? option("--target") : undefined;
if (!args.includes("--sdk-input") || !args.includes("--work"))
  throw Error("Provide --sdk-input and --work");
const result = await sealAndroidSdk(option("--sdk-input"), option("--work") + "/android.payload", preset, root, target);
if (args.includes("--verify-programs")) {
  const phone = await prepareAndroid(
    result.file,
    option("--work") + "/installation",
    result.receipt.archive_sha256,
    root,
  );
  Object.assign(result.receipt, {
    qualification: {
      host: phone.sdkRoot,
      programs_prepared: true,
      guest_boot: "not-executed",
    },
  });
}
if (args.includes("--receipt")) save(option("--receipt"), result.receipt);
console.log(JSON.stringify({ output: result.file, ...result.receipt }));
