import { hallwayInboxRound, ensureBoardQueried } from "./board/index.js";
import { hallwayMessagesRound, queryHallwayMessages, renderHallwayMessages } from "./board/hallway-messages.js";
import { escapeHtml } from "./text.js";

const subjects = new Map();
export const hallwayEmptySubject = { id: "hallway:empty", kind: "hallway", name: "Hallways", glyph: "H", status: "not queried", subtitle: "", description: "", room: "not reported", body: "not reported", recall: "not reported", updatedAt: "", messages: [], canPost: false };

export function hallwaySourceLine() {
  const round = hallwayInboxRound();
  if (round.status === "idle") return "Hallway inbox not queried";
  if (round.status === "pending") return "Asking the Hallway inbox door…";
  if (round.refusal) return `Hallway inbox unreachable · ${round.refusal}`;
  if (!Array.isArray(round.hallways)) return "Hallway list not reported by the Host";
  if (round.hallways.length === 0) return "Host connected · no Hallways reported for this room";
  return `Host connected · ${round.room ?? "room not reported"} · inbox queried ${round.queriedAt}`;
}

export function syncHallwaySubjects(conversations) {
  const round = hallwayInboxRound();
  conversations[hallwayEmptySubject.id] = hallwayEmptySubject;
  if (round.status !== "answered" || round.refusal || !Array.isArray(round.hallways)) return [];
  return round.hallways.filter(entry => typeof entry.hallway === "string" && entry.hallway).map(entry => {
    const id = `hallway:${entry.hallway}`;
    let item = subjects.get(id);
    if (!item) {
      item = { ...hallwayEmptySubject, id, hallwayId: entry.hallway, messages: [] };
      subjects.set(id, item);
    }
    Object.assign(item, { inbox: entry, name: entry.name ?? entry.hallway, status: "Host inbox answered", room: round.room ?? "not reported", updatedAt: entry.latestCreatedAt ?? "not reported", date: entry.latestCreatedAt?.slice(0, 10) ?? "date not reported", subtitle: entry.latestExcerpt ?? "No latest excerpt reported", description: "Read-only Hallway messages" });
    conversations[id] = item;
    return item;
  }).sort((a, b) => (b.inbox.latestCreatedAt ?? "").localeCompare(a.inbox.latestCreatedAt ?? ""));
}

export function queryHallway(item) {
  ensureBoardQueried();
  if (item?.hallwayId) queryHallwayMessages(item.hallwayId);
}

export function hallwayMembers(item) {
  const members = item.inbox?.members;
  return Array.isArray(members) ? members.map(member => typeof member === "string" ? member : member.room).filter(Boolean).join(" · ") || "No members reported" : "not reported";
}

export function hallwayParticipants(item) {
  const rows = hallwayMessagesRound(item.hallwayId)?.data?.messages;
  return Array.isArray(rows) ? [...new Set(rows.map(row => row.spirit || row.room).filter(Boolean))].join(" · ") || "No participants reported" : "participants not reported";
}

export function renderHallwayThread(item) {
  if (!item.hallwayId) return `<p role="status">${escapeHtml(hallwaySourceLine())}</p>`;
  return `<div class="specimen-stack" data-live-hallway="${escapeHtml(item.hallwayId)}"><p>${escapeHtml(hallwaySourceLine())} · reading clears nothing</p><button type="button" data-hallway-query>Query Host</button>${renderHallwayMessages(hallwayMessagesRound(item.hallwayId))}</div>`;
}

export function renderHallwayStatus(item) {
  const round = hallwayMessagesRound(item.hallwayId);
  const rows = round?.data?.messages;
  const messageSource = !round ? "not queried" : round.status === "pending" ? "querying" : round.refusal ? `unreachable · ${round.refusal}` : Array.isArray(rows) ? "Host connected" : "not reported";
  const facts = [["Hallway", item.hallwayId], ["Unread", item.inbox?.unread], ["Explicit attention", item.inbox?.mentions], ["Latest message", item.inbox?.latestCreatedAt], ["Messages in this read", Array.isArray(rows) ? rows.length : undefined], ["Messages source", messageSource], ["Members", hallwayMembers(item)]];
  return `<div class="specimen-stack"><section class="state-section"><h2>Hallway status</h2>${facts.map(([key, value]) => `<p><strong>${escapeHtml(key)}</strong> · ${escapeHtml(String(value ?? "not reported"))}</p>`).join("")}<p>${escapeHtml(hallwaySourceLine())}</p><button type="button" data-hallway-query>Query Host</button></section></div>`;
}

export function renderHallwayRecord(item) {
  return `<div class="specimen-stack"><section class="membership-card"><h2>${escapeHtml(item.name)}</h2><p>Members · ${escapeHtml(hallwayMembers(item))}</p><p>${escapeHtml(hallwaySourceLine())}</p></section><p>The Host reports no seals or folds for this Hallway</p></div>`;
}
