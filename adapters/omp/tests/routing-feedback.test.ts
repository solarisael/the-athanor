import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { registerSolarisaelTools } from "../solarisael-house-proof/tools.ts";

type Schema = {
  describe(description: string): Schema;
  regex(pattern: RegExp): Schema;
  optional(): Schema;
  default(value: unknown): Schema;
};

type ToolResult = {
  isError?: boolean;
  content: Array<{ type: string; text: string }>;
  details: unknown;
};

type CapturedTool = {
  name: string;
  execute: (
    toolCallId: string,
    params: Record<string, unknown>,
    signal: AbortSignal | null,
    onUpdate: (update: unknown) => void,
    ctx: { cwd?: string },
  ) => Promise<ToolResult>;
  renderCall: (...args: unknown[]) => { render(width: number): string[] };
  renderResult: (...args: unknown[]) => { render(width: number): string[] };
};

function schema(): Schema {
  return {
    describe() { return this; },
    regex() { return this; },
    optional() { return this; },
    default() { return this; },
  };
}

const zod = {
  string: schema,
  boolean: schema,
  number: schema,
  enum: (_values: string[]) => schema(),
  array: (_element: Schema) => schema(),
  object: (_shape: Record<string, Schema>) => schema(),
  literal: (_value: string | boolean) => schema(),
  discriminatedUnion: (_key: string, _variants: Schema[]) => schema(),
};

const substrateEnv = "ATHANOR_SUBSTRATE_ROOT";
const executableEnv = "ATHANOR_SUBSTRATE_EXE";
const fixtureEnv = "SOLARISAEL_TEST_SUBSTRATE_HEALTH_SCRIPT";
const temporaryRoots: string[] = [];
const originalSubstrate = process.env[substrateEnv];
const originalExecutable = process.env[executableEnv];
const originalFixture = process.env[fixtureEnv];

function registeredTools(): CapturedTool[] {
  const tools: CapturedTool[] = [];
  registerSolarisaelTools({
    zod,
    registerTool(tool: CapturedTool) { tools.push(tool); },
  });
  return tools;
}

function toolJson(result: ToolResult) {
  return JSON.parse(result.content[0].text);
}

async function sleepingSubstrate() {
  const dir = await mkdtemp(path.join(os.tmpdir(), "omp-lane-status-timeout-"));
  temporaryRoots.push(dir);
  const fixture = path.join(dir, "health-fixture.js");
  await writeFile(
    fixture,
    "setTimeout(() => console.log('{}'), 8_000);\n",
    "utf8",
  );
  process.env[substrateEnv] = dir;
  process.env[executableEnv] = process.execPath;
  process.env[fixtureEnv] = fixture;
}

afterEach(async () => {
  if (originalSubstrate === undefined) delete process.env[substrateEnv];
  else process.env[substrateEnv] = originalSubstrate;
  if (originalExecutable === undefined) delete process.env[executableEnv];
  else process.env[executableEnv] = originalExecutable;
  if (originalFixture === undefined) delete process.env[fixtureEnv];
  else process.env[fixtureEnv] = originalFixture;
  await Promise.all(temporaryRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

describe("routing tool feedback", () => {
  test("bounds stalled lane health while retaining degraded diagnostics and routing receipts", async () => {
    await sleepingSubstrate();
    const tools = registeredTools();
    const laneStatus = tools.find((tool) => tool.name === "house_lane_status");
    const dispatch = tools.find((tool) => tool.name === "house_dispatch");
    expect(laneStatus).toBeDefined();
    expect(dispatch).toBeDefined();
    expect(laneStatus!.renderCall).toBeFunction();
    expect(laneStatus!.renderResult).toBeFunction();

    const updates: unknown[] = [];
    const startedAt = performance.now();
    const statusResult = await laneStatus!.execute("lane-status", {}, null, (update) => updates.push(update), { cwd: process.cwd() });
    const elapsedMs = performance.now() - startedAt;
    const status = toolJson(statusResult);

    expect(elapsedMs).toBeLessThan(4_500);
    expect(statusResult.isError).toBeUndefined();
    expect(statusResult.details).toEqual(status);
    expect(status).toMatchObject({
      ok: true,
      lanes: expect.any(Array),
      substrate: {
        ok: false,
        configured: true,
        mode: "degraded",
        reason: "Rust substrate health timed out",
        diagnostics: [{
          category: "operation",
          stage: "startup",
          expected: { command: "athanor-substrate health", timeoutMs: 3_000 },
          observed: { timedOut: true },
        }],
      },
    });
    expect(updates).toHaveLength(1);
    expect((updates[0] as ToolResult).details).toMatchObject({ status: "running", operation: "house_lane_status" });

    const dispatchResult = await dispatch!.execute("dispatch", {
      lane: "tester",
      task: "Exercise the existing routing receipt.",
      target: "tests/routing-feedback.test.ts",
      context: [{ mode: "exact", source: "tests/routing-feedback.test.ts", reason: "focused routing feedback coverage" }],
      acceptance: ["The receipt remains ready without spawning a worker."],
      risk: "low",
    }, null, () => {}, {});
    const receipt = toolJson(dispatchResult);

    expect(dispatchResult.isError).toBeUndefined();
    expect(dispatchResult.details).toEqual(receipt);
    expect(receipt).toMatchObject({
      ok: true,
      status: "ready",
      selector: { kind: "lane", value: "tester" },
      lane: "tester",
      dispatcher: { executed: false },
      spawnPacket: {
        tool: "task",
        args: { tasks: [{ name: "Gauge" }] },
      },
    });
    expect(receipt.spawnPacket.args.tasks[0]).not.toHaveProperty("agent");
  }, 5_000);
});
