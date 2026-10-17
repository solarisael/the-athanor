import { escapeHtml } from "./text.js";
import { initRepair, queryRepairStatus, startRepair, renderRepair } from "./repair.js";
import { initSessions, ensureSessionsQueried, renderSessions, handleSessionsClick, handleSessionsChange } from "./sessions/index.js";
import { readPreferences, writePreferences } from "./preferences/index.js";
import { renderMarkdown } from "./markdown.js";
import { renderDirectStatus } from "./mechanics-live.js";
import { initChat, syncChatPanel, isLiveChat, chatState, chatMessages, chatBlockReason, say } from "./chat.js";
import { syncHallwaySubjects, hallwayEmptySubject, hallwaySourceLine, hallwayMembers, hallwayParticipants, queryHallway, renderHallwayThread, renderHallwayStatus, renderHallwayRecord } from "./hallways.js";
import { houseProject, queryProjects, projectMarkup, projectWorkState, renderProjectStatus } from "./projects.js";
import { initPulse, ensurePulseQueried, handlePulseClick, recoverPulse } from "./pulse.js";
import { initBoard, ensureBoardQueried, handleBoardClick, renderHouseBoard, hallwayInboxRound } from "./board/index.js";
import { initSediment, ensureSedimentQueried, handleSedimentClick, handleSedimentInput, renderHouseSediment, renderRoomSediment, liveShelfCounts } from "./sediment/index.js";
import {
  initHealth, ensureHealthQueried, queryHealthHost, onHostRecovered,
  STATUS_CHANNELS, statusChannel, healthSourceLine, healthSourceTone,
  accountStateRows, persistenceDetail, healthRoundStatus, roomState, askHost
} from "./health.js";
import {
  initMechanics, resetMechanicsView, saveMechanicsScroll, mechanicsScrollTop,
  handleMechanicsClick, handleMechanicsInput, renderHouseMechanics, renderMechanicsResults
} from "./mechanics.js";

const conversations = {
  connecting: { id: "connecting", kind: "direct", name: "Room", glyph: "·", room: "", description: "Loading configured rooms" },
  [houseProject.id]: houseProject
};

conversations.house = {
  id: "house",
  kind: "house",
  name: "Solarisael House",
  glyph: "✦",
  subtitle: "House scope",
  description: "Shared settings, memories, lessons, and substrate state for the whole House.",
  status: "Local",
  room: "house",
  body: "Configured rooms",
  recall: "House",
  listPreview: "Shared memory and lessons",
  updatedAt: "Now",
  messages: []
};





// The strip's five channels are read from the Host's health round in health.js.
// Only the About door keeps fixed copy, because it describes this surface's own
// authority rather than anything the Host reports.
const ABOUT_STATUS = [
  "About this surface",
  "This local surface reads Host records and sends operator chat and explicitly confirmed session controls. Memories and lessons remain read-only."
];
const modeLabels = {
  direct: "Direct messages",
  hallway: "Hallways",
  project: "Projects"
};
const SWITCHER_SCOPE_LABELS = {
  direct: "Direct",
  hallway: "Hallways",
  project: "Projects",
  house: "House"
};



const SUBJECT_VIEW_LABELS = {
  house: ["Overview", "Mechanics", "Memories & Lessons"],
  project: ["Overview", "Status", "Evidence"],
  direct: ["Chat", "Status", "Memories"],
  hallway: ["Thread", "Status", "Record"]
};
const SETTING_STATE_KEYS = {
  density: "density",
  "text-scale": "textScale",
  measure: "measure",
  contrast: "contrast",
  "reduced-motion": "reducedMotion",
  timestamps: "timestamps",
  "send-with-enter": "sendWithEnter",
  "status-visible": "statusVisible"
};
const preferences = readPreferences();
let connection = { status: "pending", rooms: [], capabilities: { repair: false } };


const state = {
  mode: "direct",
  activeId: "connecting",
  activeView: ["live", "state", "durable"].includes(new URLSearchParams(location.search).get("view"))
    ? new URLSearchParams(location.search).get("view") : "live",
  selectedMessageKey: null,
  selectedPresenceId: null,
  accessPicker: false,
  drawerView: "root",
  drafts: new Map(),
  profileAnchor: null,
  profileReturnId: null,
  houseReturn: null,
  density: "comfortable",
  textScale: "standard",
  measure: "focused",
  contrast: "grayscale",
  reducedMotion: false,
  timestamps: true,
  sendWithEnter: true,
  statusVisible: true,
  ...preferences.values,
  memberDockOpen: false,
  bellOpen: false,
  switcherOpen: false,
  switcherQuery: "",
  switcherIndex: 0
};

const shell = document.querySelector(".app-shell");
const sidebar = document.querySelector(".sidebar");
const conversationList = document.querySelector(".subject-list");
const header = document.querySelector(".subject-header");
const timeline = document.querySelector(".message-timeline");
const composer = document.querySelector(".composer");
const input = document.querySelector("#message-input");
const sendButton = composer.querySelector('button[type="submit"]');
const sendRefusal = document.querySelector("#send-refusal");
const inspectorEyebrow = document.querySelector(".inspector-eyebrow");
const inspectorTitle = document.querySelector(".inspector-title");
const inspectorContent = document.querySelector(".inspector-content");
const inspectorToggle = document.querySelector(".inspector-toggle");
const sidebarToggle = document.querySelector(".sidebar-toggle");
const accountButton = document.querySelector(".account-button");
const sidebarDrawer = document.querySelector(".sidebar-drawer");
const drawerPanes = [...document.querySelectorAll("[data-drawer-pane]")];
const subjectViewButtons = [...document.querySelectorAll("[data-subject-view]")];
const SUBJECT_VIEWS = subjectViewButtons.map(button => button.dataset.subjectView);
const drawerReturnFocus = new Map();
const clearButton = composer.querySelector(".composer-clear");
const responseStatus = document.querySelector("#response-status");
const houseDoor = document.querySelector("[data-house-door]");
const memberDock = document.querySelector(".member-dock");
const memberDockScrim = document.querySelector(".member-dock-scrim");
const profileLayer = document.querySelector(".profile-layer");
let drawerFocusGeneration = 0;
const statusPopover = document.querySelector(".status-popover");
const accountState = document.querySelector("[data-account-state]");
const switcherLayer = document.querySelector(".switcher-layer");
const bellToggle = document.querySelector(".bell-toggle");
const bellLayer = document.querySelector(".bell-layer");
const bellInboxList = bellLayer.querySelector(".hallway-inbox-list");
const bellSummary = bellLayer.querySelector("[data-hallway-inbox-summary]");
const switcherInput = switcherLayer.querySelector("input");
const switcherResults = switcherLayer.querySelector(".switcher-results");
let switcherMatches = [];
let switcherReturnFocus = null;
let bellReturnFocus = null;


function renderAvatar(glyph, size = "default") {
  const sizeClass = size === "lg" ? " avatar-lg" : "";
  return `<span class="avatar${sizeClass}" aria-hidden="true">${escapeHtml(glyph)}</span>`;
}

function roomRecipientLabel(roomId) {
  return `${roomId.charAt(0).toUpperCase()}${roomId.slice(1)} room`;
}

function renderBellIcon() {
  return `
    <svg class="bell-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"></path>
      <path d="M10 21h4"></path>
    </svg>`;
}




function renderAttentionBadges(unread, targeted, accessible = true) {
  if (unread === 0 && targeted === 0) return "";
  const label = [
    unread > 0 ? `${unread} unread` : "",
    targeted > 0 ? `${targeted} needs attention` : ""
  ].filter(Boolean).join(", ");
  const accessibility = accessible ? `aria-label="${escapeHtml(label)}"` : 'aria-hidden="true"';
  return `
    <span class="hallway-counts" ${accessibility}>
      ${unread > 0 ? `<span class="attention-count is-unread">${unread}</span>` : ""}
      ${targeted > 0 ? `<span class="attention-count is-targeted">${targeted}</span>` : ""}
    </span>`;
}

// The Bell renders the Host's own inbox round, fetched once by the board
// module. Mentions are the explicit attention: they are the room's pending
// notifications, not a number this page derived from prose.
function liveHallwayRows() {
  const round = hallwayInboxRound();
  if (round.status !== "answered" || round.refusal) return [];

  return Array.isArray(round.hallways) ? round.hallways : [];
}

function hallwayInboxTotals() {
  return liveHallwayRows().reduce(
    (totals, entry) => ({
      unread: totals.unread + Number(entry.unread ?? 0),
      targeted: totals.targeted + Number(entry.mentions ?? 0)
    }),
    { unread: 0, targeted: 0 }
  );
}

function renderBellToggle() {
  const totals = hallwayInboxTotals();
  bellToggle.dataset.hasAttention = String(totals.targeted > 0);
  bellToggle.setAttribute("aria-label", bellToggleLabel(totals));
  bellToggle.innerHTML = `
    ${renderBellIcon()}
    ${renderAttentionBadges(totals.unread, totals.targeted, false)}`;
}

// A bell nobody has asked the door for must not wear the caught-up label: an
// absent count and a zero count are different facts.
function bellToggleLabel(totals) {
  const round = hallwayInboxRound();
  if (round.status !== "answered") return "Hallway inbox, not read from the Host yet";
  if (round.refusal) return "Hallway inbox, the inbox door refused";
  if (totals.unread === 0 && totals.targeted === 0) return "Hallway inbox, caught up";

  return `Hallway inbox, ${totals.unread} unread, ${totals.targeted} needs attention`;
}

function renderHallwayInbox() {
  const round = hallwayInboxRound();
  bellSummary.textContent = bellSummaryText(round, hallwayInboxTotals());
  bellInboxList.innerHTML = renderBellRows(round);
}

// The board's four source states, in the board's own grammar: unasked, asking,
// refused, answered. Reading here asks nothing and acknowledges nothing.
function bellSummaryText(round, totals) {
  if (round.status === "idle") return "The Hallway inbox door has not been asked yet.";
  if (round.status === "pending") return "Asking the Hallway inbox door…";
  if (round.refusal) return `The Hallway inbox door refused: ${round.refusal}`;
  if (!Array.isArray(round.hallways)) return "The inbox door answered without a Hallway list.";

  return `${totals.unread} unread message${totals.unread === 1 ? "" : "s"} · ${totals.targeted} explicit attention · reading clears nothing`;
}

function renderBellRows(round) {
  if (round.status === "idle") {
    return bellAbsence("The Bell has nothing to read yet.", "It shows what the Hallway inbox door answered. Open House · Board to ask it.");
  }
  if (round.status === "pending") return bellAbsence("Asking the Hallway inbox door…", "No rows until it answers.");
  if (round.refusal) return bellAbsence("The Hallway inbox door refused.", round.refusal);
  if (!Array.isArray(round.hallways)) {
    return bellAbsence("The inbox door answered without a Hallway list.", "No rows are shown in its place.");
  }

  const ringing = round.hallways.filter(entry => Number(entry.unread ?? 0) > 0 || Number(entry.mentions ?? 0) > 0);
  if (ringing.length === 0) {
    return bellAbsence("Nothing is ringing.", `Every Hallway this room may open is caught up · queried ${round.queriedAt} local`);
  }

  return ringing.map(renderBellRow).join("");
}

// A live row names a Hallway key, never a fixture thread, so its verb goes to
// the one surface that can open the messages behind it.
function renderBellRow(entry) {
  const hallway = typeof entry.hallway === "string" && entry.hallway !== "" ? entry.hallway : "unnamed hallway";
  const stamp = (entry.latestCreatedAt ?? "").slice(0, 16) || "no timestamp";
  const latest = entry.latestExcerpt
    ? `${entry.latestRoom ?? "unknown room"}: ${entry.latestExcerpt}`
    : "No messages in this Hallway yet.";

  return `
      <article class="hallway-inbox-item" role="listitem">
        <button class="hallway-inbox-row" type="button" data-bell-board="${escapeHtml(hallway)}">
          ${renderAvatar(hallway.slice(0, 1).toUpperCase())}
          <span class="hallway-inbox-copy">
            <span class="hallway-inbox-heading">
              <strong>${escapeHtml(hallway)}</strong>
              <small>${escapeHtml(`${stamp} · Host inbox row`)}</small>
            </span>
            <span class="hallway-inbox-preview">${escapeHtml(latest)}</span>
          </span>
          ${renderAttentionBadges(Number(entry.unread ?? 0), Number(entry.mentions ?? 0))}
          <span class="hallway-inbox-verb">Open thread</span>
        </button>
      </article>`;
}

function bellAbsence(title, detail) {
  return `
      <div class="hallway-inbox-empty" role="status"><strong>${escapeHtml(title)}</strong><span>${escapeHtml(detail)}</span></div>`;
}

function directChatStatus(item) {
  if (!isLiveChat(item)) {
    if (item.room) return "Open room";
    return connection.status === "failed" ? "Rooms unavailable" : "Loading rooms";
  }
  const chat = chatState();
  const room = roomState();
  if ((chat.reason && chat.transport) || (room.status === "failed" && !room.reached)) return "Connection lost";
  if (chat.reason || room.status === "failed") return "Chat unavailable";
  return "Connected";
}

function renderSubjectRow(item, active, live) {
  const rowMeta = item.kind === "hallway"
    ? `${item.date ?? ""} · Members: ${hallwayMembers(item)} · ${item.inbox?.unread ?? "not reported"} unread`
    : item.kind === "direct" ? directChatStatus(item) : item.listPreview;
  const unread = item.kind === "hallway" ? item.inbox?.unread : 0;
  return `
    <button class="subject-row ${active ? "is-active" : ""}" type="button" data-conversation="${escapeHtml(item.id)}" data-subject-kind="${escapeHtml(item.kind)}" aria-current="${active ? "page" : "false"}">
      <span class="avatar-stack">
        ${renderAvatar(item.glyph)}
        ${item.kind === "direct" ? "" : `<span class="presence ${live ? "" : "is-quiet"}" aria-label="${escapeHtml(item.status)}"></span>`}
      </span>
      <span class="subject-row-copy">
        <span class="subject-row-heading">
          <strong>${escapeHtml(item.name)}</strong>
          <span class="subject-row-tail">
            ${item.kind === "hallway" ? renderAttentionBadges(unread, 0) : ""}
            ${item.kind === "direct" ? "" : `<time>${escapeHtml(item.updatedAt)}</time>`}
          </span>
        </span>
        <small>${escapeHtml(rowMeta)}</small>
      </span>
    </button>
  `;
}

// A wire stamp becomes the local clock time; anything else (the prototype's
// own "09:47", "now") is shown as written.
function messageTime(time) {
  const stamp = Date.parse(time);
  if (!Number.isFinite(stamp) || !/\d{4}-\d{2}-\d{2}T/.test(String(time))) return { text: String(time ?? ""), title: "" };
  const at = new Date(stamp);
  return { text: at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }), title: at.toLocaleString() };
}

function renderSteps(steps) {
  if (!steps?.length) return "";
  return `<ol class="message-steps" aria-label="Tools used">${steps.map(step => `
    <li data-status="${escapeHtml(step.status)}" title="${escapeHtml(step.summary)}">
      <span class="message-step-tool">${escapeHtml(step.tool)}</span>
      ${step.summary ? `<span class="message-step-summary">${escapeHtml(step.summary)}</span>` : ""}
      <span class="message-step-outcome">${step.status === "running" ? "…" : step.status === "error" ? "failed" : Number.isFinite(step.elapsedMs) ? `${(step.elapsedMs / 1000).toFixed(1)} s` : "ok"}</span>
    </li>`).join("")}</ol>`;
}

function messageIdentity(message, index) {
  return message.turnId ? JSON.stringify([message.turnId, message.role]) : String(index);
}

function renderMessage(message, index, selected) {
  const messageKey = messageIdentity(message, index);
  const recipients = message.toRooms?.map(roomRecipientLabel).join(", ");
  const time = messageTime(message.time);
  return `
    <article class="message ${selected ? "is-selected" : ""} ${message.draft ? "is-draft" : ""}" data-author="${escapeHtml(message.author)}" data-message-index="${index}" data-message-key="${escapeHtml(messageKey)}" data-outcome="${escapeHtml(message.outcome ?? "complete")}">
      ${renderAvatar(message.glyph)}
      <div class="message-body">
        <p class="message-meta">
          <strong>${escapeHtml(message.author)}</strong>
          <span title="${escapeHtml(time.title)}">${escapeHtml(time.text)}</span>
          ${recipients ? `<span class="message-recipient">To ${escapeHtml(recipients)}</span>` : ""}
          ${message.local ? '<span class="message-delivery">Local-only · undelivered</span>' : ""}
          ${message.undelivered ? '<span class="message-delivery">Not delivered · send again to retry</span>' : message.pending ? '<span class="message-delivery">Pending · awaiting Host confirmation</span>' : message.draft ? '<span class="message-delivery">Answering…</span>' : ""}
          ${message.outcome === "error" ? '<span class="message-delivery message-outcome">Failed</span>' : message.outcome === "aborted" ? '<span class="message-delivery message-outcome">Cancelled</span>' : ""}
        </p>
        <div class="message-thinking">${(message.thinking ?? []).map((block, position) => `<details><summary>Thinking ${position + 1}</summary><pre>${escapeHtml(block)}</pre></details>`).join("")}</div>
        <div class="message-tools">${renderSteps(message.steps)}</div>
        <div class="message-bubble" tabindex="0" role="button" aria-label="Inspect message from ${escapeHtml(message.author)}">${renderMarkdown(message.text)}${message.draft ? '<span class="message-cursor" aria-hidden="true"></span>' : ""}</div>
      </div>
    </article>
  `;
}


function renderEmptyState(headline, reasons = []) {
  return `
    <section class="timeline-empty" role="status">
      <strong>${escapeHtml(headline)}</strong>
      ${reasons.filter(Boolean).map(reason => `<span>${escapeHtml(reason)}</span>`).join("")}
    </section>
  `;
}

function renderSpecimenLead(eyebrow, title, body = "") {
  return `
    <section class="specimen-card specimen-lead surface-lead">
      <span class="eyebrow">${escapeHtml(eyebrow)}</span>
      <h2>${escapeHtml(title)}</h2>
      ${body ? `<p>${escapeHtml(body)}</p>` : ""}
    </section>
  `;
}

function renderOverviewHero(eyebrow, title, body, workState = null) {
  const workStateMarkup = workState
    ? `<div class="project-work-state"><span>${escapeHtml(workState.label)}</span><strong>${escapeHtml(workState.title)}</strong><small>${escapeHtml(workState.detail)}</small></div>`
    : "";
  return `
    <section class="overview-hero surface-lead">
      <span class="eyebrow">${escapeHtml(eyebrow)}</span>
      <h2>${escapeHtml(title)}</h2>
      <p>${escapeHtml(body)}</p>
      ${workStateMarkup}
    </section>
  `;
}

function renderFactList(entries) {
  return `<dl class="context-list">${entries.map(([term, detail]) => `<dt>${escapeHtml(term)}</dt><dd>${escapeHtml(detail)}</dd>`).join("")}</dl>`;
}

function renderDurableEntry({ date, title, mark, detail }) {
  return `<article class="durable-entry surface-row">
    <time>${escapeHtml(date)}</time>
    <strong>${escapeHtml(title)}</strong>
    <small>${escapeHtml(mark)} · ${escapeHtml(detail)}</small>
  </article>`;
}




function renderPresenceRow(presence, selected) {
  return `
    <button class="presence-row" type="button" data-presence-id="${escapeHtml(presence.id)}" aria-pressed="${String(selected)}">
      <span class="presence-glyph" aria-hidden="true">${escapeHtml(presence.glyph)}</span>
      <span><b>${escapeHtml(presence.session)}</b><small>${escapeHtml(presence.liveness)} · ${escapeHtml(presence.activity)}</small></span>
    </button>
  `;
}

function renderInspectorDoor(view, label) {
  return `<button class="inspector-door" type="button" data-inspector-view="${escapeHtml(view)}">${escapeHtml(label)}</button>`;
}

function renderInspectorDoors(title, doors) {
  return `<section class="context-card context-doors"><h3>${escapeHtml(title)}</h3>${doors.map(([view, label]) => renderInspectorDoor(view, label)).join("")}</section>`;
}



function activeMessages(item) {
  if (item.kind === "direct") return isLiveChat(item) ? chatMessages() : [];
  return item.messages ?? [];
}

function draftKey(item) {
  if (isLiveChat(item)) return `host-chat:${item.id}`;
  return item.id;
}

function composerBlockReason(item) {
  if (item.kind === "direct") return chatBlockReason(item);
  if (item.kind === "hallway") return "Watching only · no Hallway write door in this surface";
  if (item.kind === "project") return "Read-only · no project chat door";
  if (item.canPost === false) return item.sendReason;
  return null;
}

function updateComposerState() {
  const item = conversations[state.activeId];
  const liveChat = isLiveChat(item);
  const chat = chatState();
  const blockedReason = composerBlockReason(item);
  const canParticipate = blockedReason === null;
  const hasText = input.value.trim().length > 0;

  input.readOnly = !canParticipate;
  input.setAttribute("aria-readonly", String(!canParticipate));
  const messageLabel = item.kind === "direct" ? `Message ${item.name}` : "Message";
  composer.querySelector('label[for="message-input"]').textContent = messageLabel;
  input.placeholder = item.kind === "direct" ? messageLabel : canParticipate ? "Write a message" : "Watching only";
  sendButton.disabled = !canParticipate || !hasText;
  clearButton.hidden = !canParticipate || !hasText;
  const refusal = blockedReason ?? (liveChat ? chat.refusal : null);
  sendRefusal.textContent = refusal ?? "";
  sendRefusal.hidden = refusal === null;
  let responseMessage = null;
  if (liveChat && chat.unanswered && !chat.reason && !chat.refusal && !chat.sending) {
    const answering = chat.drafts.some(draft => chat.messages.some(message =>
      message.author === "operator" && message.turnId === draft.turnId &&
      !chat.messages.some(answer => answer.author === "spirit" && answer.turnId === message.turnId)));
    responseMessage = answering ? `${item.name} is answering…` : `Message sent · waiting for ${item.name}`;
  }
  responseStatus.textContent = responseMessage ?? "";
  responseStatus.hidden = responseMessage === null;
}

function visibleConversations() {
  if (state.mode === "hallway") return syncHallwaySubjects(conversations);
  return Object.values(conversations).filter(item => item.kind === state.mode);
}

function itemIsLive(item) {
  if (item.kind === "direct") return isLiveChat(item);
  if (item.kind === "hallway") return item.connection === "Connected";
  return item.status.includes("Active") || item.status.includes("presences");
}

function openConversation(id) {
  const incoming = conversations[id];
  if (!incoming) return;
  if (incoming.kind === "direct" && incoming.room && incoming.room !== connection.room) {
    if (input.value.trim() && !window.confirm("Changing rooms discards this unsent message. Continue?")) return;
    const next = new URL(location.href);
    next.searchParams.set("room", incoming.room);
    next.searchParams.set("view", state.activeView);
    location.assign(next);
    return;
  }
  const outgoing = conversations[state.activeId];
  state.drafts.set(draftKey(outgoing), input.value);
  state.activeId = id;
  state.selectedMessageKey = null;
  state.selectedPresenceId = null;
  state.accessPicker = false;
  state.memberDockOpen = false;
  input.value = state.drafts.get(draftKey(incoming)) ?? "";
  input.style.height = "auto";
  setMobileSidebarOpen(false);
  state.profileAnchor = null;
  state.profileReturnId = null;
  updateComposerState();
  if (incoming.kind === "project") queryProjects(state.activeView);
  render();
  if (incoming.kind === "hallway" && incoming.hallwayId) queryHallway(incoming);
}

function preserveMechanicsScroll() {
  if (state.activeId === "house" && state.activeView === "state") saveMechanicsScroll();
}

function openSubjectView(view, { clearMessage = false } = {}) {
  if (!SUBJECT_VIEWS.includes(view)) return false;
  preserveMechanicsScroll();
  state.activeView = view;
  state.accessPicker = false;
  if (clearMessage) state.selectedMessageKey = null;
  if (state.activeId === "house" && view === "state") ensurePulseQueried();
  if (state.activeId === "house" && view === "live") ensureBoardQueried();
  if (conversations[state.activeId].kind === "project") queryProjects(view);
  render();
  return true;
}

function focusActiveSubjectView() {
  window.requestAnimationFrame(() => {
    const activeButton = subjectViewButtons.find(button => button.dataset.subjectView === state.activeView);
    activeButton?.focus({ preventScroll: true });
  });
}

function navigateToSubjectView(id, view) {
  const item = conversations[id];
  if (!item || !SUBJECT_VIEWS.includes(view)) return;
  if (item.kind === "house" && state.activeId !== "house") {
    state.houseReturn = { mode: state.mode, activeId: state.activeId, activeView: state.activeView };
  }
  if (item.kind !== "house") state.houseReturn = null;
  state.mode = item.kind;
  state.activeView = view;
  openConversation(id);
  if (item.kind === "house" && view === "state") ensurePulseQueried();
  if (item.kind === "house" && view === "live") ensureBoardQueried();
  focusActiveSubjectView();
}

function openBell() {
  if (state.bellOpen) return;
  if (state.switcherOpen) closeSwitcher(false);
  bellReturnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : bellToggle;
  state.bellOpen = true;
  drawerFocusGeneration += 1;
  shell.inert = true;
  bellLayer.hidden = false;
  bellLayer.setAttribute("aria-hidden", "false");
  bellToggle.setAttribute("aria-expanded", "true");
  renderHallwayInbox();
  window.requestAnimationFrame(() => {
    const firstRoute = bellLayer.querySelector("[data-bell-board]");
    (firstRoute ?? bellLayer.querySelector("[data-close-bell]"))?.focus({ preventScroll: true });
  });
}

function closeBell(restoreFocus = true) {
  if (!state.bellOpen) return;
  state.bellOpen = false;
  shell.inert = false;
  bellLayer.hidden = true;
  bellLayer.setAttribute("aria-hidden", "true");
  bellToggle.setAttribute("aria-expanded", "false");
  if (restoreFocus) bellReturnFocus?.focus({ preventScroll: true });
  bellReturnFocus = null;
}

// A live inbox row names a Hallway, not a fixture thread, so the Bell routes to
// the one surface that can open the messages behind it.
function openBoardFromBell(hallway) {
  closeBell(false);
  const item = syncHallwaySubjects(conversations).find(row => row.hallwayId === hallway);
  if (item) navigateToSubjectView(item.id, "live");
}

function openSettingsFromSwitcher(trigger) {
  setMobileSidebarOpen(true);
  openDrawerView("account", trigger ?? accountButton);
  const settingsDoor = sidebarDrawer.querySelector('[data-open-drawer="settings"]');
  openDrawerView("settings", settingsDoor);
}

function openHouseMechanics() {
  resetMechanicsView();
  setDrawerView("root", null, false);
  navigateToSubjectView("house", "state");
  focusActiveSubjectView();
}

function switcherCommandRegistry() {
  const commands = [];
  Object.values(conversations).forEach(item => {
    const labels = SUBJECT_VIEW_LABELS[item.kind];
    SUBJECT_VIEWS.forEach((view, index) => {
      const viewLabel = labels[index];
      const scopeLabel = SWITCHER_SCOPE_LABELS[item.kind];
      const currentSubject = item.id === state.activeId;
      let priority = 100 - index;
      if (item.kind === state.mode) priority = 180 - index;
      if (item.kind === "house") priority = 320 - index;
      if (currentSubject) priority = 500 - index;
      if (currentSubject && view === state.activeView) priority += 100;
      commands.push({
        id: `go:${item.id}:${view}`,
        verb: "Go",
        label: item.kind === "house" ? viewLabel : `${item.name} · ${viewLabel}`,
        path: item.kind === "house" ? `House › ${viewLabel}` : `${scopeLabel} › ${item.name} › ${viewLabel}`,
        keywords: `${item.kind} ${item.name} ${view} ${viewLabel}`,
        shortcut: currentSubject ? String(index + 1) : null,
        available: true,
        priority,
        execute: () => navigateToSubjectView(item.id, view)
      });
    });
    if (item.kind === "direct") {
      commands.push({
        id: `start:${item.id}:session`,
        verb: "Start",
        label: `New ${item.name} session`,
        path: `Direct › ${item.name} › New session`,
        keywords: `new start session direct ${item.name}`,
        shortcut: null,
        available: false,
        reason: "Unavailable: Host session creation is not supported",
        priority: item.id === state.activeId ? 440 : 120
      });
    }
  });
  commands.push({
    id: "open:settings",
    verb: "Open",
    label: "Interface settings",
    path: "Account › Interface settings",
    keywords: "settings preferences interface density text contrast motion status",
    shortcut: null,
    available: true,
    priority: 450,
    execute: trigger => openSettingsFromSwitcher(trigger)
  });
  commands.push({
    id: "recall:search",
    verb: "Recall",
    label: "Search memory",
    path: "House › Memories & Lessons",
    keywords: "recall memory search akasha lessons shelf",
    shortcut: null,
    available: true,
    priority: 300,
    execute: () => {
      navigateToSubjectView("house", "durable");
      window.requestAnimationFrame(() => window.requestAnimationFrame(() => {
        timeline.querySelector("[data-durable-search]")?.focus({ preventScroll: true });
      }));
    }
  });
  return commands;
}

function normalizeSwitcherText(value) {
  return String(value ?? "").toLowerCase().replace(/\s+/g, " ").trim();
}

function filteredSwitcherCommands(query) {
  const normalizedQuery = normalizeSwitcherText(query);
  const terms = normalizedQuery === "" ? [] : normalizedQuery.split(" ");
  return switcherCommandRegistry()
    .map(command => {
      const haystack = normalizeSwitcherText(`${command.verb} ${command.label} ${command.path} ${command.keywords}`);
      if (!terms.every(term => haystack.includes(term))) return null;
      let score = command.priority;
      if (normalizedQuery !== "") {
        if (normalizeSwitcherText(command.label).startsWith(normalizedQuery)) score += 120;
        if (haystack.startsWith(normalizedQuery)) score += 80;
      }
      return { command, score };
    })
    .filter(Boolean)
    .sort((left, right) => right.score - left.score || left.command.label.localeCompare(right.command.label))
    .slice(0, 12)
    .map(entry => entry.command);
}

function renderSwitcher() {
  switcherMatches = filteredSwitcherCommands(state.switcherQuery);
  if (state.switcherIndex >= switcherMatches.length) state.switcherIndex = Math.max(0, switcherMatches.length - 1);
  if (switcherMatches.length === 0) {
    switcherResults.innerHTML = '<div class="switcher-empty">No door matches that search.</div>';
    switcherInput.removeAttribute("aria-activedescendant");
    return;
  }
  switcherResults.innerHTML = switcherMatches.map((command, index) => {
    const active = index === state.switcherIndex;
    const available = command.available !== false;
    let detail = command.path;
    if (!available && command.reason) detail = `${command.path} · ${command.reason}`;
    const shortcut = command.shortcut
      ? `<kbd class="switcher-shortcut">${escapeHtml(command.shortcut)}</kbd>`
      : "";
    return `
      <button class="switcher-result ${active ? "is-active" : ""}" id="switcher-option-${index}" type="button" role="option" tabindex="-1" data-switcher-command="${escapeHtml(command.id)}" aria-selected="${String(active)}" aria-disabled="${String(!available)}">
        <span class="switcher-verb">${escapeHtml(command.verb)}</span>
        <span class="switcher-copy"><strong>${escapeHtml(command.label)}</strong><small>${escapeHtml(detail)}</small></span>
        ${shortcut}
      </button>
    `;
  }).join("");
  switcherInput.setAttribute("aria-activedescendant", `switcher-option-${state.switcherIndex}`);
  switcherResults.querySelector(".is-active")?.scrollIntoView({ block: "nearest" });
}

function openSwitcher() {
  if (state.switcherOpen) return;
  if (state.bellOpen) closeBell(false);
  switcherReturnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  state.switcherOpen = true;
  state.switcherQuery = "";
  state.switcherIndex = 0;
  switcherInput.value = "";
  drawerFocusGeneration += 1;
  shell.inert = true;
  switcherLayer.hidden = false;
  switcherLayer.setAttribute("aria-hidden", "false");
  renderSwitcher();
  window.requestAnimationFrame(() => switcherInput.focus());
}

function closeSwitcher(restoreFocus = true) {
  if (!state.switcherOpen) return;
  state.switcherOpen = false;
  shell.inert = false;
  switcherLayer.hidden = true;
  switcherLayer.setAttribute("aria-hidden", "true");
  switcherInput.removeAttribute("aria-activedescendant");
  const returnFocus = switcherReturnFocus;
  switcherReturnFocus = null;
  if (restoreFocus && returnFocus?.isConnected) returnFocus.focus({ preventScroll: true });
}

function executeSwitcherCommand(id) {
  const command = switcherCommandRegistry().find(candidate => candidate.id === id);
  if (!command || command.available === false || typeof command.execute !== "function") return;
  const trigger = switcherReturnFocus;
  closeSwitcher(false);
  command.execute(trigger);
}

// the House roster the pickers draw from; membership vocabularies differ by surface
const HOUSE_ROOMS = ["Kintsu room", "Kodo room", "Tuner room"];


function renderCardPicker(verb, attr, options) {
  if (options.length === 0) return "";
  const picker = state.accessPicker
    ? `<div class="card-picker">${options.map(option => `<button type="button" ${attr}="${escapeHtml(option)}">${escapeHtml(option)}</button>`).join("")}</div>`
    : "";
  return `
    <div class="access-control">
      <button class="card-verb" type="button" data-access-picker aria-expanded="${String(state.accessPicker)}">${escapeHtml(verb)}</button>
      ${picker}
    </div>`;
}

function renderCollectionDoor() {
  if (state.mode !== "direct") return "";
  return '<p class="collection-door">New spirit · no creation capability</p>';
}

let renderedConversationList = null;

function renderConversationList() {
  let markup;
  if (state.mode === "hallway" && visibleConversations().length === 0) {
    markup = `<p role="status">${escapeHtml(hallwaySourceLine())}</p>`;
  } else if (state.mode === "house") {
    markup = '<div class="house-scope-note"><strong>House scope</strong><span>Shared mechanics, memory, lessons, and status.</span></div>';
  } else {
    markup = visibleConversations()
      .map(item => renderSubjectRow(item, item.id === state.activeId, itemIsLive(item)))
      .join("") + renderCollectionDoor();
  }
  if (markup === renderedConversationList) return;

  const focused = document.activeElement;
  const focusedRoom = conversationList.contains(focused) ? focused.dataset.conversation : null;
  conversationList.innerHTML = markup;
  renderedConversationList = markup;
  if (focusedRoom) {
    conversationList.querySelector(`[data-conversation="${CSS.escape(focusedRoom)}"]`)?.focus({ preventScroll: true });
  }
}


function renderHeader(item) {
  header.dataset.kind = item.kind;
  header.innerHTML = `
    ${renderAvatar(item.glyph, "lg")}
    <div class="subject-header-copy">
      <h1 class="subject-heading">${escapeHtml(item.name)}</h1>
      <p>${escapeHtml(item.kind === "direct" ? `Conversation · ${directChatStatus(item)}` : item.subtitle)}</p>
    </div>
    ${renderHeaderContext(item)}
  `;
}

function renderThreadMeta(item) {
  return `<div class="thread-header-meta"><span class="thread-date">${escapeHtml(item.date ?? "date not reported")}</span><button class="inspector-door hallway-record-door" type="button" data-hallway-record>${escapeHtml(item.name)}</button><span>${escapeHtml(hallwayParticipants(item))}</span></div>`;
}

function renderHeaderChip(text) {
  return `<span class="header-chip">${escapeHtml(text)}</span>`;
}

function renderHeaderVerb(label, verb) {
  return `<button class="header-verb" type="button" data-header-verb="${escapeHtml(verb)}">${escapeHtml(label)}</button>`;
}

// one owner for the header's contextual slot: the view's own verb, else its quietest fact
function renderHeaderContext(item) {
  if (state.activeView === "live") {
    if (item.kind === "hallway") return renderThreadMeta(item);
    return renderSessionControl(item);
  }
  if (state.activeView === "state") {
    if (item.kind === "house") return renderHeaderVerb("Local interface settings", "settings");
    if (item.kind === "direct") return "";
    if (item.kind === "hallway") return renderHeaderChip("Read-only Host queries");
    return renderHeaderChip(projectWorkState());
  }
  if (item.kind === "direct") return renderHeaderChip("Memories · read-only");
  if (item.kind === "hallway") return renderHeaderChip("Read-only Hallway record");
  if (item.kind === "project") return renderHeaderChip("Docket receipts · read-only");
  return renderHeaderChip(houseRecordLine());
}

function renderSessionControl(item) {
  if (isLiveChat(item)) return renderHeaderChip(`${chatState().messages.length} messages · ${directChatStatus(item)}`);
  if (item.kind === "project") return renderHeaderChip("No linked sessions reported");
  return "";
}


function renderTimeline(item) {
  const viewKey = `${item.id}:${draftKey(item)}`;
  const sameView = timeline.dataset.chatView === viewKey;
  const scrollTop = timeline.scrollTop;
  const follow = !sameView || timeline.scrollHeight - scrollTop - timeline.clientHeight <= 2;
  const oldRows = sameView ? [...timeline.querySelectorAll("[data-message-key]")] : [];
  const anchor = oldRows.find(row => row.getBoundingClientRect().bottom > timeline.getBoundingClientRect().top);
  const anchorOffset = anchor?.getBoundingClientRect().top;
  timeline.dataset.chatView = viewKey;
  if (item.kind === "hallway") {
    timeline.innerHTML = renderHallwayThread(item);
    return;
  }
  if (item.kind === "direct" && !isLiveChat(item)) {
    const connected = Object.values(conversations).find(candidate => isLiveChat(candidate));
    timeline.innerHTML = renderEmptyState(`Chat with ${item.name} is unavailable here`, [
      connected ? `You can open the connected conversation with ${connected.name}.` : "No connected conversation is available."
    ]) + (connected ? `<button class="inspector-door" type="button" data-open-connected-chat="${escapeHtml(connected.id)}">Open ${escapeHtml(connected.name)} chat</button>` : "");
    return;
  }
  const itemMessages = activeMessages(item);
  const firstLiveIndex = itemMessages.findIndex(message => message.live);
  const messages = itemMessages.map((message, index) => {
    const boundary = index === firstLiveIndex && item.liveBoundary
      ? `<div class="live-boundary" role="separator">${escapeHtml(item.liveBoundary)} · live updates begin</div>`
      : "";
    return boundary + renderMessage(message, index, state.selectedMessageKey === messageIdentity(message, index));
  }).join("");
  let emptyState = "";
  if (itemMessages.length === 0) {
    let headline = "No messages here yet.";
    let reason = "Nothing has been delivered here.";
    if (item.kind === "direct") {
      headline = "No messages yet";
      reason = chatBlockReason(item) ?? `Write to ${item.name} here.`;
    }
    const staleReason = item.connection === "Stale" ? "Stale means there is no current live update stream." : null;
    emptyState = renderEmptyState(headline, [reason, staleReason]);
  }

  if (isLiveChat(item) && itemMessages.length) {
    const template = document.createElement("template");
    template.innerHTML = messages;
    if (!sameView || !oldRows.length) timeline.replaceChildren();
    const existing = new Map(oldRows.map(row => [row.dataset.messageKey, row]));
    const incomingKeys = new Set([...template.content.children].map(row => row.dataset.messageKey));
    for (const [key, row] of existing) {
      if (!incomingKeys.has(key)) {
        row.remove();
        existing.delete(key);
      }
    }
    let previous = null;
    for (const next of template.content.children) {
      const row = existing.get(next.dataset.messageKey);
      const target = row ?? next.cloneNode(true);
      if (row) {
        row.className = next.className;
        row.dataset.messageIndex = next.dataset.messageIndex;
        row.dataset.outcome = next.dataset.outcome;
        row.dataset.author = next.dataset.author;
        for (const selector of [".message-meta", ".message-tools", ".message-bubble"]) {
          const current = row.querySelector(selector);
          const replacement = next.querySelector(selector);
          if (current.innerHTML !== replacement.innerHTML) current.innerHTML = replacement.innerHTML;
        }
        const thinking = row.querySelector(".message-thinking");
        const blocks = next.querySelector(".message-thinking").children;
        for (let position = 0; position < blocks.length; position++) {
          const disclosure = thinking.children[position];
          if (!disclosure) thinking.append(blocks[position].cloneNode(true));
          else {
            const text = blocks[position].querySelector("pre").textContent;
            const body = disclosure.querySelector("pre");
            if (body.textContent !== text) body.textContent = text;
          }
        }
        while (thinking.children.length > blocks.length) thinking.lastElementChild.remove();
        existing.delete(next.dataset.messageKey);
      }
      const following = previous ? previous.nextSibling : timeline.firstChild;
      if (following !== target) timeline.insertBefore(target, following);
      previous = target;
    }
    for (const row of existing.values()) row.remove();
  } else {
    timeline.innerHTML = emptyState + messages;
  }
  if (follow) timeline.scrollTop = timeline.scrollHeight;
  else if (anchor?.isConnected) timeline.scrollTop = scrollTop + anchor.getBoundingClientRect().top - anchorOffset;
  else timeline.scrollTop = scrollTop;
}


function selectedSession(item) {
  return item.presences?.find(presence => presence.id === state.selectedPresenceId) ?? null;
}



function renderProjectSurface(item) {
  timeline.innerHTML = projectMarkup(state.activeView, { renderOverviewHero, renderDurableEntry, renderFactList });
  timeline.scrollTop = 0;
}


function durableControls(item, { query = "", mark = "all", status = "" } = {}) {
  const searchLabel = item.kind === "house" ? "Search memories and lessons" : `Search ${item.name} room memories`;
  const marks = item.kind === "house"
    ? `
      <nav class="shelf-marks" aria-label="Shelf filter">
        ${[["all", "All"], ["Memory", "Memories"], ["Lesson", "Lessons"]].map(([id, label]) => `
          <button type="button" data-durable-mark="${id}" aria-pressed="${String(mark === id)}">${label}</button>`).join("")}
      </nav>`
    : "";
  return `
    <div class="shelf-controls">
      <label class="shelf-search">
        <span>${escapeHtml(searchLabel)}</span>
        <input type="search" data-durable-search value="${escapeHtml(query)}" placeholder="Search loaded records…" autocomplete="off">
      </label>
      ${marks}
      <span class="shelf-status" data-durable-status role="status">${escapeHtml(status)}</span>
    </div>`;
}

function houseRecordLine() {
  const live = liveShelfCounts(conversations.house);
  if (!live) {
    return "House record not queried.";
  }

  const memories = live.columns.memories.status === "live" ? `${live.memories} memories loaded` : `Memories ${live.columns.memories.status}`;
  const lessons = live.columns.lessons?.status === "live" ? `${live.lessons} lessons loaded` : `Lessons ${live.columns.lessons?.status ?? "not queried"}`;
  return `${memories} · ${lessons}`;
}

function renderHouseSurface() {
  const views = {
    live: `
      ${renderHouseBoard()}
      <div class="overview-grid">
        ${renderOverviewHero("House overview", "Solarisael House", "Shared mechanics, memories, lessons, and substrate state. Spirit-room memory stays inside its room.")}
        <section class="specimen-card"><h3>House record</h3><p>${escapeHtml(houseRecordLine())}</p><button type="button" data-open-subject-view="durable">Open the record</button></section>
        <section class="specimen-card"><h3>Mechanical observatory</h3><p>Observed Host, room, delivery, and retention state.</p><button type="button" data-open-subject-view="state">Open mechanics</button></section>
        <section class="specimen-card"><h3>Ownership</h3><p>House commons and room records remain separate.</p></section>
      </div>`,
    state: renderHouseMechanics(),
    durable: renderHouseSediment(conversations.house)
  };
  timeline.innerHTML = views[state.activeView];
  if (state.activeView === "state") renderMechanicsResults();
  if (state.activeView === "durable") ensureSedimentQueried(conversations.house);
  timeline.scrollTop = state.activeView === "state" ? mechanicsScrollTop() : 0;
}

function renderRoomMemories(item) {
  timeline.innerHTML = renderRoomSediment(item);
  ensureSedimentQueried(item);
  timeline.scrollTop = 0;
}

function renderSubjectView(item) {
  if (item.kind !== "direct" || state.activeView !== "state") delete timeline.dataset.stateView;
  if (item.kind === "house") {
    renderHouseSurface();
    return;
  }
  if (item.kind === "project") {
    renderProjectSurface(item);
    return;
  }
  if (state.activeView === "live") {
    renderTimeline(item);
    return;
  }
  if (state.activeView === "durable") {
    if (item.kind === "hallway") renderHallwayRecordView(item);
    else renderRoomMemories(item);
    return;
  }
  renderSubjectState(item);
}




function renderSubjectState(item) {
  if (item.kind !== "direct") {
    timeline.innerHTML = renderHallwayStatus(item);
    return;
  }
  const sameView = timeline.dataset.stateView === item.id;
  const scroll = sameView ? timeline.scrollTop : 0;
  const focused = timeline.contains(document.activeElement) ? document.activeElement : null;
  const focusedId = focused?.id;
  const selection = focused?.selectionStart;
  const selectionEnd = focused?.selectionEnd;
  const openDetails = [...timeline.querySelectorAll("details")].map(node => node.open);
  timeline.innerHTML = renderDirectStatus(item) + (item.room === connection.room ? renderSessions() : "");
  timeline.dataset.stateView = item.id;
  if (sameView) {
    [...timeline.querySelectorAll("details")].forEach((node, index) => { node.open = openDetails[index] ?? false; });
    const restored = focusedId ? document.getElementById(focusedId) : null;
    restored?.focus({ preventScroll: true });
    if (restored && Number.isInteger(selection)) restored.setSelectionRange(selection, selectionEnd);
  }
  timeline.scrollTop = scroll;
  if (item.room === connection.room) ensureSessionsQueried();
}


function renderHallwayRecordView(item) {
  timeline.innerHTML = renderHallwayRecord(item);
  timeline.scrollTop = 0;
}

function renderMemberDock(item) {
  if (item.kind !== "hallway") {
    memberDock.hidden = true;
    memberDock.innerHTML = "";
    return;
  }
  memberDock.hidden = false;
  memberDock.innerHTML = `<span class="eyebrow">Members</span><p>${escapeHtml(hallwayMembers(item))}</p><p>Presence not reported by the Host</p>`;
}

function renderPresenceProfile(item) {
  const presence = item.presences?.find(candidate => candidate.id === state.selectedPresenceId);
  if (!presence || !state.profileAnchor) {
    profileLayer.hidden = true;
    profileLayer.innerHTML = "";
    return;
  }
  const anchor = state.profileAnchor;
  profileLayer.style.setProperty("--profile-top", `${Math.max(56, anchor.top)}px`);
  profileLayer.style.setProperty("--profile-left", `${Math.max(12, anchor.left - 330)}px`);
  profileLayer.hidden = false;
  profileLayer.innerHTML = `
    <section class="presence-profile" role="dialog" aria-label="${escapeHtml(presence.spirit)} ${escapeHtml(presence.session)} profile">
      <header>
        <span class="profile-glyph" aria-hidden="true">${escapeHtml(presence.glyph)}</span>
        <div><strong>${escapeHtml(presence.spirit)}</strong><span>${escapeHtml(presence.session)} · ${escapeHtml(presence.liveness)}</span></div>
        <button type="button" data-close-profile aria-label="Close profile">×</button>
      </header>
      ${renderFactList([
        ["Room", presence.room],
        ["Session ID", presence.sessionId],
        ["Permission", presence.permission],
        ["Activity", presence.activity],
        ["Read position", presence.readPosition],
        ["Context used", presence.contextUsed],
        ["Compaction", presence.compaction],
        ["Recall", presence.recall],
        ["Evidence", presence.evidence]
      ])}
    </section>
  `;
}

function renderInspector(item) {
  const selectedMessage = state.selectedMessageKey === null ? null
    : activeMessages(item).find((message, index) => messageIdentity(message, index) === state.selectedMessageKey);
  if (!selectedMessage) state.selectedMessageKey = null;
  let selection = "";
  if (selectedMessage) {
    const authority = selectedMessage.local ? "Local · undelivered" : "Displayed snapshot";
    selection = `
      <section class="context-card context-selection">
        <h3>Current selection</h3>
        ${renderFactList([
          ["Author", selectedMessage.author],
          ["Time", selectedMessage.time],
          ["Authority", authority]
        ])}
      </section>`;
  }

  let eyebrow = "Room shelf";
  if (item.kind === "house") eyebrow = "House shelf";
  else if (item.kind === "project") eyebrow = "Project shelf";
  inspectorEyebrow.textContent = eyebrow;


  if (item.kind === "house") {
    inspectorTitle.textContent = "Solarisael House";
    inspectorContent.innerHTML = `
      <section class="context-card"><h3>House state</h3>${renderFactList([
        ["Record", houseRecordLine()],
        ["Host", healthSourceLine()],
        ["Selected room", roomState().room ?? "Not loaded"]
      ])}</section>
      ${renderInspectorDoors("Open", [
        ["state", "Mechanics"],
        ["durable", "Memories & Lessons"]
      ])}`;
    return;
  }

  if (item.kind === "project") {
    inspectorTitle.textContent = item.name;
    inspectorContent.innerHTML = `
      <section class="context-card"><h3>Docket state</h3>${renderProjectStatus(renderFactList)}</section>
      ${renderInspectorDoors("Open", [["live", "Overview"], ["state", "Status"], ["durable", "Evidence"]])}`;
    return;
  }

  if (isLiveChat(item)) {
    const chat = chatState();
    inspectorTitle.textContent = `${item.id} room`;
    inspectorContent.innerHTML = `
      <section class="context-card"><h3>Host chat ring</h3>${renderFactList([
        ["Room", item.id],
        ["Source", "Host chat snapshot"],
        ["State", chatBlockReason(item) ?? "Live"],
        ["Settled lines", chat.messages.length],
        ["Open drafts", chat.drafts.length],
        ["Sessions", "Inspect in Status"],
        ["Model", roomState().modelDefault ?? "Not reported"],
        ["Registered presences", roomState().presences?.length ?? "Not reported"]
      ])}</section>
      ${renderInspectorDoors("Navigate", [["state", "Status"], ["durable", "Memories"]])}
      ${selection}`;
    return;
  }
  if (item.kind === "direct") {
    inspectorTitle.textContent = `${item.name} room`;
    inspectorContent.innerHTML = `
      <section class="context-card"><h3>Conversation unavailable</h3>${renderFactList([
        ["Room", item.id],
        ["Connection", "Not connected"],
        ["Reason", chatBlockReason(item)]
      ])}</section>
      ${renderInspectorDoors("Navigate", [["state", "Status"], ["durable", "Memories"]])}`;
    return;
  }

  inspectorTitle.textContent = item.name;
  inspectorContent.innerHTML = `
    <section class="context-card"><h3>Hallway state</h3>${renderFactList([
      ["Room", item.room],
      ["Status", item.connection ?? item.status],
      ["Recall", item.recall]
    ])}</section>
    ${selection}`;
}
function setDrawerView(requestedView, focusTarget = null, moveFocus = true) {
  const requestedIndex = drawerPanes.findIndex(pane => pane.dataset.drawerPane === requestedView);
  const activeIndex = requestedIndex >= 0 ? requestedIndex : 0;
  const activePane = drawerPanes[activeIndex];
  state.drawerView = activePane.dataset.drawerPane;

  drawerPanes.forEach((pane, index) => {
    const isActive = index === activeIndex;
    pane.dataset.drawerPosition = index < activeIndex ? "before" : isActive ? "active" : "after";
    pane.setAttribute("aria-hidden", String(!isActive));
    pane.inert = !isActive;
  });

  sidebarDrawer.querySelectorAll("[data-open-drawer]").forEach(button => {
    button.setAttribute("aria-expanded", String(button.dataset.openDrawer === state.drawerView));
  });

  const focusGeneration = ++drawerFocusGeneration;
  if (!moveFocus) return;
  window.requestAnimationFrame(() => {
    const animations = activePane.getAnimations();
    Promise.allSettled(animations.map(animation => animation.finished)).then(() => {
      if (focusGeneration !== drawerFocusGeneration || state.drawerView !== activePane.dataset.drawerPane) return;
      const target = focusTarget || activePane.querySelector("[data-drawer-focus]");
      target?.focus({ preventScroll: true });
    });
  });
}

function openDrawerView(view, trigger) {
  drawerReturnFocus.set(view, trigger);
  setDrawerView(view);
}

function returnDrawerView() {
  const activeIndex = drawerPanes.findIndex(pane => pane.dataset.drawerPane === state.drawerView);
  if (activeIndex <= 0) return false;
  const currentView = state.drawerView;
  const previousView = drawerPanes[activeIndex - 1].dataset.drawerPane;
  setDrawerView(previousView, drawerReturnFocus.get(currentView));
  return true;
}

function syncMobileSidebarAccessibility() {
  const concealed = mobileLayout.matches && shell.dataset.sidebarOpen !== "true";
  sidebar.inert = concealed;
  sidebar.setAttribute("aria-hidden", String(concealed));
}

function syncMemberDockAccessibility() {
  const item = conversations[state.activeId];
  const mobileHallway = item.kind === "hallway" && mobileLayout.matches;
  const concealed = item.kind !== "hallway" || (mobileHallway && !state.memberDockOpen);
  shell.dataset.memberDockOpen = String(mobileHallway && state.memberDockOpen);
  memberDock.inert = concealed;
  memberDock.setAttribute("aria-hidden", String(concealed));
  memberDockScrim.hidden = !mobileHallway || !state.memberDockOpen;
  memberDockScrim.inert = memberDockScrim.hidden;
  if (item.kind === "hallway") {
    inspectorToggle.hidden = !mobileLayout.matches;
    inspectorToggle.setAttribute("aria-pressed", String(mobileHallway && state.memberDockOpen));
  }
}

function setMemberDockOpen(open) {
  state.memberDockOpen = Boolean(open);
  syncMemberDockAccessibility();
}

function setMobileSidebarOpen(open) {
  shell.dataset.sidebarOpen = String(open);
  sidebarToggle.setAttribute("aria-pressed", String(open));
  syncMobileSidebarAccessibility();
}

function closeMobileSidebar() {
  drawerFocusGeneration += 1;
  setMobileSidebarOpen(false);
  sidebarToggle.focus();
}

function revealActiveViewButton(button) {
  if (!button) return;
  button.scrollIntoView({ block: "nearest", inline: "nearest" });
  const container = button.parentElement;
  const buttonRect = button.getBoundingClientRect();
  const containerRect = container.getBoundingClientRect();
  if (buttonRect.right > containerRect.right) {
    container.scrollLeft += Math.ceil(buttonRect.right - containerRect.right) + 1;
  } else if (buttonRect.left < containerRect.left) {
    container.scrollLeft -= Math.ceil(containerRect.left - buttonRect.left) + 1;
  }
}



function activeInstrument(item) {
  if (item.kind === "house") {
    return state.activeView === "live" ? "overview" : "work";
  }
  if (item.kind === "project" && state.activeView === "live") {
    return "overview";
  }
  return state.activeView === "live" ? "chat" : "work";
}


function subjectViewLabels(item) {
  const labels = [...SUBJECT_VIEW_LABELS[item.kind]];
  return labels;
}


function render() {
  const repairBanner = document.querySelector("[data-repair-banner]");
  repairBanner.hidden = healthRoundStatus() !== "failed" && connection.status !== "failed";
  if (!repairBanner.hidden) {
    const reason = connection.status === "failed" ? connection.reason : healthSourceLine();
    const repair = connection.capabilities.repair ? renderRepair() : "";
    const markup = `<span>${escapeHtml(reason)}</span> ${repair}`;
    if (repairBanner.innerHTML !== markup) repairBanner.innerHTML = markup;
  }
  const connectedRoom = roomState();
  if (connectedRoom.status === "live" && connectedRoom.room === connection.room && connectedRoom.spirit) {
    const item = conversations[connection.room];
    if (item?.kind === "direct") {
      item.name = connectedRoom.spirit;
      item.glyph = connectedRoom.spirit.slice(0, 1);
    }
  }
  const item = conversations[state.activeId];
  const instrument = activeInstrument(item);
  if (instrument !== "chat" || item.kind === "hallway") delete timeline.dataset.chatView;
  state.mode = item.kind;
  shell.dataset.activeId = item.id;
  shell.dataset.subjectKind = item.kind;
  shell.dataset.instrument = instrument;
  shell.dataset.chatRing = isLiveChat(item) ? `host-chat:${item.id}` : "";
  shell.dataset.selectedPresenceId = state.selectedPresenceId ?? "";
  shell.dataset.hallway = String(item.kind === "hallway");
  shell.dataset.memberDockOpen = String(state.memberDockOpen);
  shell.dataset.density = state.density;
  shell.dataset.textScale = state.textScale;
  shell.dataset.measure = state.measure;
  shell.dataset.contrast = state.contrast;
  shell.dataset.reducedMotion = String(state.reducedMotion);
  shell.dataset.timestamps = String(state.timestamps);
  shell.dataset.statusVisible = String(state.statusVisible);
  inspectorToggle.hidden = item.kind === "hallway" && !mobileLayout.matches;
  houseDoor.setAttribute("aria-pressed", String(item.kind === "house"));
  document.querySelectorAll(".mode-button").forEach(button => {
    const active = button.dataset.mode === state.mode;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
  });
  const viewLabels = subjectViewLabels(item);
  let activeViewButton = null;
  subjectViewButtons.forEach((button, index) => {
    const active = button.dataset.subjectView === state.activeView;
    button.classList.toggle("is-active", active);
    button.setAttribute("aria-pressed", String(active));
    button.querySelector("[data-view-label]").textContent = viewLabels[index];
    if (active) activeViewButton = button;
  });
  composer.hidden = instrument !== "chat" || (item.kind === "direct" && !isLiveChat(item));
  const activeViewIndex = SUBJECT_VIEWS.indexOf(state.activeView);
  timeline.setAttribute("aria-label", item.kind === "house" || item.kind === "project" ? `${item.name} ${viewLabels[activeViewIndex]} view` : state.activeView === "live" ? "Message timeline" : `${item.name} ${viewLabels[activeViewIndex]} view`);
  renderConversationList();
  renderHeader(item);
  renderSubjectView(item);
  renderInspector(item);
  renderMemberDock(item);
  renderPresenceProfile(item);
  renderBellToggle();
  renderStatusStrip();
  renderAccountState();
  if (state.switcherOpen) renderSwitcher();
  syncChatPanel(item, state.activeView);
  updateComposerState();
  syncMobileSidebarAccessibility();
  syncMemberDockAccessibility();
  window.requestAnimationFrame(() => revealActiveViewButton(activeViewButton));
}

function leaveHouse() {
  const previous = state.houseReturn;
  if (!previous) return;
  state.houseReturn = null;
  state.activeView = previous.activeView;
  openConversation(previous.activeId);
  window.requestAnimationFrame(() => houseDoor.focus({ preventScroll: true }));
}

function toggleHouse() {
  if (state.activeId === "house") {
    leaveHouse();
    return;
  }
  state.houseReturn = { mode: state.mode, activeId: state.activeId, activeView: state.activeView };
  state.activeView = "live";
  openConversation("house");
  ensureBoardQueried();
}




houseDoor.addEventListener("click", toggleHouse);
conversationList.addEventListener("click", event => {
  const button = event.target.closest("[data-conversation]");
  if (button) openConversation(button.dataset.conversation);
});



function openMode(mode) {
  if (mode === "hallway") {
    const rows = syncHallwaySubjects(conversations);
    openConversation(rows[0]?.id ?? hallwayEmptySubject.id);
    if (!conversations[state.activeId].hallwayId) queryHallway();
    return;
  }
  if (state.activeId === "house") state.houseReturn = null;
  state.mode = mode;
  const first = visibleConversations()[0];
  if (first) openConversation(first.id);
}

document.querySelector(".mode-switcher").addEventListener("click", event => {
  const button = event.target.closest("[data-mode]");
  if (button) openMode(button.dataset.mode);
});

document.querySelector(".subject-views").addEventListener("click", event => {
  const button = event.target.closest("[data-subject-view]");
  if (!button) return;
  openSubjectView(button.dataset.subjectView);
});

inspectorContent.addEventListener("click", event => {
  const button = event.target.closest("[data-inspector-view]");
  if (!button) return;
  openSubjectView(button.dataset.inspectorView, { clearMessage: true });
});




function openInterfaceSettings(trigger) {
  setDrawerView("settings", trigger);
  setMobileSidebarOpen(true);
}

header.addEventListener("click", event => {
  const item = conversations[state.activeId];
  const verb = event.target.closest("[data-header-verb]");
  if (verb) {
    if (verb.dataset.headerVerb === "settings") openInterfaceSettings(verb);
    return;
  }
  if (event.target.closest("[data-hallway-record]")) {
    openSubjectView("durable");
    return;
  }
});


memberDock.addEventListener("click", event => {
  const button = event.target.closest("[data-presence-id]");
  if (!button) return;
  const closingSameProfile = state.selectedPresenceId === button.dataset.presenceId;
  state.selectedPresenceId = closingSameProfile ? null : button.dataset.presenceId;
  state.profileReturnId = closingSameProfile ? null : button.dataset.presenceId;
  state.profileAnchor = closingSameProfile ? null : button.getBoundingClientRect();
  state.selectedMessageKey = null;
  render();
});

function closePresenceProfile(restoreFocus = true) {
  const returnId = state.profileReturnId;
  state.selectedPresenceId = null;
  state.profileAnchor = null;
  state.profileReturnId = null;
  render();
  if (!restoreFocus || !returnId) return;
  window.requestAnimationFrame(() => memberDock.querySelector(`[data-presence-id="${returnId}"]`)?.focus({ preventScroll: true }));
}

profileLayer.addEventListener("click", event => {
  if (event.target.closest("[data-close-profile]")) closePresenceProfile();
});


timeline.addEventListener("click", event => {
  if (handleSessionsClick(event)) return;
  if (event.target.closest(".message-thinking")) return;
  const item = conversations[state.activeId];
  const connectedChat = event.target.closest("[data-open-connected-chat]");
  if (connectedChat) {
    const connected = conversations[connectedChat.dataset.openConnectedChat];
    if (connected && isLiveChat(connected)) {
      navigateToSubjectView(connected.id, "live");
      input.focus({ preventScroll: true });
    }
    return;
  }
  if (item.kind === "project" && event.target.closest("[data-project-refresh]")) {
    queryProjects(state.activeView);
    return;
  }
  if (item.kind === "hallway" && event.target.closest("[data-hallway-query]")) {
    queryHallway(item);
    render();
    return;
  }
  if (handleBoardClick(event)) return;
  if (handlePulseClick(event)) return;
  if (handleMechanicsClick(event)) return;
  if (handleSedimentClick(event)) return;
  const subjectDoor = event.target.closest("[data-open-subject-view]");
  if (subjectDoor) {
    openSubjectView(subjectDoor.dataset.openSubjectView);
    return;
  }
  if (event.target.closest("[data-access-picker]")) {
    state.accessPicker = !state.accessPicker;
    render();
    return;
  }
  const interfaceSettings = event.target.closest("[data-open-interface-settings]");
  if (interfaceSettings) {
    openInterfaceSettings(interfaceSettings);
    return;
  }
  const message = event.target.closest("[data-message-key]");
  if (!message) return;
  state.selectedMessageKey = message.dataset.messageKey;
  state.selectedPresenceId = null;
  shell.dataset.inspectorOpen = "true";
  inspectorToggle.setAttribute("aria-pressed", "true");
  render();
});

timeline.addEventListener("input", event => {
  if (handleSessionsChange(event)) return;
  if (handleSedimentInput(event)) return;
  handleMechanicsInput(event);
});
timeline.addEventListener("change", handleSessionsChange);

timeline.addEventListener("keydown", event => {
  if (event.key !== "Enter" && event.key !== " ") return;
  if (!event.target.matches(".message-bubble")) return;
  const message = event.target.closest("[data-message-key]");
  if (!message) return;
  event.preventDefault();
  state.selectedMessageKey = message.dataset.messageKey;
  state.selectedPresenceId = null;
  shell.dataset.inspectorOpen = "true";
  inspectorToggle.setAttribute("aria-pressed", "true");
  render();
});


composer.addEventListener("submit", async event => {
  event.preventDefault();
  const item = conversations[state.activeId];
  const text = input.value.trim();
  if (composerBlockReason(item) !== null) {
    updateComposerState();
    return;
  }
  if (!text) {
    updateComposerState();
    return;
  }
  if (isLiveChat(item)) {
    const key = draftKey(item);
    const accepted = await say(text);
    if (accepted) {
      state.drafts.set(key, "");
      if (draftKey(conversations[state.activeId]) === key && input.value.trim() === text) {
        input.value = "";
        input.style.height = "auto";
      }
    }
    updateComposerState();
    return;
  }
});

input.addEventListener("input", () => {
  input.style.height = "auto";
  input.style.height = `${Math.min(input.scrollHeight, 150)}px`;
  updateComposerState();
});

input.addEventListener("keydown", event => {
  if (event.key !== "Enter" || event.shiftKey) return;
  if (!state.sendWithEnter && !event.ctrlKey && !event.metaKey) return;
  event.preventDefault();
  if (!sendButton.disabled) composer.requestSubmit();
});

clearButton.addEventListener("click", () => {
  const item = conversations[state.activeId];
  input.value = "";
  input.style.height = "auto";
  state.drafts.set(draftKey(item), "");
  updateComposerState();
  input.focus({ preventScroll: true });
});


document.addEventListener("keydown", event => {
  if (!(event.ctrlKey || event.metaKey) || event.altKey || event.code !== "Space") return;
  event.preventDefault();
  if (state.switcherOpen) closeSwitcher();
  else openSwitcher();
});

switcherInput.addEventListener("input", () => {
  state.switcherQuery = switcherInput.value;
  state.switcherIndex = 0;
  renderSwitcher();
});

switcherInput.addEventListener("keydown", event => {
  if (event.key === "Tab") {
    event.preventDefault();
    return;
  }
  if (event.key === "Escape") {
    event.preventDefault();
    event.stopPropagation();
    closeSwitcher();
    return;
  }
  if (event.key === "ArrowDown" || event.key === "ArrowUp") {
    event.preventDefault();
    event.stopPropagation();
    if (switcherMatches.length === 0) return;
    const direction = event.key === "ArrowDown" ? 1 : -1;
    state.switcherIndex = (state.switcherIndex + direction + switcherMatches.length) % switcherMatches.length;
    renderSwitcher();
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    event.stopPropagation();
    const command = switcherMatches[state.switcherIndex];
    if (command) executeSwitcherCommand(command.id);
  }
});

switcherResults.addEventListener("click", event => {
  const result = event.target.closest("[data-switcher-command]");
  if (!result) return;
  executeSwitcherCommand(result.dataset.switcherCommand);
});

switcherLayer.addEventListener("click", event => {
  if (event.target === switcherLayer) closeSwitcher();
});

bellToggle.addEventListener("click", () => {
  if (state.bellOpen) closeBell();
  else openBell();
});

bellLayer.addEventListener("click", event => {
  if (event.target.closest("[data-bell-board]")) {
    openBoardFromBell(event.target.closest("[data-bell-board]").dataset.bellBoard);
    return;
  }
  if (event.target === bellLayer || event.target.closest("[data-close-bell]")) closeBell();
});

bellLayer.addEventListener("keydown", event => {
  if (event.key !== "Tab") return;
  const controls = [...bellLayer.querySelectorAll("button:not([disabled])")];
  if (controls.length === 0) return;
  const first = controls[0];
  const last = controls.at(-1);
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
});

document.addEventListener("keydown", event => {
  if (state.bellOpen || event.ctrlKey || event.metaKey || event.altKey || event.target.matches("input, textarea, select")) return;
  const view = SUBJECT_VIEWS[Number(event.key) - 1];
  if (!view) return;
  event.preventDefault();
  openSubjectView(view);
});

const MODE_ORDER = ["direct", "hallway", "project"];

document.addEventListener("keydown", event => {
  if (state.bellOpen || !(event.ctrlKey || event.metaKey) || event.altKey || event.target.matches("input, textarea, select")) return;
  if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
    const target = MODE_ORDER[MODE_ORDER.indexOf(state.mode) + (event.key === "ArrowRight" ? 1 : -1)];
    if (!target) return;
    event.preventDefault();
    openMode(target);
    return;
  }
  if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
  const rows = visibleConversations();
  const current = rows.findIndex(item => item.id === state.activeId);
  if (current === -1) return;
  const target = rows[current + (event.key === "ArrowDown" ? 1 : -1)];
  if (!target) return;
  event.preventDefault();
  openConversation(target.id);
});

function setInspector(open) {
  shell.dataset.inspectorOpen = String(open);
  inspectorToggle.setAttribute("aria-pressed", String(open));
}

inspectorToggle.addEventListener("click", () => {
  const item = conversations[state.activeId];
  if (item.kind === "hallway" && mobileLayout.matches) {
    const open = !state.memberDockOpen;
    setMemberDockOpen(open);
    if (open) {
      window.requestAnimationFrame(() => memberDock.querySelector("button")?.focus({ preventScroll: true }));
    }
    return;
  }
  setInspector(shell.dataset.inspectorOpen !== "true");
});
memberDockScrim.addEventListener("click", () => {
  setMemberDockOpen(false);
  inspectorToggle.focus();
});
document.querySelector(".inspector-close").addEventListener("click", () => setInspector(false));

sidebarToggle.addEventListener("click", () => {
  setMobileSidebarOpen(shell.dataset.sidebarOpen !== "true");
});

sidebarDrawer.addEventListener("click", event => {
  const mechanicsDoor = event.target.closest("[data-open-house-mechanics]");
  if (mechanicsDoor) {
    openHouseMechanics();
    return;
  }
  const backButton = event.target.closest("[data-drawer-back]");
  if (backButton) {
    returnDrawerView();
    return;
  }

  const viewButton = event.target.closest("[data-open-drawer]");
  if (viewButton) {
    openDrawerView(viewButton.dataset.openDrawer, viewButton);
    return;
  }
});

document.querySelectorAll("[data-setting]").forEach(control => {
  control.addEventListener("change", event => {
    const setting = event.target.dataset.setting;
    const value = event.target.type === "checkbox" ? event.target.checked : event.target.value;
    const stateKey = SETTING_STATE_KEYS[setting];
    if (!stateKey) return;
    state[stateKey] = value;
    preferences.problem = writePreferences(state);
    renderPreferenceStatus();
    if (setting === "status-visible") statusPopover.hidden = true;
    render();
  });
});

document.addEventListener("keydown", event => {
  if (event.key !== "Escape") return;
  if (state.bellOpen) {
    event.preventDefault();
    closeBell();
    return;
  }
  if (state.switcherOpen) {
    event.preventDefault();
    closeSwitcher();
    return;
  }
  if (!profileLayer.hidden) {
    event.preventDefault();
    closePresenceProfile();
    return;
  }
  if (returnDrawerView()) {
    event.preventDefault();
    return;
  }
  if (mobileLayout.matches && state.memberDockOpen) {
    event.preventDefault();
    setMemberDockOpen(false);
    inspectorToggle.focus();
    return;
  }
  if (mobileLayout.matches && shell.dataset.sidebarOpen === "true") {
    event.preventDefault();
    closeMobileSidebar();
    return;
  }
  if (state.activeId === "house") {
    event.preventDefault();
    leaveHouse();
    return;
  }
});

document.addEventListener("click", event => {
  // A node this click already removed — the popover's own re-query verb, once a
  // fresh round re-renders it — is not a click outside the strip.
  if (!event.target.isConnected) return;
  if (!event.target.closest(".status-bar")) {
    statusPopover.hidden = true;
    document.querySelectorAll("[data-status]").forEach(button => button.setAttribute("aria-expanded", "false"));
  }
});

// The chip text, its narrow twin, and the accessible name all come from the one
// health round, so a chip can never read connected while its label says off.
function renderStatusStrip() {
  STATUS_CHANNELS.forEach(name => {
    const button = document.querySelector(`[data-status="${name}"]`);
    const channel = statusChannel(name);
    button.querySelector(".status-full").textContent = channel.full;
    button.querySelector(".status-compact").textContent = channel.compact;
    button.setAttribute("aria-label", channel.full);
    button.dataset.tone = channel.tone;
    button.querySelector(".status-dot")?.setAttribute("data-tone", channel.tone);
  });

  // A re-query fired from inside the popover must move the popover too, or its
  // detail would keep speaking for a round the strip has already replaced.
  const expanded = document.querySelector('[data-status][aria-expanded="true"]');
  if (!expanded || statusPopover.hidden) return;
  const held = statusPopover.contains(document.activeElement);
  statusPopover.innerHTML = statusPopoverMarkup(expanded.dataset.status);
  if (held) statusPopover.querySelector("[data-status-requery]")?.focus({ preventScroll: true });
}

// The block carries its own source line, and the Host's insula reading is spoken
// in full underneath rather than left buried in the round.
function renderAccountState() {
  const operator = roomState().operator ?? "Operator not reported";
  for (const node of document.querySelectorAll("[data-operator-name]")) node.textContent = operator;
  for (const node of document.querySelectorAll("[data-operator-glyph]")) node.textContent = roomState().operator?.slice(0, 1) ?? "·";
  const detail = persistenceDetail();
  accountState.innerHTML = accountStateRows()
    .map(row => `<div><span>${escapeHtml(row.label)}</span><strong>${escapeHtml(row.value)}</strong></div>`)
    .join("")
    + `<p class="account-state-source" data-tone="${escapeHtml(healthSourceTone())}">${escapeHtml(healthSourceLine())}</p>`
    + (detail ? `<p class="account-state-note">${escapeHtml(detail)}</p>` : "");
}

// Every popover ends with the source line, so the reason for an unreachable
// Host is one click from the chip that reports it.
function statusPopoverMarkup(name) {
  const [title, detail] = name === "about"
    ? ABOUT_STATUS
    : [statusChannel(name).full, statusChannel(name).detail];
  return `<h2>${escapeHtml(title)}</h2><p>${escapeHtml(detail)}</p>`
    + `<p class="status-popover-source" data-tone="${escapeHtml(healthSourceTone())}">${escapeHtml(healthSourceLine())}</p>`
    + `<button class="card-verb" type="button" data-status-requery>Query Host</button>`;
}

document.querySelector(".status-bar").addEventListener("click", event => {
  if (event.target.closest("[data-status-requery]")) {
    queryHealthHost();
    return;
  }
  const button = event.target.closest("[data-status]");
  if (!button) return;
  const wasOpen = button.getAttribute("aria-expanded") === "true" && !statusPopover.hidden;
  document.querySelectorAll("[data-status]").forEach(item => item.setAttribute("aria-expanded", "false"));
  if (wasOpen) {
    statusPopover.hidden = true;
    return;
  }
  button.setAttribute("aria-expanded", "true");
  statusPopover.innerHTML = statusPopoverMarkup(button.dataset.status);
  statusPopover.hidden = false;
});
const subjectViews = document.querySelector(".subject-views");
const subjectViewResizeObserver = new ResizeObserver(() => {
  const activeButton = subjectViews.querySelector('[aria-pressed="true"]');
  window.requestAnimationFrame(() => revealActiveViewButton(activeButton));
});
subjectViewResizeObserver.observe(subjectViews);

const overlayInspectorLayout = window.matchMedia("(max-width: 1050px)");
const mobileLayout = window.matchMedia("(max-width: 700px)");
if (overlayInspectorLayout.matches) setInspector(false);
overlayInspectorLayout.addEventListener("change", event => {
  if (event.matches) setInspector(false);
});
mobileLayout.addEventListener("change", event => {
  if (event.matches) {
    setMobileSidebarOpen(false);
    state.memberDockOpen = false;
  }
  syncMobileSidebarAccessibility();
  syncMemberDockAccessibility();
});
setDrawerView("root", null, false);
updateComposerState();


initPulse({ requestRender: render });
initBoard({ requestRender: () => {
  const rows = syncHallwaySubjects(conversations);
  if (state.activeId === hallwayEmptySubject.id && rows.length) {
    openConversation(rows[0].id);
    return;
  }
  render();
} });
initSediment({
  requestRender: render,
  renderLead: renderSpecimenLead,
  renderShelfControls: durableControls
});
initMechanics({ timeline });
initHealth({ requestRender: render });
initRepair({ requestRender: render });
initSessions({ requestRender: render });
document.querySelector("[data-repair-banner]").addEventListener("click", event => {
  const button = event.target.closest("[data-repair]");
  if (!button) return;
  if (button.dataset.repair === "status") void queryRepairStatus();
  else void startRepair(button.dataset.repair === "service");
});
initChat({ requestRender: render });
render();

function renderPreferenceStatus() {
  const notice = document.querySelector("[data-preference-status]");
  notice.textContent = preferences.problem ?? "Saved on this device";
}

async function connectRooms() {
  try {
    const result = await askHost("/local/rooms");
    if (!Array.isArray(result.rooms) || !result.rooms.every(room => typeof room === "string") ||
        !result.rooms.includes(result.defaultRoom)) {
      throw new Error("The local proxy returned an invalid room list");
    }
    const requested = new URLSearchParams(location.search).get("room") ?? result.defaultRoom;
    if (!result.rooms.includes(requested)) throw new Error("This room is not configured on this device");
    for (const room of result.rooms) {
      if (room === "house") continue;
      Object.defineProperty(conversations, room, { enumerable: true, configurable: true, writable: true, value: {
        id: room, kind: "direct", name: room, glyph: room.slice(0, 1).toUpperCase(),
        room, description: "Configured room"
      } });
    }
    if (!conversations[requested] || conversations[requested].kind !== "direct") {
      throw new Error("The configured default is not a conversation room");
    }
    state.activeId = requested;
    delete conversations.connecting;
    connection = { ...result, room: requested, status: "live", capabilities: result.capabilities ?? { repair: false } };
    render();
    ensureHealthQueried();
  } catch (error) {
    connection = { ...connection, status: "failed", reason: error.message };
    if (conversations.connecting) conversations.connecting.description = error.message;
    render();
  }
}

for (const control of document.querySelectorAll("[data-setting]")) {
  const value = state[SETTING_STATE_KEYS[control.dataset.setting]];
  if (control.type === "checkbox") control.checked = value;
  else control.value = value;
}
renderPreferenceStatus();
void connectRooms();
// A Host that comes back is asked for every open door again: room state and
// health ride the reconnect round itself, chat asks inside chat.js, and the
// House board and Pulse ask here. Nothing is re-sent.
onHostRecovered(() => {
  ensureBoardQueried();
  recoverPulse();
  render();
});
