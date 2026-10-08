// Chat doorman: the room's ear for the chat projection.
//
// The Host owns the conversation ring. This doorman polls the snapshot,
// injects each unanswered operator say as a real turn, and reports the
// settled turn's response back as the spirit line. One say at a time, in
// ring order; a say already answered by a spirit line with the same turn id
// never injects again, so restarts re-answer nothing.
//
// While a say is being answered the doorman also keeps a draft on the Host:
// the assistant text so far and every tool the turn used, so the surface can
// show the answer forming instead of a silent gap. Text reports are
// throttled; tool steps report at once. The draft is a courtesy, never
// evidence: a lost draft report costs nothing, the settled turn is the truth.
//
// Says reach OMP as custom messages, never as `input`, so the boat door cannot
// see them on its own. The doorman hands it `/handoff` and every say that
// arrives over the boat line, and waits while the door is open.

import {
  AFTER_HANDOFF,
  BOAT_REQUEST,
  boatDoorOpen,
  handoffCommand,
  openSurfaceDoor,
  overBoatLine,
} from "./boat-door.ts";
import { hostCommand, sendHostCommand, HostUnavailable, type HostBinding } from "./host.ts";
import { topLevelSession } from "./top-level-session-fence.ts";
import { conversationText } from "./text.ts";
import { generatedTurnKey } from "./turn-origin.ts";
import { lifecyclePlan } from "./lifecycle.ts";

const CHAT_PROJECTION_ID = "chat";
const CHAT_TURN = "athanor.chat.turn";
const CHAT_DRAFT = "athanor.chat.draft";
const SAY = "athanor-chat-say";
// A say's turn can start from the say itself or from a boat door it opened.
const SAY_ORIGINS = new Set([SAY, BOAT_REQUEST, AFTER_HANDOFF]);
// The turns that can answer a say: the say, or the turn after a `/handoff` say.
const SAY_ANSWERS = new Set([SAY, AFTER_HANDOFF]);
const ACCEPTED = new Set(["athanor.chat.command_accepted"]);
const CHAT_POLL_MS = 2_000;
const CHAT_DRAFT_THROTTLE_MS = 250;
const CHAT_WARNING_COOLDOWN_MS = 60_000;
const STEP_SUMMARY_CHARS = 120;

type ChatStep = {
  toolCallId: string;
  tool: string;
  summary: string;
  status: "running" | "ok" | "error";
  startedAt: string;
  elapsedMs?: number;
};

type ChatDraft = {
  text: string;
  thinking: string[];
  completedThinking: string[];
  owned: boolean;
  steps: ChatStep[];
  reports: number;
  dirty: boolean;
  flushing: boolean;
  timer: ReturnType<typeof setTimeout> | null;
};

type ChatLine = {
  sequence: number;
  author: string;
  authorName: string;
  text: string;
  turnId: string;
};

type SettledChatReport = {
  binding: HostBinding;
  idempotencyKey: string;
  payload: Record<string, unknown>;
};

type ChatDoormanState = {
  pi: any;
  ctx: any;
  binding: HostBinding;
  timer: unknown;
  pendingSayId: string | null;
  draft: ChatDraft | null;
  settledReport: SettledChatReport | null;
  ticking: boolean;
  reporting: boolean;
  stopped: boolean;
  lastWarningAt: number;
};

const chatDoormen = new Map<string, ChatDoormanState>();

function doormanKey(binding: HostBinding): string {
  return `${binding.room}\0${binding.session}`;
}

function warn(state: ChatDoormanState, message: string): void {
  const now = Date.now();
  if (now - state.lastWarningAt < CHAT_WARNING_COOLDOWN_MS) return;
  state.lastWarningAt = now;
  console.warn(`[athanor] Chat doorman degraded: ${message}`);
}

function sayMessage(line: ChatLine): Record<string, unknown> {
  return {
    customType: SAY,
    content: [
      "<athanor-attention>",
      `Chat surface message from ${line.authorName} (say ${line.turnId}).`,
      "Answer as the room's normal turn; the chat surface renders your final response text.",
      "</athanor-attention>",
      line.text,
    ].join("\n"),
    display: true,
    attribution: "agent",
    details: { sayId: line.turnId, sequence: line.sequence },
  };
}


async function tickChatDoorman(state: ChatDoormanState): Promise<void> {
  if (state.stopped || state.ticking || state.reporting) return;
  if (topLevelSession(state.binding.room) !== state.binding.session) return;
  if (state.settledReport) {
    await deliverChatReport(state);
    return;
  }
  if (state.pendingSayId || boatDoorOpen()) return;
  if (typeof state.ctx?.isIdle === "function" && !state.ctx.isIdle()) return;
  state.ticking = true;
  try {
    const { next } = await lifecyclePlan<{ next: ChatLine | null }>(
      state.binding,
      { action: "chatNext" },
    );
    // The snapshot request yields: the session may have started another turn
    // or been retired while the Host was answering.
    if (state.stopped || topLevelSession(state.binding.room) !== state.binding.session) return;
    if (typeof state.ctx?.isIdle === "function" && !state.ctx.isIdle()) return;
    if (boatDoorOpen()) return;
    // The Host selects the next unanswered say from its own ring.
    if (!next) return;
    const command = handoffCommand(next.text);
    const nearLimit = !command && await overBoatLine(state.ctx);
    if (state.stopped || topLevelSession(state.binding.room) !== state.binding.session) return;
    if (typeof state.ctx?.isIdle === "function" && !state.ctx.isIdle()) return;
    if (boatDoorOpen()) return;
    state.pendingSayId = next.turnId;
    state.draft = { text: "", thinking: [], completedThinking: [], owned: false, steps: [], reports: 0, dirty: false, flushing: false, timer: null };

    if (command || nearLimit) {
      const opened = openSurfaceDoor(state.ctx, {
        sayId: next.turnId,
        focus: command?.focus,
        nearLimit,
        done: (handedOff) => surfaceDoorClosed(state, next, nearLimit, handedOff),
      });
      if (opened) return;
      // A `/handoff` the door refuses is not a question for the spirit.
      if (command) {
        await settleSay(state, next.turnId, { text: "", thinking: [], outcome: "aborted" });
        return;
      }
    }
    state.pi.sendMessage(sayMessage(next), { deliverAs: "nextTurn", triggerTurn: true });
  } catch (error) {
    if (!(error instanceof HostUnavailable)) {
      warn(state, error instanceof Error ? error.message : String(error));
    }
  } finally {
    state.ticking = false;
  }
}

export function startChatDoorman(pi: any, ctx: any, binding: HostBinding): void {
  if (typeof ctx?.setInterval !== "function") return;
  const key = doormanKey(binding);
  const previous = chatDoormen.get(key);
  if (previous) {
    previous.ctx = ctx;
    previous.binding = binding;
    previous.pi = pi;
    return;
  }
  const state: ChatDoormanState = {
    pi,
    ctx,
    binding,
    timer: null,
    pendingSayId: null,
    draft: null,
    settledReport: null,
    ticking: false,
    reporting: false,
    stopped: false,
    lastWarningAt: 0,
  };
  state.timer = ctx.setInterval(() => tickChatDoorman(state), CHAT_POLL_MS);
  chatDoormen.set(key, state);
}

/// OMP's extension agent_end carries a conversation snapshot, not `message`.
/// Its notification can lag behind the next run's dispatch. Pending injection
/// alone is not ownership: require the say and a settled assistant after it.
export async function noteChatTurnEnd(
  binding: HostBinding,
  event: { messages?: any[]; willContinue?: boolean },
): Promise<void> {
  const state = chatDoormen.get(doormanKey(binding));
  const messages = Array.isArray(event?.messages) ? event.messages : [];
  // willContinue also names unrelated background jobs that can wake OMP.
  // Waiting for global quiescence deadlocks a chat observer awaiting this reply.
  if (!state || state.stopped || !state.pendingSayId || state.settledReport) return;
  if (topLevelSession(state.binding.room) !== state.binding.session) return;
  const sayId = state.pendingSayId;
  const observations = messages.map((message) => {
    const sayOrigin = message?.role === "custom" && SAY_ANSWERS.has(message.customType);
    const origin = sayOrigin || message?.role === "user" || generatedTurnKey(message) !== null;
    return {
      kind: origin ? "origin" : message?.role === "assistant" ? "assistant" : "other",
      sayId: sayOrigin ? message.details?.sayId ?? null : null,
      text: message?.role === "assistant" ? conversationText(message) : "",
      thinking: displayableThinking(message),
      settled: isSettledAssistant(message),
      stopReason: typeof message?.stopReason === "string" ? message.stopReason : null,
    };
  });
  const { answer } = await lifecyclePlan<{
    answer: { text: string; thinking: string[]; outcome: "complete" | "error" | "aborted" } | null;
  }>(binding, { action: "chatOutcome", sayId, messages: observations });
  if (state.stopped || state.pendingSayId !== sayId || state.settledReport) return;
  if (topLevelSession(state.binding.room) !== state.binding.session) return;
  if (answer) await settleSay(state, sayId, answer);
}

async function settleSay(
  state: ChatDoormanState,
  sayId: string,
  answer: { text: string; thinking: string[]; outcome: "complete" | "error" | "aborted" },
): Promise<void> {
  // Capture once. Later lifecycle snapshots cannot rewrite a reply awaiting
  // acknowledgment, and draft events cannot extend its finished tool history.
  state.settledReport = {
    binding: { ...state.binding },
    idempotencyKey: `chat-turn:${sayId}`,
    payload: {
      chat_turn: {
        room: state.binding.room,
        turnId: sayId,
        authorName: state.binding.spirit,
        ...answer,
        steps: state.draft?.steps.map((step) => ({ ...step })) ?? [],
      },
    },
  };
  clearDraftTimer(state);
  await deliverChatReport(state);
}

// A near-limit say waited for the handoff and now becomes its own turn, with the
// boat's steps still in its draft. A `/handoff` that did not hand off ends here;
// one that did is answered by the turn after the handoff.
function surfaceDoorClosed(state: ChatDoormanState, line: ChatLine, nearLimit: boolean, handedOff: boolean): void {
  if (state.stopped || state.pendingSayId !== line.turnId) return;
  if (nearLimit) {
    state.pi.sendMessage(sayMessage(line), { deliverAs: "nextTurn", triggerTurn: true });
    return;
  }
  if (!handedOff) void settleSay(state, line.turnId, { text: "", thinking: [], outcome: "aborted" });
}

async function deliverChatReport(state: ChatDoormanState): Promise<void> {
  const report = state.settledReport;
  if (!report || state.stopped || state.reporting) return;
  if (topLevelSession(state.binding.room) !== state.binding.session) return;

  state.reporting = true;
  try {
    // Refresh transport expiry, not the captured payload or idempotency identity.
    await sendHostCommand(
      hostCommand(report.binding, CHAT_TURN, CHAT_PROJECTION_ID, report.payload, report.idempotencyKey),
      ACCEPTED,
    );
    state.pendingSayId = null;
    state.draft = null;
    state.settledReport = null;
  } catch (error) {
    warn(state, error instanceof Error ? error.message : String(error));
  } finally {
    state.reporting = false;
  }
}

// The doorman answering right now. One OMP process carries one top-level
// session, so the say being answered names the draft without reading the room
// from disk on every token.
function draftingDoorman(): ChatDoormanState | null {
  for (const state of chatDoormen.values()) {
    if (state.pendingSayId && state.draft?.owned && !state.settledReport && !state.stopped) return state;
  }
  return null;
}

function clearDraftTimer(state: ChatDoormanState): void {
  if (state.draft?.timer) clearTimeout(state.draft.timer);
  if (state.draft) state.draft.timer = null;
}

function scheduleDraftFlush(state: ChatDoormanState, now: boolean): void {
  const draft = state.draft;
  if (!draft || state.settledReport || state.stopped) return;
  draft.dirty = true;
  if (draft.timer) return;
  draft.timer = setTimeout(() => {
    draft.timer = null;
    void flushDraft(state);
  }, now ? 0 : CHAT_DRAFT_THROTTLE_MS);
}

async function flushDraft(state: ChatDoormanState): Promise<void> {
  const draft = state.draft;
  const sayId = state.pendingSayId;
  if (!draft || !sayId || draft.flushing || state.settledReport || state.stopped) return;
  draft.flushing = true;
  draft.dirty = false;
  draft.reports += 1;
  try {
    await sendHostCommand(
      hostCommand(
        state.binding,
        CHAT_DRAFT,
        CHAT_PROJECTION_ID,
        {
          chat_draft: {
            room: state.binding.room,
            turnId: sayId,
            authorName: state.binding.spirit,
            text: draft.text,
            steps: draft.steps,
            thinking: [...draft.completedThinking, ...draft.thinking],
          },
        },
        `chat-draft:${sayId}:${draft.reports}`,
      ),
      ACCEPTED,
    );
  } catch (error) {
    if (!(error instanceof HostUnavailable)) {
      warn(state, error instanceof Error ? error.message : String(error));
    }
  } finally {
    draft.flushing = false;
    // Text that arrived while this report was in flight goes on the next one.
    if (draft.dirty && state.draft === draft) scheduleDraftFlush(state, false);
  }
}

// This matches OMP's visible thinking blocks, not signatures or redacted data.
function displayableThinking(message: any): string[] {
  if (message?.role !== "assistant" || !Array.isArray(message.content)) return [];
  return message.content.flatMap((part: any) =>
    part?.type === "thinking" && typeof part.thinking === "string" && part.thinking.length
      ? [part.thinking] : []);
}

// A new assistant keeps earlier thinking and tool history, but replaces its text.
export function noteChatMessageStart(message: any): void {
  for (const state of chatDoormen.values()) {
    const draft = state.draft;
    if (!draft || state.stopped || state.settledReport) continue;
    const sayOrigin = message?.role === "custom" && SAY_ORIGINS.has(message.customType);
    if (message?.role === "user" || generatedTurnKey(message) !== null || sayOrigin) {
      draft.owned = sayOrigin && message.details?.sayId === state.pendingSayId;
      continue;
    }
    if (!draft.owned || message?.role !== "assistant") continue;
    draft.completedThinking.push(...draft.thinking);
    draft.thinking = [];
    draft.text = "";
    noteChatMessageUpdate(message);
  }
}

export function noteChatMessageUpdate(message: { role?: string }): void {
  const state = draftingDoorman();
  if (!state?.draft || message?.role !== "assistant") return;
  const text = conversationText(message);
  const thinking = displayableThinking(message);
  const draft = state.draft;
  if (text === draft.text && thinking.length === draft.thinking.length
    && thinking.every((block, index) => block === draft.thinking[index])) return;
  draft.text = text;
  draft.thinking = thinking;
  scheduleDraftFlush(state, false);
}

function isSettledAssistant(message: any): boolean {
  return message?.role === "assistant"
    && Boolean(message.stopReason)
    && message.stopReason !== "toolUse"
    && message.stopDetails?.type !== "pause_turn";
}

export function noteChatMessageEnd(message: any): void {
  const state = draftingDoorman();
  if (!state?.draft || message?.role !== "assistant") return;
  noteChatMessageUpdate(message);
  // agent_end may arrive after an unrelated async-result response has started.
  if (isSettledAssistant(message)) state.draft.owned = false;
}

function stepSummary(intent: unknown, args: unknown): string {
  const stated = typeof intent === "string" ? intent.trim() : "";
  if (stated) return stated.slice(0, STEP_SUMMARY_CHARS);
  if (args && typeof args === "object") {
    for (const value of Object.values(args as Record<string, unknown>)) {
      if (typeof value === "string" && value.trim()) return value.trim().slice(0, STEP_SUMMARY_CHARS);
    }
  }
  return "";
}

export function noteChatToolStart(event: { toolCallId?: unknown; toolName?: unknown; intent?: unknown; args?: unknown }): void {
  const state = draftingDoorman();
  const toolCallId = String(event?.toolCallId ?? "").trim();
  if (!state?.draft || !toolCallId) return;
  if (state.draft.steps.some((step) => step.toolCallId === toolCallId)) return;
  state.draft.steps.push({
    toolCallId,
    tool: String(event?.toolName ?? "tool"),
    summary: stepSummary(event?.intent, event?.args),
    status: "running",
    startedAt: new Date().toISOString(),
  });
  scheduleDraftFlush(state, true);
}

export function noteChatToolEnd(event: { toolCallId?: unknown; isError?: unknown }): void {
  const state = draftingDoorman();
  const toolCallId = String(event?.toolCallId ?? "").trim();
  const step = state?.draft?.steps.find((entry) => entry.toolCallId === toolCallId);
  if (!state || !step) return;
  step.status = event?.isError ? "error" : "ok";
  step.elapsedMs = Math.max(0, Date.now() - Date.parse(step.startedAt));
  scheduleDraftFlush(state, true);
}

export function stopChatDoorman(binding: HostBinding): void {
  const state = chatDoormen.get(doormanKey(binding));
  if (!state) return;
  state.stopped = true;
  clearDraftTimer(state);
  if (state.timer !== null && typeof state.ctx?.clearInterval === "function") {
    state.ctx.clearInterval(state.timer);
  }
  chatDoormen.delete(doormanKey(binding));
}
