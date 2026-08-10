import { createHash } from "node:crypto";
import path from "node:path";
import { appendFile, mkdir, readFile } from "node:fs/promises";

export const RECALL_TELEMETRY_FILENAME = "recall-turns.jsonl";
const pendingWrites = new Map<string, Promise<void>>();

function enabledValue(value: unknown): boolean {
  return /^(?:1|true|yes|on)$/i.test(String(value || "").trim());
}

export async function recallTelemetryEnabled(effectiveRoomDir: string): Promise<boolean> {
  if (process.env.SOLARISAEL_RECALL_TELEMETRY !== undefined) {
    return enabledValue(process.env.SOLARISAEL_RECALL_TELEMETRY);
  }
  try {
    const marker = JSON.parse(await readFile(path.join(effectiveRoomDir, ".solarisael-room.json"), "utf8"));
    return marker?.recallTelemetry === true;
  } catch {
    return false;
  }
}

export function recallTelemetryPath(effectiveRoomDir: string): string {
  return path.join(effectiveRoomDir, ".omp", "runtime", RECALL_TELEMETRY_FILENAME);
}

function promptHash(prompt: string): string {
  return createHash("sha256").update(prompt).digest("hex");
}

function boundedError(value: unknown): string | null {
  const message = value instanceof Error ? value.message : String(value || "");
  return message ? message.slice(0, 500) : null;
}

function routeSummary(route: Record<string, any> | null): Record<string, unknown> | null {
  if (!route) return null;
  return {
    intent: route.intent || null,
    should_auto_recall: route.shouldAutoRecall === true,
    lanes: route.lanes || null,
    reasons: Array.isArray(route.reasons) ? route.reasons : [],
    term_count: Array.isArray(route.terms) ? route.terms.length : 0,
    required_term_count: Array.isArray(route.requiredTerms) ? route.requiredTerms.length : 0,
    date_count: Array.isArray(route.dateTokens) ? route.dateTokens.length : 0,
    recognized_entity_count: Array.isArray(route.recognizedEntities) ? route.recognizedEntities.length : 0,
  };
}

export async function recordRecallTelemetry({
  effectiveRoomDir,
  sessionId,
  room,
  prompt,
  route,
  status,
  viewport = null,
  viewportDiagnostics = null,
  error = null,
  capturedAt = new Date().toISOString(),
}: {
  effectiveRoomDir: string;
  sessionId?: string | null;
  room: string;
  prompt: string;
  route: Record<string, unknown> | null;
  status: "injected" | "empty" | "skipped" | "error";
  viewport?: Record<string, unknown> | null;
  viewportDiagnostics?: Record<string, unknown> | null;
  error?: unknown;
  capturedAt?: string;
}): Promise<boolean> {
  if (!(await recallTelemetryEnabled(effectiveRoomDir))) return false;
  const target = recallTelemetryPath(effectiveRoomDir);
  const entry = {
    schema_version: 1,
    captured_at: capturedAt,
    session_id: String(sessionId || ""),
    room,
    prompt_sha256: promptHash(prompt),
    prompt_chars: prompt.length,
    status,
    route: routeSummary(route),
    viewport_diagnostics: viewportDiagnostics,
    viewport,
    error: boundedError(error),
  };
  const previous = pendingWrites.get(target) || Promise.resolve();
  const next = previous
    .catch(() => undefined)
    .then(async () => {
      await mkdir(path.dirname(target), { recursive: true });
      await appendFile(target, `${JSON.stringify(entry)}\n`, "utf8");
    })
    .finally(() => {
      if (pendingWrites.get(target) === next) pendingWrites.delete(target);
    });
  pendingWrites.set(target, next);
  await next;
  return true;
}
