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

import { existsSync } from "node:fs";
import path from "node:path";

import { discoverRustExecutable } from "../discovery.ts";
import { RustJsonlTransport } from "../rust-transport.ts";
import { hostSessionIdentity } from "./host.ts";
import { roomContext } from "./room.ts";
import { conversationText } from "./text.ts";

const TRIGGER_TIMEOUT_MS = 300;
const REMIND_STASH_LIMIT = 256;
const PROSE_STREAM_SCAN_STEP = 48;

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
  matchStart?: number | null;
  surfaceIndex?: number;
  /** Ledger fires for this lesson in this room, including this one. Optional
   * because a skewed furnace build may omit it; absence just drops the ×N. */
  fires?: number;
};

export type LessonTriggerProseDecision = {
  block: boolean;
  content: string;
  reminder: string;
  details: Record<string, unknown>;
  blockingMatchStarts: Array<number | null>;
};

type ProseStreamState = {
  room: string;
  roomDir: string;
  session: string;
  project: string | null;
  latestText: string;
  checkedText: string;
  forceScan: boolean;
  blocked: boolean;
  running: Promise<void> | null;
  ctx: any;
  pi: any;
};

const proseStreamBySession = new Map<string, ProseStreamState>();
const interruptedProseSessions = new Map<string, { at: number; resumePrefix: string | null }>();

function proseStreamKey(room: string, session: string): string {
  return `${room}\0${session}`;
}

function trimOldest<K, V>(map: Map<K, V>): void {
  if (map.size <= REMIND_STASH_LIMIT) return;
  const oldest = map.keys().next();
  if (!oldest.done) map.delete(oldest.value);
}

export function lessonTriggersDisabled(): boolean {
  return process.env.SOLARISAEL_DISABLE_LESSON_TRIGGERS === "1";
}

const projectSlugByCwd = new Map<string, string | null>();

/** The repo the hands are in, as a lesson project slug: the basename of the
 * nearest ancestor holding `.git`, lowercased with spaces folded to dashes.
 * enough: basename heuristic; an explicit registry is the door when repo
 * names and lesson project slugs diverge. */
function deriveProjectSlug(cwd: unknown): string | null {
  if (typeof cwd !== "string" || !cwd.trim()) return null;
  const start = path.resolve(cwd);
  const cached = projectSlugByCwd.get(start);
  if (cached !== undefined) return cached;

  let slug: string | null = null;
  try {
    let current = start;
    while (true) {
      if (existsSync(path.join(current, ".git"))) {
        slug = path.basename(current).trim().toLowerCase().replace(/\s+/g, "-") || null;
        break;
      }
      const parent = path.dirname(current);
      if (parent === current) break;
      current = parent;
    }
  } catch {
    slug = null;
  }
  projectSlugByCwd.set(start, slug);
  trimOldest(projectSlugByCwd);
  return slug;
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
  proseStreamBySession.clear();
  interruptedProseSessions.clear();
}

async function matchSurfaces(
  roomDir: string,
  room: string,
  session: string,
  surfaces: LessonSurface[],
  project: string | null = null,
): Promise<{ fired: FiredLesson[]; warnings: string[] }> {
  if (!surfaces.length) return { fired: [], warnings: [] };
  const client = triggerTransport(roomDir);
  if (!client) return { fired: [], warnings: [] };
  const response = await client.request(
    "lesson_trigger_match",
    { room, session, surfaces, ...(project ? { project } : {}) },
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

export class LessonTriggerFeedback {
  constructor(private readonly lines: string[]) {}

  render(_width: number): string[] {
    return this.lines;
  }
}

function lessonFeedbackRefs(details: unknown): string[] {
  const source = details && typeof details === "object" && !Array.isArray(details)
    ? details as Record<string, unknown>
    : {};
  const lessons = Array.isArray(source.lessons) ? source.lessons : [];
  const refs = lessons.flatMap((value) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) return [];
    const lesson = value as Record<string, unknown>;
    const family = String(lesson.family ?? "").trim();
    const id = Number(lesson.id);
    if (!family || !Number.isInteger(id) || id <= 0) return [];
    const fires = Number(lesson.fires);
    const suffix = Number.isFinite(fires) && fires >= 2 ? ` ×${fires}` : "";
    return [`${family}#${id}${suffix}`];
  });
  return [...new Set(refs)];
}

export function lessonTriggerMessageRenderer(message: any, options: any, theme: any): LessonTriggerFeedback {
  const details = message?.details && typeof message.details === "object" && !Array.isArray(message.details)
    ? message.details as Record<string, unknown>
    : {};
  const refs = lessonFeedbackRefs(details);
  const interrupted = details.interrupted === true;
  const blocked = details.blocked === true;
  const source = typeof details.source === "string" ? details.source : "";
  let action = "lesson reminder queued";
  if (interrupted) action = "interrupted draft · correction queued";
  else if (blocked) action = `blocked ${String(details.tool ?? "tool")} call`;
  else if (source === "tool_result") action = "lesson reminder delivered";
  const summary = ["Athanor", ...refs, action].join(" · ");
  const color = interrupted || blocked ? "warning" : "accent";
  const lines = [typeof theme?.fg === "function" ? theme.fg(color, summary) : summary];
  const content = typeof message?.content === "string" ? message.content.trim() : "";
  if (options?.expanded && content) {
    lines.push(
      "",
      ...content.split(/\r?\n/).map((line) => typeof theme?.fg === "function" ? theme.fg("muted", line) : line),
    );
  }
  return new LessonTriggerFeedback(lines);
}

export function processLessonsMessageRenderer(message: any, options: any, theme: any): LessonTriggerFeedback {
  const details = message?.details && typeof message.details === "object" && !Array.isArray(message.details)
    ? message.details as Record<string, unknown>
    : {};
  const count = typeof details.lessons === "number"
    ? details.lessons
    : Array.isArray(details.lessons) ? details.lessons.length : 0;
  const trigger = String(details.trigger ?? "").trim();
  const summary = [
    "Athanor",
    `${count || "?"} lesson${count === 1 ? "" : "s"} warm`,
    ...(trigger ? [trigger] : []),
  ].join(" · ");
  const lines = [typeof theme?.fg === "function" ? theme.fg("accent", summary) : summary];
  const content = typeof message?.content === "string" ? message.content.trim() : "";
  if (options?.expanded && content) {
    lines.push(
      "",
      ...content.split(/\r?\n/).map((line) => typeof theme?.fg === "function" ? theme.fg("muted", line) : line),
    );
  }
  return new LessonTriggerFeedback(lines);
}

// --- remind stash (tool_call -> tool_result) ---------------------------------

type StashedReminder = { reminder: string; lessons: FiredLesson[] };

const remindByToolCallId = new Map<string, StashedReminder>();

function stashReminder(toolCallId: unknown, reminder: string, lessons: FiredLesson[]): void {
  const callId = String(toolCallId ?? "").trim();
  if (!callId || !reminder) return;
  remindByToolCallId.set(callId, { reminder, lessons });
  if (remindByToolCallId.size > REMIND_STASH_LIMIT) {
    const oldest = remindByToolCallId.keys().next();
    if (!oldest.done) remindByToolCallId.delete(oldest.value);
  }
}

export function takeLessonReminder(toolCallId: unknown): StashedReminder | null {
  const callId = String(toolCallId ?? "").trim();
  if (!callId) return null;
  const stashed = remindByToolCallId.get(callId);
  if (stashed === undefined) return null;
  remindByToolCallId.delete(callId);
  return stashed.reminder ? stashed : null;
}

export function prependLessonReminder(content: unknown, reminder: string): unknown[] {
  const existing = Array.isArray(content) ? content : content === undefined || content === null ? [] : [content];
  return [{ type: "text", text: reminder }, ...existing];
}

/** Operator-visible card for a tool-lane verdict. The card's content is a
 * one-line receipt, never the whisper: the reminder itself already reaches
 * the model with the tool result, and a block's reason rides the refusal. */
export function toolLessonCard(
  lessons: FiredLesson[],
  toolName: string,
  kind: "remind" | "block",
): { message: Record<string, unknown>; options: Record<string, unknown> } | null {
  if (!lessons.length) return null;
  const refs = lessons.map(lessonRef).join(", ");
  const content = kind === "block"
    ? `Athanor lesson ${refs} blocked the ${toolName} call; the reason was returned with the tool result.`
    : `Athanor lesson reminder ${refs} was delivered with the ${toolName} result.`;
  return {
    message: {
      customType: "solarisael-lesson-trigger",
      content,
      display: true,
      attribution: "agent",
      details: {
        ...lessonDecisionDetails(lessons, []),
        source: kind === "block" ? "tool_call" : "tool_result",
        interrupted: false,
        blocked: kind === "block",
        tool: toolName,
      },
    },
    options: { deliverAs: "nextTurn", triggerTurn: false },
  };
}

// --- taps --------------------------------------------------------------------

export async function lessonTriggerToolCall(
  event: any,
  ctx: any,
): Promise<{ block: true; reason: string; lessons: FiredLesson[] } | undefined> {
  try {
    if (lessonTriggersDisabled()) return undefined;
    const toolName = String(event?.toolName ?? "");
    if (toolName !== "edit" && toolName !== "write") return undefined;
    const surfaces = extractToolSurfaces(toolName, event?.input);
    if (!surfaces.length) return undefined;

    const { room, effectiveRoomDir } = roomContext(ctx?.cwd);
    const session = hostSessionIdentity(ctx, effectiveRoomDir);
    const project = deriveProjectSlug(ctx?.cwd);
    const { fired } = await matchSurfaces(effectiveRoomDir, room, session, surfaces, project);
    if (!fired.length) return undefined;

    const blocking = fired.filter((entry) => entry.urgency === "block");
    if (blocking.length) return { block: true, reason: renderBlockReason(blocking), lessons: blocking };

    stashReminder(event?.toolCallId, renderReminder(fired), fired);
    return undefined;
  } catch {
    // Fail-open: a thrown tool_call handler is a fail-CLOSED block in OMP, so an
    // unreachable furnace, a timeout, or a malformed response must pass through.
    return undefined;
  }
}

function lessonDecisionDetails(fired: FiredLesson[], warnings: string[]): Record<string, unknown> {
  return {
    lessons: fired.map((entry) => ({
      family: entry.family,
      id: entry.id,
      urgency: entry.urgency,
      patternKind: entry.patternKind,
      fires: entry.fires,
    })),
    warnings,
  };
}

export async function lessonTriggerProseDecision(args: {
  room: string;
  roomDir: string;
  session: string;
  text: string;
  project?: string | null;
}): Promise<LessonTriggerProseDecision | null> {
  try {
    if (lessonTriggersDisabled()) return null;
    const text = String(args.text ?? "");
    if (!text.trim()) return null;
    const { fired, warnings } = await matchSurfaces(args.roomDir, args.room, args.session, [
      { kind: "prose", text },
    ], args.project ?? null);
    if (!fired.length) return null;

    const blocking = fired.filter((entry) => entry.urgency === "block");
    const advisory = fired.filter((entry) => entry.urgency !== "block");
    const interruptParts = [
      blocking.length ? renderBlockReason(blocking) : "",
      advisory.length ? renderReminder(advisory) : "",
    ].filter(Boolean);

    return {
      block: blocking.length > 0,
      content: blocking.length ? interruptParts.join("\n\n") : renderReminder(fired),
      reminder: renderReminder(fired),
      details: lessonDecisionDetails(fired, warnings),
      blockingMatchStarts: blocking.map((entry) =>
        typeof entry.matchStart === "number" ? entry.matchStart : null
      ),
    };
  } catch {
    return null;
  }
}

function proseStreamBinding(ctx: any): {
  key: string;
  room: string;
  roomDir: string;
  session: string;
  project: string | null;
} {
  const { room, effectiveRoomDir } = roomContext(ctx?.cwd);
  const session = hostSessionIdentity(ctx, effectiveRoomDir);
  return {
    key: proseStreamKey(room, session),
    room,
    roomDir: effectiveRoomDir,
    session,
    project: deriveProjectSlug(ctx?.cwd),
  };
}

function newProseStreamState(ctx: any, pi: any): { key: string; state: ProseStreamState } {
  const binding = proseStreamBinding(ctx);
  const state: ProseStreamState = {
    room: binding.room,
    roomDir: binding.roomDir,
    session: binding.session,
    project: binding.project,
    latestText: "",
    checkedText: "",
    forceScan: false,
    blocked: false,
    running: null,
    ctx,
    pi,
  };
  proseStreamBySession.set(binding.key, state);
  trimOldest(proseStreamBySession);
  return { key: binding.key, state };
}

export function resetLessonTriggerProseStream(event: any, ctx: any, pi: any): void {
  try {
    if (event?.message?.role !== "assistant") return;
    newProseStreamState(ctx, pi);
  } catch {
    // Fail-open: stream bookkeeping never gets custody of the conversation.
  }
}

function stringIndexAtUtf8ByteOffset(text: string, byteOffset: number): number | null {
  if (!Number.isSafeInteger(byteOffset) || byteOffset < 0) return null;
  if (byteOffset === 0) return 0;

  let bytes = 0;
  for (let index = 0; index < text.length;) {
    const codePoint = text.codePointAt(index);
    if (codePoint === undefined) return null;
    const codeUnits = codePoint > 0xffff ? 2 : 1;
    let codeBytes: number;
    if (codePoint <= 0x7f) codeBytes = 1;
    else if (codePoint <= 0x7ff) codeBytes = 2;
    else if (codePoint <= 0xffff) codeBytes = 3;
    else codeBytes = 4;
    bytes += codeBytes;
    index += codeUnits;
    if (bytes === byteOffset) return index;
    if (bytes > byteOffset) return null;
  }
  return null;
}

function proseResumePrefix(text: string, matchStarts: Array<number | null>): string | null {
  if (!matchStarts.length || matchStarts.some((start) => start === null)) return null;
  const earliestByteOffset = Math.min(...matchStarts as number[]);
  const matchStart = stringIndexAtUtf8ByteOffset(text, earliestByteOffset);
  if (matchStart === null) return null;

  let boundary = -1;
  const sentenceBoundary = /[.!?]["'”’)\]}]*(?=\s)/gu;
  for (const match of text.matchAll(sentenceBoundary)) {
    const candidate = match.index + match[0].length;
    if (candidate >= matchStart) break;
    boundary = candidate;
  }
  const blankLineBoundary = /\r?\n[ \t]*\r?\n/gu;
  for (const match of text.matchAll(blankLineBoundary)) {
    const candidate = match.index + match[0].length;
    if (candidate >= matchStart) break;
    boundary = Math.max(boundary, candidate);
  }
  if (boundary < 0) return null;

  return text.slice(0, boundary).trimEnd() || null;
}

function replaceAssistantText(message: any, prefix: string): any {
  if (typeof message?.content === "string") return { ...message, content: prefix };
  const field = Array.isArray(message?.content) ? "content" : Array.isArray(message?.parts) ? "parts" : null;
  if (field) {
    let remaining = prefix;
    let sawText = false;
    const parts = message[field].map((part: any) => {
      let text: string | null = null;
      if (typeof part === "string") text = part;
      else if (part?.type === "text" && typeof part.text === "string") text = part.text;
      if (text === null) return part;
      if (sawText && remaining.startsWith("\n")) remaining = remaining.slice(1);
      sawText = true;
      const kept = remaining.slice(0, text.length);
      remaining = remaining.slice(kept.length);
      return typeof part === "string" ? kept : { ...part, text: kept };
    });
    return sawText ? { ...message, [field]: parts } : message;
  }
  if (typeof message?.text === "string") return { ...message, text: prefix };
  return message;
}

function queueProseContinuation(
  key: string,
  state: ProseStreamState,
  decision: LessonTriggerProseDecision,
): void {
  if (typeof state.pi?.sendMessage !== "function") return;
  const canInterrupt = decision.block && typeof state.ctx?.abort === "function";
  const resumePrefix = canInterrupt
    ? proseResumePrefix(state.checkedText, decision.blockingMatchStarts)
    : null;
  let content = decision.reminder;
  if (canInterrupt) content = decision.content;
  if (canInterrupt && resumePrefix !== null) {
    content += "\nYour interrupted draft above was trimmed at the violation. Continue exactly from its end; do not restate anything above the cut.";
  }
  // enough: trim keeps the prefix in context; a post-settlement splice of prefix+continuation into one message is renderer polish behind ContextEventResult — door named, not built.
  state.pi.sendMessage(
    {
      customType: "solarisael-lesson-trigger",
      content,
      display: true,
      attribution: "agent",
      details: {
        ...decision.details,
        source: "message_update",
        interrupted: canInterrupt,
      },
    },
    { deliverAs: "nextTurn", triggerTurn: canInterrupt },
  );

  if (!canInterrupt) return;
  state.blocked = true;
  interruptedProseSessions.set(key, { at: Date.now(), resumePrefix });
  trimOldest(interruptedProseSessions);
  try {
    state.ctx.abort();
  } catch {
    state.blocked = false;
    interruptedProseSessions.delete(key);
  }
}

async function runProseStreamPump(key: string, state: ProseStreamState): Promise<void> {
  try {
    while (proseStreamBySession.get(key) === state && !state.blocked) {
      const unchecked = state.latestText.length - state.checkedText.length;
      if (!state.forceScan && unchecked < PROSE_STREAM_SCAN_STEP) return;
      if (!state.latestText.trim() || state.latestText === state.checkedText) {
        state.forceScan = false;
        return;
      }

      const text = state.latestText;
      state.checkedText = text;
      state.forceScan = false;
      const decision = await lessonTriggerProseDecision({
        room: state.room,
        roomDir: state.roomDir,
        session: state.session,
        project: state.project,
        text,
      });
      if (proseStreamBySession.get(key) !== state || state.blocked) return;
      if (decision) {
        queueProseContinuation(key, state, decision);
        return;
      }
    }
  } catch (error) {
    console.warn(`[athanor] Lesson prose stream degraded: ${error instanceof Error ? error.message : String(error)}`);
  } finally {
    if (proseStreamBySession.get(key) !== state) return;
    state.running = null;
    const unchecked = state.latestText.length - state.checkedText.length;
    const hasUncheckedText = state.latestText !== state.checkedText;
    if (!state.blocked && hasUncheckedText && (state.forceScan || unchecked >= PROSE_STREAM_SCAN_STEP)) {
      state.running = runProseStreamPump(key, state);
    }
  }
}

export function lessonTriggerProseStreamUpdate(event: any, ctx: any, pi: any): Promise<void> {
  try {
    if (lessonTriggersDisabled()) return Promise.resolve();
    if (event?.message?.role !== "assistant") return Promise.resolve();
    const eventType = String(event?.assistantMessageEvent?.type ?? "");
    if (eventType !== "text_delta" && eventType !== "text_end") return Promise.resolve();

    const binding = proseStreamBinding(ctx);
    let state = proseStreamBySession.get(binding.key);
    const text = conversationText(event.message);
    if (!state || (state.latestText && !text.startsWith(state.latestText))) {
      state = newProseStreamState(ctx, pi).state;
    }
    if (state.blocked) return state.running ?? Promise.resolve();

    state.ctx = ctx;
    state.pi = pi;
    state.latestText = text;
    if (eventType === "text_end") state.forceScan = true;
    const unchecked = state.latestText.length - state.checkedText.length;
    if (!state.running && (state.forceScan || unchecked >= PROSE_STREAM_SCAN_STEP)) {
      state.running = runProseStreamPump(binding.key, state);
    }
    return state.running ?? Promise.resolve();
  } catch {
    return Promise.resolve();
  }
}

export function filterInterruptedLessonProse(
  messages: any[],
  room: string,
  session: string,
): any[] {
  try {
    const key = proseStreamKey(room, session);
    const interruption = interruptedProseSessions.get(key);
    if (!interruption) return messages;
    let interruptedIndex = -1;
    for (let index = messages.length - 1; index >= 0; index--) {
      const message = messages[index];
      if (message?.role === "assistant" && message?.stopReason === "aborted") {
        interruptedIndex = index;
        break;
      }
    }
    if (interruptedIndex < 0) return messages;
    interruptedProseSessions.delete(key);
    if (interruption.resumePrefix === null) {
      return [
        ...messages.slice(0, interruptedIndex),
        ...messages.slice(interruptedIndex + 1),
      ];
    }
    const interrupted = replaceAssistantText(messages[interruptedIndex], interruption.resumePrefix);
    if (interrupted === messages[interruptedIndex]) return messages;
    return [
      ...messages.slice(0, interruptedIndex),
      interrupted,
      ...messages.slice(interruptedIndex + 1),
    ];
  } catch {
    return messages;
  }
}

export async function lessonTriggerProseAddition(args: {
  room: string;
  roomDir: string;
  session: string;
  text: string;
  timestamp: number;
  cwd?: string;
}): Promise<
  | {
    role: "custom";
    customType: "solarisael-lesson-trigger";
    content: string;
    display: true;
    details: Record<string, unknown>;
    attribution: "agent";
    timestamp: number;
  }
  | null
> {
  const decision = await lessonTriggerProseDecision({ ...args, project: deriveProjectSlug(args.cwd) });
  if (!decision) return null;
  // A completed generation can only advise the next turn. Live blocks use the
  // message_update tap above and never reach this fallback when they interrupt.
  return {
    role: "custom",
    customType: "solarisael-lesson-trigger",
    content: decision.reminder,
    display: true,
    details: decision.details,
    attribution: "agent",
    timestamp: args.timestamp,
  };
}
