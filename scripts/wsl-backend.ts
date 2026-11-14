import { buildWslBackend } from "./tooling/wsl-qualification";
const result = await buildWslBackend(
  undefined,
  process.argv.includes("--with-device-apps"),
);
console.log(JSON.stringify(result.report));
