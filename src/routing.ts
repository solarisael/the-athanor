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

function formatAcceptance(acceptance: string[]): string {
  return acceptance.length ? acceptance.map((line) => `- ${line}`).join("\n") : "- Return a receipt explaining what was checked and what remains unknown.";
}

function stableTaskId(lane: WorkerLaneName): string {
  return lane.replace(/(^|-)([a-z])/g, (_match, _dash, char) => char.toUpperCase()).slice(0, 32);
}

export function buildDispatchReceipt(request: DispatchRequest): DispatchReceipt {
  const errors: string[] = [];
  const warnings: string[] = [];
  const lane = getWorkerLane(request?.lane);
  const task = String(request?.task || "").trim();
  const target = String(request?.target || "").trim();
  const acceptance = cleanLines(request?.acceptance);
  const context = normalizeContext(request?.context);

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

  const role = `${lane.name}: ${lane.description}`;
  const assignment = [
    "# Target",
    target || task,
    "",
    "# Change",
    [
      `Execute this ${lane.name} work packet exactly as assigned.`,
      "Do not infer the operator's broader intent beyond the packet.",
      "Do not broaden scope.",
      "If the packet is ambiguous or conflicts with observed source state, stop and report the blocker.",
      "",
      task,
    ].join("\n"),
    "",
    "# Acceptance",
    formatAcceptance(acceptance),
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
          "Execute the validated Athanor worker packet from the main model.",
          "# Constraints",
          `Lane: ${lane.name}`,
          `Configured agent: ${lane.ompAgent}`,
          `Model role: ${lane.modelRole}; the agent definition selects the runtime model.`,
          `Risk: ${request?.risk || "low"}`,
          "Do not infer user intent beyond the packet.",
          "# Contract",
          role,
          "",
          "Context fragments:",
          formatContext(context),
          "",
          "Return evidence, uncertainties, and exact changed/checked artifacts.",
        ].join("\n"),
        tasks: [{
          name: stableTaskId(lane.name),
          ...(lane.ompAgent === "task" ? {} : { agent: lane.ompAgent }),
          task: assignment,
        }],
      },
    },
  };
}
