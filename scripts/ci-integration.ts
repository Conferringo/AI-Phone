import { mkdtempSync, rmSync } from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { ROOT, run, output } from "./tooling/process";
import { waitDatabase } from "./tooling/native-check";
import { verifyTaskRecovery } from "./tooling/task-recovery";
const args = process.argv.slice(2),
  option = (name: string) => args[args.indexOf(name) + 1]!,
  container = option("--container"),
  port = Number(option("--port"));
if (!container.startsWith("aiphone-") || port === 55482)
  throw Error("Disposable container and non-production port required");
const row = JSON.parse(output(["docker", "inspect", container]))[0];
if (
  row.Config.Labels["dev.aiphone.test"] !== "permissions" ||
  !row.HostConfig.PortBindings["5432/tcp"].some(
    (binding: any) =>
      binding.HostIp === "127.0.0.1" && binding.HostPort === String(port),
  )
)
  throw Error("PostgreSQL fixture label and loopback port required");
await waitDatabase(container);
run(
  [
    "docker",
    "exec",
    "-i",
    container,
    "psql",
    "-X",
    "-v",
    "ON_ERROR_STOP=1",
    "-U",
    "ai_phone",
    "-d",
    "ai_phone",
  ],
  {
    stdio: ["pipe", "ignore", "inherit"],
    input: `DO $fixture$ BEGIN
 IF NOT EXISTS(SELECT 1 FROM pg_roles WHERE rolname='aiphone_test') THEN CREATE ROLE aiphone_test LOGIN; END IF;
 ALTER ROLE aiphone_test WITH LOGIN NOSUPERUSER CREATEDB NOCREATEROLE NOREPLICATION NOBYPASSRLS PASSWORD 'isolated-persistence-fixture';
END $fixture$;`,
  },
);
const env = {
  ...process.env,
  AI_PHONE_TEST_DATABASE_URL: `postgres://ai_phone:isolated-fixture-password@127.0.0.1:${port}/ai_phone`,
  AI_PHONE_TEST_PERSISTENCE_DATABASE_URL: `postgres://aiphone_test:isolated-persistence-fixture@127.0.0.1:${port}/ai_phone`,
  AI_PHONE_TEST_PG_CONTAINER: container,
  AI_PHONE_TEST_PG_PORT: String(port),
};
function executable(raw: string, testing: boolean) {
  const rows = raw
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line))
    .filter(
      (row) =>
        row.reason === "compiler-artifact" &&
        row.executable &&
        row.profile.test === testing &&
        JSON.stringify(row.target.kind) === '["bin"]' &&
        row.target.name === "ai-phone-assistant",
    );
  if (rows.length !== 1)
    throw Error("Expected one Cargo-selected assistant executable");
  return resolve(rows[0].executable);
}
const assistant = executable(
    output(
      [
        "cargo",
        "build",
        "-p",
        "ai-phone-assistant",
        "--locked",
        "--message-format=json",
      ],
      { cwd: ROOT, env },
    ),
    false,
  ),
  binary = executable(
    output(
      [
        "cargo",
        "test",
        "-p",
        "ai-phone-assistant",
        "--no-run",
        "--locked",
        "--message-format=json",
      ],
      { cwd: ROOT, env },
    ),
    true,
  );
run(
  [
    binary,
    "--ignored",
    "--skip",
    "model_adapter::tests::live_model_capabilities",
    "--skip",
    "worker::process_recovery_tests::worker_child",
    "--test-threads=1",
  ],
  { cwd: ROOT, env: { ...env, AI_PHONE_TEST_ASSISTANT_BINARY: assistant } },
);
const folder = mkdtempSync(join(tmpdir(), "aiphone-ci-recovery-"));
try {
  console.log(
    JSON.stringify(
      await verifyTaskRecovery({
        container,
        port,
        testBinary: binary,
        assistant,
        folder: join(folder, "run"),
      }),
    ),
  );
} finally {
  rmSync(folder, { recursive: true });
}
console.log(
  "PASS: isolated database, permissions, restore and worker-process recovery; no production device or provider calls",
);
