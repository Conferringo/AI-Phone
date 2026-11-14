import { readFileSync, writeFileSync, existsSync, unlinkSync } from "node:fs";
import { join, basename } from "node:path";
const root = process.env.LOCATION_FIXTURE!,
  args = process.argv.slice(2),
  name = basename(process.argv[1]!).replace(/\.ts$/, "");
if (name === "appops") {
  if (args[0] === "get")
    console.log(
      "Uid mode: MOCK_LOCATION: " + readFileSync(join(root, "appop"), "utf8"),
    );
  else writeFileSync(join(root, "appop"), args.at(-1)!);
} else if (args[1] === "is-location-enabled")
  console.log(readFileSync(join(root, "enabled"), "utf8"));
else if (args[1] === "set-location-enabled")
  writeFileSync(join(root, "enabled"), args[2]!);
else {
  const operation = args[2],
    provider = args[3],
    file = join(root, "provider-" + provider);
  if (operation === "add-test-provider") {
    if (provider === "network" && existsSync(join(root, "fail-network"))) {
      unlinkSync(join(root, "fail-network"));
      process.exit(42);
    }
    writeFileSync(file, "");
  } else if (operation === "remove-test-provider") {
    if (existsSync(file)) unlinkSync(file);
  } else if (!existsSync(file)) process.exit(43);
  else if (operation === "set-test-provider-location")
    writeFileSync(join(root, "fix"), args[5]!);
}
