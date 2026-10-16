import { STATE_ENTRY, type Cursor, type Entry } from "./source.ts";

export type Checkpoint = { baselineId: string | null; cursor: Cursor | null };
export type Scope = { room: string; sourceSession: string; generation: string };

export function restoreCheckpoint(entries: Entry[], scope: Scope): Checkpoint | null {
  let state: Checkpoint | null = null;
  for (const entry of entries) {
    if (entry.type !== "custom" || entry.customType !== STATE_ENTRY) continue;
    const data = entry.data;
    if (data?.room !== scope.room || data?.sourceSession !== scope.sourceSession || data?.generation !== scope.generation) continue;
    if (data.version !== 1) throw new Error("Orihon checkpoint version is unsupported");
    if (data.stage === "baseline") {
      if (state || (data.baselineId !== null && typeof data.baselineId !== "string")) {
        throw new Error("Orihon baseline is malformed or repeated");
      }
      state = { baselineId: data.baselineId, cursor: null };
    } else if (data.stage === "checkpoint") {
      if (!state || typeof data.cursor?.entryId !== "string" || !data.cursor.entryId
        || !Number.isInteger(data.cursor.nextRecord) || data.cursor.nextRecord < 1) {
        throw new Error("Orihon checkpoint has no baseline or valid source position");
      }
      state.cursor = data.cursor;
    } else {
      throw new Error("Orihon checkpoint stage is unsupported");
    }
  }
  return state;
}

export function persistBaseline(pi: any, scope: Scope, entries: Entry[]): Checkpoint {
  const baselineId = entries.length ? entries[entries.length - 1].id : null;
  pi.appendEntry(STATE_ENTRY, { version: 1, stage: "baseline", ...scope, baselineId });
  return { baselineId, cursor: null };
}

export function persistCheckpoint(pi: any, scope: Scope, cursor: Cursor): void {
  pi.appendEntry(STATE_ENTRY, { version: 1, stage: "checkpoint", ...scope, cursor });
}
