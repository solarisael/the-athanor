import { requestOrgan, organFailure } from "./organ.ts";
import type { HostBinding } from "./host.ts";
import type { ResolvedRecallMode } from "./recall-policy.ts";

const RECALL_TIMEOUT_MS = 120_000;

type AgentContext = {
  agent?: { kind: string; name: string; parentId?: string };
};

export function automaticRecallAllowed(ctx: AgentContext): boolean {
  return ctx.agent?.kind !== "sub";
}

export function parentContextRequest(ctx: AgentContext): string | null {
  if (ctx.agent?.kind !== "sub" || ctx.agent.name === "memory-research") return null;
  const destination = ctx.agent.parentId ? `agent://${ctx.agent.parentId}` : null;
  return [
    "Ask Parent protocol (Ask Mama / Ask Daddy): use your task packet and supplied lessons.",
    "For missing intent, historical decisions, or memory context, ask your spawning parent a specific question.",
    destination
      ? `Send the question with write to ${destination}. Include what you need and which decision it blocks.`
      : "No parent address is available. Return the missing-context question as a blocker.",
    "Inspect repository facts yourself when the task requires it. Never invent a missing decision or work merely to stay busy.",
    "Continue only independent parts of the assigned task. If none remain, yield a blocked-context report; the parent can resume you by message.",
    "If messaging is unavailable, return the question as a blocker. Do not call wait solely to await a parent reply.",
    "Do not call Recall directly. Your parent retrieves and supplies the relevant evidence.",
  ].join("\n");
}

export function registerSubagentRecallProtocol(pi): void {
  pi.on("before_agent_start", (_event, ctx: AgentContext) => {
    const content = parentContextRequest(ctx);
    if (!content) return;
    return {
      message: { customType: "athanor-ask-parent", content, display: false },
    };
  });
  pi.on("tool_call", (event, ctx: AgentContext) => {
    const directRecall = event?.toolName === "recall";
    const deviceRecall = event?.toolName === "write"
      && /^xd:\/\/recall(?:[/?#]|$)/i.test(String(event?.input?.path ?? "").trim());
    if (!directRecall && !deviceRecall) return;
    const reason = parentContextRequest(ctx);
    if (reason) return { block: true, reason };
  });
}


export type RecallProjection = "auto" | "manual";

// Manual reads retain complete database bodies; automatic reads keep the native excerpt bounds.
export async function recallWithRouting(
  room: string,
  query: string,
  {
    binding,
    signal,
    temporalDecay = false,
    projection = "auto",
    timeoutMs = RECALL_TIMEOUT_MS,
    rerankCandidateTopK,
    mode,
  }: {
    binding: HostBinding;
    signal?: AbortSignal;
    temporalDecay?: boolean;
    projection?: RecallProjection;
    timeoutMs?: number;
    rerankCandidateTopK?: number;
    mode?: ResolvedRecallMode;
  },
) {
  const params = {
    query,
    ...(Number.isSafeInteger(rerankCandidateTopK) && rerankCandidateTopK > 0
      ? { rerank_candidate_top_k: rerankCandidateTopK } : {}),
    ...(mode ? { mode } : {}),
    ...(temporalDecay ? { temporal_decay: true } : {}),
    ...(projection === "manual" ? { projection: "manual" } : {}),
  };
  try {
    if (binding.room !== room) throw new Error("foreign room binding refused");
    const result = await requestOrgan(binding, "recall", params, { signal, timeoutMs });
    return { ok: true, result };
  } catch (error) {
    return { ok: false, result: { ok: false, query, ...organFailure(error) } };
  }
}
