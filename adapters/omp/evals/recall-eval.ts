import { createHash, randomUUID } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { planContextLessons, prepareContext } from "../house-proof/context.ts";
import { hostCommand, sendHostCommand, type HostBinding } from "../house-proof/host.ts";
import { RecallPolicyHostClient } from "../house-proof/recall-policy.ts";

type PriorTurn = {
  kind: "user" | "assistant" | "generated";
  text: string;
  turn_id?: string | null;
};

type EvalCase = {
  id: string;
  prior_turns?: PriorTurn[];
  query: string;
  room: string;
  scope?: { target_scope?: "room" | "house"; room?: string };
  expected_action?: "skip" | "reuse" | "search";
  expected_memory_ids?: number[];
  forbidden_memory_ids?: number[];
  expected_passage?: string;
  active_project?: string | null;
  recognized_entities?: string[];
  operator?: string;
  provenance?: { source?: string; note?: string };
  authored_probe?: boolean;
};

type CaseFile = {
  schema_version: 1;
  private?: true;
  binding?: { room?: string; spirit?: string; session?: string; operator?: string };
  cases: EvalCase[];
};

type StageStatus = "ok" | "unavailable" | "failed" | "invalid" | "not-run";
type Score = boolean | null;
const ANALYSIS_RESULT = new Set(["athanor.context.analyzed"]);
const MAX_TEXT_DIGEST = 64;
const DEFAULT_TIMEOUT_MS = 120_000;

function text(value: unknown): string {
  return typeof value === "string" ? value : value == null ? "" : String(value);
}

function digest(value: unknown): string {
  return createHash("sha256").update(JSON.stringify(value)).digest("hex").slice(0, MAX_TEXT_DIGEST);
}
function safeError(error: unknown): Record<string, string> {
  if (!error || typeof error !== "object") return { name: "unknown" };
  const value = error as Record<string, unknown>;
  return {
    name: typeof value.name === "string" ? value.name : "Error",
    ...(typeof value.code === "string" ? { code: value.code } : {}),
  };
}

function ids(value: unknown): number[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((candidate) => {
    if (!candidate || typeof candidate !== "object") return [];
    const row = candidate as Record<string, unknown>;
    const id = row.memory_id ?? row.memoryId ?? row.id;
    return typeof id === "number" && Number.isSafeInteger(id) ? [id] : [];
  });
}

function candidateText(value: unknown): string {
  if (!value || typeof value !== "object") return "";
  const row = value as Record<string, unknown>;
  return [row.title, row.excerpt, row.text, row.body, row.content, row.passage]
    .filter((part): part is string => typeof part === "string")
    .join("\n");
}

function containsPassage(candidates: unknown, expected: string): boolean {
  if (!expected) return false;
  const needle = expected.trim();
  return Array.isArray(candidates)
    && candidates.some((candidate) => candidateText(candidate).includes(needle));
}

function expectedIdScore(expected: number[] | undefined, observed: number[]): Score {
  if (expected === undefined) return null;
  if (expected.length === 0) return observed.length === 0;
  const observedSet = new Set(observed);
  return expected.every((id) => observedSet.has(id));
}

function forbiddenScore(forbidden: number[] | undefined, observed: number[]): Score {
  if (forbidden === undefined) return null;
  const forbiddenSet = new Set(forbidden);
  return observed.every((id) => !forbiddenSet.has(id));
}

function actualAction(decision: Record<string, unknown>, workingSetPresent: boolean): "skip" | "reuse" | "search" {
  if (decision.shouldRecall === true) return "search";
  return workingSetPresent ? "reuse" : "skip";
}

function validateCaseFile(value: unknown): CaseFile {
  if (!value || typeof value !== "object") throw new Error("case file must be an object");
  const root = value as Record<string, unknown>;
  if (root.schema_version !== 1 || root.private !== true || !Array.isArray(root.cases)) {
    throw new Error("private case file requires private:true, schema_version 1, and cases[]");
  }
  const seen = new Set<string>();
  const cases = root.cases.map((candidate, index) => {
    if (!candidate || typeof candidate !== "object") throw new Error(`cases[${index}] must be an object`);
    const item = candidate as Record<string, unknown>;
    const id = text(item.id).trim();
    const expectedAction = item.expected_action;
    if (expectedAction !== undefined && !["skip", "reuse", "search"].includes(text(expectedAction))) {
      throw new Error(`case ${id || index} has invalid expected_action`);
    }
    const query = text(item.query);
    const room = text(item.room).trim();
    if (!id || seen.has(id)) throw new Error(`cases[${index}] has a missing or duplicate id`);
    if (!query.trim()) throw new Error(`case ${id} has an empty query`);
    if (!room) throw new Error(`case ${id} has an empty room`);
    seen.add(id);
    const priorTurns = item.prior_turns;
    if (priorTurns !== undefined && (!Array.isArray(priorTurns) || priorTurns.some((turn) => {
      if (!turn || typeof turn !== "object") return true;
      const row = turn as Record<string, unknown>;
      return !["user", "assistant", "generated"].includes(text(row.kind)) || !text(row.text);
    }))) throw new Error(`case ${id} has invalid prior_turns`);
    if (item.expected_memory_ids !== undefined && (!Array.isArray(item.expected_memory_ids)
      || item.expected_memory_ids.some((memoryId) => !Number.isSafeInteger(memoryId)))) {
      throw new Error(`case ${id} has invalid expected_memory_ids`);
    }
    if (item.forbidden_memory_ids !== undefined && (!Array.isArray(item.forbidden_memory_ids)
      || item.forbidden_memory_ids.some((memoryId) => !Number.isSafeInteger(memoryId)))) {
      throw new Error(`case ${id} has invalid forbidden_memory_ids`);
    }
    return {
      id,
      query,
      room,
      prior_turns: priorTurns as PriorTurn[] | undefined,
      scope: item.scope as EvalCase["scope"],
      expected_action: expectedAction as EvalCase["expected_action"],
      expected_memory_ids: item.expected_memory_ids as number[] | undefined,
      forbidden_memory_ids: item.forbidden_memory_ids as number[] | undefined,
      expected_passage: typeof item.expected_passage === "string" ? item.expected_passage : undefined,
      active_project: typeof item.active_project === "string" ? item.active_project : null,
      recognized_entities: Array.isArray(item.recognized_entities)
        ? item.recognized_entities.map(text).filter(Boolean) : [],
      operator: typeof item.operator === "string" ? item.operator : undefined,
      provenance: item.provenance && typeof item.provenance === "object"
        ? item.provenance as EvalCase["provenance"] : undefined,
      authored_probe: typeof item.authored_probe === "boolean" ? item.authored_probe : undefined,
    } satisfies EvalCase;
  });
  const binding = root.binding && typeof root.binding === "object"
    ? root.binding as CaseFile["binding"] : undefined;
  return { schema_version: 1, binding, cases };
}

function usage(): string {
  return [
    "Usage: bun evals/recall-eval.ts --cases <private.json> [--out <receipt.json>] [--mode baseline|after] [--room <current-room>] [--spirit <name>] [--session <id>] [--operator <name>] [--credential-env <ENV_NAME>]",
    "The case file stays private. The receipt contains case IDs, hashes, stage summaries, and scores; it does not contain prompts or memory IDs.",
    "Expected case fields: id, prior_turns, query, room, expected_action, expected_memory_ids, forbidden_memory_ids, expected_passage.",
    "Omit gold fields to leave that metric unscored. Use expected_memory_ids: [] to score an expected empty result.",
    "Optional metadata includes provenance and authored_probe; authored probes never contribute to scores.",
  ].join("\n");
}

function arg(name: string, args: string[]): string | undefined {
  const index = args.indexOf(name);
  return index >= 0 ? args[index + 1] : undefined;
}

async function analyze(binding: HostBinding, item: EvalCase, operator: string): Promise<Record<string, unknown>> {
  const priorTurns = item.prior_turns ?? [];
  const contextCharacters = priorTurns.reduce((sum, turn) => sum + turn.text.length, 0);
  const response = await sendHostCommand(
    hostCommand(binding, "athanor.context.analyze", "context", {
      context_request: {
        prompt: item.query,
        recognizedEntities: item.recognized_entities ?? [],
        contextCharacters,
        activeSpirit: binding.spirit,
        operator,
        routingModeEnabled: true,
      },
    }),
    ANALYSIS_RESULT,
    undefined,
    DEFAULT_TIMEOUT_MS,
  );
  if (!response.analysis || typeof response.analysis !== "object") throw new Error("Host analysis omitted analysis");
  return response.analysis as Record<string, unknown>;
}

function presentationFromRecallBlock(block: Record<string, unknown> | undefined): Record<string, unknown> | null {
  if (!block) return null;
  const details = block.details && typeof block.details === "object"
    ? block.details as Record<string, unknown> : {};
  if (details.presentation && typeof details.presentation === "object") {
    return details.presentation as Record<string, unknown>;
  }
  const content = typeof block.content === "string" ? block.content : "";
  const open = content.indexOf("<athanor-memories>");
  const close = content.lastIndexOf("</athanor-memories>");
  if (open < 0 || close <= open) return null;
  const body = content.slice(open + "<athanor-memories>".length, close);
  const start = body.indexOf("{");
  const end = body.lastIndexOf("}");
  if (start < 0 || end < start) return null;
  try {
    const parsed = JSON.parse(body.slice(start, end + 1));
    return parsed && typeof parsed === "object" ? parsed as Record<string, unknown> : null;
  } catch {
    return null;
  }
}

type NativeContextRun = {
  stage: Record<string, unknown>;
  action: "skip" | "reuse" | "search" | null;
  presentation: Record<string, unknown> | null;
};

async function nativeContext(
  binding: HostBinding,
  item: EvalCase,
  runId: string,
  credential?: string,
): Promise<NativeContextRun> {
  const history = (item.prior_turns ?? []).map((turn, index) => ({
    kind: turn.kind,
    text: turn.text,
    turnId: turn.turn_id ?? `${runId}:${item.id}:prior:${index}`,
  }));
  const currentTurnId = `${runId}:${item.id}:current`;
  history.push({ kind: "user", text: item.query, turnId: currentTurnId });
  const capabilities = { automaticRecall: true, topLevel: true };
  const client = new RecallPolicyHostClient(binding);
  let priorRefreshAt: string | null = null;
  let warmupTurns = 0;

  const prepareTurn = async (index: number) => {
    const turn = history[index];
    const visibleHistory = history.slice(0, index + 1);
    const plan = await planContextLessons(binding, {
      turnId: turn.turnId,
      activeProject: item.active_project ?? null,
      managerAvailable: false,
      deadline: Date.now() + DEFAULT_TIMEOUT_MS,
      nativeUser: false,
      capabilities,
    });
    const prepared = await prepareContext(binding, {
      turnId: turn.turnId,
      prompt: turn.text,
      nativeUser: false,
      visibleTurnIds: visibleHistory.map((entry) => entry.turnId),
      history: visibleHistory,
      existingBlockKinds: [],
      contextCharacters: visibleHistory.reduce((sum, entry) => sum + entry.text.length, 0),
      userTurnOrdinal: visibleHistory.filter((entry) => entry.kind === "user").length,
      freshConversation: warmupTurns === 0,
      activeProject: item.active_project ?? null,
      toolEvidenceRevision: 0,
      toolEvidenceEpoch: "recall-eval",
      capabilities,
      planToken: plan.token,
      credential: plan.requiresCredential ? credential : undefined,
      legacyMemo: null,
      legacyRejection: null,
      recallTelemetryOverride: process.env.ATHANOR_RECALL_TELEMETRY,
      priorPresence: null,
    });
    return { plan, prepared };
  };

  for (let index = 0; index < history.length - 1; index++) {
    if (history[index].kind !== "user") continue;
    await prepareTurn(index);
    warmupTurns++;
  }
  if (warmupTurns > 0) {
    priorRefreshAt = (await client.inspect()).recallPolicy.lastRefreshAt;
  }
  const { plan, prepared } = await prepareTurn(history.length - 1);
  const current = prepared.turns.find((turn) => turn.turnId === currentTurnId);
  const recallBlock = [...prepared.turns].reverse()
    .flatMap((turn) => [...turn.blocks].reverse())
    .find((block) => block.kind === "recall-context");
  const recallDetails = recallBlock?.details && typeof recallBlock.details === "object"
    ? recallBlock.details as Record<string, unknown> : {};
  const jev = recallDetails.jev && typeof recallDetails.jev === "object"
    ? recallDetails.jev as Record<string, unknown> : {};
  const presentation = presentationFromRecallBlock(recallBlock as Record<string, unknown> | undefined);
  const snapshot = await client.inspect();
  const refreshReason = snapshot.recallPolicy.lastRefreshReason;
  const refreshAt = snapshot.recallPolicy.lastRefreshAt;
  const refreshed = Boolean(refreshAt) && refreshAt !== priorRefreshAt;
  const refreshFailed = refreshed && refreshReason === "failed";
  const observedEmpty = refreshed && !refreshFailed && presentation === null
    && snapshot.recallPolicy.workingSetEntries === 0;
  const action = refreshed ? "search" : presentation?.found === true ? "reuse" : "skip";
  const presentationCandidates = presentation?.retrievalCandidates;
  return {
    action,
    presentation,
    stage: {
      status: "ok" satisfies StageStatus,
      planRequiresCredential: plan.requiresCredential,
      planRuleCount: plan.rules.length,
      planWarningCount: plan.warnings.length,
      preparedTurnCount: prepared.turns.length,
      currentBlockCount: current?.blocks.length ?? 0,
      recallBlock: Boolean(recallBlock),
      presentationFound: presentation?.found === true,
      observedEmpty,
      refreshFailed,
      presentedCandidateCount: Array.isArray(presentationCandidates) ? presentationCandidates.length : 0,
      refreshReason,
      refreshAt: Boolean(refreshAt),
      jevStatus: typeof jev.status === "string" ? jev.status : null,
      action,
      replayed: prepared.replayed,
      warningCount: prepared.warnings.length,
      activityCount: prepared.activities.length,
      historyTurnCount: history.length,
      warmupTurns,
    },
  };
}

function unavailableNativeContext(error: unknown): NativeContextRun {
  return {
    action: null,
    presentation: null,
    stage: { status: "unavailable" satisfies StageStatus, error: safeError(error) },
  };
}

async function runCase(
  item: EvalCase,
  input: CaseFile,
  options: { spirit: string; session: string; operator: string; mode: string; runId: string; credential?: string },
): Promise<Record<string, unknown>> {
  const binding: HostBinding = {
    room: item.room,
    spirit: options.spirit,
    session: `${options.session}:recall-eval:${options.runId}:${item.id}`,
  };
  const routeStage: Record<string, unknown> = {
    status: "unavailable" satisfies StageStatus,
    priorTurnCount: item.prior_turns?.length ?? 0,
    historyDigest: digest(item.prior_turns ?? []),
    queryDigest: digest(item.query),
  };
  let contextStage: Record<string, unknown> = { status: "unavailable" satisfies StageStatus };
  let nativeAction: NativeContextRun["action"] = null;
  let nativePresentation: Record<string, unknown> | null = null;
  try {
    const native = await nativeContext(binding, item, options.runId, options.credential);
    contextStage = native.stage;
    nativeAction = native.action;
    nativePresentation = native.presentation;
  } catch (error) {
    const native = unavailableNativeContext(error);
    contextStage = native.stage;
    nativePresentation = native.presentation;
  }
  const policyStage: Record<string, unknown> = { status: "unavailable" satisfies StageStatus };
  const retrievalStage: Record<string, unknown> = { status: "unavailable" satisfies StageStatus };
  const viewportStage: Record<string, unknown> = { status: "unavailable" satisfies StageStatus };
  const scores: Record<string, Score> = {
    action: item.expected_action === undefined ? null : false,
    retrievalRecall: null,
    admittedPrecision: null,
    expectedEvidencePresent: expectedIdScore(item.expected_memory_ids, []),
    forbiddenAbsent: forbiddenScore(item.forbidden_memory_ids, []),
    passageSupported: item.expected_passage !== undefined ? false : null,
  };
  let analysis: Record<string, unknown> | undefined;
  try {
    analysis = await analyze(binding, item, item.operator ?? input.binding?.operator ?? options.operator);
    const route = (analysis.route && typeof analysis.route === "object")
      ? analysis.route as Record<string, unknown> : {};
    routeStage.status = "ok" satisfies StageStatus;
    routeStage.intent = text(route.intent);
    routeStage.shouldAutoRecall = route.shouldAutoRecall === true;
    routeStage.termCount = Array.isArray(route.terms) ? route.terms.length : 0;
    routeStage.requiredTermCount = Array.isArray(route.requiredTerms) ? route.requiredTerms.length : 0;
    routeStage.reasonCount = Array.isArray(route.reasons) ? route.reasons.length : 0;
  } catch (error) {
    routeStage.status = "failed" satisfies StageStatus;
    routeStage.error = safeError(error);
  }

  let decision: Record<string, unknown> | undefined;
  try {
    if (!analysis?.route || typeof analysis.route !== "object") throw new Error("query classification unavailable");
    const route = analysis.route as Record<string, unknown>;
    const policy = await new RecallPolicyHostClient(binding).evaluate({
      queryRoute: {
        intent: text(route.intent),
        terms: Array.isArray(route.terms) ? route.terms.map(text) : [],
        requiredTerms: Array.isArray(route.requiredTerms) ? route.requiredTerms.map(text) : [],
        recognizedEntities: Array.isArray(route.recognizedEntities) ? route.recognizedEntities.map(text) : [],
      },
      activeProject: item.active_project ?? null,
      conversationTokens: (item.prior_turns ?? []).reduce((sum, turn) => sum + turn.text.length, 0),
      workingSetPresent: false,
    });
    decision = policy.decision as unknown as Record<string, unknown>;
    const action = actualAction(decision, false);
    policyStage.status = "ok" satisfies StageStatus;
    policyStage.reuseAvailable = false;
    policyStage.diagnosticAction = action;
    policyStage.shouldRecall = decision.shouldRecall === true;
    policyStage.clearWorkingSet = decision.clearWorkingSet === true;
    policyStage.queryTermCount = Array.isArray(decision.queryTerms) ? decision.queryTerms.length : 0;
    policyStage.resolvedMode = text(decision.resolvedMode);
    policyStage.refreshReason = text(decision.refreshReason) || null;
  } catch (error) {
    policyStage.status = "failed" satisfies StageStatus;
    policyStage.error = safeError(error);
    scores.action = null;
  }

  const targetScope = item.scope?.target_scope ?? "room";
  const action = nativeAction;
  policyStage.action = action;
  policyStage.actionSource = action ? "native-context" : "native-context-unavailable";
  if (action && item.expected_action) {
    scores.action = item.expected_action === action;
  }
  const finalCandidates = nativePresentation?.retrievalCandidates;
  const finalIds = ids(finalCandidates);
  if (action !== "search" && action !== "reuse") {
    scores.retrievalRecall = null;
    scores.admittedPrecision = null;
    scores.expectedEvidencePresent = action === null
      ? null : expectedIdScore(item.expected_memory_ids, []);
    scores.forbiddenAbsent = null;
    scores.passageSupported = null;
    retrievalStage.status = "not-run" satisfies StageStatus;
    retrievalStage.reason = action === null ? "native_context_unavailable" : `native_context_${action}`;
    viewportStage.status = "not-run" satisfies StageStatus;
    viewportStage.reason = action === null ? "native_context_unavailable" : `native_context_${action}`;
  } else if (contextStage.refreshFailed === true) {
    scores.expectedEvidencePresent = null;
    scores.forbiddenAbsent = null;
    scores.passageSupported = null;
    retrievalStage.status = "failed" satisfies StageStatus;
    retrievalStage.reason = "native-refresh-failed";
    viewportStage.status = "not-run" satisfies StageStatus;
    viewportStage.reason = "native-refresh-failed";
  } else if (!nativePresentation && contextStage.observedEmpty === true) {
    scores.expectedEvidencePresent = expectedIdScore(item.expected_memory_ids, []);
    scores.forbiddenAbsent = forbiddenScore(item.forbidden_memory_ids, []);
    scores.passageSupported = item.expected_passage === undefined ? null : false;
    retrievalStage.status = "ok" satisfies StageStatus;
    retrievalStage.source = "native-empty-refresh";
    retrievalStage.candidateRecallScored = false;
    viewportStage.status = "ok" satisfies StageStatus;
    viewportStage.source = "native-empty-refresh";
    viewportStage.found = false;
    viewportStage.keptCount = 0;
  } else if (!nativePresentation) {
    scores.retrievalRecall = null;
    scores.admittedPrecision = null;
    scores.expectedEvidencePresent = null;
    scores.forbiddenAbsent = null;
    scores.passageSupported = null;
    retrievalStage.status = "unavailable" satisfies StageStatus;
    retrievalStage.reason = "native_presentation_unavailable";
    viewportStage.status = "unavailable" satisfies StageStatus;
    viewportStage.reason = "native_presentation_unavailable";
  } else {
    retrievalStage.status = "ok" satisfies StageStatus;
    retrievalStage.source = "native-context";
    retrievalStage.candidateCount = finalIds.length;
    retrievalStage.candidateRecallScored = false;
    retrievalStage.rerankerStatus = contextStage.jevStatus ?? null;
    viewportStage.status = "ok" satisfies StageStatus;
    viewportStage.source = "native-context";
    viewportStage.found = nativePresentation.found === true;
    viewportStage.keptCount = finalIds.length;
    viewportStage.suppressedCount = null;
    scores.retrievalRecall = null;
    scores.expectedEvidencePresent = expectedIdScore(item.expected_memory_ids, finalIds);
    scores.forbiddenAbsent = forbiddenScore(item.forbidden_memory_ids, finalIds);
    scores.passageSupported = item.expected_passage !== undefined
      ? containsPassage(finalCandidates, item.expected_passage) : null;
  }

  const scored = Object.values(scores).filter((value): value is boolean => typeof value === "boolean");
  return {
    id: item.id,
    room: item.room,
    scope: { room: item.room, includesHouse: true, source: "native-room-bound-context" },
    requestedScope: targetScope,
    scopeOverrideExercised: false,
    scopeRefusalScored: false,
    mode: options.mode,
    authored_probe: item.authored_probe ?? null,
    provenanceDigest: item.provenance ? digest(item.provenance) : null,
    stages: {
      nativeContext: contextStage,
      classification: routeStage,
      policy: policyStage,
      retrieval: retrievalStage,
      viewport: viewportStage,
    },
    scores: {
      ...scores,
      scoredCount: scored.length,
      passedCount: scored.filter(Boolean).length,
    },
  };
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  if (args.includes("--help") || args.includes("-h")) {
    console.log(usage());
    return;
  }
  const casesPath = arg("--cases", args);
  if (!casesPath) throw new Error(`--cases is required\n\n${usage()}`);
  const input = validateCaseFile(JSON.parse(readFileSync(casesPath, "utf8")));
  const room = arg("--room", args) ?? input.binding?.room ?? process.env.ATHANOR_EVAL_ROOM;
  if (!room) throw new Error("provide --room, binding.room, or ATHANOR_EVAL_ROOM for the authenticated current room");
  if (input.cases.some((item) => item.room !== room)) {
    throw new Error("every case room must equal the authenticated current room; sibling-room cases are refused");
  }
  const spirit = arg("--spirit", args) ?? input.binding?.spirit;
  const session = arg("--session", args) ?? input.binding?.session;
  if (!spirit || !session) throw new Error("provide --spirit and --session, or binding.spirit and binding.session in the private case file");
  const operator = arg("--operator", args) ?? input.binding?.operator ?? spirit;
  const mode = arg("--mode", args) ?? "baseline";
  if (mode !== "baseline" && mode !== "after") throw new Error("--mode must be baseline or after");
  const credentialEnv = arg("--credential-env", args);
  const credential = credentialEnv ? process.env[credentialEnv] : undefined;
  const runId = randomUUID();
  const startedAt = new Date().toISOString();
  const results: Record<string, unknown>[] = [];
  for (const item of input.cases) {
    results.push(await runCase(item, input, { spirit, session, operator, mode, runId, credential }));
  }
  const checksByKind: Record<string, { scored: number; passed: number }> = {};
  for (const result of results) {
    for (const [name, value] of Object.entries(result.scores as Record<string, unknown>)) {
      if (typeof value !== "boolean") continue;
      const group = result.authored_probe === true ? "authored" : "real";
      const counts = checksByKind[`${group}.${name}`] ??= { scored: 0, passed: 0 };
      counts.scored++;
      if (value) counts.passed++;
    }
  }
  const output = {
    schemaVersion: 2,
    evaluation: "recall-boundary",
    mode,
    startedAt,
    completedAt: new Date().toISOString(),
    caseCount: results.length,
    dependencies: {
      hostUrl: process.env.ATHANOR_HOST_URL ? "configured" : "default_loopback",
      credential: { env: credentialEnv ?? null, provided: Boolean(credential) },
      embedder: {
        urlConfigured: Boolean(process.env.ATHANOR_EMBED_URL),
        modelConfigured: Boolean(process.env.ATHANOR_EMBED_MODEL),
        dimension: process.env.ATHANOR_EMBED_DIMENSION ? "configured" : "default_or_host_owned",
      },
      jev: {
        policy: "host-native",
        reranker: "reported_per_retrieval_stage",
        providerConfigured: Boolean(process.env.ATHANOR_JEV_PROVIDER),
      },
    },
    scores: {
      checksByKind,
      missingGoldUnscored: true,
    },
    cases: results,
    privacy: {
      promptsPublished: false,
      memoryIdsPublished: false,
      sourcePathsPublished: false,
    },
  };
  const outPath = arg("--out", args);
  const serialized = JSON.stringify(output, null, 2) + "\n";
  if (outPath) writeFileSync(outPath, serialized, "utf8");
  else process.stdout.write(serialized);
}

if (import.meta.main) {
  await main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
