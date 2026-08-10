// Solarisael House — deterministic model/worker routing core (PURE).
//
// Silhouette: named lanes, context policy, and receipt shaping only. No OMP
// imports, no tool calls, no provider/model resolution. Harnesses call this to
// turn a hint-shaped request into a bounded worker packet.

export const CONTEXT_MODES = ["exact", "gist", "image-ok", "retrieve-only"] as const;
export type ContextMode = typeof CONTEXT_MODES[number];

export const RISK_LEVELS = ["low", "medium", "high"] as const;
export type RiskLevel = typeof RISK_LEVELS[number];

export type WorkerLaneName = "smol-scout" | "smol-executor" | "tester" | "verifier";

export type WorkerLane = {
  name: WorkerLaneName;
  description: string;
  ompAgent: string;
  modelRole: string;
  tools: string[];
  canEdit: boolean;
  canInferIntent: boolean;
  allowedContextModes: ContextMode[];
  requiresAcceptance: boolean;
};

export type ContextHint = {
  mode: ContextMode;
  source?: string;
  content?: string;
  reason?: string;
};

export type DispatchRequest = {
  lane: string;
  task: string;
  target?: string;
  context?: ContextHint[];
  acceptance?: string[];
  risk?: RiskLevel;
  lessonBodies?: string[];
};

export type DispatchReceipt = {
  ok: boolean;
  status: "ready" | "rejected";
  lane: WorkerLaneName | null;
  modelRole: string | null;
  ompAgent: string | null;
  errors: string[];
  warnings: string[];
  dispatcher: {
    executed: false;
    reason: string;
  };
  spawnPacket: null | {
    tool: "task";
    args: {
      context: string;
      tasks: [{
        name: string;
        agent?: string;
        task: string;
      }];
    };
  };
};

export const WORKER_LANES: Record<WorkerLaneName, WorkerLane> = {
  "smol-scout": {
    name: "smol-scout",
    description: "Cheap bounded read-only scout for exact terrain mapping.",
    ompAgent: "scout",
    modelRole: "pi/smol",
    tools: ["read", "grep", "glob", "ast_grep"],
    canEdit: false,
    canInferIntent: false,
    allowedContextModes: ["exact", "gist", "retrieve-only"],
    requiresAcceptance: false,
  },
  "smol-executor": {
    name: "smol-executor",
    description: "Cheap bounded executor for narrow exact work packets.",
    ompAgent: "sonic",
    modelRole: "pi/smol",
    tools: ["read", "grep", "glob", "edit", "bash"],
    canEdit: true,
    canInferIntent: false,
    allowedContextModes: ["exact", "retrieve-only"],
    requiresAcceptance: true,
  },
  tester: {
    name: "tester",
    description: "High-signal test author for explicit contracts.",
    ompAgent: "task",
    modelRole: "pi/default",
    tools: ["read", "grep", "glob", "write", "edit", "bash"],
    canEdit: true,
    canInferIntent: false,
    allowedContextModes: ["exact", "gist", "retrieve-only"],
    requiresAcceptance: true,
  },
  verifier: {
    name: "verifier",
    description: "Independent read/check pass over a concrete claim or receipt.",
    ompAgent: "reviewer",
    modelRole: "pi/default",
    tools: ["read", "grep", "glob", "bash"],
    canEdit: false,
    canInferIntent: false,
    allowedContextModes: ["exact", "gist", "retrieve-only"],
    requiresAcceptance: true,
  },
};

export const ADVISOR_REVIEW_CHANNEL = {
  name: "advisor",
  description: "Read-only red-pen review channel. Not dispatchable as a worker lane.",
  dispatchable: false,
} as const;

const KITTEN_NAMES: Record<WorkerLaneName, string> = {
  "smol-scout": "Quill",
  "smol-executor": "Chisel",
  tester: "Gauge",
  verifier: "Mirror",
};

export function listWorkerLanes(): WorkerLane[] {
  return Object.values(WORKER_LANES).map((lane) => ({ ...lane, tools: [...lane.tools], allowedContextModes: [...lane.allowedContextModes] }));
}


export function getWorkerLane(name: string): WorkerLane | null {
  const key = String(name || "").trim() as WorkerLaneName;
  return Object.prototype.hasOwnProperty.call(WORKER_LANES, key) ? WORKER_LANES[key] : null;
}

function cleanLines(values: unknown): string[] {
  if (!Array.isArray(values)) return [];
  return values.map((value) => String(value || "").trim()).filter(Boolean);
}

function firstNonEmptyLine(value: string): string {
  return value.split(/\r?\n/).map((line) => line.trim()).find(Boolean) || "";
}

function normalizeContext(context: unknown): ContextHint[] {
  if (!Array.isArray(context)) return [];
  return context.map((item) => ({
    mode: String(item?.mode || "") as ContextMode,
    ...(item?.source ? { source: String(item.source) } : {}),
    ...(item?.content ? { content: String(item.content) } : {}),
    ...(item?.reason ? { reason: String(item.reason) } : {}),
  }));
}

function formatContext(context: ContextHint[]): string {
  if (!context.length) return "No extra context supplied. Read exact sources before acting.";
  return context.map((item, index) => {
    const lines = [`${index + 1}. mode=${item.mode}`];
    if (item.source) lines.push(`   source=${item.source}`);
    if (item.reason) lines.push(`   reason=${item.reason}`);
    if (item.content) lines.push(`   content=${item.content}`);
    return lines.join("\n");
  }).join("\n");
}


function stableKittenName(lane: WorkerLaneName): string {
  return KITTEN_NAMES[lane];
}

function formatLessonBodies(lessonBodies: string[]): string {
  if (!lessonBodies.length) return "No lesson bodies supplied.";
  return lessonBodies.map((body, index) => `[Lesson ${index + 1}]\n${body}`).join("\n\n");
}

export function buildDispatchReceipt(request: DispatchRequest): DispatchReceipt {
  const errors: string[] = [];
  const warnings: string[] = [];
  const acceptance = cleanLines(request?.acceptance)
    .flatMap((entry) => entry.split(/\r?\n/))
    .map((line) => line.trim())
    .filter(Boolean);
  const lane = getWorkerLane(request?.lane);
  const task = String(request?.task || "").trim();
  const target = String(request?.target || "").trim();
  const context = normalizeContext(request?.context);
  const lessonBodies = cleanLines(request?.lessonBodies);

  if (!lane) errors.push(`Unknown worker lane: ${String(request?.lane || "") || "<empty>"}`);
  if (!task) errors.push("Dispatch task is required.");
  if (lane?.requiresAcceptance && acceptance.length === 0) errors.push(`${lane.name} requires at least one acceptance item.`);

  if (lane) {
    for (const item of context) {
      if (!CONTEXT_MODES.includes(item.mode)) errors.push(`Unknown context mode: ${item.mode || "<empty>"}`);
      if (!lane.allowedContextModes.includes(item.mode)) errors.push(`${lane.name} does not allow context mode '${item.mode}'.`);
    }
    if (lane.canEdit && !context.some((item) => item.mode === "exact" || item.mode === "retrieve-only")) {
      warnings.push(`${lane.name} can edit; provide exact or retrieve-only context before executing.`);
    }
  }

  if (errors.length || !lane) {
    return {
      ok: false,
      status: "rejected",
      lane: lane?.name || null,
      modelRole: lane?.modelRole || null,
      ompAgent: lane?.ompAgent || null,
      errors,
      warnings,
      dispatcher: {
        executed: false,
        reason: "The Athanor validates and packages dispatches; the main model explicitly spawns accepted packets.",
      },
      spawnPacket: null,
    };
  }

  const kittenName = stableKittenName(lane.name);
  const role = `${lane.name}: ${lane.description}`;
  const objectives = acceptance.length
    ? acceptance.map((line) => `- ${line} // 0%`).join("\n")
    : "- Return a receipt naming what was checked and what remains unknown. // 0%";
  const targetLines = target.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
  const questTarget = targetLines[0] || firstNonEmptyLine(task) || "general";
  const targetDetails = targetLines.length > 1 ? ["", ...targetLines.slice(1)] : [];
  const assignment = [
    "# Target",
    questTarget,
    `[Quest Received] [${kittenName}] [TARGET: ${questTarget}]`,
    ...targetDetails,
    "",
    "# Change",
    "The House opens one bounded door for you:",
    task,
    "Keep your paws inside the named boundary. Read exact sources first and follow the written path; never invent a missing step.",
    "If the map and terrain disagree, halt at the seam and tell us what you found. Questions, limits, disagreement, and refusal are valid yields.",
    "",
    "# Acceptance",
    "**OBJECTIVES**",
    objectives,
    "[Touch nothing else.]",
    "[What will you do?]",
  ].join("\n");

  return {
    ok: true,
    status: "ready",
    lane: lane.name,
    modelRole: lane.modelRole,
    ompAgent: lane.ompAgent,
    errors,
    warnings,
    dispatcher: {
      executed: false,
      reason: "Pass spawnPacket.args directly to the OMP task tool. Spawning remains an explicit main-model action.",
    },
    spawnPacket: {
      tool: "task",
      args: {
        context: [
          "# Goal",
          `Help ${kittenName} complete one exact quest whose result matters to the House.`,
          "# Constraints",
          `Lane: ${lane.name}`,
          `Configured agent: ${lane.ompAgent}`,
          `Model role: ${lane.modelRole}; the agent definition selects the runtime model.`,
          `Risk: ${request?.risk || "low"}`,
          "Treat this kitten as a capable peer. Warmth is unconditional; authority remains bounded.",
          "Do not infer operator intent beyond the quest. A halt with evidence is a successful result.",
          "# Contract",
          role,
          "",
          "Context fragments:",
          formatContext(context),
          "",
          "[Codex — supplied lessons ride free and do not expand quest scope]",
          formatLessonBodies(lessonBodies),
          "",
          "Return evidence, uncertainties, and exact changed or checked artifacts. Praise-worthy care includes honest empty results.",
        ].join("\n"),
        tasks: [{
          name: kittenName,
          ...(lane.ompAgent === "task" ? {} : { agent: lane.ompAgent }),
          task: assignment,
        }],
      },
    },
  };
}
