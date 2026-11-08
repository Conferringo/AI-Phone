import {
  mkdtempSync,
  readdirSync,
  readFileSync,
  writeFileSync,
  rmSync,
} from "node:fs";
import { join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { spawnSync } from "node:child_process";
import { ROOT, run, output } from "./process";
import { json } from "./inventory";
import { isolatedEnvironment } from "./verify";

export function requireNativeDatabase(container: string, port: number) {
  if (
    !container.startsWith("aiphone-native-") ||
    !Number.isInteger(port) ||
    port < 1024 ||
    port > 65535 ||
    port === 55482
  )
    throw Error("Native checks require a dedicated disposable database");
  const row = JSON.parse(output(["docker", "inspect", container]))[0],
    binding = row.HostConfig.PortBindings["5432/tcp"];
  if (
    row.Config.Labels["dev.aiphone.test"] !== "native-runtime" ||
    binding.length !== 1 ||
    binding[0].HostIp !== "127.0.0.1" ||
    binding[0].HostPort !== String(port)
  )
    throw Error("Native database fixture owner differs");
}
export function waitDatabase(container: string) {
  return (async () => {
    for (let index = 0; index < 40; index++) {
      const result = spawnSync(
        "docker",
        [
          "exec",
          container,
          "pg_isready",
          "-h",
          "127.0.0.1",
          "-U",
          "ai_phone",
          "-d",
          "ai_phone",
        ],
        { stdio: "ignore" },
      );
      if (result.status === 0) return;
      await Bun.sleep(500);
    }
    throw Error("Disposable PostgreSQL did not become ready");
  })();
}
export function verifyNative(root = ROOT) {
  const manifest = json(join(root, "runtime/services/manifest.json")),
    checks: Record<string, unknown> = {};
  for (const role of ["assistant", "runner"]) {
    const folder = mkdtempSync(join(tmpdir(), "aiphone-service-version-"));
    try {
      const env = {
          ...isolatedEnvironment(folder),
          HOME: folder,
          USERPROFILE: folder,
        },
        result = run([join(root, "runtime/services/bin", role), "--version"], {
          cwd: folder,
          env,
          stdio: ["ignore", "pipe", "pipe"],
          encoding: "utf8",
          timeout: 10000,
        });
      if (
        String(result.stdout).trim() !==
          `ai-phone-${role} ${manifest.versions[role]}` ||
        String(result.stderr).trim() ||
        readdirSync(folder).length
      )
        throw Error("Native version receipt differs");
      checks[role] = {
        program: "ai-phone-" + role,
        version: manifest.versions[role],
        state_created: false,
      };
    } finally {
      rmSync(folder, { recursive: true });
    }
  }
  const folder = mkdtempSync(join(tmpdir(), "aiphone-service-vision-"));
  try {
    const env = isolatedEnvironment(folder);
    if (manifest.vision !== "linux-tesseract-bundled") {
      env.AI_PHONE_TESSERACT = process.env.AI_PHONE_TEST_TESSERACT;
      env.TESSDATA_PREFIX = process.env.AI_PHONE_TEST_TESSDATA;
    }
    const png = readFileSync(join(root, "runtime/services/fixtures/ocr.png")),
      result = run([join(root, "runtime/services/bin/vision")], {
        cwd: folder,
        env,
        input: png,
        stdio: ["pipe", "pipe", "pipe"],
        timeout: 10000,
      }),
      value = JSON.parse(String(result.stdout));
    if (
      value.width !== png.readUInt32BE(16) ||
      value.height !== png.readUInt32BE(20) ||
      value.grid.length !== 2048 ||
      !value.regions.some(
        (row: any) =>
          row.confidence >= 0.8 && row.text.replace(/\s/g, "") === "7319",
      )
    )
      throw Error("Bundled OCR did not recognize the fixture");
    checks.vision = {
      fixture_text: "7319",
      width: value.width,
      height: value.height,
      grid: 2048,
    };
  } finally {
    rmSync(folder, { recursive: true });
  }
  return checks;
}
export function nativeDatabaseReceipt(
  root: string,
  container: string,
  port: number,
) {
  requireNativeDatabase(container, port);
  const folder = mkdtempSync(join(tmpdir(), "aiphone-native-db-")),
    program = resolve(root, "runtime/services/bin/assistant");
  try {
    const env = {
      ...isolatedEnvironment(folder),
      AI_PHONE_PG_PORT: String(port),
    };
    writeFileSync(
      join(folder, ".env"),
      "POSTGRES_DB=ai_phone\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=isolated-native-fixture-password\n",
      { mode: 0o600 },
    );
    for (const flag of ["--prepare-database", "--migrate"])
      run([program, flag], { cwd: folder, env, stdio: "pipe", timeout: 30000 });
    const status = JSON.parse(
      output([program, "--database-check"], {
        cwd: folder,
        env,
        timeout: 15000,
      }),
    );
    if (
      status.runtime !== "ai_phone_runtime" ||
      status.privileged !== false ||
      status.ddl !== false
    )
      throw Error("Native runtime database role differs");
    const credentials = json(join(folder, "data/database/credentials.json")),
      pgEnv = { ...process.env, PGPASSWORD: credentials.runtime.password },
      args = [
        "exec",
        "-e",
        "PGPASSWORD",
        container,
        "psql",
        "-h",
        "127.0.0.1",
        "-X",
        "-v",
        "ON_ERROR_STOP=1",
        "-U",
        credentials.runtime.username,
        "-d",
        "ai_phone",
        "-tA",
        "-c",
      ];
    const migrations = Number(
      output(
        [
          "docker",
          ...args,
          "SELECT count(*) FROM _sqlx_migrations WHERE success;",
        ],
        { env: pgEnv, timeout: 10000 },
      ),
    );
    const ddl = spawnSync(
      "docker",
      [...args, "CREATE TABLE native_probe_forbidden(id int);"],
      { env: pgEnv, encoding: "utf8", timeout: 10000 },
    );
    if (
      !Number.isInteger(migrations) ||
      migrations < 1 ||
      ddl.status === 0 ||
      !ddl.stderr.toLowerCase().includes("permission denied")
    )
      throw Error("Native runtime DDL restriction missing");
    return {
      runtime: "ai_phone_runtime",
      migrations,
      ddl_refused: true,
      fixture_only: true,
    };
  } finally {
    rmSync(folder, { recursive: true });
  }
}
