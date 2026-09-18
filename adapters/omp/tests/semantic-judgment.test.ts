import { expect, test } from "bun:test";
import { tokenUsage, type Judge, type JudgmentResult, type Questions } from "@oh-my-pi/pi-ai/judgment";
import type { ExtensionAPI, ExtensionContext, ToolCallEvent, TurnEndEvent } from "@oh-my-pi/pi-coding-agent/extensibility/extensions";
import {
  installSemanticJudgmentShadow, scoreCompletedDraftShadow, scoreToolCallShadow,
  selectSemanticRoute, SEMANTIC_SCORE_SCHEMA_VERSION, SEMANTIC_SHADOW_THRESHOLDS,
  SEMANTIC_THRESHOLD_POLICY, type EligibilityProvider, type SemanticJudge,
  type SemanticPacket,
} from "../house-proof/semantic-judgment.ts";
import { semanticJudgmentCorpus } from "./fixtures/semantic-judgment-corpus.ts";

const lessons = [{ id: 194, body: "Keep AST facts separate from semantic interpretation." }];
const packet: SemanticPacket = {
  project: "owned-fixture", language: "typescript", sourceClassification: "synthetic",
  excerpt: "const answer = ast.value;",
  deterministicEvidence: { regex: [], ast: ["assignment parsed by fixture parser"] },
};
const answers = {
  applies: { type: "noul", noul: 0.9 },
  violates: { type: "noul", noul: 0.1 },
  severity: { type: "score", score: 0.1, probabilities: { "0": 0.9, "1": 0.1, "2": 0, "3": 0 }, confidence: 0.9 },
  route: { type: "choice", choice: "silence", probabilities: { silence: 0.9, remind: 0.05, review: 0.05 }, confidence: 0.9 },
} as const;

function backendFixture(provider = "typesafe") {
  const requests: { state: unknown; questions: Questions }[] = [];
  const backend: SemanticJudge = {
    preferredKind: "typesafe",
    judge: {
      label: "typesafe/jev-latest",
      async judge<Q extends Questions>(request: Parameters<Judge["judge"]>[0]): Promise<JudgmentResult<Q>> {
        requests.push(structuredClone(request));
        return {
          api: provider === "typesafe" ? "typesafe" : "openai-responses",
          provider, model: provider === "typesafe" ? "jev-latest" : "fallback-model",
          usage: tokenUsage(12, 3),
          // This fixture answers only the four questions the real scorer sends.
          answers: structuredClone(answers) as unknown as JudgmentResult<Q>["answers"],
        };
      },
    },
  };
  return { backend, requests };
}
test("locked calibration corpus covers reviewed coding, writing, design and refusal cases", () => {
  expect(semanticJudgmentCorpus.map((fixture) => fixture.id)).toEqual([
    "coding-comment-restates-code",
    "coding-comment-names-external-contract",
    "coding-comment-quoted-review-example",
    "writing-antithesis-violation",
    "writing-direct-comparison",
    "writing-adversarial-instruction",
    "design-muted-required-reason",
    "design-muted-decorative-kicker",
    "design-single-click-commit",
    "design-separated-confirmation",
    "design-lesson-unrelated-to-copy-edit",
    "privacy-refusal-secret-like-content",
  ]);
  expect([...new Set(semanticJudgmentCorpus.flatMap((fixture) =>
    fixture.lessons.map((lesson) => lesson.id)
  ))].sort((left, right) => left - right)).toEqual([194, 294, 302, 408]);
  expect(new Set(semanticJudgmentCorpus.map((fixture) => fixture.expected.route)))
    .toEqual(new Set(["silence", "remind", "review", "refused"]));
  for (const fixture of semanticJudgmentCorpus) {
    expect(fixture.proposal.length).toBeLessThanOrEqual(8192);
    expect(fixture.lessons.length).toBeGreaterThan(0);
    expect(fixture.lessons.length).toBeLessThanOrEqual(8);
  }
});
test("measured thresholds derive the advisory route from scored evidence", () => {
  expect(selectSemanticRoute({ applies: 0.91, violates: 0.32, severity: 0.54 })).toBe("silence");
  expect(selectSemanticRoute({ applies: 0.91, violates: 0.91, severity: 1.35 })).toBe("remind");
  expect(selectSemanticRoute({ applies: 0.91, violates: 0.95, severity: 2.60 })).toBe("review");
  expect(selectSemanticRoute({ applies: 0.85, violates: 0.86, severity: 1.92 })).toBe("review");
});



test("four closed questions carry exact evidence and full attributable receipts", async () => {
  const { backend, requests } = backendFixture();
  const receipt = await scoreToolCallShadow({ ...packet, diff: "const answer = ast.value;" }, lessons, backend);
  expect(requests).toHaveLength(1);
  expect(Object.keys(requests[0].questions)).toEqual(["applies", "violates", "severity", "route"]);
  expect(requests[0].state).toEqual({
    schemaVersion: SEMANTIC_SCORE_SCHEMA_VERSION, boundary: "tool-call",
    project: packet.project, language: packet.language, sourceClassification: "synthetic",
    eligibleLessons: lessons, excerpt: packet.excerpt, deterministicEvidence: packet.deterministicEvidence,
    proposedHunkOrProse: "const answer = ast.value;",
    tool: { name: null, transport: "direct" },
  });
  expect(receipt).toMatchObject({
    status: "scored", boundary: "tool-call",
    toolCallId: null, toolName: null, transport: null,
    provider: "typesafe", api: "typesafe",
    model: "jev-latest", fallback: "none", answers, route: "silence",
    resolverRoute: "typesafe/jev-latest", schemaVersion: SEMANTIC_SCORE_SCHEMA_VERSION,
    redactionProfile: "synthetic-owned-v1", thresholdPolicy: SEMANTIC_THRESHOLD_POLICY,
    thresholds: SEMANTIC_SHADOW_THRESHOLDS,
    confidence: { applies: null, violates: null, severity: 0.9, route: 0.9 },
    tokenUsage: { input: 12, output: 3, cacheRead: 0, cacheWrite: 0 },
  });
  expect(receipt.probabilities.applies.true).toBe(0.9);
  expect(receipt.probabilities.violates.true).toBe(0.1);
  expect(receipt.probabilities.severity).toEqual(answers.severity.probabilities);
  expect(receipt.probabilities.route).toEqual(answers.route.probabilities);
  expect(receipt.inputDigest).toMatch(/^[a-f0-9]{64}$/);
  expect(receipt.latencyMs).toBeGreaterThanOrEqual(0);
});

test("owned prose passes; every other source classification refuses locally", async () => {
  const { backend, requests } = backendFixture();
  expect((await scoreCompletedDraftShadow({ ...packet, sourceClassification: "owned", prose: "Exact facts stay authoritative." }, lessons, backend)).status).toBe("scored");
  for (const sourceClassification of ["unknown", "intimate", "secret-like", "client", "", "unrecognized"]) {
    const receipt = await scoreCompletedDraftShadow({ ...packet, sourceClassification, prose: "harmless-looking" }, lessons, backend);
    expect(receipt.status).toBe("refused");
    expect(receipt.provider).toBeNull();
  }
  expect(requests).toHaveLength(1);
});

test("secret-like proposal, excerpt, lesson, evidence and metadata never reach a backend", async () => {
  const { backend, requests } = backendFixture();
  const secret = "sk-1234567890123456";
  const cases = [
    { packet: { ...packet, diff: secret }, lessons },
    { packet: { ...packet, diff: JSON.stringify({ api_key: "fixture-credential" }) }, lessons },
    { packet: { ...packet, diff: "password\t=\tfixture-credential" }, lessons },
    { packet: { ...packet, diff: "secret\n:\nfixture-credential" }, lessons },
    { packet: { ...packet, diff: "safe", excerpt: secret }, lessons },
    { packet: { ...packet, diff: "safe" }, lessons: [{ id: 194, body: secret }] },
    { packet: { ...packet, diff: "safe", deterministicEvidence: { regex: [secret], ast: [] } }, lessons },
    { packet: { ...packet, diff: "safe", project: secret }, lessons },
  ];
  for (const fixture of cases) {
    const receipt = await scoreToolCallShadow(fixture.packet, fixture.lessons, backend);
    expect(receipt.status).toBe("refused");
    expect(receipt.reason).toBe("redaction profile refused secret-like content");
    expect(JSON.stringify(receipt)).not.toContain(secret);
  }
  expect(requests).toHaveLength(0);
});

test("oversized packets refuse rather than silently dropping privacy context", async () => {
  const { backend, requests } = backendFixture();
  const receipt = await scoreToolCallShadow({ ...packet, diff: "x".repeat(8193) }, lessons, backend);
  expect(receipt.status).toBe("refused");
  expect(requests).toHaveLength(0);
});

test("actual fallback provider cannot masquerade as Jev", async () => {
  const { backend } = backendFixture("openai");
  const receipt = await scoreCompletedDraftShadow({ ...packet, prose: "owned fixture" }, lessons, backend);
  expect(receipt).toMatchObject({
    status: "scored", provider: "openai", api: "openai-responses",
    model: "fallback-model", fallback: "llm", resolverRoute: "typesafe/jev-latest",
  });
});

test("missing backend, empty eligibility, and provider failure are inconclusive", async () => {
  const { backend, requests } = backendFixture();
  const missing = await scoreCompletedDraftShadow({ ...packet, prose: "fixture" }, lessons);
  expect(missing).toMatchObject({ status: "inconclusive", fallback: "unavailable", provider: null, answers: null });
  const empty = await scoreCompletedDraftShadow({ ...packet, prose: "fixture" }, [], backend);
  expect(empty.reason).toBe("eligibility unavailable");
  expect(requests).toHaveLength(0);
  const failed = await scoreCompletedDraftShadow({ ...packet, prose: "fixture" }, lessons, {
    preferredKind: "typesafe", judge: { label: "typesafe/jev-latest", judge: async () => { throw new Error("private backend detail"); } },
  });
  expect(failed).toMatchObject({ status: "inconclusive", fallback: "unavailable", provider: null, reason: "judgment failed" });
  expect(JSON.stringify(failed)).not.toContain("private backend detail");
});

function hookFixture(eligibilityProvider?: EligibilityProvider, backend: SemanticJudge | null = backendFixture().backend) {
  const handlers = new Map<string, (event: never, ctx: ExtensionContext) => unknown>();
  const pi = {
    on: (name: string, handler: (event: never, ctx: ExtensionContext) => unknown) => { handlers.set(name, handler); },
  } as Pick<ExtensionAPI, "on">;
  let resolutions = 0;
  const installed = installSemanticJudgmentShadow(pi, {
    eligibilityProvider,
    resolveBackend: () => { resolutions++; return backend ?? undefined; },
  });
  return {
    ...installed,
    resolutions: () => resolutions,
    emit(event: ToolCallEvent | TurnEndEvent) {
      // No other host capability exists in this fixture; attempts to enforce,
      // send prose, invoke tools, or persist would fail rather than go unnoticed.
      return handlers.get(event.type)!(event as never, {} as ExtensionContext);
    },
  };
}

const eligible: EligibilityProvider = () => ({ ...structuredClone(packet), lessons: structuredClone(lessons) });
const turn = (): TurnEndEvent => ({
  type: "turn_end", turnIndex: 1, toolResults: [],
  message: {
    role: "assistant", content: [{ type: "text", text: "The delivered prose stays intact." }],
    api: "openai-responses", provider: "openai", model: "fixture",
    usage: tokenUsage(1, 1),
    stopReason: "stop", timestamp: 0,
  },
});

async function settled(fixture: ReturnType<typeof hookFixture>, count: number) {
  for (let attempt = 0; attempt < 100 && fixture.getReceipts().length !== count; attempt++) {
    await new Promise(resolve => setTimeout(resolve, 1));
  }
  expect(fixture.getReceipts()).toHaveLength(count);
}

test("installed hooks refuse escaped-whitespace secrets before resolution", async () => {
  const { backend, requests } = backendFixture();
  const fixture = hookFixture(eligible, backend);
  for (const content of ["password\t=\tfixture-credential", "secret\n:\nfixture-credential"]) {
    fixture.emit({
      type: "tool_call", toolCallId: content, toolName: "write",
      input: { path: "owned.ts", content },
    });
  }
  await settled(fixture, 2);
  expect(fixture.getReceipts().map(receipt => receipt.status)).toEqual(["refused", "refused"]);
  expect(fixture.resolutions()).toBe(0);
  expect(requests).toHaveLength(0);
});

test("installed eligibility observes edit/write/AST and completed turns only after hooks return", async () => {
  const { backend, requests } = backendFixture();
  const observed: string[] = [];
  const fixture = hookFixture(observation => {
    observed.push(observation.toolName ?? observation.boundary);
    return eligible(observation);
  }, backend);
  const events: (ToolCallEvent | TurnEndEvent)[] = [
    { type: "tool_call", toolCallId: "1", toolName: "edit", input: { patch: "+const x = 1;" } },
    { type: "tool_call", toolCallId: "2", toolName: "write", input: { path: "owned.ts", content: "const x = 1;" } },
    { type: "tool_call", toolCallId: "3", toolName: "write", input: { i: "stage AST rewrite", path: "xd://ast_edit", content: "{\"ops\":[]}" } },
    { type: "tool_call", toolCallId: "3", toolName: "ast_edit", input: { ops: [{ pat: "x", out: "y" }] } },
    turn(),
  ];
  const original = structuredClone(events);
  for (const event of events) expect(fixture.emit(event)).toBeUndefined();
  expect(observed).toEqual([]);
  expect(requests).toHaveLength(0);
  await settled(fixture, 4);
  expect(observed).toEqual(["edit", "write", "ast_edit", "completed-draft"]);
  expect(requests).toHaveLength(4);
  expect(events).toEqual(original);
  expect(fixture.getReceipts().map(receipt => receipt.boundary)).toEqual(["tool-call", "tool-call", "tool-call", "completed-draft"]);
});

test("pending eligibility cannot hold tool execution or delivered prose hostage", async () => {
  let release!: (value: Awaited<ReturnType<EligibilityProvider>>) => void;
  const pending = new Promise<Awaited<ReturnType<EligibilityProvider>>>(resolve => { release = resolve; });
  const fixture = hookFixture(() => pending);
  const event = turn();
  expect(fixture.emit(event)).toBeUndefined();
  await new Promise(resolve => setTimeout(resolve, 1));
  expect(fixture.getReceipts()).toEqual([]);
  expect(event).toEqual(turn());
  release({ ...packet, lessons });
  await settled(fixture, 1);
});

test("production-shaped installation without eligibility never resolves or calls a backend", async () => {
  const { backend, requests } = backendFixture();
  const fixture = hookFixture(undefined, backend);
  expect(fixture.emit(turn())).toBeUndefined();
  await settled(fixture, 1);
  expect(fixture.getReceipts()[0]).toMatchObject({ status: "inconclusive", reason: "eligibility provider unavailable" });
  expect(fixture.resolutions()).toBe(0);
  expect(requests).toHaveLength(0);
});
test("eligible hooks require an explicitly injected runtime backend", async () => {
  const fixture = hookFixture(eligible, null);
  fixture.emit(turn());
  await settled(fixture, 1);
  expect(fixture.getReceipts()[0]).toMatchObject({
    status: "inconclusive",
    reason: "judgment backend unavailable",
  });
  expect(fixture.resolutions()).toBe(1);
});


test("unavailable or failing eligibility yields one honest receipt per invocation", async () => {
  for (const provider of [() => undefined, () => { throw new Error("private eligibility detail"); }] satisfies EligibilityProvider[]) {
    const fixture = hookFixture(provider);
    fixture.emit(turn());
    await settled(fixture, 1);
    expect(fixture.getReceipts()[0].status).toBe("inconclusive");
    expect(fixture.resolutions()).toBe(0);
  }
});

test("receipt storage is a bounded in-memory ring with detached snapshots", async () => {
  const fixture = hookFixture();
  for (let index = 0; index < 65; index++) fixture.emit(turn());
  await settled(fixture, 64);
  const snapshot = fixture.getReceipts();
  snapshot[0].reason = "caller changed its copy";
  expect(fixture.getReceipts()[0].reason).not.toBe("caller changed its copy");
});

test("all built-in, future, MCP and direct device names enter the production denominator", async () => {
  const { backend, requests } = backendFixture();
  const fixture = hookFixture(undefined, backend);
  const names = ["read", "grep", "glob", "bash", "hub", "edit", "write", "ast_edit", "future_tool", "mcp__server__tool", "__proto__"];
  for (const toolName of names) {
    fixture.emit({ type: "tool_call", toolCallId: toolName, toolName, input: { value: "owned" } });
  }
  fixture.emit(turn());
  await settled(fixture, names.length + 1);
  const coverage = fixture.getCoverage();
  expect(coverage.logicalCalls).toBe(names.length);
  expect(coverage.draftObservations).toBe(1);
  expect(coverage.byTool).toEqual(Object.fromEntries(names.map(name => [name, 1])));
  expect(coverage.byDisposition["eligibility-unavailable"]).toBe(names.length);
  expect(Object.values(coverage.byDisposition).reduce((sum, count) => sum + count, 0)).toBe(names.length);
  expect(fixture.resolutions()).toBe(0);
  expect(requests).toEqual([]);
});

test("mounted outer attempts own identity even without inner dispatch or a result", async () => {
  const observations: Parameters<EligibilityProvider>[0][] = [];
  const fixture = hookFixture(observation => { observations.push(observation); return eligible(observation); });
  const content = "{ \"ops\": [] }";
  fixture.emit({ type: "tool_call", toolCallId: "mounted", toolName: "write", input: { path: "xd://ast_edit", content } });
  fixture.emit({ type: "tool_call", toolCallId: "mounted", toolName: "ast_edit", input: { normalized: true } });
  fixture.emit({ type: "tool_call", toolCallId: "blocked", toolName: "write", input: { path: "xd://unknown_device", content: "{}" } });
  fixture.emit({ type: "tool_call", toolCallId: "direct", toolName: "ast_edit", input: { ops: [] } });
  fixture.emit({ type: "tool_call", toolCallId: "direct", toolName: "ast_edit", input: { ops: [] } });
  await settled(fixture, 3);
  expect(observations[0]).toMatchObject({
    toolCallId: "mounted", toolName: "ast_edit", transport: "xd",
  });
  expect(JSON.parse(observations[0].proposal).content).toBe(content);
  expect(fixture.getCoverage()).toMatchObject({
    logicalCalls: 3, transportAliases: 2, duplicateEvents: 2,
    byTool: { ast_edit: 2, unknown_device: 1 }, byDisposition: { scored: 3 },
  });
  expect(fixture.getReceipts().map(receipt => ({
    toolCallId: receipt.toolCallId,
    toolName: receipt.toolName,
    transport: receipt.transport,
  }))).toEqual([
    { toolCallId: "mounted", toolName: "ast_edit", transport: "xd" },
    { toolCallId: "blocked", toolName: "unknown_device", transport: "xd" },
    { toolCallId: "direct", toolName: "ast_edit", transport: null },
  ]);
});

test("unsafe mounted and dynamic inputs refuse before eligibility and stay snapshotted", async () => {
  let eligibilityCalls = 0;
  const { backend, requests } = backendFixture();
  const fixture = hookFixture(observation => { eligibilityCalls++; return eligible(observation); }, backend);
  const input = { path: "xd://custom", content: "password\t=\tprivate" };
  fixture.emit({ type: "tool_call", toolCallId: "unsafe", toolName: "write", input });
  input.content = "{}";
  const encoded = {
    path: "xd://custom",
    content: JSON.stringify({ value: "password\t=\tprivate" }),
  };
  fixture.emit({ type: "tool_call", toolCallId: "encoded", toolName: "write", input: encoded });
  fixture.emit({
    type: "tool_call", toolCallId: "encoded", toolName: "custom",
    input: { value: "password\t=\tprivate" },
  });
  let reads = 0;
  fixture.emit({
    type: "tool_call", toolCallId: "dynamic", toolName: "custom",
    input: { get value() { return ++reads === 1 ? "owned" : "secret\n:\nprivate"; } },
  });
  await settled(fixture, 3);
  expect(fixture.getCoverage()).toMatchObject({
    duplicateEvents: 1,
    byDisposition: { "privacy-refused": 3 },
  });
  expect(eligibilityCalls).toBe(0);
  expect(fixture.resolutions()).toBe(0);
  expect(requests).toEqual([]);
  const absent = hookFixture();
  absent.emit({ type: "tool_call", toolCallId: "unsafe-path", toolName: "write", input: { path: "xd://secret=private", content: "{}" } });
  expect(absent.getCoverage().byDisposition["privacy-refused"]).toBe(1);
});

test("safe inputs are serialized before deferred eligibility sees subsequent mutation", async () => {
  let proposal = "";
  const fixture = hookFixture(observation => { proposal = observation.proposal; return undefined; });
  const input = { value: "original" };
  fixture.emit({ type: "tool_call", toolCallId: "snapshot", toolName: "custom", input });
  input.value = "secret=private";
  await settled(fixture, 1);
  expect(JSON.parse(proposal)).toEqual({ value: "original" });
  expect(fixture.getCoverage().byDisposition.ineligible).toBe(1);
});

test("unserializable attempts close locally without eligibility or backend access", async () => {
  let eligibilityCalls = 0;
  const fixture = hookFixture(() => { eligibilityCalls++; return undefined; });
  const cycle: Record<string, unknown> = {};
  cycle.self = cycle;
  const inputs = [cycle, { value: 1n }, { get value() { throw new Error("private detail"); } }];
  for (const [index, input] of inputs.entries()) {
    fixture.emit({ type: "tool_call", toolCallId: String(index), toolName: "custom", input });
  }
  await settled(fixture, 3);
  expect(fixture.getCoverage().byDisposition.unserializable).toBe(3);
  expect(eligibilityCalls).toBe(0);
  expect(fixture.resolutions()).toBe(0);
});

test("terminal counters distinguish eligibility refusal and backend outcomes", async () => {
  const failing = backendFixture().backend;
  failing.judge.judge = async () => { throw new Error("backend failed"); };
  const cases = [
    { fixture: hookFixture(eligible), disposition: "scored" },
    { fixture: hookFixture(() => { throw new Error("eligibility failed"); }), disposition: "eligibility-unavailable" },
    { fixture: hookFixture(() => undefined), disposition: "ineligible" },
    { fixture: hookFixture(() => ({ ...packet, lessons: [] })), disposition: "ineligible" },
    { fixture: hookFixture(eligible, null), disposition: "backend-unavailable" },
    { fixture: hookFixture(eligible, failing), disposition: "backend-failed" },
  ] as const;
  for (const { fixture, disposition } of cases) {
    fixture.emit({ type: "tool_call", toolCallId: disposition, toolName: "read", input: { path: "owned.ts" } });
    await settled(fixture, 1);
    const coverage = fixture.getCoverage();
    expect(coverage.byDisposition[disposition]).toBe(1);
    expect(Object.values(coverage.byDisposition).reduce((sum, count) => sum + count, 0)).toBe(coverage.logicalCalls);
  }
});

test("receipt rotation and detached snapshots preserve full coverage and bounded deduplication", () => {
  const fixture = hookFixture();
  for (let index = 0; index < 4097; index++) {
    fixture.emit({ type: "tool_call", toolCallId: String(index), toolName: "read", input: {} });
  }
  expect(fixture.getReceipts()).toHaveLength(64);
  expect(fixture.getCoverage()).toMatchObject({
    logicalCalls: 4097, byTool: { read: 4097 }, byDisposition: { "eligibility-unavailable": 4097 },
  });
  fixture.emit({ type: "tool_call", toolCallId: "4096", toolName: "read", input: {} });
  expect(fixture.getCoverage().duplicateEvents).toBe(1);
  fixture.emit({ type: "tool_call", toolCallId: "0", toolName: "read", input: {} });
  expect(fixture.getCoverage().logicalCalls).toBe(4098);
  const snapshot = fixture.getCoverage();
  snapshot.byTool.read = 0;
  snapshot.byDisposition["eligibility-unavailable"] = 0;
  expect(fixture.getCoverage().byTool.read).toBe(4098);
  expect(fixture.getCoverage().byDisposition["eligibility-unavailable"]).toBe(4098);
});
