import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { ROOT, nativeProgram, output, run } from "./process";
export type Environment = Record<string, string>;
export function environment(root = ROOT): Environment {
  return JSON.parse(
    output([nativeProgram(root), "--runtime-environment", root, homedir()], {
      cwd: root,
    }),
  );
}
export function profile(
  root = ROOT,
  operation = "show",
  args: string[] = [],
  request?: unknown,
) {
  if (operation === "show")
    return JSON.parse(
      readFileSync(join(root, "data/deployment/profile.json"), "utf8"),
    );
  const parameters =
    operation === "validate"
      ? ["--validate-host-profile"]
      : operation === "write"
        ? ["--write-host-profile", root]
        : ["--prepare-host-profile", root, ...args];
  return JSON.parse(
    output([nativeProgram(root), ...parameters], {
      input: JSON.stringify(request),
    }),
  );
}
export function startExecutionServices(root = ROOT) {
  run([nativeProgram(root), "--source-services", root, "start"], { cwd: root });
}
export function stopServices(root = ROOT) {
  run([nativeProgram(root), "--source-services", root, "stop"], { cwd: root });
}
