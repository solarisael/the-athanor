import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { z } from "@oh-my-pi/omptype/zod";
import { installOrihonTurns, ORIHON_COMPACTION_NOTE } from "../house-proof/orihon/turn.ts";
import { entryRecords, imageReferences, lastSourceIdBefore, replayRecords, type Entry } from "../house-proof/orihon/source.ts";
import { readRecordedImages, readTranscriptPage } from "../house-proof/orihon/transcript.ts";
import { markViewCache } from "../house-proof/orihon/cache.ts";

const temporary: string[] = [];
afterEach(() => {
  for (const directory of temporary.splice(0)) rmSync(directory, { recursive: true, force: true });
});

function transcript(entries: unknown[]): string {
  const directory = mkdtempSync(path.join(tmpdir(), "orihon-live-"));
  temporary.push(directory);
  const file = path.join(directory, "child.jsonl");
  writeFileSync(file, entries.map((entry) => JSON.stringify(entry)).join("\n") + "\n");
  return file;
}

const timestamp = "2026-10-23T14:00:00.000Z";

test("child zoom pages the middle of a long Unicode report instead of clipping it away", async () => {
  const text = "first\n" + "縫い目🌕".repeat(12_000) + "\nlast";
  const file = transcript([
    { type: "session", id: "child" },
    { id: "report", parentId: null, type: "message", timestamp, message: {
      role: "assistant", content: [{ type: "text", text }],
    } },
  ]);
  let offset: number | null = 0;
  const parts: string[] = [];
  while (offset !== null) {
    const page = await readTranscriptPage(file, "child", offset);
    for (const record of page.records) {
      expect(record.sourceId).toBe("report:talk");
      expect(Buffer.byteLength(record.text)).toBeLessThanOrEqual(30_000);
      parts.push(record.text);
    }
    if (page.nextOffset !== null) expect(page.nextOffset).toBeGreaterThan(offset);
    offset = page.nextOffset;
  }
  expect(parts.join("")).toBe(text);
});

test("image references preserve user words and refuse substituted source bytes", async () => {
  const content = [{ type: "text", text: "look at this" }, { type: "image", mimeType: "image/png", data: "YWJj" }];
  const entry: Entry = { id: "image", parentId: null, type: "message", timestamp,
    message: { role: "user", attribution: "user", content } };
  const captured = entryRecords(entry, "child", () => null);
  expect(captured[0].text).toBe("look at this");
  const manifest = JSON.parse(captured[1].text);
  expect(manifest.sourceEntry).toBe("image");
  const file = transcript([{ type: "session", id: "child" }, entry]);
  expect(await readRecordedImages(file, "child", manifest.sourceEntry, manifest.images)).toEqual([
    { type: "image", mimeType: "image/png", data: "YWJj" },
  ]);
  await expect(readRecordedImages(file, "another-session", "image", manifest.images)).rejects.toThrow("session");
  const references = imageReferences(content);
  content[1].data = "ZGVm";
  writeFileSync(file, [{ type: "session", id: "child" }, entry].map((value) => JSON.stringify(value)).join("\n") + "\n");
  await expect(readRecordedImages(file, "child", "image", references)).rejects.toThrow("no longer match");
});

test("dispatch folding preserves actual shared instructions and output constraints", () => {
  const context = "Do not edit the database; report the exact mismatched source.";
  const tasks = [{ name: "worker", task: "inspect the query", outputSchema: { type: "object", required: ["evidence"] } }];
  const entry: Entry = { id: "dispatch", parentId: null, type: "message", timestamp,
    message: { role: "assistant", content: [{ type: "toolCall", id: "call", name: "task", arguments: { context, tasks } }] } };
  const [record] = entryRecords(entry, "session", () => null);
  const projected = JSON.parse(record.foldText!);
  expect(projected.context).toBe(context);
  expect(projected.tasks).toEqual(tasks);
  expect(projected.audit).toBe(record.sourceId);

  const lesson = { id: 9, type: "coding", lesson: "full canonical body" };
  const supplied = `${context}\n[Codex — supplied lessons ride free and do not expand quest scope]\n[Lesson 1]\n${JSON.stringify(lesson)}`;
  entry.message.content[0].arguments.context = supplied;
  const [withLessons] = entryRecords(entry, "session", () => null);
  const compact = JSON.parse(withLessons.foldText!);
  expect(compact.context.instructions).toBe(context);
  expect(compact.context.suppliedLessons.count).toBe(1);
  expect(JSON.parse(withLessons.text).input.context).toBe(supplied);
});

test("a combined report retains navigation to every child beyond one native reference batch", () => {
  const jobs = Array.from({ length: 33 }, (_, index) => ({ jobId: `worker-${index}`, type: "task" }));
  const entry: Entry = {
    id: "combined", parentId: null, type: "custom_message", timestamp,
    customType: "async-result", display: true, content: "the complete combined report", details: { jobs },
  };
  const records = entryRecords(entry, "parent", () => null, (job) => `session-${job}`);
  expect(records[0].text).toBe("the complete combined report");
  expect(records.flatMap((record) => record.children ?? []).map((child) => child.name))
    .toEqual(jobs.map((job) => job.jobId));
  for (const record of records) expect(record.children!.length).toBeLessThanOrEqual(16);
  expect(new Set(records.map((record) => record.sourceId)).size).toBe(records.length);
});

async function turnFixture(fail: boolean, run: (handler: Function, ctx: any, messages: any[], provider: Function, requests: any[], controls: any) => Promise<void>) {
  const keys = ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID"] as const;
  const saved = Object.fromEntries(keys.map((key) => [key, process.env[key]]));
  const requests: any[] = [];
  const timers: ReturnType<typeof setTimeout>[] = [];
  const controls: any = { responseDelay: 0, prompt: "new question" };
  // The installed harness hands tools over as live omptype schemas, the same shape House tools build with pi.zod.
  const liveTools = [
    { name: "zoom", description: "open a line", parameters: z.object({ id: z.number().int().min(0).optional(), name: z.string().optional() }) },
    { name: "disabled", description: "not active", parameters: z.object({}) },
  ];
  const server = Bun.serve({
    hostname: "127.0.0.1", port: 0,
    fetch(request, server) { return server.upgrade(request) ? undefined : new Response("not found", { status: 404 }); },
    websocket: {
      message(socket, input) {
        const request = JSON.parse(String(input));
        requests.push(request);
        const response = JSON.stringify({
          correlation_id: request.message_id, command_or_event_type: "athanor.organ.result",
          ...(fail ? { error: { code: "orihon_view_not_ready", message: "required leaf is pending", retryable: true } } : {
            result: { ok: true, ready: true, purpose: "turn", complete: true, prefixChanged: false,
              instructionsVersion: "orihon-live-v1", chat: { revision: 1, units: 1, throughSeq: 1,
                bytes: 26, lines: 1, blocks: [], tail: "<chat>\n0+1|old summary\n</chat>" } },
          }),
        });
        if (controls.responseDelay) timers.push(setTimeout(() => socket.send(response), controls.responseDelay));
        else socket.send(response);
      },
    },
  });
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  process.env.ATHANOR_HOST_TOKEN = "test-only";
  process.env.ATHANOR_HOST_HOUSE_ID = "test-house";
  try {
    const prior = { role: "user", attribution: "user", content: "old question", timestamp: 1 };
    const answer = { role: "assistant", content: [{ type: "text", text: "old raw answer" }], timestamp: 2 };
    const current = { role: "user", attribution: "user", content: "new question", timestamp: 3 };
    const call = { role: "assistant", content: [{ type: "toolCall", id: "call", name: "read", arguments: {} }] };
    const reply = { role: "toolResult", toolCallId: "call", toolName: "read", content: "current result" };
    const entries = [prior, answer, current].map((message, index) => ({
      id: `entry${index}`, parentId: index ? `entry${index - 1}` : null, timestamp, type: "message", message,
    }));
    const binding = { room: "test", spirit: "Test", session: "session" };
    const mode = { mode: "fold", generation: "generation", instructions: "stable instructions" };
    const scope = { room: "test", spirit: "Test", sourceSession: "session", generation: "generation" };
    const identity = { key: "key", binding, session: "session", entries };
    const capture = {
      identity: () => identity, cachedMode: () => mode,
      captureNow: async () => ({ ...identity, mode, scope, state: { baselineId: null, cursor: null } }),
    };
    const hooks = new Map<string, Function>();
    installOrihonTurns({ on: (name: string, handler: Function) => hooks.set(name, handler),
      getAllTools: () => liveTools, getActiveTools: () => ["zoom"] }, capture as any, () => controls.prompt);
    const ctx = { model: { provider: "test", id: "test", api: "openai-responses" }, getSystemPrompt: () => ["stable instructions"],
      sessionManager: { getSessionId: () => "session" }, aborted: false,
      abort() { this.aborted = true; }, ui: { notify() {} } };
    Object.assign(controls, { hooks, capture, identity });
    await hooks.get("before_agent_start")!({}, ctx);
    await run(hooks.get("context")!, ctx, [prior, answer,
      { role: "compactionSummary", summary: "old compacted conversation" },
      { role: "custom", customType: "unknown-old-context", display: false, content: "old hidden history" },
      { role: "custom", customType: "tool-roster-notice", display: false, content: "authoritative tools" },
      current, call, reply], hooks.get("before_provider_request")!, requests, controls);
  } finally {
    for (const timer of timers) clearTimeout(timer);
    server.stop(true);
    for (const key of keys) {
      if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
    }
  }
}

test("the turn prefix carries each live tool's JSON Schema object, not the omptype callable JSON.stringify drops", async () => {
  await turnFixture(false, async (handler, ctx, messages, _provider, requests) => {
    await handler({ messages }, ctx);
    const turn = requests.find((request) => request.organ_request?.operation === "orihon_turn");
    expect(turn).toBeDefined();
    expect(turn.organ_request.params.prefix.tools).toEqual([{
      name: "zoom", description: "open a line",
      inputSchema: { type: "object", properties: { id: { type: "integer", minimum: 0 }, name: { type: "string" } } },
    }]);
  });
});

test("a new fold turn removes prior raw and compacted history but preserves current tool pairing", async () => {
  await turnFixture(false, async (handler, ctx, messages) => {
    const result = await handler({ messages }, ctx);
    expect(ctx.aborted).toBe(false);
    expect(result.messages[0].role).toBe("user");
    expect(result.messages[0].attribution).toBe("agent");
    expect(result.messages.slice(-3)).toEqual(messages.slice(-3));
    const text = JSON.stringify(result.messages);
    expect(text).toContain("old summary");
    expect(text).toContain("authoritative tools");
    for (const old of ["old question", "old raw answer", "old compacted conversation", "old hidden history"]) {
      expect(text).not.toContain(old);
    }
  });
});

test("a pending fold prevents provider dispatch rather than falling back to raw history", async () => {
  await turnFixture(true, async (handler, ctx, messages) => {
    const result = await handler({ messages }, ctx);
    expect(ctx.aborted).toBe(true);
    const text = JSON.stringify(result.messages);
    expect(text).toContain("orihon_view_not_ready");
    expect(text).not.toContain("old raw answer");
    expect(text).not.toContain("new question");
  });
});

test("the provider gate refuses raw dispatch when OMP skips or times out the context hook", async () => {
  await turnFixture(false, async (_handler, ctx, _messages, provider) => {
    await provider({ payload: { input: "unbounded raw transcript" } }, ctx);
    expect(ctx.aborted).toBe(true);
  });
});

test("side requests outside the agent loop neither abort nor change their payload", async () => {
  await turnFixture(false, async (handler, ctx, messages, provider, _requests, { hooks }) => {
    await handler({ messages }, ctx);
    await hooks.get("agent_end")({}, ctx);
    const payload = { input: "side request", tools: [] };
    expect(await provider({ payload }, ctx)).toBeUndefined();
    expect(payload).toEqual({ input: "side request", tools: [] });
    expect(ctx.aborted).toBe(false);
  });
});

test("the gate waits for the current context preparation", async () => {
  await turnFixture(false, async (handler, ctx, messages, provider, _requests, controls) => {
    controls.responseDelay = 20;
    const work = handler({ messages }, ctx);
    await provider({ payload: {} }, ctx);
    await work;
    expect(ctx.aborted).toBe(false);
  });
});

test("the observed wire tool set and order supplies the next prefix", async () => {
  await turnFixture(false, async (handler, ctx, messages, provider, requests, controls) => {
    await handler({ messages }, ctx);
    const tools = [
      { type: "function", name: "second", description: "second on registry", parameters: { type: "object" } },
      { type: "web_search" },
      { type: "function", name: "first", description: "first on registry", parameters: { type: "object", properties: { n: { type: "number" } } } },
    ];
    await provider({ payload: { tools } }, ctx);
    // The next turn's prefix is built before its own payload exists, so the observation outlives agent_end.
    await controls.hooks.get("agent_end")({}, ctx);
    controls.prompt = "a later question";
    const user = { role: "user", attribution: "user", content: controls.prompt, timestamp: 4 };
    controls.identity.entries.push({ id: "entry3", parentId: "entry2", timestamp, type: "message", message: user });
    await handler({ messages: [...messages, user] }, ctx);
    expect(requests.at(-1).organ_request.params.prefix.tools).toEqual([
      { name: "second", description: "second on registry", inputSchema: tools[0].parameters },
      { name: "first", description: "first on registry", inputSchema: tools[2].parameters },
    ]);
  });
});

test("identity failure outside live mode is tolerated but a known live identity fails closed", async () => {
  const hooks = new Map<string, Function>();
  const ctx = { sessionManager: { getSessionId: () => "never-live" }, aborted: false, abort() { this.aborted = true; } };
  installOrihonTurns({ on: (name: string, handler: Function) => hooks.set(name, handler) }, {
    identity() { throw new Error("identity missing"); },
  } as any, () => null);
  await hooks.get("before_agent_start")!({}, ctx);
  await hooks.get("before_provider_request")!({ payload: {} }, ctx);
  expect(ctx.aborted).toBe(false);

  await turnFixture(false, async (_handler, liveCtx, _messages, provider, _requests, controls) => {
    controls.capture.identity = () => { throw new Error("identity missing"); };
    await provider({ payload: {} }, liveCtx);
    expect(liveCtx.aborted).toBe(true);
  });
});

test("one preparation deadline includes capture time and a slow Host", async () => {
  await turnFixture(false, async (handler, ctx, messages, _provider, requests, controls) => {
    const capture = controls.capture.captureNow;
    let deadline = 0;
    controls.capture.captureNow = async (context: any, options: any) => {
      deadline = options.deadline;
      await Bun.sleep(30);
      return capture(context, options);
    };
    controls.responseDelay = 25_100;
    const started = Date.now();
    const result = await handler({ messages }, ctx);
    expect(deadline).toBeGreaterThanOrEqual(started + 25_000);
    expect(requests[0].organ_request.params.waitMs).toBeLessThan(20_000);
    expect(Date.now() - started).toBeLessThan(26_000);
    expect(ctx.aborted).toBe(true);
    expect(JSON.stringify(result)).not.toContain("new question");
  });
}, 30_000);

test("a superseded context cannot authorize a later request", async () => {
  await turnFixture(false, async (handler, ctx, messages, provider, _requests, controls) => {
    const capture = controls.capture.captureNow;
    let release!: () => void;
    let started!: () => void;
    const entered = new Promise<void>((resolve) => { started = resolve; });
    controls.capture.captureNow = async () => {
      started();
      await new Promise<void>((resolve) => { release = resolve; });
      return capture();
    };
    const old = handler({ messages }, ctx);
    await entered;
    await controls.hooks.get("agent_end")({}, ctx);
    release();
    expect(await old).toBeUndefined();
    await controls.hooks.get("before_agent_start")({}, ctx);
    await provider({ payload: {} }, ctx);
    expect(ctx.aborted).toBe(true);
  });
});

test("fold compaction returns the installed result shape and drops its fixed note from the continued turn", async () => {
  await turnFixture(false, async (handler, ctx, messages, _provider, requests, { hooks }) => {
    await handler({ messages }, ctx);
    const before = requests.length;
    const result = await hooks.get("session_before_compact")({
      preparation: { firstKeptEntryId: "kept", tokensBefore: 123 },
    }, ctx);
    expect(result).toEqual({ compaction: {
      summary: ORIHON_COMPACTION_NOTE, firstKeptEntryId: "kept", tokensBefore: 123,
    } });
    const remaining = { role: "assistant", content: "remaining work" };
    const continued = await handler({ messages: [
      { role: "compactionSummary", summary: result.compaction.summary }, remaining,
    ] }, ctx);
    expect(continued.messages.at(-1)).toEqual(remaining);
    expect(JSON.stringify(continued)).toContain("old summary");
    expect(JSON.stringify(continued)).not.toContain(ORIHON_COMPACTION_NOTE);
    expect(requests.length).toBe(before);
  });
});

test("the frontier includes the replay's thirty-third resolved child", () => {
  const jobs = Array.from({ length: 33 }, (_, index) => ({ jobId: `worker-${index}`, type: "task" }));
  const entry: Entry = { id: "combined", parentId: null, type: "custom_message", timestamp,
    customType: "async-result", display: true, content: "report", details: { jobs } };
  const resolveChild = (job: string) => `session-${job}`;
  const records = [...replayRecords([entry], "parent", null, null, { resolveChild })];
  expect(lastSourceIdBefore([entry], "parent", null, null, resolveChild)).toBe("combined:report:children:32");
  expect(records.at(-1)!.record.sourceId).toBe("combined:report:children:32");
});

test("a collision-suffixed job id resolves its child transcript by the artifact agentId", () => {
  const jobs = [{ jobId: "worker-2", agentId: "worker", type: "task" }, { jobId: "plain", type: "task" }];
  const entry: Entry = { id: "combined", parentId: null, type: "custom_message", timestamp,
    customType: "async-result", display: true, content: "report", details: { jobs } };
  const opened: string[] = [];
  const resolveChild = (artifact: string) => { opened.push(artifact); return artifact === "worker-2" ? null : `session-${artifact}`; };
  const records = [...replayRecords([entry], "parent", null, null, { resolveChild })];
  expect(opened).toEqual(["worker", "plain"]);
  expect(records[0]!.record.children).toEqual([{ name: "worker-2", session: "session-worker" }, { name: "plain", session: "session-plain" }]);
});

test("a split expanded dispatch folds its task once and leaves plain long messages lossless", () => {
  const context = "inspect\n[Codex — supplied lessons ride free and do not expand quest scope]\n[Lesson 1]\n"
    + JSON.stringify({ id: 9, type: "coding", lesson: "x".repeat(1_100_000) });
  const entry: Entry = { id: "dispatch", parentId: null, type: "message", timestamp, message: {
    role: "assistant", content: [{ type: "toolCall", id: "call", name: "task", arguments: {
      context, tasks: [{ name: "worker", task: "inspect" }],
    } }],
  } };
  const records = entryRecords(entry, "parent", () => null);
  expect(records.length).toBeGreaterThan(1);
  expect(JSON.parse(records[0].foldText!).tasks).toEqual([{ name: "worker", task: "inspect" }]);
  expect(JSON.parse(records[1].foldText!)).toEqual({
    continuationOf: "dispatch:tool:0", part: 2, of: records.length, exactAuditOnly: true,
  });
  const plain = entryRecords({ ...entry, message: { role: "user", attribution: "user", content: context } }, "parent", () => null);
  expect(plain.map((record) => record.text).join("")).toBe(context);
  expect(plain.every((record) => record.foldText === undefined)).toBe(true);
});

test("the cache anchor stays on the last whole block rather than the partial tail", () => {
  const control = { type: "ephemeral", ttl: "1h" };
  const whole = { type: "text", text: "4+1|a\n5+1|b\n6+1|c\n7+1|d\n" };
  const tail = { type: "text", text: "8+1|tail\n</chat>", cache_control: control };
  const current = { type: "text", text: "new user input", cache_control: control };
  const payload = {
    system: [{ type: "text", text: "system", cache_control: control }],
    tools: [{ name: "zoom", cache_control: control }],
    messages: [whole, tail, current].map((block) => ({ role: "user", content: [block] })),
  };
  const before = JSON.stringify(payload, (key, value) => key === "cache_control" ? undefined : value);
  expect(markViewCache(payload, whole.text).changed).toBe(true);
  expect((whole as any).cache_control).toEqual(control);
  expect(tail.cache_control).toBeUndefined();
  expect(current.cache_control).toEqual(control);
  expect(JSON.stringify(payload, (key, value) => key === "cache_control" ? undefined : value)).toBe(before);
  expect(markViewCache(payload, "a block not in the request").changed).toBe(false);
});
