import {
  mkdirSync,
  writeFileSync,
  readFileSync,
  copyFileSync,
  existsSync,
  readdirSync,
  statSync,
  openSync,
  closeSync,
} from "node:fs";
import { join } from "node:path";
import { randomUUID } from "node:crypto";
import { spawnSync } from "node:child_process";
import assert from "node:assert/strict";
import { run, output, atomicJson } from "./process";

const childTest = "worker::process_recovery_tests::worker_child";
class Gate {
  held = false;
  private done!: () => void;
  private promise = new Promise<void>((resolve) => {
    this.done = resolve;
  });
  async hold() {
    this.held = true;
    await this.promise;
  }
  release() {
    this.done();
  }
}
class Model {
  counts: Record<string, number> = {};
  hold = new Gate();
  longHold = new Gate();
  server = Bun.serve({
    hostname: "127.0.0.1",
    port: 0,
    idleTimeout: 0,
    fetch: async (request: Request) => {
      const body = await request.json(),
        name = body.model as string,
        index = (this.counts[name] || 0) + 1;
      this.counts[name] = index;
      const calls = new Set(
          body.input
            .filter((item: any) => item.name === "memory_save")
            .map((item: any) => item.call_id),
        ),
        completed = new Set(
          body.input
            .filter(
              (item: any) =>
                item.type === "function_call_output" &&
                calls.has(item.call_id) &&
                JSON.stringify(item.output).includes("已保存记忆"),
            )
            .map((item: any) => item.call_id),
        );
      for (const item of body.input) {
        if (!Array.isArray(item.content)) continue;
        const texts = item.content.map((part: any) => part.text || "");
        if (
          texts[0]?.includes("tool: memory_save\n") &&
          texts.slice(1).includes("已保存记忆")
        )
          completed.add(texts[0]);
      }
      const longIndex = completed.size + 1;
      if (name === "long" && index === 17) await this.longHold.hold();
      if (name === "database-outage") await this.hold.hold();
      if (name === "transient" && index < 3)
        return Response.json(
          { error: { code: "rate_limit" } },
          { status: index === 1 ? 503 : 429 },
        );
      if (name === "quota")
        return Response.json(
          { error: { code: "insufficient_quota" } },
          { status: 429 },
        );
      const call = (tool: string, args: unknown) => ({
        type: "function_call",
        name: tool,
        arguments: JSON.stringify(args),
        call_id: name + "-" + index,
      });
      let item: any;
      if (name === "wait")
        item =
          index === 1
            ? call("task_wait", { seconds: 12, reason: "等待本地恢复演练计时" })
            : call("task_finish", {
                status: "needs_input",
                summary: "计时验证已返回，等待维护者核对校验值。",
              });
      else if (name === "long")
        item =
          longIndex <= 56
            ? call("memory_save", { content: "isolated memory " + longIndex })
            : call("task_finish", {
                status: "needs_input",
                summary: "56轮检查点已走完，等待维护者核对校验值。",
              });
      else if (name === "stream" && index === 1)
        item = call("memory_save", { content: "FORBIDDEN_PARTIAL_STREAM" });
      else if (name === "database-outage")
        item = call("memory_save", {
          content: "must-not-run-during-checkpoint-failure",
        });
      else
        item = {
          type: "message",
          role: "assistant",
          content: [{ type: "output_text", text: "fixture reply" }],
        };
      const event =
        name === "stream" && index === 1
          ? { type: "response.output_item.done", item }
          : { type: "response.completed", response: { output: [item] } };
      return new Response("data: " + JSON.stringify(event) + "\n\n", {
        headers: { "Content-Type": "text/event-stream" },
      });
    },
  });
  close() {
    this.hold.release();
    this.longHold.release();
    this.server.stop(true);
  }
}
type Arguments = {
  container: string;
  port: number;
  testBinary: string;
  assistant: string;
  folder: string;
};
class Driver {
  names: string[] = [];
  processes: { child: ReturnType<typeof Bun.spawn>; log: number }[] = [];
  reports: string[] = [];
  constructor(
    readonly args: Arguments,
    readonly model: Model,
  ) {
    if (
      !args.container.startsWith("aiphone-") ||
      args.port === 55482 ||
      output([
        "docker",
        "inspect",
        "-f",
        '{{index .Config.Labels "dev.aiphone.test"}}',
        args.container,
      ]) !== "permissions"
    )
      throw Error("Disposable PostgreSQL required");
  }
  sql(database: string, sql: string) {
    return output(
      [
        "docker",
        "exec",
        "-i",
        this.args.container,
        "psql",
        "-X",
        "-v",
        "ON_ERROR_STOP=1",
        "-U",
        "ai_phone",
        "-d",
        database,
        "-tA",
      ],
      { input: sql },
    );
  }
  ready() {
    return (
      spawnSync(
        "docker",
        [
          "exec",
          this.args.container,
          "psql",
          "-X",
          "-U",
          "ai_phone",
          "-d",
          "ai_phone",
          "-tAc",
          "SELECT 1;",
        ],
        { stdio: "ignore" },
      ).status === 0
    );
  }
  env(root: string) {
    return {
      ...process.env,
      AI_PHONE_RECOVERY_CHILD: "1",
      AI_PHONE_PROJECT_ROOT: root,
      AI_PHONE_PG_PORT: String(this.args.port),
      RUST_TEST_THREADS: "1",
    };
  }
  fixture(name: string) {
    const database = "aiphone_recovery_" + randomUUID().replaceAll("-", "");
    this.names.push(database);
    this.sql("ai_phone", `CREATE DATABASE "${database}";`);
    const root = join(this.args.folder, name);
    mkdirSync(root, { mode: 0o700 });
    writeFileSync(join(root, ".isolated-recovery-test"), "");
    writeFileSync(
      join(root, ".env"),
      `POSTGRES_DB=${database}\nPOSTGRES_USER=ai_phone\nPOSTGRES_PASSWORD=isolated-fixture-password\n`,
      { mode: 0o600 },
    );
    const shared = join(this.args.folder, "role-credentials.json"),
      credentials = join(root, "data/database/credentials.json");
    if (existsSync(shared))
      atomicJson(credentials, {
        ...JSON.parse(readFileSync(shared, "utf8")),
        database,
      });
    const env = this.env(root);
    run([this.args.assistant, "--prepare-database"], {
      cwd: root,
      env,
      stdio: "pipe",
    });
    if (!existsSync(shared)) {
      copyFileSync(credentials, shared);
    }
    run([this.args.assistant, "--migrate"], { cwd: root, env, stdio: "pipe" });
    atomicJson(join(root, "data/model/credentials.json"), {
      api_key: "",
      base_url: `http://127.0.0.1:${this.model.server.port}/v1`,
      model: name,
      provider: "compatible",
      protocol: "responses",
    });
    const conversation = this.sql(
        database,
        "SELECT id FROM conversations LIMIT 1;",
      ),
      message = randomUUID(),
      runId = randomUUID(),
      context = literal([
        {
          role: "user",
          content: [{ type: "input_text", text: "你好，这是一轮本地校验。" }],
        },
      ]);
    this.sql(
      database,
      `INSERT INTO messages(id,conversation_id,role,text) VALUES('${message}','${conversation}','assistant',''); INSERT INTO runs(id,conversation_id,assistant_message_id,request_id,request_fingerprint,status,model_revision,context) VALUES('${runId}','${conversation}','${message}','${randomUUID()}','fixture','queued',1,${context}::jsonb);`,
    );
    return { root, database, runId, message };
  }
  start(root: string) {
    const log = openSync(
        join(root, "worker-" + randomUUID() + ".log"),
        "wx",
        0o600,
      ),
      child = Bun.spawn(
        [
          this.args.testBinary,
          "--exact",
          childTest,
          "--ignored",
          "--nocapture",
        ],
        { env: this.env(root), stdout: log, stderr: log },
      );
    this.processes.push({ child, log });
    return child;
  }
  async stop(child: ReturnType<typeof Bun.spawn>) {
    if (child.exitCode === null) child.kill("SIGKILL");
    await child.exited;
  }
  row(db: string, id: string) {
    return JSON.parse(
      this.sql(
        db,
        `SELECT row_to_json(x) FROM (SELECT status,step,loop_state,pending_tools,extract(epoch FROM next_wake_at)*1000 wake FROM runs WHERE id='${id}') x;`,
      ),
    );
  }
  async wait<T>(
    predicate: () => T | Promise<T>,
    timeout = 35,
  ): Promise<NonNullable<T>> {
    const end = Date.now() + timeout * 1000;
    while (Date.now() < end) {
      const value = await predicate();
      if (value) return value;
      for (const { child } of this.processes)
        if (
          child.exitCode !== null &&
          ![0, 137].includes(child.exitCode) &&
          child.signalCode !== "SIGKILL"
        )
          throw Error("Worker subprocess failed; inspect private logs");
      await Bun.sleep(150);
    }
    throw Error("Recovery check timed out; inspect private logs");
  }
  async waitReplay() {
    const { root, database: db, runId: id, message } = this.fixture("wait"),
      action = randomUUID(),
      call = {
        type: "function_call",
        call_id: "cached-write",
        local_action_id: action,
        name: "workspace_write",
        arguments: JSON.stringify({
          path: "never-rewrite.txt",
          content: "WRONG_REPLAY",
        }),
      },
      context = [
        {
          role: "user",
          content: [{ type: "input_text", text: "你好，这是一轮本地校验。" }],
        },
        call,
      ];
    this.sql(
      db,
      `UPDATE runs SET context=${literal(context)}::jsonb,pending_tools=${literal([call])}::jsonb WHERE id='${id}'; INSERT INTO harness_events(id,run_id,message_id,kind,title,status,result) VALUES('${action}','${id}','${message}','tool','cached','completed',${literal([{ type: "input_text", text: JSON.stringify({ success: true }) }])}::jsonb);`,
    );
    const file = join(root, "data/workspaces/admin/never-rewrite.txt");
    mkdirSync(join(file, ".."), { recursive: true });
    writeFileSync(file, "ORIGINAL_COMPLETED_RESULT");
    const before = statSync(file, { bigint: true }).mtimeNs;
    let child = this.start(root);
    const row = await this.wait(() => {
        const row = this.row(db, id);
        return row.wake ? row : null;
      }),
      wake = row.wake;
    await this.stop(child);
    child = this.start(root);
    await Bun.sleep(700);
    assert.equal(this.row(db, id).wake, wake);
    assert.equal(this.model.counts.wait, 1);
    run(["docker", "restart", this.args.container], { stdio: "ignore" });
    await this.wait(() => this.ready());
    assert.equal(this.row(db, id).wake, wake);
    process.kill(child.pid, "SIGSTOP");
    await Bun.sleep(1000);
    process.kill(child.pid, "SIGCONT");
    await this.wait(() => this.row(db, id).status === "paused", 25);
    await this.stop(child);
    assert.equal(readFileSync(file, "utf8"), "ORIGINAL_COMPLETED_RESULT");
    assert.equal(statSync(file, { bigint: true }).mtimeNs, before);
    assert.equal(
      this.sql(
        db,
        `SELECT count(*) FROM harness_events WHERE run_id='${id}' AND title='等待结束';`,
      ),
      "1",
    );
    assert.equal(this.model.counts.wait, 2);
    this.reports.push(
      "SIGKILL/restart + PostgreSQL restart + SIGSTOP/resume preserve wait and do not repeat a completed write",
    );
  }
  async unknown() {
    const { root, database: db, runId: id, message } = this.fixture("unknown"),
      action = randomUUID(),
      call = {
        type: "function_call",
        call_id: "unknown-write",
        local_action_id: action,
        name: "workspace_write",
        arguments: JSON.stringify({
          path: "forbidden.txt",
          content: "DO_NOT_RUN",
        }),
      };
    this.sql(
      db,
      `UPDATE runs SET status='running',context=${literal([{ role: "user", content: [{ type: "input_text", text: "你好" }] }, call])}::jsonb,pending_tools=${literal([call])}::jsonb WHERE id='${id}'; INSERT INTO harness_events(id,run_id,message_id,kind,title,status) VALUES('${action}','${id}','${message}','tool','unknown','running');`,
    );
    const child = this.start(root);
    await this.wait(() => this.row(db, id).status === "paused");
    await this.stop(child);
    assert(!existsSync(join(root, "data/workspaces/admin/forbidden.txt")));
    assert.equal(this.model.counts.unknown || 0, 0);
    this.reports.push(
      "Interrupted side effect with unknown result pauses without execution or model call",
    );
  }
  async provider() {
    for (const name of ["transient", "stream", "quota"]) {
      const { root, database: db, runId: id } = this.fixture(name);
      let child = this.start(root);
      if (name === "transient") {
        await this.wait(() => this.row(db, id).loop_state.model_retries === 1);
        const wake = this.row(db, id).wake;
        await this.stop(child);
        child = this.start(root);
        assert.equal(this.row(db, id).wake, wake);
      }
      await this.wait(() =>
        ["succeeded", "paused"].includes(this.row(db, id).status),
      );
      assert.equal(
        this.row(db, id).status,
        name === "quota" ? "paused" : "succeeded",
      );
      await this.stop(child);
      assert.equal(this.sql(db, "SELECT count(*) FROM memories;"), "0");
      assert.equal(
        this.model.counts[name],
        ({ transient: 3, stream: 2, quota: 1 } as Record<string, number>)[name],
      );
    }
    this.reports.push(
      "Provider retries persist; incomplete SSE does not execute tools; exhausted quota pauses",
    );
  }
  async outage() {
    const { root, database: db, runId: id } = this.fixture("database-outage"),
      child = this.start(root);
    await this.wait(() => this.model.hold.held, 15);
    run(["docker", "stop", "--time", "3", this.args.container], {
      stdio: "ignore",
    });
    this.model.hold.release();
    await this.wait(
      () =>
        readdirSync(root)
          .filter((name) => name.startsWith("worker-"))
          .some((name) =>
            readFileSync(join(root, name), "utf8").includes(
              "holding the checkpoint",
            ),
          ),
      20,
    );
    assert.equal(child.exitCode, null);
    run(["docker", "start", this.args.container], { stdio: "ignore" });
    await this.wait(() => this.ready());
    await this.wait(() =>
      ["failed", "paused"].includes(this.row(db, id).status),
    );
    await this.stop(child);
    assert.equal(this.sql(db, "SELECT count(*) FROM memories;"), "0");
    this.reports.push(
      "PostgreSQL outage preserves terminal checkpoint without tool execution",
    );
  }
  async longRun() {
    const { root, database: db, runId: id } = this.fixture("long");
    let child = this.start(root);
    await this.wait(() => this.model.longHold.held);
    assert.equal(this.sql(db, "SELECT count(*) FROM memories;"), "16");
    await this.stop(child);
    this.model.longHold.release();
    child = this.start(root);
    await this.wait(() => this.row(db, id).status === "paused", 55);
    assert(this.row(db, id).step >= 56);
    await this.stop(child);
    assert.equal(this.sql(db, "SELECT count(*) FROM memories;"), "56");
    assert.equal(
      this.sql(db, "SELECT count(DISTINCT content) FROM memories;"),
      "56",
    );
    this.reports.push(
      "56 committed effects survive slices and an interrupted model call exactly once",
    );
  }
  async close() {
    this.model.hold.release();
    this.model.longHold.release();
    for (const { child, log } of this.processes) {
      await this.stop(child);
      closeSync(log);
    }
    run(["docker", "start", this.args.container], { stdio: "ignore" });
    await this.wait(() => this.ready());
    for (const name of this.names)
      this.sql("ai_phone", `DROP DATABASE "${name}" WITH (FORCE);`);
  }
}
function literal(value: unknown) {
  return "'" + JSON.stringify(value).replaceAll("'", "''") + "'";
}
export async function verifyTaskRecovery(args: Arguments) {
  mkdirSync(args.folder, { mode: 0o700 });
  const model = new Model(),
    driver = new Driver(args, model);
  try {
    for (const check of [
      () => driver.waitReplay(),
      () => driver.unknown(),
      () => driver.provider(),
      () => driver.outage(),
      () => driver.longRun(),
    ])
      await check();
    const report = {
      passed: driver.reports,
      host_sleep_reboot_verified: false,
      physical_power_loss_verified: false,
      production_devices_or_conversations_changed: false,
    };
    atomicJson(join(args.folder, "report.json"), report);
    return report;
  } finally {
    await driver.close();
    model.close();
  }
}
