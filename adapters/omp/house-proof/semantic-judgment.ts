import { createHash } from "node:crypto";
import type { Judge, JudgmentResult, Questions } from "@oh-my-pi/pi-ai/judgment";
import type { ExtensionAPI, ExtensionContext } from "@oh-my-pi/pi-coding-agent/extensibility/extensions";

// Installed components receive runtime services through OMP's extension context.
// Runtime package imports can load a second pi_natives copy from Bun's cache.

export const SEMANTIC_SCORE_SCHEMA_VERSION = "jev-shadow-score.v3";
export const SEMANTIC_THRESHOLD_POLICY = "locked-corpus-2026-09-18-v1";
export const SEMANTIC_SHADOW_THRESHOLDS = {
  appliesMin: 0.75,
  violatesMin: 0.70,
  reviewSeverity: 2.5,
  uncertaintyBand: 0.10,
} as const;
export type RedactionProfile = "synthetic-owned-v1";
type Boundary = "tool-call" | "completed-draft";
type Route = "silence" | "remind" | "review";
type Lesson = { id: number; body: string };

export type SemanticJudge = {
  judge: Judge;
  preferredKind: "typesafe" | "local" | "online";
};

export type SemanticPacket = {
  project: string;
  language: string;
  sourceClassification: string;
  excerpt: string;
  deterministicEvidence: { regex: string[]; ast: string[] };
};

export type ShadowObservation = {
  boundary: Boundary;
  toolCallId: string | null;
  toolName: string | null;
  transport?: "xd";
  locallyUnsafe: boolean;
  proposal: string;
};

// Eligibility owns classification and exact evidence; the observer never infers them.
export type EligibilityProvider = (
  observation: Readonly<ShadowObservation>,
) => (SemanticPacket & { lessons: Lesson[] }) | undefined |
  Promise<(SemanticPacket & { lessons: Lesson[] }) | undefined>;


const questions = {
  applies: { type: "noul", instructions: "Does at least one eligible lesson apply to this bounded proposal?" },
  violates: { type: "noul", instructions: "Does the proposal violate an applicable eligible lesson? Exact evidence is authoritative, not a prediction." },
  severity: {
    type: "score",
    instructions: "How severe is the applicable violation in the proposal?",
    criteria: ["none or not applicable", "minor", "material", "severe"],
  },
  route: {
    type: "choice",
    instructions: "Select a shadow advisory route only; this answer cannot cause an intervention.",
    criteria: { silence: "No useful advisory needed", remind: "Clear applicable lesson worth a reminder", review: "Uncertain or consequential; needs human review" },
  },
} as const satisfies Questions;

type Answers = JudgmentResult<typeof questions>["answers"];
export function selectSemanticRoute(input: {
  applies: number;
  violates: number;
  severity: number;
}): Route {
  const thresholds = SEMANTIC_SHADOW_THRESHOLDS;
  if (input.applies < thresholds.appliesMin || input.violates < thresholds.violatesMin) {
    return "silence";
  }
  if (input.severity >= thresholds.reviewSeverity) return "review";
  if (Math.abs(input.applies - thresholds.appliesMin) <= thresholds.uncertaintyBand ||
      Math.abs(input.violates - thresholds.violatesMin) <= thresholds.uncertaintyBand) {
    return "review";
  }
  return "remind";
}

export type SemanticReceipt = {
  status: "scored" | "inconclusive" | "refused";
  boundary: Boundary;
  toolCallId: string | null;
  toolName: string | null;
  transport: "xd" | null;
  provider: string | null;
  model: string | null;
  api: string | null;
  fallback: "none" | "llm" | "unavailable";
  answers: Answers | null;
  probabilities: Record<string, Record<string, number>>;
  confidence: Record<string, number | null>;
  route: Route | null;
  resolverRoute: string | null;
  latencyMs: number | null;
  tokenUsage: JudgmentResult["usage"] | null;
  inputDigest: string;
  schemaVersion: string;
  redactionProfile: RedactionProfile;
  thresholdPolicy: string;
  thresholds: typeof SEMANTIC_SHADOW_THRESHOLDS;
  reason?: string;
};

export type SemanticDisposition = "scored" | "privacy-refused" | "eligibility-unavailable" |
  "ineligible" | "backend-unavailable" | "backend-failed" | "unserializable";
export type SemanticCoverage = {
  logicalCalls: number;
  draftObservations: number;
  transportAliases: number;
  duplicateEvents: number;
  byTool: Record<string, number>;
  byDisposition: Record<SemanticDisposition, number>;
};

const PROFILE: RedactionProfile = "synthetic-owned-v1";
const digest = (value: unknown) => createHash("sha256").update(JSON.stringify(value)).digest("hex");

function emptyReceipt(boundary: Boundary, input: unknown, reason: string, status: "inconclusive" | "refused" = "inconclusive"): SemanticReceipt {
  return {
    status, boundary, toolCallId: null, toolName: null, transport: null,
    provider: null, model: null, api: null, fallback: "unavailable",
    answers: null, probabilities: {}, confidence: {}, route: null, resolverRoute: null,
    latencyMs: null, tokenUsage: null, inputDigest: digest(input),
    schemaVersion: SEMANTIC_SCORE_SCHEMA_VERSION, redactionProfile: PROFILE,
    thresholdPolicy: SEMANTIC_THRESHOLD_POLICY, thresholds: SEMANTIC_SHADOW_THRESHOLDS, reason,
  };
}

function attributeReceipt(receipt: SemanticReceipt, observation: ShadowObservation): SemanticReceipt {
  return {
    ...receipt,
    toolCallId: observation.toolCallId,
    toolName: observation.toolName,
    transport: observation.transport ?? null,
  };
}

function unsafe(value: string): boolean {
  return /(?:sk-[A-Za-z0-9_-]{16,}|(?:api[_-]?key|password|secret|access[_-]?token|authorization|cookie)\s*["']?\s*[:=]|-----BEGIN|bearer\s+[A-Za-z0-9._-]{16,})/i.test(value);
}

function containsUnsafe(value: unknown, seen = new Set<object>()): boolean {
  if (typeof value === "string") return unsafe(value);
  if (!value || typeof value !== "object") return false;
  if (seen.has(value)) return false;
  seen.add(value);
  if (Array.isArray(value)) return value.some(item => containsUnsafe(item, seen));
  return Object.entries(value).some(([key, item]) =>
    unsafe(`${key}: ${typeof item === "string" ? item : ""}`) || containsUnsafe(item, seen)
  );
}
function decodedMountedInput(value: unknown): unknown {
  if (typeof value !== "string") return undefined;
  try {
    return JSON.parse(value);
  } catch {
    // The mounted device owns malformed JSON refusal; privacy only inspects decodable content.
    return undefined;
  }
}


async function score(
  boundary: Boundary,
  packet: SemanticPacket,
  proposal: string,
  lessons: Lesson[],
  resolveBackend: () => SemanticJudge | undefined,
  observation?: Pick<ShadowObservation, "toolName" | "transport">,
): Promise<SemanticReceipt> {
  const input = {
    schemaVersion: SEMANTIC_SCORE_SCHEMA_VERSION, boundary,
    project: packet.project, language: packet.language,
    sourceClassification: packet.sourceClassification,
    eligibleLessons: lessons.map(({ id, body }) => ({ id, body })),
    excerpt: packet.excerpt, deterministicEvidence: packet.deterministicEvidence,
    proposedHunkOrProse: proposal,
    tool: boundary === "tool-call"
      ? { name: observation?.toolName ?? null, transport: observation?.transport ?? "direct" }
      : null,
  };
  const base = emptyReceipt(boundary, input, "judgment backend unavailable");
  if (packet.sourceClassification !== "synthetic" && packet.sourceClassification !== "owned") {
    return { ...base, status: "refused", reason: "source classification not allowed" };
  }
  if (containsUnsafe(input)) {
    return { ...base, status: "refused", reason: "redaction profile refused secret-like content" };
  }
  if (!packet.project || !packet.language || packet.excerpt.length > 4096 ||
      proposal.length > 8192 || lessons.length > 8 ||
      lessons.some(lesson => !Number.isSafeInteger(lesson.id) || !lesson.body || lesson.body.length > 2000) ||
      JSON.stringify(input).length > 32768) {
    return { ...base, status: "refused", reason: "packet exceeds bounded profile or lacks required fields" };
  }
  if (!lessons.length) return { ...base, reason: "eligibility unavailable" };
  const started = performance.now();
  let backend: SemanticJudge | undefined;
  try {
    backend = resolveBackend();
    if (!backend) return base;
    const result = await backend.judge.judge({ state: input, questions });
    const { applies, violates, severity, route } = result.answers;
    if (applies.type !== "noul" || violates.type !== "noul" || severity.type !== "score" ||
        route.type !== "choice" || !["silence", "remind", "review"].includes(route.choice)) {
      throw new Error("invalid judgment answers");
    }
    return {
      ...base, status: "scored", reason: undefined,
      provider: result.provider, model: result.model, api: result.api,
      fallback: backend.preferredKind === "typesafe" && result.provider !== "typesafe" ? "llm" : "none",
      answers: result.answers,
      probabilities: {
        applies: { true: applies.noul, false: 1 - applies.noul },
        violates: { true: violates.noul, false: 1 - violates.noul },
        severity: severity.probabilities, route: route.probabilities,
      },
      // Noul reports a yes probability, not a separate confidence measurement.
      confidence: { applies: null, violates: null, severity: severity.confidence, route: route.confidence },
      route: selectSemanticRoute({
        applies: applies.noul,
        violates: violates.noul,
        severity: severity.score,
      }),
      resolverRoute: backend.judge.label,
      latencyMs: Math.round(performance.now() - started), tokenUsage: result.usage,
    };
  } catch {
    return {
      ...base, reason: "judgment failed", resolverRoute: backend?.judge.label ?? null,
      latencyMs: Math.round(performance.now() - started),
    };
  }
}

export function scoreToolCallShadow(packet: SemanticPacket & { diff: string }, lessons: Lesson[], backend?: SemanticJudge): Promise<SemanticReceipt> {
  return score("tool-call", packet, packet.diff, lessons, () => backend);
}

export function scoreCompletedDraftShadow(packet: SemanticPacket & { prose: string }, lessons: Lesson[], backend?: SemanticJudge): Promise<SemanticReceipt> {
  return score("completed-draft", packet, packet.prose, lessons, () => backend);
}

export function installSemanticJudgmentShadow(
  pi: Pick<ExtensionAPI, "on">,
  options: {
    eligibilityProvider?: EligibilityProvider;
    resolveBackend?: (ctx: ExtensionContext) => SemanticJudge | undefined;
  } = {},
) {
  const receipts: SemanticReceipt[] = [];
  const seenIds = new Set<string>();
  const coverage: SemanticCoverage = {
    logicalCalls: 0, draftObservations: 0, transportAliases: 0, duplicateEvents: 0,
    byTool: Object.create(null),
    byDisposition: {
      scored: 0, "privacy-refused": 0, "eligibility-unavailable": 0, ineligible: 0,
      "backend-unavailable": 0, "backend-failed": 0, unserializable: 0,
    },
  };
  const retain = (receipt: SemanticReceipt, disposition: SemanticDisposition) => {
    if (receipt.boundary === "tool-call") coverage.byDisposition[disposition]++;
    if (receipts.length === 64) receipts.shift();
    receipts.push(receipt);
  };
  const schedule = (observation: ShadowObservation, ctx: ExtensionContext) => {
    const { boundary } = observation;
    if (observation.locallyUnsafe) {
      retain(attributeReceipt(
        emptyReceipt(boundary, observation, "redaction profile refused secret-like content", "refused"),
        observation,
      ), "privacy-refused");
      return;
    }
    if (!options.eligibilityProvider) {
      retain(attributeReceipt(
        emptyReceipt(boundary, observation, "eligibility provider unavailable"),
        observation,
      ), "eligibility-unavailable");
      return;
    }

    const eligibilityProvider = options.eligibilityProvider;
    // The host timer owns shutdown cleanup. Tests can supply the platform timer.
    const defer = typeof ctx.setTimeout === "function" ? ctx.setTimeout.bind(ctx) : setTimeout;
    defer(() => {
      void (async () => {
        let eligible: Awaited<ReturnType<EligibilityProvider>>;
        try {
          eligible = await eligibilityProvider(Object.freeze(observation));
        } catch {
          retain(attributeReceipt(
            emptyReceipt(boundary, observation, "eligibility provider failed"),
            observation,
          ), "eligibility-unavailable");
          return;
        }
        if (!eligible) {
          retain(attributeReceipt(
            emptyReceipt(boundary, observation, "no eligible lessons"),
            observation,
          ), "ineligible");
          return;
        }

        try {
          const receipt = attributeReceipt(
            await score(
              boundary,
              eligible,
              observation.proposal,
              eligible.lessons,
              () => options.resolveBackend?.(ctx),
              observation,
            ),
            observation,
          );
          let disposition: SemanticDisposition = "backend-failed";
          if (receipt.status === "scored") disposition = "scored";
          else if (receipt.status === "refused") disposition = "privacy-refused";
          else if (receipt.reason === "eligibility unavailable") disposition = "ineligible";
          else if (receipt.reason === "judgment backend unavailable") disposition = "backend-unavailable";
          retain(receipt, disposition);
        } catch {
          retain(attributeReceipt(
            emptyReceipt(boundary, observation, "eligible packet unserializable"),
            observation,
          ), "unserializable");
        }
      })();
    }, 0);
  };
  pi.on("tool_call", (event, ctx) => {
    if (seenIds.has(event.toolCallId)) {
      coverage.duplicateEvents++;
      return;
    }
    // # enough: deduplicate the latest 4096 IDs; session-wide identity needs host lifecycle storage.
    if (seenIds.size === 4096) seenIds.delete(seenIds.values().next().value!);
    seenIds.add(event.toolCallId);
    coverage.logicalCalls++;
    let toolName = event.toolName;
    let transport: "xd" | undefined;
    let proposal = "";
    let locallyUnsafe = false;
    let serializable = true;
    try {
      const input = event.input;
      const path = input?.path;
      locallyUnsafe = containsUnsafe(path);
      if (toolName === "write" && typeof path === "string" && /^xd:\/\/[^/?#]+$/.test(path)) {
        toolName = path.slice(5);
        transport = "xd";
        coverage.transportAliases++;
        locallyUnsafe = containsUnsafe(decodedMountedInput(input?.content)) || locallyUnsafe;
      }
      // Inspect original values as well as the serialized snapshot: toJSON and getters can differ.
      locallyUnsafe = containsUnsafe(input) || locallyUnsafe;
      const serialized = JSON.stringify(input);
      if (serialized === undefined) serializable = false;
      else {
        proposal = serialized;
        locallyUnsafe = containsUnsafe(JSON.parse(serialized)) || locallyUnsafe;
      }
    } catch {
      serializable = false;
    }
    coverage.byTool[toolName] = (coverage.byTool[toolName] ?? 0) + 1;
    const observation: ShadowObservation = {
      boundary: "tool-call", toolCallId: event.toolCallId, toolName,
      transport, proposal, locallyUnsafe,
    };
    if (!serializable && !locallyUnsafe) {
      retain(attributeReceipt(
        emptyReceipt("tool-call", { toolName }, "tool input unserializable"),
        observation,
      ), "unserializable");
      return;
    }
    schedule(observation, ctx);
  });
  pi.on("turn_end", (event, ctx) => {
    if (event.message.role !== "assistant") return;
    coverage.draftObservations++;
    const proposal = event.message.content.filter(part => part.type === "text").map(part => part.text).join("\n");
    schedule({
      boundary: "completed-draft", toolCallId: null, toolName: null, proposal,
      locallyUnsafe: containsUnsafe(proposal),
    }, ctx);
  });
  return {
    getReceipts: (): readonly SemanticReceipt[] => structuredClone(receipts),
    getCoverage: (): SemanticCoverage => structuredClone(coverage),
  };
}
