import { ROOT } from "./tooling/process";
import { buildNative } from "./tooling/native-build";
import {
  buildPhone,
  installerApk,
  notificationHelper,
} from "./tooling/phone-apps";
import { webCommand } from "./tooling/web-runtime";
const [operation, ...args] = process.argv.slice(2);
const root = args.includes("--root") ? args[args.indexOf("--root") + 1]! : ROOT;
switch (operation) {
  case "native":
    console.log(JSON.stringify(await buildNative(root)));
    break;
  case "phone":
    if (args.includes("--build"))
      console.log(JSON.stringify(await buildPhone(root)));
    else if (args.includes("--notification-probe"))
      console.log(notificationHelper(root));
    else
      console.log(
        installerApk(root, args[args.indexOf("--app") + 1] as "home" | "input"),
      );
    break;
  case "web":
    console.log(JSON.stringify(await webCommand(root)));
    break;
  default:
    throw Error("Expected artifacts native|phone|web");
}
