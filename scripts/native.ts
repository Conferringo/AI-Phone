import { ROOT } from "./tooling/process";
import {
  environment,
  profile,
  guestApps,
  startExecutionServices,
  stopServices,
  service,
  launchRole,
  type ServiceRole,
} from "./tooling/native";
const [operation, ...args] = process.argv.slice(2);
switch (operation) {
  case "env":
    console.log(JSON.stringify(environment()));
    break;
  case "profile":
    console.log(JSON.stringify(profile(ROOT, args[0], args.slice(1))));
    break;
  case "apps":
    console.log(
      JSON.stringify(
        guestApps(
          ROOT,
          args[0] as "verify" | "download" | "phone-install",
          args.slice(1),
        ),
      ),
    );
    break;
  case "start":
    startExecutionServices();
    break;
  case "launch":
    console.log(JSON.stringify(launchRole(args[0] as ServiceRole)));
    break;
  case "stop":
    stopServices();
    break;
  case "service":
    console.log(
      JSON.stringify(
        service(ROOT, args[0]!, args[1] as "start" | "stop" | "status"),
      ),
    );
    break;
  default:
    throw Error("Expected native env|profile|apps|start|stop|service");
}
