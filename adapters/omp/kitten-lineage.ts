// OMP task-event diagnostics.
//
// OMP reports a subagent twice: a progress event when the work is handed out
// and a lifecycle event when it settles. Task results are no longer written as
// automatic room memories (enrolled workers keep them in their task book), so
// this module only counts and joins those events for kitten_lineage_status.

const compactLine = (value: unknown): string => String(value ?? "").replace(/\s+/g, " ").trim();

export function kittenLifecycleJoinKey(payload: unknown): string {
  const event = payload && typeof payload === "object" && !Array.isArray(payload)
    ? payload as Record<string, unknown>
    : {};
  const parentToolCallId = compactLine(event.parentToolCallId);
  const index = Number(event.index);
  if (parentToolCallId && Number.isInteger(index) && index >= 0) return `${parentToolCallId}:${index}`;
  return compactLine(event.id);
}

type KittenLineageDiagnostics = {
  progressEvents: number;
  lifecycleEvents: number;
  lifecycleWithoutProgress: number;
  lastProgressId: string | null;
  lastLifecycleId: string | null;
  lastLifecycleStatus: string | null;
  lastProgressKeys: string[];
  lastLifecycleKeys: string[];
};

const lineageDiagnostics: KittenLineageDiagnostics = {
  progressEvents: 0,
  lifecycleEvents: 0,
  lifecycleWithoutProgress: 0,
  lastProgressId: null,
  lastLifecycleId: null,
  lastLifecycleStatus: null,
  lastProgressKeys: [],
  lastLifecycleKeys: [],
};

function safeKeys(payload: unknown): string[] {
  return payload && typeof payload === "object" && !Array.isArray(payload)
    ? Object.keys(payload).sort().slice(0, 24)
    : [];
}

export function noteKittenProgress(payload: unknown, id: string): void {
  lineageDiagnostics.progressEvents += 1;
  lineageDiagnostics.lastProgressId = id || null;
  lineageDiagnostics.lastProgressKeys = safeKeys(payload);
}

export function noteKittenLifecycle(payload: unknown, id: string, progressFound: boolean): void {
  lineageDiagnostics.lifecycleEvents += 1;
  if (!progressFound) lineageDiagnostics.lifecycleWithoutProgress += 1;
  lineageDiagnostics.lastLifecycleId = id || null;
  lineageDiagnostics.lastLifecycleStatus = compactLine((payload as Record<string, unknown>)?.status) || null;
  lineageDiagnostics.lastLifecycleKeys = safeKeys(payload);
}

export function kittenLineageDiagnostics(): KittenLineageDiagnostics {
  return {
    ...lineageDiagnostics,
    lastProgressKeys: [...lineageDiagnostics.lastProgressKeys],
    lastLifecycleKeys: [...lineageDiagnostics.lastLifecycleKeys],
  };
}

export function kittenLineageDisabled(environ: Record<string, string | undefined> = process.env): boolean {
  return environ.ATHANOR_REPLAY_MODE === "1" || environ.ATHANOR_DISABLE_KITTEN_LINEAGE === "1";
}
