import { roomState, askHost, noteHostFailure, onHostRecovered } from "./health.js";

let round = { status: "idle", messages: [], drafts: [], reason: null };
let optimistic = [];
let retry = null;
let sending = false;
let refusal = null;
let poll = null;
let querying = null;
let panel = null;
let requestRender = () => {};

export function initChat(deps) {
  requestRender = deps.requestRender;
  // A recovered Host is asked for the ring again; nothing pending is re-sent.
  onHostRecovered(() => { if (panel) void querySnapshot(); });
}

export function isLiveChat(item) {
  return item.kind === "direct" && item.id === roomState().room;
}

export function chatState() {
  return { ...round, sending, refusal, unanswered: hasUnanswered() };
}

// The ring's lines, then the Host's open drafts as spirit lines still
// forming, then this surface's own unconfirmed says.
export function chatMessages() {
  const drafts = (round.drafts ?? []).map(draft => ({ ...draft, author: "spirit", draft: true }));
  return [...round.messages, ...drafts, ...optimistic].map(message => ({
    ...message,
    role: message.author,
    author: message.authorName,
    glyph: message.authorName.slice(0, 1),
    time: message.at,
    steps: Array.isArray(message.steps) ? message.steps : [],
    thinking: message.thinking ?? [],
    outcome: message.outcome ?? "complete",
    draft: message.draft === true,
    pending: message.pending === true,
    undelivered: message.undelivered === true
  }));
}

export function chatBlockReason(item) {
  const room = roomState();
  if (!room.room) {
    if (room.status !== "failed") return "Host room not queried";
    return room.reached ? `Room state not served · ${room.reason}` : `Host unreachable · ${room.reason}`;
  }
  if (!isLiveChat(item)) return "Not this Host's room";
  if (round.reason) return `${round.transport ? "Host unreachable" : "Chat refused"} · ${round.reason}`;
  if (round.status === "idle") return "Chat not queried";
  if (round.status === "pending" && round.messages.length === 0) return "Querying Host chat";
  if (sending) return "Sending to Host…";
  return null;
}

export function syncChatPanel(item, view) {
  const next = isLiveChat(item) && view === "live" ? item.id : null;
  if (next === panel) return;
  panel = next;
  if (next) void querySnapshot();
}

// An undelivered line never reached the Host, so nobody is answering it; only
// lines the Host holds (or is still confirming) count as open.
function hasUnanswered() {
  const answered = new Set(round.messages.filter(message => message.author === "spirit").map(message => message.turnId));
  return optimistic.some(message => !message.undelivered) || round.messages.some(message => message.author === "operator" && !answered.has(message.turnId));
}

// A dead Host is not polled; the health loop owns that clock and recovery
// asks for the ring again. An answer forming is read at reading pace.
function schedulePoll() {
  clearTimeout(poll);
  poll = null;
  if (round.reason || !hasUnanswered()) return;
  poll = setTimeout(() => { void querySnapshot(); }, round.drafts?.length ? 400 : 2000);
}

function failRound(error) {
  round = { ...round, status: "failed", reason: error.message, transport: error.transport === true, drafts: [] };
  if (error.transport) noteHostFailure(error.message);
}

export function querySnapshot() {
  if (querying) return querying;
  querying = readSnapshot();
  return querying;
}

async function readSnapshot() {
  round = { ...round, status: "pending" };
  queueMicrotask(requestRender);
  try {
    const result = await askHost("/live/chat/snapshot");
    const drafts = result.drafts ?? [];
    if (result.room !== roomState().room || !Array.isArray(result.messages) || result.messages.some(message =>
      !Number.isFinite(message.sequence) || !["operator", "spirit"].includes(message.author) ||
      typeof message.authorName !== "string" || typeof message.text !== "string" ||
      typeof message.turnId !== "string" || !Number.isFinite(Date.parse(message.at))) ||
      !Array.isArray(drafts) || drafts.some(draft =>
      typeof draft.turnId !== "string" || typeof draft.authorName !== "string" || typeof draft.text !== "string")) {
      throw new Error("Host answered without a valid room chat snapshot");
    }
    for (const message of [...result.messages, ...drafts]) {
      if (message.thinking !== undefined && (!Array.isArray(message.thinking) ||
        message.thinking.some(block => typeof block !== "string"))) {
        throw new Error("Invalid chat thinking: expected an array of displayable text blocks");
      }
    }
    if (result.messages.some(message => message.outcome !== undefined &&
      !["complete", "error", "aborted"].includes(message.outcome))) {
      throw new Error("Invalid chat outcome: expected complete, error, or aborted");
    }
    round = { status: "live", room: result.room, reason: null, messages: result.messages.sort((a, b) => a.sequence - b.sequence), drafts };
    optimistic = optimistic.filter(message => !round.messages.some(row => row.author === "operator" && row.turnId === message.turnId));
  } catch (error) {
    failRound(error);
  } finally {
    querying = null;
    schedulePoll();
    requestRender();
  }
}

export async function say(text) {
  if (sending) return false;
  const attempt = retry?.text === text ? retry : { text, sayId: crypto.randomUUID() };
  retry = attempt;
  refusal = null;
  sending = true;
  // An undelivered line for other text is abandoned; only this text retries.
  optimistic = optimistic.filter(message => !message.undelivered || message.turnId === attempt.sayId);
  const line = optimistic.find(message => message.turnId === attempt.sayId);
  if (line) {
    line.undelivered = false;
  } else {
    optimistic.push({ author: "operator", authorName: roomState().operator ?? "Operator", text,
      at: new Date().toISOString(), turnId: attempt.sayId, pending: true });
  }
  requestRender();
  try {
    await askHost("/live/chat/say", attempt);
    retry = null;
    await querySnapshot();
    return true;
  } catch (error) {
    if (error.status === 400) {
      refusal = error.message;
      retry = null;
      optimistic = optimistic.filter(message => message.turnId !== attempt.sayId);
    } else {
      const undelivered = optimistic.find(message => message.turnId === attempt.sayId);
      if (undelivered) undelivered.undelivered = true;
      failRound(error);
    }
    return false;
  } finally {
    sending = false;
    schedulePoll();
    requestRender();
  }
}
