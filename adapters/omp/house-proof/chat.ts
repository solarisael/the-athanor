// Chat doorman: the room's ear for the chat projection.
//
// The Host owns the conversation ring. This doorman polls the snapshot,
// injects each unanswered operator say as a real turn, and reports the
// settled turn's response back as the spirit line. One say at a time, in
// ring order; a say already answered by a spirit line with the same turn id
// never injects again, so restarts re-answer nothing.

import { hostCommand, sendHostCommand, HostUnavailable, type HostBinding } from "./host.ts";
import { topLevelSession } from "./top-level-session-fence.ts";
import { conversationText } from "./text.ts";
import { generatedTurnKey } from "./turn-origin.ts";

const CHAT_PROJECTION_ID = "chat";
const CHAT_SUBSCRIBE = "athanor.chat.subscribe";
const CHAT_TURN = "athanor.chat.turn";
const SNAPSHOT = new Set(["athanor.chat.snapshot"]);
const ACCEPTED = new Set(["athanor.chat.command_accepted"]);
const CHAT_POLL_MS = 2_000;
const CHAT_WARNING_COOLDOWN_MS = 60_000;

type ChatLine = {
  sequence: number;
  author: string;
  authorName: string;
  text: string;
  turnId: string;
};

type ChatDoormanState = {
  pi: any;
  ctx: any;
  binding: HostBinding;
  timer: unknown;
  pendingSayId: string | null;
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
    customType: "athanor-chat-say",
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

function parseLines(response: Record<string, any>): ChatLine[] {
  const messages = Array.isArray(response?.messages) ? response.messages : [];
  return messages
    .filter((message: any) => message && typeof message === "object")
    .map((message: any) => ({
      sequence: Number(message.sequence ?? 0),
      author: String(message.author ?? ""),
      authorName: String(message.authorName ?? ""),
      text: String(message.text ?? ""),
      turnId: String(message.turnId ?? ""),
    }));
}

async function tickChatDoorman(state: ChatDoormanState): Promise<void> {
  if (state.stopped || state.ticking || state.pendingSayId || state.reporting) return;
  if (topLevelSession(state.binding.room) !== state.binding.session) return;
  if (typeof state.ctx?.isIdle === "function" && !state.ctx.isIdle()) return;
  state.ticking = true;
  try {
    const snapshot = await sendHostCommand(
      hostCommand(state.binding, CHAT_SUBSCRIBE, CHAT_PROJECTION_ID, {}),
      SNAPSHOT,
    );
    // The snapshot request yields: the session may have started another turn
    // or been retired while the Host was answering.
    if (state.stopped || topLevelSession(state.binding.room) !== state.binding.session) return;
    if (typeof state.ctx?.isIdle === "function" && !state.ctx.isIdle()) return;
    const lines = parseLines(snapshot);
    const answered = new Set(
      lines.filter((line) => line.author === "spirit").map((line) => line.turnId),
    );
    const unanswered = lines
      .filter((line) => line.author === "operator" && !answered.has(line.turnId))
      .sort((a, b) => a.sequence - b.sequence);
    const next = unanswered[0];
    if (!next) return;
    state.pendingSayId = next.turnId;
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
    previous.pi = pi;
    return;
  }
  const state: ChatDoormanState = {
    pi,
    ctx,
    binding,
    timer: null,
    pendingSayId: null,
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
  if (!state || !state.pendingSayId || state.reporting) return;
  const sayId = state.pendingSayId;
  const originIndex = messages.findLastIndex((message) =>
    message?.role === "custom"
    && message.customType === "athanor-chat-say"
    && message.details?.sayId === sayId
  );
  if (originIndex < 0) return;
  const afterOrigin = messages.slice(originIndex + 1);
  const nextOrigin = afterOrigin.findIndex((message) =>
    message?.role === "user" || generatedTurnKey(message) !== null
  );
  const ownedMessages = nextOrigin < 0 ? afterOrigin : afterOrigin.slice(0, nextOrigin);
  // A delayed snapshot may include async-result follow-ups after this say's
  // answer. Keep its first settled response, never the latest session reply.
  // OMP re-samples pause_turn stops, so those are progress, not completion.
  const assistant = ownedMessages.find((message) =>
    message?.role === "assistant"
    && message.stopReason
    && message.stopReason !== "toolUse"
    && message.stopDetails?.type !== "pause_turn"
  );
  if (!assistant) return;
  const responseText = conversationText(assistant);
  state.reporting = true;
  try {
    await sendHostCommand(
      hostCommand(
        state.binding,
        CHAT_TURN,
        CHAT_PROJECTION_ID,
        {
          chat_turn: {
            room: state.binding.room,
            turnId: sayId,
            authorName: state.binding.spirit,
            text: responseText,
          },
        },
        `chat-turn:${sayId}`,
      ),
      ACCEPTED,
    );
    state.pendingSayId = null;
  } catch (error) {
    warn(state, error instanceof Error ? error.message : String(error));
  } finally {
    state.reporting = false;
  }
}

export function stopChatDoorman(binding: HostBinding): void {
  const state = chatDoormen.get(doormanKey(binding));
  if (!state) return;
  state.stopped = true;
  if (state.timer !== null && typeof state.ctx?.clearInterval === "function") {
    state.ctx.clearInterval(state.timer);
  }
  chatDoormen.delete(doormanKey(binding));
}
