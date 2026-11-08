import { qualifyWsl } from "./tooling/wsl-qualification";
console.log(
  JSON.stringify(
    await qualifyWsl(undefined, process.argv.includes("--with-device-apps")),
  ),
);
