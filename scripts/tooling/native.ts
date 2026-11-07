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
export type ServiceSpec = { command: string[]; environment: Environment };
export function service(
  root: string,
  name: string,
  operation: "start" | "stop" | "status",
  spec?: ServiceSpec,
) {
  return JSON.parse(
    output([nativeProgram(root), "--service-control", root, name, operation], {
      input: spec ? JSON.stringify(spec) : undefined,
    }),
  );
}
export function guestApps(
  root: string,
  operation: "verify" | "download" | "phone-install",
  kinds: string[] = [],
) {
  return JSON.parse(
    output(
      [nativeProgram(root), "--guest-app-resources", root, operation, ...kinds],
      { env: { ...process.env, ...environment(root) } },
    ),
  );
}
export function framedRequest(body: Uint8Array) {
  const frame = Buffer.alloc(body.length + 4);
  frame.writeUInt32BE(body.length);
  frame.set(body, 4);
  return frame;
}
export function unframe(body: Buffer) {
  if (body.length < 4 || body.readUInt32BE() !== body.length - 4)
    throw Error("Native IPC frame invalid");
  return body.subarray(4);
}
export function nativeIpc(
  program: string,
  operation: string,
  body: Uint8Array,
  root = ROOT,
) {
  const result = run([program, operation], {
    cwd: root,
    input: framedRequest(body),
    stdio: ["pipe", "pipe", "pipe"],
  });
  return unframe(Buffer.from(result.stdout!));
}
export type ServiceRole =
  | "assistant"
  | "runner"
  | "media"
  | "channels"
  | "web"
  | "lan"
  | "emulator";
export function launchRole(role: ServiceRole, root = ROOT) {
  return JSON.parse(
    output([nativeProgram(root), "--source-services", root, "launch", role], {
      cwd: root,
    }),
  );
}
export function startExecutionServices(root = ROOT) {
  run([nativeProgram(root), "--source-services", root, "start"], { cwd: root });
}
export function stopServices(root = ROOT) {
  run([nativeProgram(root), "--source-services", root, "stop"], { cwd: root });
}
