import { describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import { EventEmitter } from "node:events";
import { PassThrough } from "node:stream";
import path from "node:path";
import { JsonObject, RustJsonlTransport, RustTransportError, RustTransportOutcomeUnknownError, TransportUnavailableError } from "../rust-transport.ts";

const fixture = `
const mode = process.argv[2];
const rl = require("node:readline").createInterface({ input: process.stdin });
rl.on("line", (line) => {
  const request = JSON.parse(line);
  if (process.env.RECORD_FILE) require("node:fs").appendFileSync(process.env.RECORD_FILE, line + "\\n");
  if (mode === "success") process.stdout.write(JSON.stringify({ protocol: 1, id: request.id, result: { echoed: request.params.value } }) + "\\n");
  else if (mode === "delayed") setTimeout(() => process.stdout.write(JSON.stringify({ protocol: 1, id: request.id, result: { echoed: request.params.value } }) + "\\n"), 40);
  else if (mode === "error") { process.stderr.write("diagnostic-too-long\\n"); process.stdout.write(JSON.stringify({ protocol: 1, id: request.id, error: { code: "NOPE", message: "nope", retryable: true, details: { value: 3 } } }) + "\\n"); }
  else if (mode === "malformed") process.stdout.write("not-json\\n");
  else if (mode === "both") process.stdout.write(JSON.stringify({ protocol: 1, id: request.id, result: true, error: null }) + "\\n");
  else if (mode === "neither") process.stdout.write(JSON.stringify({ protocol: 1, id: request.id }) + "\\n");
  else if (mode === "oversized") process.stdout.write("x".repeat(1024 * 1024 + 1));
  else if (mode === "duplicate") { const response = JSON.stringify({ protocol: 1, id: request.id, result: true }) + "\\n"; process.stdout.write(response + response); }
  else if (mode === "out-of-order") { setTimeout(() => process.stdout.write(JSON.stringify({ protocol: 1, id: request.id, result: request.params.value }) + "\\n"), request.params.delay); }
  else if (mode === "exit") process.exit(0);
  else if (mode === "exit-with-stderr") process.stderr.write("token=topsecret diagnostic tail\\n", () => process.exit(23));
  else if (mode === "signal") process.stderr.write("worker received signal\\n", () => process.kill(process.pid, "SIGTERM"));
  else if (mode === "stderr") { process.stderr.write("diagnostic\\n"); }
});
`;

async function withFixture<T>(mode: string, fn: (executable: string) => Promise<T>): Promise<T> {
  const dir = await mkdtemp(path.join(os.tmpdir(), "house-omp-transport-"));
  const file = path.join(dir, "fixture.cjs");
  await writeFile(file, fixture, "utf8");
  try { return await fn(file); } finally { await rm(dir, { recursive: true, force: true }); }
}
function transport(executable: string, mode: string) {
  return new RustJsonlTransport({ executable: process.execPath, args: [executable, mode] });
}

describe("Rust JSONL transport", () => {
  test("returns successful result", async () => withFixture("success", async (file) => {
    const client = transport(file, "success");
    await expect(client.request("remember", { value: "ok" })).resolves.toEqual({ echoed: "ok" });
    await client.close();
  }));

  test("propagates structured errors, bounded stderr, and compatibility fields", async () => withFixture("error", async (file) => {
    const client = new RustJsonlTransport({ executable: process.execPath, args: [file, "error"], stderrLimitBytes: 8 });
    const error = await client.request("remember", { value: 1 }).catch((reason) => reason);
    expect(error).toMatchObject({
      code: "NOPE",
      message: "nope",
      retryable: true,
      details: {
        value: 3,
        category: "operation",
        operation: "remember",
        owner: { component: "RustJsonlTransport", path: "rust-transport.ts", symbol: "RustJsonlTransport" },
        execution: { request_dispatched: true, write_outcome: "unknown", retry: "safe_now" },
      },
    });
    expect(error.stderr).toBe("diagnost");
    expect(error.details.observed.request).toEqual({ id: "1", method: "remember", dispatched: true });
    expect(error instanceof RustTransportError).toBe(true);
    await client.close();
  }));

  test("rejects malformed output and all pending calls", async () => withFixture("malformed", async (file) => {
    const client = transport(file, "malformed");
    await expect(client.request("remember", {})).rejects.toThrow("malformed JSON");
    await expect(client.request("remember", {})).rejects.toThrow("malformed JSON");
  }));
  test("rejects matched malformed valid envelopes without orphaning", async () => {
    for (const mode of ["both", "neither"]) await withFixture(mode, async (file) => {
      const client = transport(file, mode);
      await expect(client.request("remember", {})).rejects.toThrow("exactly one");
      await expect(client.request("remember", {})).rejects.toThrow("exactly one");
    });
  });

  test("isolates circular local serialization failures", async () => withFixture("out-of-order", async (file) => {
    const client = transport(file, "out-of-order");
    const first = client.request("remember", { value: "ok", delay: 10 });
    const circular: JsonObject = {};
    circular.self = circular;
    await expect(client.request("remember", circular)).rejects.toThrow();
    await expect(first).resolves.toEqual("ok");
    await client.close();
  }));

  test("bounds oversized output without exposing its content", async () => withFixture("oversized", async (file) => {
    const client = transport(file, "oversized");
    const error = await client.request("remember", {}).catch((reason) => reason);
    expect(error).toMatchObject({
      code: "RUST_TRANSPORT_OUTPUT_TOO_LARGE",
      message: expect.stringContaining("exceeds maximum"),
      retryable: false,
      details: {
        category: "protocol",
        stage: "request_parse",
        execution: { request_dispatched: true, write_outcome: "unknown", retry: "after_change" },
      },
    });
    expect(error.details.expected).toEqual({ stdout_line_bytes_at_most: 1024 * 1024 });
    expect(error.details.observed).toMatchObject({ stdout_content: "omitted", request: { id: "1", method: "remember", dispatched: true } });
  }));

  test("validates timeout before spawning and rejects overflow", async () => withFixture("success", async (file) => {
    const client = transport(file, "success");
    await expect(client.request("remember", {}, { timeoutMs: -1 })).rejects.toThrow("timeoutMs");
    await expect(client.request("remember", {}, { timeoutMs: Number.MAX_SAFE_INTEGER })).rejects.toThrow("timeoutMs");
    await expect(client.request("remember", { value: "still-works" })).resolves.toEqual({ echoed: "still-works" });
    await client.close();
  }));

  test("pre-aborted request with timeout does not spawn or poison the transport", async () => withFixture("success", async (file) => {
    const client = transport(file, "success");
    const controller = new AbortController();
    controller.abort();

    await expect(client.request("remember", {}, { signal: controller.signal, timeoutMs: 2_147_483_647 })).rejects.toThrow("cancelled");
    await expect(client.request("remember", { value: "still-works" })).resolves.toEqual({ echoed: "still-works" });
    await client.close();
  }));

  test("definitive requests ignore post-dispatch abort but honor pre-abort", async () => withFixture("delayed", async (file) => {
    const recordFile = path.join(os.tmpdir(), `house-omp-transport-${Date.now()}-${Math.random()}.log`);
    const client = new RustJsonlTransport({
      executable: process.execPath,
      args: [file, "delayed"],
      env: { RECORD_FILE: recordFile },
    });
    const controller = new AbortController();
    const response = client.request("remember", { value: "committed" }, {
      signal: controller.signal,
      timeoutMs: 1000,
      settleDefinitively: true,
    });
    setTimeout(() => controller.abort(), 5);
    await expect(response).resolves.toEqual({ echoed: "committed" });
    await expect(client.request("remember", { value: "still-alive" }, { settleDefinitively: true })).resolves.toEqual({ echoed: "still-alive" });

    const preAborted = new AbortController();
    preAborted.abort();
    const error = await client.request("remember", { value: "not-sent" }, {
      signal: preAborted.signal,
      settleDefinitively: true,
    }).catch((reason) => reason);
    expect(error).toMatchObject({
      code: "RUST_TRANSPORT_REQUEST_CANCELLED",
      message: "Rust transport request was cancelled",
      retryable: true,
      details: {
        operation: "remember",
        execution: { request_dispatched: false, write_outcome: "not_started", retry: "safe_now" },
      },
    });
    await new Promise((resolve) => setTimeout(resolve, 10));
    const records = await Bun.file(recordFile).exists() ? await Bun.file(recordFile).text() : "";
    expect(records).not.toContain("not-sent");
    await client.close();
    await rm(recordFile, { force: true });
  }));

  test("definitive timeout reports unknown outcome, reconciliation, and evicts the worker", async () => withFixture("delayed", async (file) => {
    const client = transport(file, "delayed");
    const started = performance.now();
    const response = client.request("remember", { value: "uncertain" }, {
      timeoutMs: 10,
      settleDefinitively: true,
    });
    const error = await response.catch((reason) => reason);
    const elapsed = performance.now() - started;
    expect(error).toBeInstanceOf(RustTransportOutcomeUnknownError);
    expect(error).toMatchObject({
      code: "AUTHORITATIVE_OUTCOME_UNKNOWN",
      retryable: false,
      details: {
        category: "transport",
        stage: "request_parse",
        operation: "remember",
        owner: { component: "RustJsonlTransport", path: "rust-transport.ts", symbol: "RustJsonlTransport" },
        execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" },
      },
    });
    expect(error.message).toContain("authoritative outcome is unknown");
    expect(error.details.observed).toMatchObject({
      request: { id: "1", method: "remember", dispatched: true },
      timeout: "after_dispatch",
      transport_failure_code: "RUST_TRANSPORT_REQUEST_TIMEOUT",
    });
    expect(elapsed).toBeLessThan(250);
    expect(client.usable).toBe(false);
    const unavailable = await client.request("remember", { value: "must-not-reuse" }).catch((reason) => reason);
    expect(unavailable).toMatchObject({
      code: "AUTHORITATIVE_OUTCOME_UNKNOWN",
      message: expect.stringContaining("unusable"),
      retryable: false,
      details: { execution: { request_dispatched: false, write_outcome: "not_started", retry: "reconcile_first" } },
    });
  }));

  test("bounds concurrent pending requests", async () => withFixture("out-of-order", async (file) => {
    const client = transport(file, "out-of-order");
    const requests = Array.from({ length: 1025 }, (_, index) => client.request("remember", { value: index, delay: 1000 }));
    const outcomes = await Promise.allSettled(requests);
    expect(outcomes.some((outcome) => outcome.status === "rejected" && String((outcome as PromiseRejectedResult).reason.message).includes("limit"))).toBe(true);
    await client.close();
  }));

  test("pre-dispatch cancellation stays usable while timeout poisons the transport", async () => withFixture("success", async (file) => {
    const client = transport(file, "success");
    const controller = new AbortController();
    controller.abort();

    await expect(client.request("remember", {}, { signal: controller.signal })).rejects.toThrow("cancelled");
    await expect(client.request("remember", { value: "still-works" })).resolves.toEqual({ echoed: "still-works" });
    await client.close();

    const timeoutClient = transport(file, "success");
    await expect(timeoutClient.request("remember", {}, { timeoutMs: 1 })).rejects.toThrow("timed out");
    await expect(timeoutClient.request("remember", {})).rejects.toThrow("timed out");
    await timeoutClient.close();
  }));

  test("marks post-dispatch cancellation as safely retryable", async () => withFixture("delayed", async (file) => {
    const client = transport(file, "delayed");
    const controller = new AbortController();
    const response = client.request("remember", { value: "cancelled-after-write" }, { signal: controller.signal });
    setTimeout(() => controller.abort(), 5);
    const error = await response.catch((reason) => reason);
    expect(error).toMatchObject({
      code: "RUST_TRANSPORT_REQUEST_CANCELLED",
      message: "Rust transport request was cancelled",
      retryable: true,
      details: {
        operation: "remember",
        observed: {
          cancellation: "after_dispatch",
          request: { id: "1", method: "remember", dispatched: true },
        },
        execution: { request_dispatched: true, write_outcome: "unknown", retry: "safe_now" },
      },
    });
    await client.close();
  }));

  test("correlates concurrent out-of-order responses and rejects duplicates", async () => withFixture("out-of-order", async (file) => {
    const client = transport(file, "out-of-order");
    const first = client.request("remember", { value: "slow", delay: 25 });
    const second = client.request("remember", { value: "fast", delay: 1 });
    await expect(Promise.all([first, second])).resolves.toEqual(["slow", "fast"]);
    await client.close();
  }));

  test("reports child exit code and bounded, redacted stderr evidence", async () => withFixture("exit-with-stderr", async (file) => {
    const client = new RustJsonlTransport({
      executable: process.execPath,
      args: [file, "exit-with-stderr"],
      stderrLimitBytes: 16,
    });
    const error = await client.request("remember", {}).catch((reason) => reason);
    expect(error).toBeInstanceOf(TransportUnavailableError);
    expect(error).toMatchObject({
      code: "RUST_TRANSPORT_CHILD_EXITED",
      message: "Rust transport child exited (23)",
      retryable: true,
      details: {
        category: "transport",
        stage: "request_parse",
        operation: "remember",
        observed: {
          exit_code: 23,
          signal: null,
          request: { id: "1", method: "remember", dispatched: true },
          stderr: { bytes_collected: 16, limit_bytes: 16, truncated: true },
        },
        execution: { request_dispatched: true, write_outcome: "unknown", retry: "safe_now" },
      },
    });
    expect(error.details.evidence).toContainEqual(expect.objectContaining({ type: "process_exit", exit_code: 23, signal: null }));
    expect(error.details.evidence).toContainEqual(expect.objectContaining({ type: "stderr", limit_bytes: 16, truncated: true }));
    expect(error.stderr).not.toContain("topsecret");
    expect(Buffer.byteLength(error.stderr)).toBeLessThanOrEqual(16);
  }));


  test("reports spawn targets and pre-dispatch state", async () => withFixture("success", async (file) => {
    const cwd = path.dirname(file);
    const executable = path.join(cwd, "missing-rust-transport");
    const client = new RustJsonlTransport({ executable, cwd });
    const error = await client.request("remember", { value: "not-sent" }).catch((reason) => reason);
    expect(error).toBeInstanceOf(TransportUnavailableError);
    expect(error).toMatchObject({
      code: "RUST_TRANSPORT_SPAWN_FAILED",
      message: "Unable to start Rust transport process",
      retryable: true,
      details: {
        category: "transport",
        stage: "spawn",
        operation: "remember",
        owner: { component: "RustJsonlTransport", path: "rust-transport.ts", symbol: "RustJsonlTransport" },
        execution: { request_dispatched: false, write_outcome: "not_started", retry: "safe_now" },
      },
    });
    expect(error.details.observed.request).toEqual({ id: "1", method: "remember", dispatched: false });
    expect(error.details.targets).toEqual(expect.arrayContaining([executable, cwd, "rust-transport.ts#RustJsonlTransport"]));
  }));

  test("duplicate response IDs make transport unusable", async () => withFixture("duplicate", async (file) => {
    const client = transport(file, "duplicate");
    const outcomes = await Promise.allSettled([
      client.request("remember", {}),
      client.request("remember", {}),
    ]);
    expect(outcomes[0]).toMatchObject({ status: "fulfilled", value: true });
    expect(outcomes[1]).toMatchObject({ status: "rejected" });
    expect((outcomes[1] as PromiseRejectedResult).reason.message).toContain("unknown or duplicate");
    await client.close();
  }));
  test("close is idempotent and waits for observed child exit", async () => {
    const child = Object.assign(new EventEmitter(), {
      stdin: new PassThrough(),
      stdout: new PassThrough(),
      stderr: new PassThrough(),
      pid: undefined,
      kill: () => true,
    });
    const client = new RustJsonlTransport({
      executable: "synthetic-rust-worker",
      shutdownGraceMs: 50,
      shutdownTimeoutMs: 100,
      spawnProcess: (() => {
        queueMicrotask(() => child.emit("spawn"));
        return child;
      }) as any,
    });
    const request = client.request("remember", {}).catch(() => undefined);
    const closing = client.close();
    expect(client.close()).toBe(closing);
    let settled = false;
    void closing.finally(() => { settled = true; });
    await Bun.sleep(5);
    expect(settled).toBe(false);
    child.emit("close", 0, null);
    await expect(closing).resolves.toBeUndefined();
    await request;
  });

  test("close force-kills a stubborn child after the grace period", async () => {
    const signals: string[] = [];
    const child = Object.assign(new EventEmitter(), {
      stdin: new PassThrough(),
      stdout: new PassThrough(),
      stderr: new PassThrough(),
      pid: undefined,
      kill: (signal: string) => {
        signals.push(signal);
        queueMicrotask(() => child.emit("close", null, signal));
        return true;
      },
    });
    const client = new RustJsonlTransport({
      executable: "synthetic-rust-worker",
      shutdownGraceMs: 1,
      shutdownTimeoutMs: 100,
      spawnProcess: (() => child) as any,
    });
    const request = client.request("remember", {}).catch(() => undefined);
    await expect(client.close()).resolves.toBeUndefined();
    expect(signals).toEqual(["SIGKILL"]);
    await request;
  });

  test("close rejects when force-kill produces no exit receipt", async () => {
    const child = Object.assign(new EventEmitter(), {
      stdin: new PassThrough(),
      stdout: new PassThrough(),
      stderr: new PassThrough(),
      pid: undefined,
      kill: () => true,
    });
    const client = new RustJsonlTransport({
      executable: "synthetic-rust-worker",
      shutdownGraceMs: 1,
      shutdownTimeoutMs: 10,
      spawnProcess: (() => child) as any,
    });
    const request = client.request("remember", {}).catch(() => undefined);
    await expect(client.close()).rejects.toThrow("shutdown deadline");
    await request;
  });


  test("preserves reported child termination signals", async () => {
    const child = Object.assign(new EventEmitter(), {
      stdin: new PassThrough(),
      stdout: new PassThrough(),
      stderr: new PassThrough(),
      killed: false,
      pid: undefined,
    });
    child.stdin.once("data", () => queueMicrotask(() => child.emit("close", null, "SIGTERM")));
    const client = new RustJsonlTransport({
      executable: "synthetic-rust-worker",
      cwd: "synthetic-cwd",
      spawnProcess: (() => {
        queueMicrotask(() => child.emit("spawn"));
        return child;
      }) as any,
    });
    const error = await client.request("remember", {}).catch((reason) => reason);
    expect(error).toMatchObject({
      code: "RUST_TRANSPORT_CHILD_EXITED",
      retryable: true,
      details: {
        observed: {
          signal: "SIGTERM",
          exit_code: null,
          request: { id: "1", method: "remember", dispatched: true },
        },
        evidence: expect.arrayContaining([expect.objectContaining({ type: "process_exit", signal: "SIGTERM", exit_code: null })]),
      },
    });
  });
});
