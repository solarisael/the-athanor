import { createHash } from "node:crypto";
import { GENERATED_TURN_ORIGINS } from "../turn-origin.ts";

export const ATTEMPT_ENTRY = "athanor-orihon-attempt-v1";
export const STATE_ENTRY = "athanor-orihon-state-v1";
export const MAX_RECORDS = 64;
export const MAX_TEXT_BYTES = 1024 * 1024;
export const BATCH_TARGET_BYTES = 4 * 1024 * 1024;
// A combined report names every finished job; the Host accepts this many child references on one record.
export const MAX_CHILDREN = 16;

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

export type ChildReference = { name: string; session: string };

export type CaptureRecord = {
  sourceId: string;
  parentUserId: string | null;
  kind: "user" | "talk" | "tool" | "echo" | "work" | "note";
  speaker: string;
  text: string;
  saidAt: string;
  timestampObserved: boolean;
  // The semantic fold input when `text` is an expanded dispatch payload. `text` stays the audit record.
  foldText?: string;
  // Exact child identities behind a combined task report.
  children?: ChildReference[];
};

export type Cursor = { entryId: string; nextRecord: number };
export type PendingRecord = { record: CaptureRecord; cursor: Cursor };

export type ReplayOptions = {
  // Replay stops before this entry: a fresh turn captures strictly the records before its own user page.
  stopBeforeEntryId?: string;
  // The child harness session behind a finished task job, or null when no transcript proves one.
  resolveChild?: (jobId: string) => string | null;
};

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

export function message(entry: Entry): any {
  return entry.type === "custom_message" ? { ...entry, role: "custom" } : entry.message;
}

export function isPage(entry: Entry): boolean {
  const value = message(entry);
  return value?.role === "user" && value.attribution === "user" && value.steering !== true;
}

export function textContent(content: unknown): string {
  if (typeof content === "string") return content;
  if (!Array.isArray(content)) return "";
  return content.filter((block) => block?.type === "text" && typeof block.text === "string")
    .map((block) => block.text).join("\n");
}

export type ImageReference = { index: number; mimeType: string; sha256: string };

export function imageReferences(content: unknown): ImageReference[] {
  if (!Array.isArray(content)) return [];
  return content.flatMap((block, index) => block?.type === "image"
    && typeof block.data === "string" && typeof block.mimeType === "string"
    ? [{
      index, mimeType: block.mimeType,
      sha256: createHash("sha256").update(block.mimeType).update("\0").update(block.data).digest("hex"),
    }] : []);
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

type VisibleRecord = Pick<CaptureRecord, "kind" | "speaker" | "text" | "foldText" | "children"> & { suffix: string };

function sha256(text: string): string {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

// A fold text above the record ceiling is dropped; the split audit text folds instead.
function withinCeiling(foldText: string): string | undefined {
  return Buffer.byteLength(foldText, "utf8") > MAX_TEXT_BYTES ? undefined : foldText;
}

// Keep task instructions intact. Only a recognized, separately supplied lesson
// bundle is represented by an exact audit reference instead of being paid for twice.
function lessonBundle(bodies: string[]): object {
  const text = JSON.stringify(bodies);
  return { count: bodies.length, bytes: Buffer.byteLength(text, "utf8"), sha256: sha256(text) };
}

function taskContext(context: unknown): unknown {
  if (typeof context !== "string") return context;
  const marker = "\n[Codex — supplied lessons ride free and do not expand quest scope]\n";
  const boundary = context.indexOf(marker);
  if (boundary < 0) return context;
  const sections = context.slice(boundary + marker.length).split(/\n?\[Lesson \d+\]\n/).filter((text) => text.trim());
  try {
    if (!sections.length || sections.some((text) => {
      const lesson = JSON.parse(text);
      return typeof lesson.lesson !== "string" || typeof lesson.type !== "string" || !Number.isInteger(lesson.id);
    })) return context;
    return { instructions: context.slice(0, boundary), suppliedLessons: lessonBundle(sections) };
  } catch {
    return context;
  }
}

function toolFoldText(name: string, toolCallId: unknown, args: any, sourceId: string): string | undefined {
  if (name === "task") {
    return withinCeiling(JSON.stringify({ ...args, context: taskContext(args?.context), toolCallId, audit: sourceId }));
  }
  if (name === "write" && args?.path === "xd://house_dispatch" && typeof args.content === "string") {
    try {
      const request = JSON.parse(args.content);
      if (!Array.isArray(request.lessonBodies) || !request.lessonBodies.every((body: unknown) => typeof body === "string")) return;
      return withinCeiling(JSON.stringify({
        ...request, lessonBodies: lessonBundle(request.lessonBodies),
        toolCallId, audit: sourceId, tool: "house_dispatch",
      }));
    } catch {
      return;
    }
  }
}

type PersistedCall = { sourceId: string; text: string };

function attemptedRecords(entry: Entry, session: string, persisted: (toolCallId: string) => PersistedCall | null): VisibleRecord[] {
  const data = entry.data;
  if (data?.sourceSession !== session) return [];
  if (data?.version !== 1 || typeof data.toolCallId !== "string" || typeof data.toolName !== "string"
    || (typeof data.input !== "string" && typeof data.unavailableReason !== "string")
    || (data.part !== undefined && !(Number.isInteger(data.part) && Number.isInteger(data.parts) && data.part >= 1 && data.part <= data.parts))) {
    throw new Error(`OMP attempted input ${entry.id} is malformed`);
  }
  const speaker = `OMP attempted tool ${data.toolName} (${data.toolCallId})`;
  if (typeof data.unavailableReason === "string") {
    // Entries written before inputs were split: the gap they recorded stays explicit.
    return [{ suffix: "attempt", kind: "tool", speaker,
      text: JSON.stringify({ toolCallId: data.toolCallId, toolName: data.toolName, unavailableReason: data.unavailableReason }) }];
  }
  if (data.part !== undefined) {
    // One oversized input lives in several entries; each part is its own exact record.
    return [{ suffix: "attempt", kind: "tool", speaker: `${speaker} part ${data.part} of ${data.parts}`, text: data.input }];
  }
  const call = persisted(data.toolCallId);
  let foldText = call && call.text === data.input
    ? withinCeiling(JSON.stringify({ toolCallId: data.toolCallId, sameAsPersisted: true, persistedSourceId: call.sourceId })) : undefined;
  if (!foldText) {
    try {
      foldText = toolFoldText(data.toolName, data.toolCallId, JSON.parse(data.input), `${entry.id}:attempt`);
    } catch {
      // Inputs need not be JSON; their exact bytes remain the fold input.
    }
  }
  return [{ suffix: "attempt", kind: "tool", speaker, text: data.input, ...(foldText ? { foldText } : {}) }];
}

function persistedToolText(block: any): string {
  return JSON.stringify({ toolCallId: block.id, input: block.arguments });
}

function assistantRecords(entry: Entry, value: any): VisibleRecord[] {
  const records: VisibleRecord[] = [];
  const text = textContent(value.content);
  if (text) records.push({ suffix: "talk", kind: "talk", speaker: "", text });
  const content = Array.isArray(value.content) ? value.content : [];
  for (const [index, block] of content.entries()) {
    if (block?.type !== "toolCall") continue;
    const suffix = `tool:${index}`;
    const foldText = toolFoldText(String(block.name ?? ""), block.id, block.arguments, `${entry.id}:${suffix}`);
    records.push({
      suffix, kind: "tool", speaker: `OMP persisted tool ${String(block.name ?? "")}`,
      text: persistedToolText(block), ...(foldText ? { foldText } : {}),
    });
  }
  return records;
}

function backgroundRecord(entry: Entry, value: any, resolveChild?: (jobId: string) => string | null): VisibleRecord {
  const jobs = value.details?.jobs;
  if (!Array.isArray(jobs) || !jobs.length || jobs.some((job) => typeof job?.jobId !== "string" || !job.jobId)) {
    throw new Error(`OMP background report ${entry.id} has no source job identities`);
  }
  const allTasks = jobs.every((job) => job.type === "task");
  const label = allTasks ? "OMP combined task report" : "OMP background report";
  const children: ChildReference[] = [];
  // OMP suffixes a colliding job id (worker, worker-2) but the child transcript keeps the
  // artifact id (details.jobs[].agentId), so the artifact id names the file; the job id is
  // the label a reader sees in the report.
  if (allTasks && resolveChild) {
    for (const job of jobs) {
      const artifact = typeof job.agentId === "string" && job.agentId ? job.agentId : job.jobId;
      const session = resolveChild(artifact);
      if (session) children.push({ name: job.jobId, session });
    }
  }
  return {
    suffix: "report", kind: allTasks ? "work" : "echo",
    speaker: `${label} ${jobs.map((job) => job.jobId).join(", ")}`, text: textContent(value.content),
    ...(children.length ? { children } : {}),
  };
}

function customRecords(entry: Entry, value: any, resolveChild?: (jobId: string) => string | null): VisibleRecord[] {
  if (value.display !== true) return [];
  if (value.customType === "collab-prompt" && value.attribution === "user") {
    return [{ suffix: "note", kind: "note", speaker: "", text: textContent(value.content) }];
  }
  if (value.customType === "irc:incoming" && typeof value.details?.from === "string"
    && typeof value.details?.message === "string") {
    return [{ suffix: "work", kind: "work", speaker: `OMP peer ${value.details.from}`, text: value.details.message }];
  }
  if (typeof value.customType === "string" && GENERATED_TURN_ORIGINS.has(value.customType)) {
    // A door opens a turn nobody typed; the Host keeps a work speaker verbatim.
    return [{ suffix: "door", kind: "work", speaker: `OMP door ${value.customType}`,
      text: `[${value.customType}]\n${textContent(value.content)}` }];
  }
  return value.customType === "async-result" ? [backgroundRecord(entry, value, resolveChild)] : [];
}

function messageRecords(entry: Entry, value: any, resolveChild?: (jobId: string) => string | null): VisibleRecord[] {
  switch (value.role) {
    case "user": {
      if (value.attribution !== "user") return [];
      const kind = value.steering === true ? "note" : "user";
      return [{ suffix: kind, kind, speaker: "", text: textContent(value.content) }];
    }
    case "assistant": return assistantRecords(entry, value);
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
    case "custom": return customRecords(entry, value, resolveChild);
    default: return [];
  }
}

// Byte-bounded contiguous parts on character boundaries. The same text always splits the same way.
export function splitParts(text: string, limit = MAX_TEXT_BYTES): string[] {
  if (Buffer.byteLength(text, "utf8") <= limit) return [text];
  const parts: string[] = [];
  let part = "";
  let bytes = 0;
  for (const character of text) {
    const size = Buffer.byteLength(character, "utf8");
    if (bytes + size > limit) {
      parts.push(part);
      part = "";
      bytes = 0;
    }
    part += character;
    bytes += size;
  }
  if (part) parts.push(part);
  return parts;
}

function expandParts(record: CaptureRecord): CaptureRecord[] {
  if (record.kind === "echo") return [record];
  const parts = splitParts(record.text);
  if (parts.length === 1) return [record];
  return parts.map((text, index) => ({
    ...record, text, sourceId: `${record.sourceId}#part-${index + 1}-of-${parts.length}`,
    ...(record.foldText === undefined ? {} : {
      foldText: index === 0 ? record.foldText
        : JSON.stringify({ continuationOf: record.sourceId, part: index + 1, of: parts.length, exactAuditOnly: true }),
    }),
  }));
}

function expandChildren(record: CaptureRecord): CaptureRecord[] {
  const children = record.children;
  if (!children || children.length <= MAX_CHILDREN) return [record];
  const records = [{ ...record, children: children.slice(0, MAX_CHILDREN) }];
  for (let index = MAX_CHILDREN; index < children.length; index += MAX_CHILDREN) {
    const part = children.slice(index, index + MAX_CHILDREN);
    records.push({
      ...record, sourceId: `${record.sourceId}:children:${index}`, children: part,
      speaker: "OMP report child references",
      text: JSON.stringify({ reportSourceId: record.sourceId, children: part }),
    });
  }
  return records;
}

// Records of one entry in capture order, without the page link. A gap in the
// source throws; the caller decides whether that stalls replay.
export function entryRecords(entry: Entry, session: string, persisted: (toolCallId: string) => PersistedCall | null,
  resolveChild?: (jobId: string) => string | null): CaptureRecord[] {
  const value = message(entry);
  const records = entry.type === "custom" && entry.customType === ATTEMPT_ENTRY
    ? attemptedRecords(entry, session, persisted) : value ? messageRecords(entry, value, resolveChild) : [];
  const images = records.length ? imageReferences(value?.content) : [];
  for (let index = 0; index < images.length; index += 8) {
    records.push({
      suffix: `images:${index}`, kind: "work", speaker: "OMP image attachments",
      text: JSON.stringify({ sourceEntry: entry.id, images: images.slice(index, index + 8) }),
    });
  }
  return records.flatMap(({ suffix, ...record }) => expandParts({
    ...record, sourceId: `${entry.id}:${suffix}`, parentUserId: null, ...sourceTime(entry, value),
  }).flatMap(expandChildren));
}

export type TranscriptRecord = Pick<CaptureRecord, "sourceId" | "kind" | "speaker" | "text" | "saidAt">;

// Transcript records for reading a child's own log: no pages, no fold identity.
// The task prompt a parent sends is a user message without operator
// attribution; the log shows it as a note from its sender. An unreadable
// entry becomes a visible gap instead of a skipped line.
export function transcriptRecords(entry: Entry, session: string): TranscriptRecord[] {
  if (!hasReplayContent(entry)) return [];
  try {
    const value = message(entry);
    if (value?.role === "user" && value.attribution !== "user") {
      return [{ sourceId: `${entry.id}:prompt`, kind: "note", speaker: `OMP ${String(value.attribution ?? "generated")} prompt`,
        text: textContent(value.content), saidAt: sourceTime(entry, value).saidAt }];
    }
    return entryRecords(entry, session, () => null).map(({ sourceId, kind, speaker, text, saidAt }) => ({ sourceId, kind, speaker, text, saidAt }));
  } catch (error) {
    return [{ sourceId: `${entry.id}:gap`, kind: "echo", speaker: "OMP transcript gap", saidAt: entry.timestamp,
      text: error instanceof Error ? error.message : String(error) }];
  }
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

export function hasReplayContent(entry: Entry): boolean {
  return entry.type === "message" || entry.type === "custom_message"
    || (entry.type === "custom" && entry.customType === ATTEMPT_ENTRY);
}

// The persisted tool call an attempted input duplicates: the assistant message
// precedes its attempt entry, and both sit under the same user page.
function persistedCallLookup(entries: Entry[], byId: EntryIndex): (entry: Entry, toolCallId: string) => PersistedCall | null {
  return (attempt, toolCallId) => {
    const start = byId.get(attempt.id)!.index;
    for (let index = start - 1; index >= 0; index--) {
      const entry = entries[index];
      if (isPage(entry)) return null;
      if (entry.type !== "message" || entry.message?.role !== "assistant") continue;
      const content = Array.isArray(entry.message.content) ? entry.message.content : [];
      const position = content.findIndex((block: any) => block?.type === "toolCall" && block.id === toolCallId);
      if (position >= 0) return { sourceId: `${entry.id}:tool:${position}`, text: persistedToolText(content[position]) };
    }
    return null;
  };
}

// The user page an entry hangs under, or null for the page-less prelude of a
// session that a door opened. The generation cannot adopt an older page merely
// because its reply arrived later.
function pageOf(entry: Entry, index: number, byId: EntryIndex, baselineIndex: number):
  { entry: Entry; index: number } | null | "before-generation" {
  const page = isPage(entry) ? { entry, index } : parentPage(entry, byId);
  if (page && page.index <= baselineIndex) return "before-generation";
  if (page && page.index > index) throw new Error(`OMP user page follows its child ${entry.id}`);
  return page;
}

function firstPageIndex(entries: Entry[], baselineIndex: number): number {
  for (let index = baselineIndex + 1; index < entries.length; index++) {
    if (isPage(entries[index])) return index;
  }
  return entries.length;
}

export function* replayRecords(entries: Entry[], session: string, baselineId: string | null, cursor: Cursor | null,
  options: ReplayOptions = {}): Generator<PendingRecord> {
  const byId: EntryIndex = new Map(entries.map((entry, index) => [entry.id, { entry, index }]));
  const { baselineIndex, checkpointIndex } = replayPosition(byId, baselineId, cursor);
  const stopIndex = options.stopBeforeEntryId === undefined ? entries.length : byId.get(options.stopBeforeEntryId)?.index;
  if (stopIndex === undefined) throw new Error(`Orihon turn boundary ${options.stopBeforeEntryId} is not an OMP entry`);
  const persisted = persistedCallLookup(entries, byId);
  const preludeEnd = firstPageIndex(entries, baselineIndex);
  // A page's record id is `<entry>:user`, or the first part when the page text split.
  const pageIds = new Map<string, string>();
  const pageId = (page: Entry): string => {
    let id = pageIds.get(page.id);
    if (id === undefined) {
      id = entryRecords(page, session, () => null).find((record) => record.kind === "user")?.sourceId ?? `${page.id}:user`;
      pageIds.set(page.id, id);
    }
    return id;
  };
  for (let index = checkpointIndex; index < stopIndex; index++) {
    const entry = entries[index];
    if (!hasReplayContent(entry)) continue;
    const page = pageOf(entry, index, byId, baselineIndex);
    if (page === "before-generation") continue;
    const records = entryRecords(entry, session, (toolCallId) => persisted(entry, toolCallId), options.resolveChild);
    const offset = recordOffset(entry, records, cursor);
    if (!records.length) continue;
    // Only the prelude before the generation's first user page may hang without a page.
    if (!page && index > preludeEnd) throw new Error(`OMP entry ${entry.id} has no source user page`);
    for (let recordIndex = offset; recordIndex < records.length; recordIndex++) {
      const record = records[recordIndex];
      record.parentUserId = record.kind === "user" || !page ? null : pageId(page.entry);
      if (record.kind !== "echo" && Buffer.byteLength(record.text, "utf8") > MAX_TEXT_BYTES) {
        throw new Error(`OMP source ${record.sourceId} exceeds the Orihon non-echo limit after splitting`);
      }
      yield { record, cursor: { entryId: entry.id, nextRecord: recordIndex + 1 } };
    }
  }
}

// The last captured record of this generation before an entry (null: before
// the end of the session): the frontier a fresh turn's view ends at. Null when
// nothing of the generation precedes it.
export function lastSourceIdBefore(entries: Entry[], session: string, baselineId: string | null, entryId: string | null,
  resolveChild?: ReplayOptions["resolveChild"]): string | null {
  const byId: EntryIndex = new Map(entries.map((entry, index) => [entry.id, { entry, index }]));
  const { baselineIndex } = replayPosition(byId, baselineId, null);
  const stop = entryId === null ? entries.length : byId.get(entryId)?.index;
  if (stop === undefined) throw new Error(`Orihon turn boundary ${entryId} is not an OMP entry`);
  const persisted = persistedCallLookup(entries, byId);
  for (let index = stop - 1; index > baselineIndex; index--) {
    const entry = entries[index];
    if (!hasReplayContent(entry) || pageOf(entry, index, byId, baselineIndex) === "before-generation") continue;
    const records = entryRecords(entry, session, (toolCallId) => persisted(entry, toolCallId), resolveChild);
    if (records.length) return records[records.length - 1].sourceId;
  }
  return null;
}
