import { HostUnavailable, hostCommand, sendHostCommand, type HostBinding, type HostResponse } from "./host.ts";

const CONTEXT_ANALYZE = "athanor.context.analyze";
const CONTEXT_ANALYZED = "athanor.context.analyzed";
const CONTEXT_VIEWPORT = "athanor.context.viewport";
const CONTEXT_VIEWPORTED = "athanor.context.viewported";

export type QueryRoute = {
  query: string;
  terms: string[];
  requiredTerms: string[];
  optionalTerms: string[];
  quotedPhrases: string[];
  codeTokens: string[];
  dateTokens: string[];
  entityHints: string[];
  stopwordStrippedQuery: string;
  intent: string;
  recognizedEntities: string[];
  entityResolutionSuggested: boolean;
  recallQuery: string;
  shouldAutoRecall: boolean;
  lanes: Record<string, boolean>;
  reasons: string[];
};
export type ContextAnalysis = {
  route: QueryRoute;
  keywordDirectives: Array<{ keyword: string; directive: string }>;
  keywordReminder: { keywords: string[]; text: string } | null;
  processTrigger: string | null;
  nudge: { band: number; pct: number; tokens: number; text: string } | null;
  roomReminder: string;
  routingReminder: string | null;
};
export type RecallViewport = {
  keptCandidates: unknown[];
  suppressions: Array<{ identity: string; reason: string }>;
  diagnostics: { kept: number; suppressed: number; reasons: Record<string, number> };
  presentation: Record<string, any>;
};

export function parseContextAnalysisResponse(response: HostResponse): ContextAnalysis {
  const analysis = response.analysis;
  if (!analysis || typeof analysis !== "object" || Array.isArray(analysis)) {
    throw new HostUnavailable("Context Host response omitted analysis");
  }
  const route = (analysis as Record<string, unknown>).route;
  if (!route || typeof route !== "object" || Array.isArray(route)) {
    throw new HostUnavailable("Context Host analysis omitted the query route");
  }
  return analysis as ContextAnalysis;
}

export async function analyzeContext(
  binding: HostBinding,
  request: {
    prompt: string;
    recognizedEntities?: string[];
    contextCharacters?: number;
    activeSpirit: string;
    operator: string;
    routingModeEnabled?: boolean;
  },
  idempotencyKey?: string,
): Promise<ContextAnalysis> {
  const response = await sendHostCommand(
    hostCommand(binding, CONTEXT_ANALYZE, "context", { context_request: request }, idempotencyKey),
    new Set([CONTEXT_ANALYZED]),
  );
  return parseContextAnalysisResponse(response);
}

export async function applyRecallViewport(
  binding: HostBinding,
  result: Record<string, unknown>,
  mode: "automatic" | "manual",
  idempotencyKey?: string,
): Promise<RecallViewport> {
  const response = await sendHostCommand(
    hostCommand(
      binding,
      CONTEXT_VIEWPORT,
      "context",
      { recall_result: result, context_viewport_mode: mode },
      idempotencyKey,
    ),
    new Set([CONTEXT_VIEWPORTED]),
  );
  return response.result as RecallViewport;
}
