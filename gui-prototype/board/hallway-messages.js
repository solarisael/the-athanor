// Hallway messages — the peers' own prose, read on the operator's open.
//
// The concern: each Hallway row on the Docket board discloses the messages
// behind it, newest first, through POST /live/hallway/messages. This is the one
// place peer prose crosses into the panel, so the escape lives here and no
// caller gets to decide it.
//
// Read-only on both sides of the proxy: opening a drawer advances no cursor and
// acknowledges nothing. The route is frozen in the contract, and the installed
// Host may not serve it yet — a 404 renders as a named pending door, never as
// zero messages.
//
// board/index.js injects the panel grammar it owns, so one refusal voice speaks
// for every door on that surface.

import { escapeHtml } from "../text.js";

const MESSAGES_ROUTE = "/live/hallway/messages";
const MESSAGE_LIMIT = 30;

// One round per hallway key, plus the drawers the operator has open. Both
// survive the board's 60s poll, which rebuilds every row: native <details>
// state dies with the markup that carried it, so a drawer being read would
// slam shut a minute after it opened.
const rounds = new Map();
const openDrawers = new Set();

// No fallbacks: initBoard wires these before the first render, and a missing
// wire should throw here rather than render a door that never refuses.
let askDoor;
let absence;
let countedNoun;
let ledgerStamp;

export function initHallwayMessages(options) {
  askDoor = options.askDoor;
  absence = options.absence;
  countedNoun = options.countedNoun;
  ledgerStamp = options.ledgerStamp;
}

export function handleHallwayMessagesClick(event) {
  const toggle = event.target.closest("[data-hallway-toggle]");
  if (!toggle) return false;

  return toggleDrawer(toggle.closest("[data-hallway-drawer]"));
}

// Same click-before-flip contract the evidence drawer runs on: the click
// arrives before the browser flips <details>, so !drawer.open reads as "about
// to open", and the disclosure itself stays the browser's to run.
function toggleDrawer(drawer) {
  const hallway = drawer?.dataset.hallwayDrawer;
  if (!hallway) return false;

  if (drawer.open) {
    openDrawers.delete(hallway);
    return true;
  }

  openDrawers.add(hallway);
  if (rounds.get(hallway)?.status !== "pending") askMessagesDoor(hallway);
  return true;
}

// One Hallway per open, never with the board round: the inbox row above already
// carries the counts and the latest excerpt, and pulling every peer's prose in
// every Hallway is a read nobody asked for.
async function askMessagesDoor(hallway) {
  rounds.set(hallway, { status: "pending" });
  paintDrawer(hallway);

  const answer = await askDoor(MESSAGES_ROUTE, { hallway, limit: MESSAGE_LIMIT });

  rounds.set(hallway, { status: "answered", ...answer });
  paintDrawer(hallway);
}

// Painted into the live drawer by hand: a full re-render rebuilds the row and
// shuts the disclosure the operator just opened. The chip is painted with the
// body, never after it — a chip left on the previous round is the contradiction
// this drawer exists to refuse. The match walks the nodes because a hallway key
// reaching querySelector as text is a selector-injection door.
function paintDrawer(hallway) {
  const body = nodeFor("[data-hallway-body]", "hallwayBody", hallway);
  if (body) body.innerHTML = renderHallwayMessages(rounds.get(hallway));

  const chip = nodeFor("[data-hallway-chip]", "hallwayChip", hallway);
  if (chip) chip.innerHTML = doorChip(hallway);
}

function nodeFor(selector, dataKey, hallway) {
  return [...document.querySelectorAll(selector)].find(node => node.dataset[dataKey] === hallway);
}

// A row the inbox door sent without a hallway key can be shown but not opened:
// this door takes a key and this page will not invent one.
export function renderHallwayDrawer(hallway) {
  if (typeof hallway !== "string" || hallway.trim() === "") {
    return absence("This row carries no hallway key, so its messages cannot be asked for.");
  }

  const open = openDrawers.has(hallway) ? " open" : "";
  return `
      <details class="evidence-drawer hallway-drawer"${open} data-hallway-drawer="${escapeHtml(hallway)}">
        <summary data-hallway-toggle>Messages · newest first · limit ${MESSAGE_LIMIT} <span class="mechanics-row-flags" data-hallway-chip="${escapeHtml(hallway)}">${doorChip(hallway)}</span></summary>
        <div class="evidence-drawer-body" data-hallway-body="${escapeHtml(hallway)}">${renderHallwayMessages(rounds.get(hallway))}</div>
      </details>`;
}

// The chip and the body read one round, so a row can never wear a connected
// chip over a refusal.
function doorChip(hallway) {
  const round = rounds.get(hallway);
  if (!round) return '<span data-tone="quiet">door not asked</span>';
  if (round.status === "pending") return '<span data-tone="quiet">door asking</span>';
  if (round.refusal) {
    const label = pendingOnHost(round.refusal) ? "door pending on this Host" : "door refused";
    return `<span data-tone="attention">${label}</span>`;
  }

  const rows = messageRows(round);
  if (!rows) return '<span data-tone="attention">no message list</span>';

  return `<span data-tone="steady">${escapeHtml(countedNoun(rows.length, "message"))}</span>`;
}

// The drawer body, from a round alone. Every other function here reaches the
// browser; this one does not, which is what makes the hostile-markup proof in
// hallway-messages.test.js possible.
export function renderHallwayMessages(round) {
  if (!round) return absence("Not asked yet — open this Hallway to read its messages.");
  if (round.status === "pending") return absence("Reading the Hallway messages door…");
  if (round.refusal) return absence(refusalLine(round.refusal));

  const rows = messageRows(round);
  if (!rows) return absence("The messages door answered without a message list. No messages are shown.");
  if (rows.length === 0) return absence("This Hallway holds no messages yet.");

  const older = round.data?.hasMore
    ? absence(`Older messages stand behind this door. These are the newest ${countedNoun(rows.length, "message")}.`)
    : "";

  return `${rows.map(renderMessage).join("")}${older}`;
}

function refusalLine(refusal) {
  if (refusal.includes("unknown live route")) {
    return `The messages door is not mapped in this proxy: ${refusal}. No messages are shown.`;
  }
  if (pendingOnHost(refusal)) {
    return `The Hallway messages door is frozen in the contract, and this Host does not serve it yet: ${refusal}. No messages stand in its place.`;
  }

  return `The Hallway messages door refused: ${refusal}. No messages are shown.`;
}

function pendingOnHost(refusal) {
  return refusal.includes("HTTP 404");
}

// A collection that is not an array refuses rather than reading as empty: the
// door either sent a message list or it did not.
function messageRows(round) {
  const rows = round.data?.messages;
  return Array.isArray(rows) ? rows : undefined;
}

function renderMessage(message) {
  const speaker = `${textOr(message.room, "unknown room")} · ${textOr(message.spirit, "unnamed spirit")}`;
  const identity = Number.isInteger(message.id) ? `#${message.id}` : "no id";
  const body = typeof message.body === "string" ? message.body.trim() : "";
  const chips = messageChips(message);

  return `
        <article class="hallway-message">
          <p class="hallway-message-meta">
            <strong>${escapeHtml(speaker)}</strong>
            <time>${escapeHtml(ledgerStamp(message.createdAt))}</time>
            <span>${escapeHtml(identity)}</span>
            ${chips ? `<span class="mechanics-row-flags">${chips}</span>` : ""}
          </p>
          ${body ? `<p class="hallway-message-body">${escapeHtml(body)}</p>` : absence("This message carries no body.")}
        </article>`;
}

// A reply marker points at an id the door can serve, so a replyTo that is not
// an integer wears no marker rather than a broken one.
function messageChips(message) {
  const reply = Number.isInteger(message.replyTo)
    ? `<span data-tone="quiet">reply to #${message.replyTo}</span>`
    : "";
  const rooms = Array.isArray(message.toRooms) ? message.toRooms : [];

  return [reply, ...rooms.map(room => `<span data-tone="quiet">${escapeHtml(`to ${textOr(room, "unnamed room")}`)}</span>`)]
    .filter(Boolean)
    .join("");
}

function textOr(value, fallback) {
  return typeof value === "string" && value.trim() !== "" ? value.trim() : fallback;
}
