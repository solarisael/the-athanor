import { roomState, askHost, noteHostFailure, onHostRecovered } from "./health.js";

let round = { status: "idle", messages: [], drafts: [], reason: null };
let optimistic = [];
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
  return { ...round, sending, refusal: refusal?.message ?? null, unanswered: hasUnanswered() };
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
    unconfirmed: message.unconfirmed === true
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

function hasUnanswered() {
  const answered = new Set(round.messages.filter(message => message.author === "spirit").map(message => message.turnId));
  return optimistic.some(message => !message.unconfirmed) || round.messages.some(message => message.author === "operator" && !answered.has(message.turnId));
}

// A dead Host is not polled; the health loop owns that clock and recovery
// asks for the ring again. An answer forming is read at reading pace.
function schedulePoll() {
  clearTimeout(poll);
  poll = null;
  if (round.reason || (!hasUnanswered() && optimistic.length === 0)) return;
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
    optimistic = optimistic.filter(message => {
      const confirmed = round.messages.some(row => row.author === "operator" &&
        row.turnId === message.turnId && row.text === message.text);

      if (confirmed) completeSend(message);

      return !confirmed;
    });
  } catch (error) {
    failRound(error);
  } finally {
    querying = null;
    schedulePoll();
    requestRender();
  }
}

function completeSend(message) {
  if (message.accepted) return;

  message.accepted = true;
  message.pending = false;
  message.unconfirmed = false;
  if (refusal?.turnId === message.turnId) refusal = null;
  message.onAccepted();
}

export async function say(text, onAccepted) {
  if (sending) return false;

  const room = roomState().room;
  let message = optimistic.find(row => !row.accepted && row.room === room && row.text === text);

  if (!message) {
    message = {
      room, author: "operator", authorName: roomState().operator ?? "Operator", text,
      at: new Date().toISOString(), turnId: crypto.randomUUID(), pending: true
    };
    optimistic.push(message);
  }

  const retryingUnconfirmed = message.unconfirmed === true;

  message.onAccepted = onAccepted;
  message.unconfirmed = false;
  refusal = null;
  sending = true;
  requestRender();

  try {
    const result = await askHost("/live/chat/say", { text, sayId: message.turnId });
    const accepted = result?.message;

    if (result?.room !== room || result.accepted !== true ||
      (result.repeated !== true && result.repeated !== false) ||
      (result.repeated ? accepted !== null :
        accepted?.author !== "operator" || accepted.turnId !== message.turnId || accepted.text !== text)) {
      throw new Error("Host answered without matching chat acceptance");
    }

    completeSend(message);
    await querySnapshot();
    return true;
  } catch (error) {
    if (message.accepted) return true;

    if (!retryingUnconfirmed && error.status >= 400 && error.status < 500) {
      refusal = { turnId: message.turnId, message: error.message };
      optimistic = optimistic.filter(row => row !== message);
    } else {
      message.unconfirmed = true;
      if (error.transport) failRound(error);
      else refusal = { turnId: message.turnId, message: error.message };
    }

    return false;
  } finally {
    sending = false;
    schedulePoll();
    requestRender();
  }
}
