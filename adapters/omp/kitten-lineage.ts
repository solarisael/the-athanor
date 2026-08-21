// OMP quest-event translation.
//
// OMP reports a subagent twice: a progress event when the quest is handed out
// and a lifecycle event when it settles. Joining those two partial events is
// harness bookkeeping and stays here. Everything the join produces — whether a
// quest settled, what its memory says, and its write-once key — is decided by
// the Host.

export type KittenQuestProgress = {
  id?: unknown;
  index?: unknown;
  agent?: unknown;
  task?: unknown;
  assignment?: unknown;
  sessionFile?: unknown;
  parentToolCallId?: unknown;
  attemptId?: unknown;
};

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

// Docket attribution, when the room is standing inside a claimed quest. The
// attempt id is read at dispatch so a later claim cannot retroactively adopt
// work it never handed out, and it is additive: absent env, absent field, and
// every lineage consumer behaves exactly as before.
export function activeAttemptId(environ: Record<string, string | undefined> = process.env): string | null {
  return compactLine(environ.ATHANOR_ACTIVE_ATTEMPT_ID) || null;
}

export function stampAttemptId<T extends Record<string, unknown>>(
  event: T,
  environ: Record<string, string | undefined> = process.env,
): T {
  const attemptId = activeAttemptId(environ);
  return attemptId ? { ...event, attemptId } : event;
}

type KittenLineageDiagnostics = {
  progressEvents: number;
  lifecycleEvents: number;
  lifecycleWithoutProgress: number;
  writeCommitted: number;
  writeFailed: number;
  lastProgressId: string | null;
  lastLifecycleId: string | null;
  lastLifecycleStatus: string | null;
  lastProgressKeys: string[];
  lastLifecycleKeys: string[];
  lastAttemptId: string | null;
};

const lineageDiagnostics: KittenLineageDiagnostics = {
  progressEvents: 0,
  lifecycleEvents: 0,
  lifecycleWithoutProgress: 0,
  writeCommitted: 0,
  writeFailed: 0,
  lastProgressId: null,
  lastLifecycleId: null,
  lastLifecycleStatus: null,
  lastProgressKeys: [],
  lastLifecycleKeys: [],
  lastAttemptId: null,
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
  const attemptId = compactLine((payload as Record<string, unknown>)?.attemptId);
  if (attemptId) lineageDiagnostics.lastAttemptId = attemptId;
}

export function noteKittenLifecycle(payload: unknown, id: string, progressFound: boolean): void {
  lineageDiagnostics.lifecycleEvents += 1;
  if (!progressFound) lineageDiagnostics.lifecycleWithoutProgress += 1;
  lineageDiagnostics.lastLifecycleId = id || null;
  lineageDiagnostics.lastLifecycleStatus = compactLine((payload as Record<string, unknown>)?.status) || null;
  lineageDiagnostics.lastLifecycleKeys = safeKeys(payload);
}

export function noteKittenLineageWrite(committed: boolean): void {
  if (committed) lineageDiagnostics.writeCommitted += 1;
  else lineageDiagnostics.writeFailed += 1;
}

export function kittenLineageDiagnostics(): KittenLineageDiagnostics {
  return {
    ...lineageDiagnostics,
    lastProgressKeys: [...lineageDiagnostics.lastProgressKeys],
    lastLifecycleKeys: [...lineageDiagnostics.lastLifecycleKeys],
  };
}

export function kittenLineageDisabled(environ: Record<string, string | undefined> = process.env): boolean {
  return environ.SOLARISAEL_REPLAY_MODE === "1" || environ.ATHANOR_DISABLE_KITTEN_LINEAGE === "1";
}
