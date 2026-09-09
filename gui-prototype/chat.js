import { roomState } from "./health.js";

let round = { status: "idle", messages: [], reason: null };
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
}

export function isLiveChat(item) {
  return item.kind === "direct" && item.id === roomState().room;
}

export function chatState() {
  return { ...round, sending, refusal, unanswered: hasUnanswered() };
}

export function chatMessages() {
  return [...round.messages, ...optimistic].map(message => ({
    ...message,
    author: message.authorName,
    glyph: message.authorName.slice(0, 1),
    time: message.at,
    pending: message.pending === true
  }));
}

export function chatBlockReason(item) {
  const room = roomState();
  if (!room.room) {
    if (room.status !== "failed") return "Host room not queried";
    return room.reached ? `Room state not served · ${room.reason}` : `Host unreachable · ${room.reason}`;
  }
  if (!isLiveChat(item)) return "Not this Host's room";
  if (round.reason) return `Host unreachable · ${round.reason}`;
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
  return optimistic.length > 0 || round.messages.some(message => message.author === "operator" && !answered.has(message.turnId));
}

function schedulePoll() {
  clearTimeout(poll);
  poll = null;
  if (hasUnanswered()) poll = setTimeout(() => { void querySnapshot(); }, 2000);
}

async function post(path, body) {
  const response = await fetch(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body)
  });
  if (!response.ok) {
    let reason = `Host answered ${response.status}`;
    if (response.headers.get("content-type")?.includes("application/json")) {
      const result = await response.json();
      if (typeof result.error === "string") reason = result.error;
    }
    throw Object.assign(new Error(reason), { status: response.status });
  }
  return response.json();
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
    const result = await post("/live/chat/snapshot", {});
    if (result.room !== roomState().room || !Array.isArray(result.messages) || result.messages.some(message =>
      !Number.isFinite(message.sequence) || !["operator", "spirit"].includes(message.author) ||
      typeof message.authorName !== "string" || typeof message.text !== "string" ||
      typeof message.turnId !== "string" || !Number.isFinite(Date.parse(message.at)))) {
      throw new Error("Host answered without a valid room chat snapshot");
    }
    round = { status: "live", reason: null, messages: result.messages.sort((a, b) => a.sequence - b.sequence) };
    optimistic = optimistic.filter(message => !round.messages.some(row => row.author === "operator" && row.turnId === message.turnId));
  } catch (error) {
    round = { ...round, status: "failed", reason: error.message };
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
  if (!optimistic.some(message => message.turnId === attempt.sayId)) {
    optimistic.push({ author: "operator", authorName: roomState().operator ?? "Operator", text,
      at: new Date().toISOString(), turnId: attempt.sayId, pending: true });
  }
  requestRender();
  try {
    await post("/live/chat/say", attempt);
    retry = null;
    await querySnapshot();
    return true;
  } catch (error) {
    if (error.status === 400) {
      refusal = error.message;
      retry = null;
      optimistic = optimistic.filter(message => message.turnId !== attempt.sayId);
    } else {
      round = { ...round, status: "failed", reason: error.message };
    }
    return false;
  } finally {
    sending = false;
    schedulePoll();
    requestRender();
  }
}
