// Lesson trigger taps — the thin skin over `lesson_trigger_match`.
//
// Everything ugly lives here in one named place: payload extraction from mutate
// tools, the substrate request, verdict rendering, and the per-toolCallId remind
// stash. index.ts only wires hooks. No matching judgment is written in
// TypeScript: eligibility, regex/AST matching, repeat policy, and urgency are
// the furnace's (house-core) decisions; this module carries payloads there and
// renders whatever comes back.
//
// Fail-open is structural, not aspirational: OMP fails a tool call CLOSED when a
// tool_call handler throws, so every exported entry point wraps its whole body
// and answers undefined/null on any error, timeout, or malformed response.

import { discoverRustExecutable } from "../discovery.ts";
import { RustJsonlTransport } from "../rust-transport.ts";
import { hostSessionIdentity } from "./host.ts";
import { roomContext } from "./room.ts";

const TRIGGER_TIMEOUT_MS = 300;
const REMIND_STASH_LIMIT = 256;

export type LessonSurface = {
  kind: "tool" | "prose";
  tool?: string;
  path?: string;
  text: string;
};

type FiredLesson = {
  family: string;
  id: number;
  title: string;
  lesson: string;
  proofPattern: string | null;
  urgency: "block" | "remind";
  surface: "tool" | "prose";
  path: string | null;
  patternKind: "regex" | "ast";
  pattern: string;
};

export function lessonTriggersDisabled(): boolean {
  return process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS === "1";
}

const transports = new Map<string, RustJsonlTransport>();

function triggerTransport(roomDir: string): RustJsonlTransport | null {
  const executable = discoverRustExecutable();
  if (!executable) return null;
  const key = `${executable}\0${roomDir}`;
  let current = transports.get(key);
  if (current && !current.usable) {
    transports.delete(key);
    void current.close().catch(() => {});
    current = undefined;
  }
  if (!current) {
    current = new RustJsonlTransport({ executable, cwd: roomDir });
    transports.set(key, current);
  }
  return current;
}

export function closeLessonTriggerTransports(): void {
  for (const transport of transports.values()) void transport.close().catch(() => {});
  transports.clear();
}

async function matchSurfaces(
  roomDir: string,
  room: string,
  session: string,
  surfaces: LessonSurface[],
): Promise<{ fired: FiredLesson[]; warnings: string[] }> {
  if (!surfaces.length) return { fired: [], warnings: [] };
  const client = triggerTransport(roomDir);
  if (!client) return { fired: [], warnings: [] };
  const response = await client.request(
    "lesson_trigger_match",
    { room, session, surfaces },
    { timeoutMs: TRIGGER_TIMEOUT_MS },
  ) as Record<string, unknown>;
  if (!response || response.ok !== true) return { fired: [], warnings: [] };
  const fired = Array.isArray(response.fired) ? response.fired as FiredLesson[] : [];
  // Wire rule: warnings are always serialized, but a skewed substrate build is
  // still read defensively — a missing array never crashes a healthy response.
  const warnings = Array.isArray(response.warnings) ? response.warnings.map(String) : [];
  return { fired: fired.filter((entry) => entry && typeof entry === "object"), warnings };
}

// --- payload extraction ------------------------------------------------------
// enough: edit+write; ast_edit and bash text later.

const EDIT_SECTION_HEADER = /^\[([^\]\n]+)#[0-9A-Fa-f]{4}\]$/;

function editSurfaces(patch: string): LessonSurface[] {
  // The edit tool's whole input is one hashline payload: `[PATH#TAG]` sections
  // whose `+TEXT` rows are the final content. Per-section paths are what let the
  // furnace infer a language, so sections are carried separately.
  const surfaces: LessonSurface[] = [];
  let currentPath: string | null = null;
  let rows: string[] = [];
  const flush = () => {
    if (currentPath && rows.length) {
      surfaces.push({ kind: "tool", tool: "edit", path: currentPath, text: rows.join("\n") });
    }
    rows = [];
  };
  for (const line of patch.split(/\r?\n/)) {
    const header = EDIT_SECTION_HEADER.exec(line.trim());
    if (header) {
      flush();
      currentPath = header[1];
      continue;
    }
    if (line.startsWith("+")) rows.push(line.slice(1));
  }
  flush();
  return surfaces;
}

/** Internal-URI writes carry organ arguments (memory bodies, hallway posts,
 * lesson bodies), not code. Scanning them fires triggers on prose that merely
 * MENTIONS a pattern — proven live by ledger row 6 (#334 blocking a memory
 * write whose body named the pattern it documents). */
const INTERNAL_URI = /^[a-z][a-z0-9+.-]*:\/\//i;

function internalUriPath(filePath: string): boolean {
  return INTERNAL_URI.test(filePath) && !/^[A-Za-z]:[\\/]/.test(filePath);
}

export function extractToolSurfaces(toolName: string, input: unknown): LessonSurface[] {
  const args = (input && typeof input === "object" ? input : {}) as Record<string, unknown>;
  if (toolName === "write") {
    const text = typeof args.content === "string" ? args.content : "";
    if (!text.trim()) return [];
    const filePath = String(args.path ?? "").trim();
    if (internalUriPath(filePath)) return [];
    return [{ kind: "tool", tool: "write", ...(filePath ? { path: filePath } : {}), text }];
  }
  if (toolName === "edit") {
    const patch = typeof args.input === "string" ? args.input : "";
    if (!patch.trim()) return [];
    return editSurfaces(patch);
  }
  return [];
}

// --- verdict rendering -------------------------------------------------------

function lessonBody(entry: FiredLesson): string {
  const lines = [String(entry.title ?? "").trim(), String(entry.lesson ?? "").trim()];
  const proof = entry.proofPattern ? String(entry.proofPattern).trim() : "";
  if (proof) lines.push(`Proof pattern: ${proof}`);
  return lines.filter(Boolean).join("\n");
}

function lessonRef(entry: FiredLesson): string {
  return `${String(entry.family ?? "lesson")}#${Number(entry.id ?? 0)}`;
}

export function renderBlockReason(fired: FiredLesson[]): string {
  return fired
    .map((entry) => [
      `<system-interrupt reason="lesson_violation" lesson="${lessonRef(entry)}">`,
      lessonBody(entry),
      "</system-interrupt>",
    ].join("\n"))
    .join("\n\n");
}

export function renderReminder(fired: FiredLesson[]): string {
  return fired
    .map((entry) => [
      `<system-reminder reason="lesson" lesson="${lessonRef(entry)}">`,
      lessonBody(entry),
      "</system-reminder>",
    ].join("\n"))
    .join("\n\n");
}

// --- remind stash (tool_call -> tool_result) ---------------------------------

const remindByToolCallId = new Map<string, string>();

function stashReminder(toolCallId: unknown, reminder: string): void {
  const callId = String(toolCallId ?? "").trim();
  if (!callId || !reminder) return;
  remindByToolCallId.set(callId, reminder);
  if (remindByToolCallId.size > REMIND_STASH_LIMIT) {
    const oldest = remindByToolCallId.keys().next();
    if (!oldest.done) remindByToolCallId.delete(oldest.value);
  }
}

export function takeLessonReminder(toolCallId: unknown): string | null {
  const callId = String(toolCallId ?? "").trim();
  if (!callId) return null;
  const reminder = remindByToolCallId.get(callId);
  if (reminder === undefined) return null;
  remindByToolCallId.delete(callId);
  return reminder || null;
}

export function prependLessonReminder(content: unknown, reminder: string): unknown[] {
  const existing = Array.isArray(content) ? content : content === undefined || content === null ? [] : [content];
  return [{ type: "text", text: reminder }, ...existing];
}

// --- taps --------------------------------------------------------------------

export async function lessonTriggerToolCall(
  event: any,
  ctx: any,
): Promise<{ block: true; reason: string } | undefined> {
  try {
    if (lessonTriggersDisabled()) return undefined;
    const toolName = String(event?.toolName ?? "");
    if (toolName !== "edit" && toolName !== "write") return undefined;
    const surfaces = extractToolSurfaces(toolName, event?.input);
    if (!surfaces.length) return undefined;

    const { room, effectiveRoomDir } = roomContext(ctx?.cwd);
    const session = hostSessionIdentity(ctx, effectiveRoomDir);
    const { fired } = await matchSurfaces(effectiveRoomDir, room, session, surfaces);
    if (!fired.length) return undefined;

    const blocking = fired.filter((entry) => entry.urgency === "block");
    if (blocking.length) return { block: true, reason: renderBlockReason(blocking) };

    stashReminder(event?.toolCallId, renderReminder(fired));
    return undefined;
  } catch {
    // Fail-open: a thrown tool_call handler is a fail-CLOSED block in OMP, so an
    // unreachable furnace, a timeout, or a malformed response must pass through.
    return undefined;
  }
}

export async function lessonTriggerProseAddition(args: {
  room: string;
  roomDir: string;
  session: string;
  text: string;
  timestamp: number;
}): Promise<
  | {
    role: "custom";
    customType: "solarisael-lesson-trigger";
    content: string;
    display: false;
    details: Record<string, unknown>;
    attribution: "agent";
    timestamp: number;
  }
  | null
> {
  try {
    if (lessonTriggersDisabled()) return null;
    const text = String(args.text ?? "");
    if (!text.trim()) return null;
    const { fired, warnings } = await matchSurfaces(args.roomDir, args.room, args.session, [
      { kind: "prose", text },
    ]);
    if (!fired.length) return null;
    // Prose fires never block: the generation already happened. They read as
    // reminders on the next turn, anchored at the current turn only.
    return {
      role: "custom",
      customType: "solarisael-lesson-trigger",
      content: renderReminder(fired),
      display: false,
      details: {
        lessons: fired.map((entry) => ({
          family: entry.family,
          id: entry.id,
          urgency: entry.urgency,
          patternKind: entry.patternKind,
        })),
        warnings,
      },
      attribution: "agent",
      timestamp: args.timestamp,
    };
  } catch {
    // Advisory only. A degraded furnace never perturbs context injection.
    return null;
  }
}
