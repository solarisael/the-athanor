export const ATTEMPT_ENTRY = "athanor-orihon-attempt-v1";
export const STATE_ENTRY = "athanor-orihon-state-v1";
export const MAX_RECORDS = 64;
export const MAX_TEXT_BYTES = 1024 * 1024;
export const BATCH_TARGET_BYTES = 4 * 1024 * 1024;

export type Entry = {
  id: string;
  parentId: string | null;
  timestamp: string;
  type: string;
  customType?: string;
  data?: any;
  message?: any;
  content?: unknown;
  display?: boolean;
  attribution?: string;
  details?: any;
};

export type CaptureRecord = {
  sourceId: string;
  parentUserId: string | null;
  kind: "user" | "talk" | "tool" | "echo" | "work" | "note";
  speaker: string;
  text: string;
  saidAt: string;
  timestampObserved: boolean;
};

export type Cursor = { entryId: string; nextRecord: number };
export type PendingRecord = { record: CaptureRecord; cursor: Cursor };

export function sourceEntries(ctx: any): { session: string; entries: Entry[] } {
  const manager = ctx?.sessionManager;
  const session = manager?.getSessionId?.();
  const entries = manager?.getEntries?.();
  if (typeof session !== "string" || !session.trim() || !Array.isArray(entries)) {
    throw new Error("OMP replay source or actual session identity is unavailable");
  }
  const ids = new Set<string>();
  for (const entry of entries) {
    if (typeof entry?.id !== "string" || !entry.id || ids.has(entry.id)) {
      throw new Error("OMP replay source has a missing or duplicate entry ID");
    }
    ids.add(entry.id);
  }
  return { session, entries };
}

function message(entry: Entry): any {
  return entry.type === "custom_message" ? { ...entry, role: "custom" } : entry.message;
}

function isPage(entry: Entry): boolean {
  const value = message(entry);
  return value?.role === "user" && value.attribution === "user" && value.steering !== true;
}

function textContent(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content.filter((block) => block?.type === "text" && typeof block.text === "string")
    .map((block) => block.text).join("\n");
}

function sourceTime(entry: Entry, value: any): { saidAt: string; timestampObserved: boolean } {
  if (entry.type === "custom" && entry.customType === ATTEMPT_ENTRY) {
    return { saidAt: entry.timestamp, timestampObserved: true };
  }
  if (typeof value?.timestamp === "number" && Number.isFinite(value.timestamp)) {
    return { saidAt: new Date(value.timestamp).toISOString(), timestampObserved: false };
  }
  if (typeof entry.timestamp !== "string" || !Number.isFinite(Date.parse(entry.timestamp))) {
    throw new Error(`OMP entry ${entry.id} has no valid source timestamp`);
  }
  return { saidAt: entry.timestamp, timestampObserved: false };
}

type VisibleRecord = Pick<CaptureRecord, "kind" | "speaker" | "text"> & { suffix: string };

function attemptedRecords(entry: Entry, session: string): VisibleRecord[] {
  const data = entry.data;
  if (data?.sourceSession !== session) return [];
  if (typeof data.unavailableReason === "string") throw new Error(`OMP original attempted input ${entry.id} is unavailable (${data.unavailableReason})`);
  if (data?.version !== 1 || typeof data.toolCallId !== "string" || typeof data.toolName !== "string"
    || typeof data.input !== "string") throw new Error(`OMP attempted input ${entry.id} is malformed`);
  return [{
    suffix: "attempt", kind: "tool", speaker: `OMP attempted tool ${data.toolName} (${data.toolCallId})`, text: data.input,
  }];
}

function assistantRecords(value: any): VisibleRecord[] {
  const records: VisibleRecord[] = [];
  const text = textContent(value.content);
  if (text) records.push({ suffix: "talk", kind: "talk", speaker: "", text });
  const content = Array.isArray(value.content) ? value.content : [];
  for (const [index, block] of content.entries()) {
    if (block?.type !== "toolCall") continue;
    records.push({
      suffix: `tool:${index}`, kind: "tool", speaker: `OMP persisted tool ${String(block.name ?? "")}`,
      text: JSON.stringify({ toolCallId: block.id, input: block.arguments }),
    });
  }
  return records;
}

function backgroundRecord(entry: Entry, value: any): VisibleRecord {
  const jobs = value.details?.jobs;
  if (!Array.isArray(jobs) || !jobs.length || jobs.some((job) => typeof job?.jobId !== "string" || !job.jobId)) {
    throw new Error(`OMP background report ${entry.id} has no source job identities`);
  }
  const allTasks = jobs.every((job) => job.type === "task");
  const label = allTasks ? "OMP combined task report" : "OMP background report";
  return {
    suffix: "report", kind: allTasks ? "work" : "echo",
    speaker: `${label} ${jobs.map((job) => job.jobId).join(", ")}`, text: textContent(value.content),
  };
}

function customRecords(entry: Entry, value: any): VisibleRecord[] {
  if (value.display !== true) return [];
  if (value.customType === "collab-prompt" && value.attribution === "user") {
    return [{ suffix: "note", kind: "note", speaker: "", text: textContent(value.content) }];
  }
  if (value.customType === "irc:incoming" && typeof value.details?.from === "string"
    && typeof value.details?.message === "string") {
    return [{ suffix: "work", kind: "work", speaker: `OMP peer ${value.details.from}`, text: value.details.message }];
  }
  return value.customType === "async-result" ? [backgroundRecord(entry, value)] : [];
}

function messageRecords(entry: Entry, value: any): VisibleRecord[] {
  switch (value.role) {
    case "user": {
      if (value.attribution !== "user") return [];
      const kind = value.steering === true ? "note" : "user";
      return [{ suffix: kind, kind, speaker: "", text: textContent(value.content) }];
    }
    case "assistant": return assistantRecords(value);
    case "toolResult": return [{
      suffix: "echo", kind: "echo", speaker: `OMP tool ${String(value.toolName ?? "")}`,
      text: JSON.stringify({ toolCallId: value.toolCallId, isError: value.isError === true, text: textContent(value.content) }),
    }];
    case "bashExecution":
    case "pythonExecution": {
      const name = value.role === "bashExecution" ? "bash" : "python";
      return [
        { suffix: "tool", kind: "tool", speaker: `OMP user ${name}`, text: String(value.command ?? value.code ?? "") },
        { suffix: "echo", kind: "echo", speaker: `OMP user ${name}`, text: JSON.stringify({ output: value.output, exitCode: value.exitCode, cancelled: value.cancelled }) },
      ];
    }
    case "custom": return customRecords(entry, value);
    default: return [];
  }
}

function visibleRecords(entry: Entry, session: string): CaptureRecord[] {
  const value = message(entry);
  const records = entry.type === "custom" && entry.customType === ATTEMPT_ENTRY
    ? attemptedRecords(entry, session) : value ? messageRecords(entry, value) : [];
  return records.map(({ suffix, ...record }) => ({
    ...record, sourceId: `${entry.id}:${suffix}`, parentUserId: null, ...sourceTime(entry, value),
  }));
}

function parentPage(entry: Entry, byId: Map<string, { entry: Entry; index: number }>): { entry: Entry; index: number } | null {
  const seen = new Set<string>([entry.id]);
  let parentId = entry.parentId;
  while (parentId !== null) {
    if (typeof parentId !== "string" || seen.has(parentId)) throw new Error(`OMP ancestry is invalid at ${entry.id}`);
    seen.add(parentId);
    const parent = byId.get(parentId);
    if (!parent) throw new Error(`OMP ancestor ${parentId} is missing for ${entry.id}`);
    if (isPage(parent.entry)) return parent;
    parentId = parent.entry.parentId;
  }
  return null;
}

type EntryIndex = Map<string, { entry: Entry; index: number }>;

function replayPosition(byId: EntryIndex, baselineId: string | null, cursor: Cursor | null) {
  const baselineIndex = baselineId === null ? -1 : byId.get(baselineId)?.index;
  if (baselineIndex === undefined) throw new Error(`Orihon baseline source ${baselineId} is missing`);
  const checkpointIndex = cursor ? byId.get(cursor.entryId)?.index : baselineIndex + 1;
  if (checkpointIndex === undefined || (cursor && checkpointIndex <= baselineIndex)) {
    throw new Error("Orihon checkpoint source is missing or precedes its baseline");
  }
  return { baselineIndex, checkpointIndex };
}

function recordOffset(entry: Entry, records: CaptureRecord[], cursor: Cursor | null): number {
  const offset = cursor?.entryId === entry.id ? cursor.nextRecord : 0;
  if (!Number.isInteger(offset) || offset < 0 || offset > records.length) {
    throw new Error(`Orihon checkpoint no longer matches OMP entry ${entry.id}`);
  }
  return offset;
}

function hasReplayContent(entry: Entry): boolean {
  return entry.type === "message" || entry.type === "custom_message"
    || (entry.type === "custom" && entry.customType === ATTEMPT_ENTRY);
}

export function* replayRecords(entries: Entry[], session: string, baselineId: string | null, cursor: Cursor | null): Generator<PendingRecord> {
  const byId: EntryIndex = new Map(entries.map((entry, index) => [entry.id, { entry, index }]));
  const { baselineIndex, checkpointIndex } = replayPosition(byId, baselineId, cursor);
  for (let index = checkpointIndex; index < entries.length; index++) {
    const entry = entries[index];
    if (!hasReplayContent(entry)) continue;
    const page = isPage(entry) ? { entry, index } : parentPage(entry, byId);
    // The generation cannot adopt an older page merely because its reply arrived later.
    if (page && page.index <= baselineIndex) continue;
    const records = visibleRecords(entry, session);
    const offset = recordOffset(entry, records, cursor);
    if (!records.length) continue;
    if (!page) throw new Error(`OMP entry ${entry.id} has no source user page`);
    if (page.index > index) throw new Error(`OMP user page follows its child ${entry.id}`);
    for (let recordIndex = offset; recordIndex < records.length; recordIndex++) {
      const record = records[recordIndex];
      record.parentUserId = record.kind === "user" ? null : `${page.entry.id}:user`;
      if (record.kind !== "echo" && Buffer.byteLength(record.text, "utf8") > MAX_TEXT_BYTES) {
        throw new Error(`OMP source ${record.sourceId} exceeds the Orihon non-echo limit`);
      }
      yield { record, cursor: { entryId: entry.id, nextRecord: recordIndex + 1 } };
    }
  }
}
