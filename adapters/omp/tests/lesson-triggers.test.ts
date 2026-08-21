// Adversarial proofs for the OMP lesson-trigger taps.
//
// The transport is faked ONLY as a dumb pipe: a real child process, spawned
// through the real RustJsonlTransport, that writes canned NDJSON chosen by its
// argv mode. It never inspects params, never matches anything, never filters a
// row — every `if` inside a fake is production logic that escaped testing.
// Matching itself lives in Rust and is proved at the PostgreSQL boundary in
// crates/house-substrate/tests/lesson_trigger_integration.rs.
//
// Every test names the mutation it kills and carries a `red-proof:` line: the
// exact edit that must make it fail.

import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test";

import { RustJsonlTransport } from "../rust-transport.ts";

let effectiveRoomDir = "";

mock.module("../solarisael-house-proof/room.ts", () => ({
  applyPromptDirectives: async () => ({ state: { operator: "Sol", embodiedSpirit: "Kodo" } }),
  roomContext: () => ({ room: "kodo", spirit: "Kodo", operator: "Sol", effectiveRoomDir }),
  writeActiveSpiritSnapshot: async () => undefined,
}));
mock.module("../solarisael-house-proof/conversation-log.ts", () => ({
  logConversationWindow: async () => ({ fresh: false, loggedTurns: [] }),
}));
mock.module("../giga.ts", () => ({
  closeGigaTransports: async () => undefined,
  ingestGigaLoggedTurnsDetached: () => undefined,
}));
mock.module("../solarisael-house-proof/recall.ts", () => ({
  closeRustRecallTransports: () => undefined,
  recallWithRouting: async () => ({ ok: false, result: null }),
}));
mock.module("../solarisael-house-proof/tools.ts", () => ({
  closeRustRememberTransports: () => undefined,
  registerSolarisaelTools: () => undefined,
  writeRustMemory: async () => undefined,
}));
mock.module("../solarisael-house-proof/hallway.ts", () => ({
  projectHallwayInbox: async () => ({
    changed: false,
    inbox: { ok: true, hallways: [] },
  }),
}));
mock.module("../solarisael-house-proof/anamnesis.ts", () => ({
  closeRustAnamnesisTransports: () => undefined,
  formatAnamnesisContext: () => "",
  queryAnamnesis: async () => ({ ok: true }),
}));
mock.module("../solarisael-house-proof/substrate.ts", () => ({
  catchBoat: async () => ({ ok: true, found: false }),
  closePaperBoatTransports: () => undefined,
}));
mock.module("../solarisael-house-proof/entity-resolution.ts", () => ({
  resolveEntities: async () => ({ ok: true, matches: [] }),
}));
mock.module("../solarisael-house-proof/recall-telemetry.ts", () => ({
  recordRecallTelemetry: async () => true,
}));
mock.module("../solarisael-house-proof/triggers.ts", () => ({
  processLessonsReminder: async () => null,
}));
mock.module("../solarisael-house-proof/context.ts", () => ({
  analyzeContext: async () => ({
    route: {
      entityResolutionSuggested: false,
      intent: "technical_project",
      terms: [],
      requiredTerms: [],
      recognizedEntities: [],
    },
  }),
  applyRecallViewport: async () => ({ presentation: { found: [], warnings: [] }, diagnostics: {} }),
}));
mock.module("../solarisael-house-proof/recall-policy.ts", () => ({
  RecallPolicyHostClient: class {
    async inspect() { return { recallPolicy: null }; }
    async evaluate() { return { decision: { shouldRecall: false }, snapshot: { recallPolicy: null } }; }
    async completeRefresh() { return { recallPolicy: null }; }
    async failRefresh() { return { recallPolicy: null }; }
    async invalidateAfterCompaction() { return undefined; }
  },
  isMutateTool: () => false,
  markToolEvidence: () => undefined,
  hasToolEvidence: () => false,
}));

const {
  closeLessonTriggerTransports,
  extractToolSurfaces,
  filterInterruptedLessonProse,
  lessonTriggerMessageRenderer,
  lessonTriggerProseStreamUpdate,
  lessonTriggerToolCall,
  prependLessonReminder,
  resetLessonTriggerProseStream,
  takeLessonReminder,
} = await import("../solarisael-house-proof/lesson-triggers.ts");

// The pipe. argv[2] picks one canned byte sequence; the request is recorded
// verbatim and otherwise ignored. No branch here depends on the payload.
const fixture = `
const mode = process.argv[2];
const record = process.env.RECORD_FILE;
const fired = {
  block: [{
    family: "coding", id: 316, title: "Zero inference budget",
    lesson: "A missing path is a halt-and-ask, not permission to invent continuation behavior.",
    proofPattern: "unmapped seam", urgency: "block", surface: "tool",
    path: "src/danger.ts", patternKind: "regex", pattern: "invent"
  }],
  remind: [{
    family: "coding", id: 240, title: "A fake must be dumb",
    lesson: "Its job is to return rows, not to filter them.",
    proofPattern: null, urgency: "remind", surface: "tool",
    path: "src/danger.ts", patternKind: "regex", pattern: "if \\\\("
  }],
  prose: [{
    family: "coding", id: 19, title: "Ugly interop in one named place",
    lesson: "Keep the adapter seam in a single named module.",
    proofPattern: null, urgency: "remind", surface: "prose",
    path: null, patternKind: "regex", pattern: "interop"
  }],
  proseBlock: [{
    family: "writing", id: 408, title: "Retire the canned antithesis",
    lesson: "Rewrite the local sentence through a different structure.",
    proofPattern: null, urgency: "block", surface: "prose",
    path: null, patternKind: "regex", pattern: "negative hinge"
  }],
  proseBlockTrim: [{
    family: "writing", id: 408, title: "Retire the canned antithesis",
    lesson: "Rewrite the local sentence through a different structure.",
    proofPattern: null, urgency: "block", surface: "prose",
    path: null, patternKind: "regex", pattern: "negative hinge",
    matchStart: 39, surfaceIndex: 0
  }],
  proseBlockFirst: [{
    family: "writing", id: 408, title: "Retire the canned antithesis",
    lesson: "Rewrite the local sentence through a different structure.",
    proofPattern: null, urgency: "block", surface: "prose",
    path: null, patternKind: "regex", pattern: "negative hinge",
    matchStart: 4, surfaceIndex: 0
  }],
  proseBlockAst: [{
    family: "writing", id: 408, title: "Retire the canned antithesis",
    lesson: "Rewrite the local sentence through a different structure.",
    proofPattern: null, urgency: "block", surface: "prose",
    path: null, patternKind: "ast", pattern: "negative hinge",
    matchStart: null, surfaceIndex: 0
  }],
  proseBlockBlank: [{
    family: "writing", id: 408, title: "Retire the canned antithesis",
    lesson: "Rewrite the local sentence through a different structure.",
    proofPattern: null, urgency: "block", surface: "prose",
    path: null, patternKind: "regex", pattern: "negative hinge",
    matchStart: 42, surfaceIndex: 0
  }],
  quiet: []
};
const rl = require("node:readline").createInterface({ input: process.stdin });
rl.on("line", (line) => {
  if (record) require("node:fs").appendFileSync(record, line + "\\n");
  if (mode === "garbage") { process.stdout.write("not-json\\n"); return; }
  if (mode === "silent") return;
  if (mode === "error") {
    const id = JSON.parse(line).id;
    process.stdout.write(JSON.stringify({ protocol: 1, id, error: { code: "SUBSTRATE_DOWN", message: "no database", retryable: false } }) + "\\n");
    return;
  }
  const id = JSON.parse(line).id;
  const response = JSON.stringify({
    protocol: 1, id,
    result: { ok: true, fired: fired[mode] || [], warnings: [] }
  }) + "\\n";
  if (mode === "slowQuiet") {
    setTimeout(() => process.stdout.write(response), 25);
    return;
  }
  process.stdout.write(response);
});
`;

const originalRequest = RustJsonlTransport.prototype.request;
const originalExecutable = process.env.ATHANOR_SUBSTRATE_EXE;
const originalKillSwitch = process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS;

let fixtureDir = "";
let recordFile = "";
let pipe: RustJsonlTransport | null = null;
let observed: Array<{ method: string; params: unknown }> = [];

async function installPipe(mode: string): Promise<void> {
  const file = path.join(fixtureDir, "fixture.cjs");
  await writeFile(file, fixture, "utf8");
  pipe = new RustJsonlTransport({
    executable: process.execPath,
    args: [file, mode],
    env: { RECORD_FILE: recordFile },
  });
}

async function recordedRequests(): Promise<string[]> {
  try {
    const text = await readFile(recordFile, "utf8");
    return text.split("\n").filter((line) => line.trim().length > 0);
  } catch {
    return [];
  }
}

function toolCall(toolName: string, input: unknown, toolCallId = "call-1") {
  return { toolName, toolCallId, input };
}

beforeEach(async () => {
  fixtureDir = await mkdtemp(path.join(os.tmpdir(), "athanor-lesson-triggers-"));
  effectiveRoomDir = fixtureDir;
  recordFile = path.join(fixtureDir, "requests.ndjson");
  observed = [];
  process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
  delete process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS;
  RustJsonlTransport.prototype.request = function (method: string, params: unknown, options?: unknown) {
    observed.push({ method, params });
    if (!pipe) throw new Error("test bug: no fixture pipe installed");
    return (originalRequest as any).call(pipe, method, params, options);
  } as typeof RustJsonlTransport.prototype.request;
});

afterEach(async () => {
  RustJsonlTransport.prototype.request = originalRequest;
  closeLessonTriggerTransports();
  if (pipe) await pipe.close().catch(() => undefined);
  pipe = null;
  if (originalExecutable === undefined) delete process.env.ATHANOR_SUBSTRATE_EXE;
  else process.env.ATHANOR_SUBSTRATE_EXE = originalExecutable;
  if (originalKillSwitch === undefined) delete process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS;
  else process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS = originalKillSwitch;
  await rm(fixtureDir, { recursive: true, force: true });
  effectiveRoomDir = "";
});

describe("lesson trigger tool_call tap", () => {
  // Kills: the block verdict dropped (tap returns undefined on a block) or
  // rendered without the coordinates the model needs to obey it — lesson ref,
  // title, lesson body.
  // red-proof: return `undefined` instead of `{ block: true, reason }` for
  // urgency "block" in lessonTriggerToolCall.
  test("renders a block verdict carrying the lesson coordinates", async () => {
    await installPipe("block");
    const verdict = await lessonTriggerToolCall(
      toolCall("write", { path: "src/danger.ts", content: "invent the missing path" }),
      { cwd: effectiveRoomDir },
    );
    expect(verdict).toBeDefined();
    expect(verdict!.block).toBe(true);
    expect(verdict!.reason).toContain('reason="lesson_violation"');
    expect(verdict!.reason).toContain('lesson="coding#316"');
    expect(verdict!.reason).toContain("Zero inference budget");
    expect(verdict!.reason).toContain("halt-and-ask");
    expect(observed.map((entry) => entry.method)).toEqual(["lesson_trigger_match"]);
  });

  // Kills: fail-closed regression. OMP treats a throwing tool_call handler as a
  // BLOCK, so an unwrapped await on a dead substrate silently locks every edit
  // and write in the session. All three real failure shapes are exercised
  // through the actual child: unparseable bytes, no answer at all (300ms
  // timeout), and a structured error envelope.
  // red-proof: remove the try/catch around the transport call in
  // lessonTriggerToolCall (or let one `await` escape it).
  test("passes through on garbage, silence, and structured transport errors", async () => {
    for (const mode of ["garbage", "silent", "error"]) {
      await rm(recordFile, { force: true });
      observed = [];
      await installPipe(mode);
      const verdict = await lessonTriggerToolCall(
        toolCall("write", { path: "src/danger.ts", content: "invent the missing path" }),
        { cwd: effectiveRoomDir },
      );
      expect(verdict, `${mode} must pass through`).toBeUndefined();
      expect(observed).toHaveLength(1);
      expect(await recordedRequests(), `${mode} must have reached the child`).toHaveLength(1);
      closeLessonTriggerTransports();
      await pipe!.close().catch(() => undefined);
      pipe = null;
    }
  });

  // Kills: the kill switch checked after the request is already in flight, so a
  // disabled tap still spawns the substrate and still pays the latency. The
  // proof is the child's own record file: zero bytes crossed the pipe.
  // red-proof: move the `lessonTriggersDisabled()` check below the
  // `transport.request(...)` call.
  test("kill switch prevents the transport call entirely", async () => {
    await installPipe("block");
    process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS = "1";
    const verdict = await lessonTriggerToolCall(
      toolCall("write", { path: "src/danger.ts", content: "invent the missing path" }),
      { cwd: effectiveRoomDir },
    );
    expect(verdict).toBeUndefined();
    expect(observed).toHaveLength(0);
    expect(await recordedRequests()).toHaveLength(0);
  });

  // Kills: the v1 tool ceiling dropped, so every bash/read/glob call round-trips
  // to PostgreSQL. Also pins the empty-payload branch: absent, blank and
  // malformed inputs must not become a request that matches everything.
  // red-proof: delete the `toolName !== "edit" && toolName !== "write"` guard,
  // or the `if (!surfaces.length) return undefined` early exit.
  test("never calls the substrate for out-of-ceiling tools or empty payloads", async () => {
    await installPipe("block");
    const passthroughs = [
      toolCall("bash", { command: "rm -rf /", content: "invent" }),
      toolCall("read", { path: "src/danger.ts" }),
      toolCall("write", { path: "src/danger.ts", content: "   " }),
      toolCall("write", null),
      toolCall("edit", { input: "" }),
      toolCall("edit", { input: 42 }),
    ];
    for (const event of passthroughs) {
      await expect(lessonTriggerToolCall(event, { cwd: effectiveRoomDir })).resolves.toBeUndefined();
    }
    expect(observed).toHaveLength(0);
    expect(await recordedRequests()).toHaveLength(0);
  });

  // Kills: hashline sections flattened into one pathless blob, which throws away
  // the per-section path the Rust side needs to infer a language (and makes
  // every ast pattern skip with a warning forever).
  // red-proof: make editSurfaces return a single surface with no `path`.
  test("extracts one edit surface per hashline section with its own path", () => {
    const surfaces = extractToolSurfaces("edit", {
      input: "[src/a.ts#1A2B]\nPUT 1.=1:\n+const a = 1;\n[db/schema.sql#3C4D]\nPUT 2.=2:\n+SELECT 1;\n",
    });
    expect(surfaces.map((surface) => surface.path)).toEqual(["src/a.ts", "db/schema.sql"]);
    expect(surfaces.every((surface) => surface.kind === "tool" && surface.tool === "edit")).toBe(true);
    expect(surfaces[0].text).toContain("const a = 1;");
    expect(surfaces[0].text).not.toContain("SELECT 1;");
    expect(surfaces[1].text).toContain("SELECT 1;");
  });
});

describe("lesson trigger reminder flow", () => {
  // Kills: a remind verdict escalated into a block (the model gets stopped for
  // an advisory lesson) and a reminder that is never stashed for the result tap.
  // red-proof: drop the urgency check and return `{ block: true, reason }` for
  // every fire.
  test("remind never blocks and is stashed for the matching tool_result", async () => {
    await installPipe("remind");
    const verdict = await lessonTriggerToolCall(
      toolCall("write", { path: "src/danger.ts", content: "if (true) {}" }, "call-remind"),
      { cwd: effectiveRoomDir },
    );
    expect(verdict).toBeUndefined();

    const stashed = takeLessonReminder("call-remind");
    expect(stashed?.reminder).toContain('<system-reminder reason="lesson" lesson="coding#240">');
    expect(stashed?.reminder).toContain("A fake must be dumb");
    expect(stashed?.lessons.map((lesson) => `${lesson.family}#${lesson.id}`)).toEqual(["coding#240"]);
  });

  // Kills: the stash never cleared, so one remind repeats on every later tool
  // result of the session; and a mismatched tool call id silently borrowing
  // another call's reminder.
  // red-proof: delete the `remindByToolCallId.delete(callId)` in
  // takeLessonReminder.
  test("a stashed reminder is consumed once and is not borrowed by other calls", async () => {
    await installPipe("remind");
    await lessonTriggerToolCall(
      toolCall("write", { path: "src/danger.ts", content: "if (true) {}" }, "call-owner"),
      { cwd: effectiveRoomDir },
    );
    expect(takeLessonReminder("call-other")).toBeNull();
    expect(takeLessonReminder("")).toBeNull();
    expect(takeLessonReminder(undefined)).toBeNull();
    expect(takeLessonReminder("call-owner")).toBeTruthy();
    expect(takeLessonReminder("call-owner")).toBeNull();
  });

  // Kills: the tool's own content REPLACED by the reminder instead of the
  // reminder being prepended ahead of it — the failure mode that silently eats
  // every tool result the lesson touches.
  // red-proof: `return [{ type: "text", text: reminder }]` in
  // prependLessonReminder.
  test("prepends the reminder and preserves the original content verbatim", () => {
    const original = [
      { type: "text", text: "tool said one" },
      { type: "image", data: "abc" },
    ];
    const frozen = JSON.stringify(original);

    const merged = prependLessonReminder(original, "<system-reminder>lesson</system-reminder>");
    expect(merged).toHaveLength(3);
    expect(merged[0]).toEqual({ type: "text", text: "<system-reminder>lesson</system-reminder>" });
    expect(merged.slice(1)).toEqual(original);
    expect(JSON.stringify(original)).toBe(frozen);

    // Non-array and absent content must still keep whatever the tool returned.
    expect(prependLessonReminder("bare string", "R")).toEqual([{ type: "text", text: "R" }, "bare string"]);
    expect(prependLessonReminder(undefined, "R")).toEqual([{ type: "text", text: "R" }]);
    expect(prependLessonReminder(null, "R")).toEqual([{ type: "text", text: "R" }]);
  });
});

describe("lesson trigger live prose tap", () => {
  function assistantEvent(text: string, type = "text_end") {
    return {
      message: {
        role: "assistant",
        content: [{ type: "text", text }],
      },
      assistantMessageEvent: { type, contentIndex: 0 },
    };
  }

  function streamHarness(sessionID: string) {
    const sent: Array<{ message: Record<string, any>; options: Record<string, any> }> = [];
    let aborts = 0;
    const ctx = {
      cwd: effectiveRoomDir,
      sessionID,
      abort() {
        aborts++;
      },
    };
    const pi = {
      sendMessage(message: Record<string, any>, options: Record<string, any>) {
        sent.push({ message, options });
      },
    };
    return { ctx, pi, sent, aborts: () => aborts };
  }

  // Kills: a prose block downgraded into next-turn advice, operator feedback
  // hidden, generation left running, or an omitted skew-wire offset treated as
  // a usable cut point.
  // red-proof: force `canInterrupt = false`, `display = false`, or default an
  // absent `matchStart` to a number in queueProseContinuation.
  test("queues a hidden correction and aborts a live prose block", async () => {
    await installPipe("proseBlock");
    const harness = streamHarness("stream-block");
    const text = "A sufficiently long streamed assistant sentence reaches the Rust matcher and violates the lesson.";
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);

    await lessonTriggerProseStreamUpdate(assistantEvent(text), harness.ctx, harness.pi);

    expect(harness.aborts()).toBe(1);
    expect(harness.sent).toHaveLength(1);
    expect(harness.sent[0].options).toEqual({ deliverAs: "nextTurn", triggerTurn: true });
    expect(harness.sent[0].message).toMatchObject({
      customType: "solarisael-lesson-trigger",
      display: true,
      attribution: "agent",
      details: {
        source: "message_update",
        interrupted: true,
      },
    });
    expect(harness.sent[0].message.content).toContain('reason="lesson_violation"');
    expect(harness.sent[0].message.content).toContain('lesson="writing#408"');
    expect(harness.sent[0].message.content).not.toContain("Your interrupted draft above was trimmed");
    expect(observed).toEqual([{
      method: "lesson_trigger_match",
      params: {
        room: "kodo",
        session: "stream-block",
        surfaces: [{ kind: "prose", text }],
      },
    }]);

    const userMessage = { role: "user", content: "original request" };
    const aborted = { role: "assistant", content: text, stopReason: "aborted" };
    expect(filterInterruptedLessonProse(
      [userMessage, aborted],
      "kodo",
      "stream-block",
    )).toEqual([userMessage]);
    expect(filterInterruptedLessonProse(
      [userMessage, aborted],
      "kodo",
      "stream-block",
    )).toEqual([userMessage, aborted]);
  });

  // Kills: taking the violating sentence with the clean prefix, flattening the
  // assistant message, or forgetting to tell the continuation not to restate.
  // red-proof: use `matchStart` as the slice boundary or delete the aborted
  // message even when proseResumePrefix returns a prefix.
  test("keeps two clean sentences and replaces only the aborted text blocks", async () => {
    await installPipe("proseBlockTrim");
    const harness = streamHarness("stream-trim");
    const text = "First sentence. Second sentence! Third negative hinge appears in the violating draft.";
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);

    await lessonTriggerProseStreamUpdate(assistantEvent(text), harness.ctx, harness.pi);

    const resumeLine = "Your interrupted draft above was trimmed at the violation. Continue exactly from its end; do not restate anything above the cut.";
    expect(harness.sent[0].message.content.endsWith(resumeLine)).toBe(true);
    const userMessage = { role: "user", content: "original request" };
    const aborted = {
      role: "assistant",
      content: [
        { type: "thinking", thinking: "private scratch" },
        { type: "text", text, signature: "keep-me" },
      ],
      stopReason: "aborted",
      responseId: "response-1",
    };
    const filtered = filterInterruptedLessonProse([userMessage, aborted], "kodo", "stream-trim");

    expect(filtered).toEqual([
      userMessage,
      {
        ...aborted,
        content: [
          aborted.content[0],
          { type: "text", text: "First sentence. Second sentence!", signature: "keep-me" },
        ],
      },
    ]);
    expect(aborted.content[1].text).toBe(text);
  });

  // Kills: inventing a cut before the first completed sentence.
  // red-proof: accept the match offset itself as a resumable boundary.
  test("drops the aborted message when the match is inside the first sentence", async () => {
    await installPipe("proseBlockFirst");
    const harness = streamHarness("stream-first");
    const text = "The negative hinge appears before a sufficiently long remainder in this first sentence.";
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);

    await lessonTriggerProseStreamUpdate(assistantEvent(text), harness.ctx, harness.pi);

    expect(harness.sent[0].message.content).not.toContain("Your interrupted draft above was trimmed");
    const userMessage = { role: "user", content: "original request" };
    const aborted = { role: "assistant", content: text, stopReason: "aborted" };
    expect(filterInterruptedLessonProse([userMessage, aborted], "kodo", "stream-first")).toEqual([userMessage]);
  });

  // Kills: treating an AST hit's null byte offset as zero or as end-of-draft.
  // red-proof: coerce null `matchStart` values before proseResumePrefix.
  test("drops the aborted message for an AST hit without a match offset", async () => {
    await installPipe("proseBlockAst");
    const harness = streamHarness("stream-ast");
    const text = "A complete clean sentence. The negative hinge remains in a long violating sentence.";
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);

    await lessonTriggerProseStreamUpdate(assistantEvent(text), harness.ctx, harness.pi);

    expect(harness.sent[0].message.content).not.toContain("Your interrupted draft above was trimmed");
    const userMessage = { role: "user", content: "original request" };
    const aborted = { role: "assistant", content: text, stopReason: "aborted" };
    expect(filterInterruptedLessonProse([userMessage, aborted], "kodo", "stream-ast")).toEqual([userMessage]);
  });

  // Kills: recognizing only punctuation boundaries and discarding a complete
  // paragraph whose final line deliberately has no terminator.
  // red-proof: remove blankLineBoundary from proseResumePrefix.
  test("keeps a clean paragraph before a blank-line boundary", async () => {
    await installPipe("proseBlockBlank");
    const harness = streamHarness("stream-blank");
    const text = "Kept paragraph without punctuation\n\nThird negative hinge appears in a long enough draft.";
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);

    await lessonTriggerProseStreamUpdate(assistantEvent(text), harness.ctx, harness.pi);

    const aborted = { role: "assistant", content: text, stopReason: "aborted", model: "house" };
    expect(filterInterruptedLessonProse([aborted], "kodo", "stream-blank")).toEqual([
      { ...aborted, content: "Kept paragraph without punctuation" },
    ]);
  });

  // Kills: display=true exposing the hidden system payload as the default card,
  // or a generic notice that omits which lesson acted and what it did.
  // red-proof: remove the registered renderer or render message.content collapsed.
  test("renders a compact operator-visible intervention with expandable evidence", () => {
    const message = {
      content: '<system-interrupt reason="lesson_violation" lesson="writing#408">\nRewrite the sentence.\n</system-interrupt>',
      details: {
        lessons: [{ family: "writing", id: 408, urgency: "block", patternKind: "regex" }],
        source: "message_update",
        interrupted: true,
      },
    };
    const theme = { fg: (color: string, text: string) => `${color}:${text}` };

    const collapsed = lessonTriggerMessageRenderer(message, { expanded: false }, theme);
    expect(collapsed.render(80)).toEqual([
      "warning:Athanor · writing#408 · interrupted draft · correction queued",
    ]);

    const expanded = lessonTriggerMessageRenderer(message, { expanded: true }, theme);
    expect(expanded.render(80)).toEqual([
      "warning:Athanor · writing#408 · interrupted draft · correction queued",
      "",
      'muted:<system-interrupt reason="lesson_violation" lesson="writing#408">',
      "muted:Rewrite the sentence.",
      "muted:</system-interrupt>",
    ]);

    const reminder = lessonTriggerMessageRenderer({
      details: {
        lessons: [{ family: "coding", id: 19 }],
        interrupted: false,
      },
    }, { expanded: false }, theme);
    expect(reminder.render(80)).toEqual([
      "accent:Athanor · coding#19 · lesson reminder queued",
    ]);
  });

  // Kills: advisory prose accidentally aborting a turn, or disappearing after
  // the stream tap consumes the Rust cooldown before the context fallback.
  // red-proof: send every decision with `triggerTurn: true`.
  test("queues a reminder without aborting advisory prose", async () => {
    await installPipe("prose");
    const harness = streamHarness("stream-remind");
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);

    await lessonTriggerProseStreamUpdate(
      assistantEvent("A long advisory prose sample crosses the scan floor and reaches the matcher cleanly."),
      harness.ctx,
      harness.pi,
    );

    expect(harness.aborts()).toBe(0);
    expect(harness.sent).toHaveLength(1);
    expect(harness.sent[0].options).toEqual({ deliverAs: "nextTurn", triggerTurn: false });
    expect(harness.sent[0].message.content).toContain('<system-reminder reason="lesson" lesson="coding#19">');
    expect(harness.sent[0].message.details).toMatchObject({
      source: "message_update",
      interrupted: false,
    });
  });

  // Kills: a duplicate text_end arriving while Rust is in flight leaving
  // forceScan armed, then recursively starting pumps with no unchecked text.
  // red-proof: reschedule on forceScan without requiring new text.
  test("settles duplicate text_end events while the matcher is pending", async () => {
    await installPipe("slowQuiet");
    const harness = streamHarness("stream-duplicate-end");
    const event = assistantEvent("A completed prose sample can emit duplicate terminal events while matching remains in flight.");
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);

    const first = lessonTriggerProseStreamUpdate(event, harness.ctx, harness.pi);
    const duplicate = lessonTriggerProseStreamUpdate(event, harness.ctx, harness.pi);
    await Promise.all([first, duplicate]);

    expect(observed).toHaveLength(1);
    expect(harness.sent).toHaveLength(0);
    expect(harness.aborts()).toBe(0);
  });

  // Kills: thinking/tool deltas paying the Rust round trip, and a degraded
  // matcher turning into an unhandled rejection from the detached index tap.
  // red-proof: delete the assistant event-type gate or the decision try/catch.
  test("passes through non-text updates and transport failure", async () => {
    await installPipe("proseBlock");
    const harness = streamHarness("stream-pass");
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);
    await lessonTriggerProseStreamUpdate(
      assistantEvent("private thinking that must never enter the prose matcher", "thinking_delta"),
      harness.ctx,
      harness.pi,
    );
    expect(observed).toHaveLength(0);
    expect(harness.sent).toHaveLength(0);
    expect(harness.aborts()).toBe(0);

    closeLessonTriggerTransports();
    await pipe!.close().catch(() => undefined);
    pipe = null;
    await installPipe("silent");
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, harness.ctx, harness.pi);
    await lessonTriggerProseStreamUpdate(
      assistantEvent("A long prose sample reaches a silent matcher and must leave the live turn untouched."),
      harness.ctx,
      harness.pi,
    );
    expect(harness.sent).toHaveLength(0);
    expect(harness.aborts()).toBe(0);
  });

  // Kills: source implementation present without the OMP event wiring that
  // actually exposes it to live assistant deltas.
  // red-proof: remove either message hook from index.ts.
  test("registers assistant stream lifecycle hooks", async () => {
    const { default: registerAdapter } = await import("../index.ts?lesson-trigger-stream-hooks");
    const names: string[] = [];
    registerAdapter({
      setLabel() {},
      events: { on: () => () => undefined },
      on(name: string) {
        names.push(name);
      },
    });
    expect(names).toContain("message_start");
    expect(names).toContain("message_update");
  });
});

describe("lesson trigger prose injection", () => {
  type ContextHandler = (
    event: { messages: Array<Record<string, unknown>> },
    context: { cwd: string; sessionID: string },
  ) => Promise<{ messages: Array<Record<string, any>> } | undefined>;

  async function contextHandler(specifier: string): Promise<ContextHandler> {
    const { default: registerAdapter } = await import(specifier);
    let handler: ContextHandler | undefined;
    registerAdapter({
      setLabel() {},
      events: { on: () => () => undefined },
      on(name: string, candidate: ContextHandler) {
        if (name === "context") handler = candidate;
      },
    });
    if (!handler) throw new Error("context hook was not registered");
    return handler;
  }

  function lessonAdditions(messages: Array<Record<string, any>>) {
    return messages.filter((message) => message.customType === "solarisael-lesson-trigger");
  }

  const user = (id: string, content: string) => ({ id, role: "user", content });
  const assistant = (content: string) => ({ role: "assistant", content });

  // Kills the 3515 cache-poisoning class: an addition anchored anywhere but the
  // current turn, or an earlier turn's addition removed/moved when a new one is
  // produced — either shifts the rendered suffix and destroys the prefix cache.
  // Also pins the no-assistant-yet branch: nothing to quote, nothing injected.
  // red-proof: call `removeMemoCustomType(turnMemo, "solarisael-lesson-trigger")`
  // before merging the new addition in index.ts.
  test("anchors at the current turn and never disturbs an earlier turn", async () => {
    await installPipe("prose");
    const handler = await contextHandler("../index.ts?lesson-trigger-prose");
    const sessionID = "prose-session";
    const turn1 = user("turn-1", "first request");
    const turn2 = user("turn-2", "second request");
    const turn3 = user("turn-3", "third request");

    const first = await handler({ messages: [turn1] }, { cwd: effectiveRoomDir, sessionID });
    expect(lessonAdditions(first?.messages ?? [])).toHaveLength(0);

    const second = await handler(
      { messages: [turn1, assistant("I will invent the missing interop path"), turn2] },
      { cwd: effectiveRoomDir, sessionID },
    );
    const secondAdditions = lessonAdditions(second?.messages ?? []);
    expect(secondAdditions).toHaveLength(1);
    expect(secondAdditions[0].customType).toBe("solarisael-lesson-trigger");
    expect(secondAdditions[0].content).toContain('lesson="coding#19"');
    expect(second!.messages.at(-1)).toEqual(secondAdditions[0]);

    const secondBytes = JSON.stringify(second!.messages);
    const third = await handler(
      {
        messages: [
          turn1,
          assistant("I will invent the missing interop path"),
          turn2,
          assistant("still inventing interop"),
          turn3,
        ],
      },
      { cwd: effectiveRoomDir, sessionID },
    );
    const thirdMessages = third!.messages;
    expect(JSON.stringify(thirdMessages.slice(0, second!.messages.length))).toBe(secondBytes);
    expect(lessonAdditions(thirdMessages)).toHaveLength(2);
    expect(lessonAdditions(thirdMessages)[0]).toEqual(secondAdditions[0]);
    expect(thirdMessages.at(-1)).toEqual(lessonAdditions(thirdMessages)[1]);
  });

  // Kills: feeding the trimmed array only to conversation logging while the
  // provider still receives the original aborted draft.
  // red-proof: return only anchorTurnAdditions from composeContextAdditions when
  // filtering changed messages but no custom addition exists.
  test("returns a trimmed-only ContextEventResult to the provider", async () => {
    await installPipe("proseBlockTrim");
    const handler = await contextHandler("../index.ts?lesson-trigger-trim-propagation");
    const sessionID = "prose-trim-propagation";
    const ctx = {
      cwd: effectiveRoomDir,
      sessionID,
      abort() {},
    };
    const pi = { sendMessage() {} };
    const text = "First sentence. Second sentence! Third negative hinge appears in the violating draft.";
    resetLessonTriggerProseStream({ message: { role: "assistant" } }, ctx, pi);
    await lessonTriggerProseStreamUpdate({
      message: { role: "assistant", content: [{ type: "text", text }] },
      assistantMessageEvent: { type: "text_end", contentIndex: 0 },
    }, ctx, pi);
    process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS = "1";

    const prompt = user("turn-trim", "original request");
    const aborted = {
      role: "assistant",
      content: [{ type: "text", text }],
      stopReason: "aborted",
      responseId: "response-trim",
    };
    const result = await handler({ messages: [prompt, aborted] }, ctx);

    expect(result).toEqual({
      messages: [
        prompt,
        {
          ...aborted,
          content: [{ type: "text", text: "First sentence. Second sentence!" }],
        },
      ],
    });
  });

  // Kills: a prose failure escaping into the context hook. A dead substrate must
  // cost the turn nothing — no throw, no addition, history byte-identical to the
  // input messages.
  // red-proof: remove the try/catch (or the `?? null`) around the prose call in
  // lessonTriggerProseAddition / its index.ts wiring.
  test("a failing prose call leaves the turn untouched", async () => {
    await installPipe("silent");
    const handler = await contextHandler("../index.ts?lesson-trigger-prose-failure");
    const messages = [user("turn-1", "first request"), assistant("inventing interop"), user("turn-2", "second")];
    const result = await handler({ messages }, { cwd: effectiveRoomDir, sessionID: "prose-failure" });
    expect(lessonAdditions(result?.messages ?? [])).toHaveLength(0);
  });
});
