// Board — the Docket instrument in House slot 1.
//
// Owns three read-only live wires to this room's Host panel door, all through
// the serve.ts proxy so no credential ever reaches this page: the quest board,
// the Hallway inbox, and one evidence drawer per quest row. It holds no stamped
// snapshot, so it has nothing to fall back to and does not pretend otherwise:
// when a door refuses, the surface names which door refused and why, and
// renders no rows at all.
// An empty board is an empty board; a missing door is a missing door.

import { escapeHtml } from "../text.js";

const BOARD_ROUTE = "/live/docket/board";
const INBOX_ROUTE = "/live/hallway/inbox";
const EVIDENCE_ROUTE = "/live/docket/evidence";
const BOARD_LIMIT = 50;
const EVIDENCE_LIMIT = 50;
const POLL_MS = 60000;
const REFUSAL_DETAIL_LIMIT = 120;
const LEDGER_LINE_LIMIT = 140;
const KIND_LABEL_LIMIT = 40;

// Board-local source state: idle → pending → answered. An answered round holds
// one result per door, because one door may answer while the other refuses.
// Nothing outside this module reads it; the shell sees markup and a render
// request.
let source = { status: "idle" };
let requestRender = () => {};
let pollTimer = null;

// One evidence round per quest, kept across board re-renders. Both of these
// exist because the 60s poll rebuilds every row: native <details> state dies
// with the markup that carried it, so a drawer the operator is reading would
// slam shut and lose its evidence a minute after they opened it.
const evidence = new Map();
const openDrawers = new Set();

export function initBoard(options) {
  requestRender = options.requestRender;
}

// Slot entry asks again rather than checking a cache: the board moves under
// other rooms' hands between visits.
export function ensureBoardQueried() {
  queryPanelHost();
  armVisiblePoll();
}

export function handleBoardClick(event) {
  if (event.target.closest("[data-board-refresh]")) {
    queryPanelHost();
    return true;
  }

  const drawerToggle = event.target.closest("[data-evidence-toggle]");
  if (drawerToggle) return toggleEvidenceDrawer(drawerToggle.closest("[data-evidence-drawer]"));

  return false;
}

// The click arrives before the browser flips <details>, so !drawer.open reads
// as "about to open". Nothing here calls preventDefault: the disclosure itself
// stays the browser's to run, and the fetch only rides along.
function toggleEvidenceDrawer(drawer) {
  const questId = drawer?.dataset.evidenceDrawer;
  if (!questId) return false;

  if (drawer.open) {
    openDrawers.delete(questId);
    return true;
  }

  openDrawers.add(questId);
  if (evidence.get(questId)?.status !== "pending") askEvidenceDoor(questId);
  return true;
}

// # enough: 60s visible poll; WS push is the production path
function armVisiblePoll() {
  if (pollTimer !== null) return;

  pollTimer = window.setInterval(() => {
    if (!document.querySelector("[data-board-surface]")) {
      window.clearInterval(pollTimer);
      pollTimer = null;
      return;
    }
    if (document.visibilityState === "visible") queryPanelHost();
  }, POLL_MS);
}

async function queryPanelHost() {
  if (source.status === "pending") return;

  source = { status: "pending" };
  requestRender();

  const [board, hallways] = await Promise.all([
    askDoor(BOARD_ROUTE, { limit: BOARD_LIMIT }),
    askDoor(INBOX_ROUTE, {})
  ]);

  source = {
    status: "answered",
    queriedAt: new Date().toTimeString().slice(0, 5),
    board,
    hallways
  };
  requestRender();
}

// The evidence door is asked per open drawer, never with the board round: a
// board of fifty quests would otherwise pull fifty full receipt bodies nobody
// asked to read.
async function askEvidenceDoor(questId) {
  evidence.set(questId, { status: "pending" });
  paintEvidenceDrawer(questId);

  const answer = await askDoor(EVIDENCE_ROUTE, { questId, limit: EVIDENCE_LIMIT });

  evidence.set(questId, { status: "answered", readAt: new Date().toTimeString().slice(0, 5), ...answer });
  paintEvidenceDrawer(questId);
}

// Painted into the live drawer by hand instead of through requestRender: a full
// re-render would rebuild the row and shut the disclosure the operator just
// opened. The match walks the nodes rather than building a selector, because a
// questId reaching querySelector as text is a selector-injection door.
function paintEvidenceDrawer(questId) {
  const body = [...document.querySelectorAll("[data-evidence-body]")]
    .find(node => node.dataset.evidenceBody === questId);
  if (body) body.innerHTML = evidenceMarkup(questId);
}

// One door, one answer: the parsed payload, or the exact reason it refused.
async function askDoor(route, body) {
  try {
    const response = await fetch(route, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body)
    });
    const text = await response.text();
    if (!response.ok) return { refusal: describeRefusal(response.status, text) };

    const payload = parseJson(text);
    if (payload === undefined) return { refusal: `HTTP ${response.status} · answer was not JSON` };

    return { data: payload };
  } catch (error) {
    return { refusal: error instanceof Error ? error.message : "no route to Host" };
  }
}

// A refusal body may be typed JSON from the panel, an empty axum 404, or proxy
// prose. All three are evidence; this is the one place that untangles them.
function describeRefusal(status, text) {
  const typed = parseJson(text);
  const detail = typeof typed?.error === "string" ? typed.error : text.trim().slice(0, REFUSAL_DETAIL_LIMIT);

  return detail ? `HTTP ${status} · ${detail}` : `HTTP ${status}`;
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch {
    return undefined;
  }
}

function boardCount(value) {
  // Number() first: a Host count never reaches markup as caller text.
  return Number(value ?? 0).toLocaleString("en-US");
}

function countedNoun(value, singular) {
  return `${boardCount(value)} ${Number(value) === 1 ? singular : `${singular}s`}`;
}

function stateTone(state) {
  if (state === "blocked" || state === "quarantined" || state === "refused") return "attention";
  if (state === "offered" || state === "claimed" || state === "submitted") return "steady";

  return "quiet";
}

function importanceTone(importance) {
  return importance === "blocker" ? "attention" : "quiet";
}

function acceptanceTone(notMet, pending) {
  if (notMet > 0) return "attention";
  if (pending > 0) return "quiet";

  return "steady";
}

// The Host sends RFC 3339 UTC; the label stays UTC so no row claims a timezone
// the Host never sent.
function deadlineLabel(value) {
  return `${value.slice(0, 10)} ${value.slice(11, 16)} UTC`;
}

function deadlineProximity(value, now) {
  const due = Date.parse(value);
  if (Number.isNaN(due)) return { tone: "quiet", text: "deadline unreadable" };

  const hours = (due - now) / 3600000;
  if (hours < 0) return { tone: "attention", text: `overdue ${spanLabel(-hours)}` };
  if (hours < 24) return { tone: "attention", text: `due in ${spanLabel(hours)}` };
  if (hours < 72) return { tone: "steady", text: `due in ${spanLabel(hours)}` };

  return { tone: "quiet", text: `due in ${spanLabel(hours)}` };
}

function spanLabel(hours) {
  if (hours < 1) return `${Math.round(hours * 60)} min`;
  if (hours < 48) return `${Math.round(hours)} h`;

  return `${Math.round(hours / 24)} days`;
}

// One line means one line: the first line, flattened and clipped. The body
// below keeps the whole text, so nothing is lost by refusing to let a
// 650-byte receipt wear a row.
function oneLine(value, limit) {
  const text = String(value ?? "").replace(/\s+/g, " ").trim();
  if (text.length <= limit) return text;

  return `${text.slice(0, limit - 1).trimEnd()}…`;
}

// Same UTC grammar the deadline uses, guarded: an absent or short stamp is
// named as absent rather than sliced into a half date.
function ledgerStamp(value) {
  if (typeof value !== "string" || value.length < 16) return "no timestamp";

  return deadlineLabel(value);
}

function verdictTone(verdict) {
  if (verdict === "not_met") return "attention";
  if (verdict === "met") return "steady";

  return "quiet";
}

function isSettledVerdict(item) {
  return typeof item.verdict === "string" && item.verdict !== "pending";
}

// docket.quest_receipts.kind is free TEXT with no CHECK (0023_docket.sql:442)
// and quest_report passes any caller string straight through, so only the
// substrate's own two defaults are House words. Anything else renders as what
// it is — a label the caller chose — because a receipt may not borrow authority
// its kind never had.
const SUBSTRATE_RECEIPT_KINDS = new Map([
  ["progress", "Progress"],
  ["submission", "Submission"]
]);

function receiptKind(value) {
  const raw = typeof value === "string" ? value.trim() : "";
  if (!raw) return { label: "no kind recorded", callerNamed: true };

  const house = SUBSTRATE_RECEIPT_KINDS.get(raw);
  if (house) return { label: house, callerNamed: false };

  return { label: oneLine(raw, KIND_LABEL_LIMIT), callerNamed: true };
}

// authored_role is CHECK-constrained to executor|reviewer
// (0023_docket.sql:452-453), so anything else means the row reached this page
// from somewhere the schema does not reach.
function receiptRole(value) {
  return value === "executor" || value === "reviewer" ? value : "role outside the schema";
}

// A verdict's reasoning is not in a receipt: settleItem writes no receipt at
// all (docket.rs:1410-1431), it writes an item_settled ledger event carrying
// the reviewer's body. The criterion row joins the two by position so a verdict
// never renders as a bare word with its evidence somewhere else on the page.
function settlementsByPosition(events) {
  const settlements = new Map();
  for (const event of events) {
    if (event.eventKind !== "item_settled") continue;

    const position = Number(event.detail?.position);
    if (!Number.isInteger(position)) continue;

    settlements.set(position, event);
  }

  return settlements;
}

function renderCriterionRow(item, settlement) {
  const verdict = typeof item.verdict === "string" ? item.verdict : "";
  const settledBy = item.settledByRoom
    ? `${item.settledByRoom} · ${item.settledBySpirit ?? "unnamed spirit"}`
    : "unsettled";
  const reasoning = typeof settlement?.detail?.body === "string" ? settlement.detail.body : "";
  const criterion = typeof item.criterion === "string" ? item.criterion : "";

  return `
        <details class="mechanics-row evidence-row">
          <summary>
            <span class="mechanics-row-title"><strong>${escapeHtml(oneLine(criterion, LEDGER_LINE_LIMIT) || "no criterion text")}</strong><small>${escapeHtml(`criterion ${boardCount(item.position)} · ${settledBy}`)}</small></span>
            <span class="mechanics-row-value"><small>Settled</small><code>${escapeHtml(ledgerStamp(settlement?.createdAt))}</code></span>
            <span class="mechanics-row-flags"><span data-tone="${verdictTone(verdict)}">${escapeHtml(verdict || "no verdict recorded")}</span></span>
          </summary>
          <div class="mechanics-row-body">
            ${criterion ? `<p>${escapeHtml(criterion)}</p>` : absence("The Host sent this acceptance item without criterion text.")}
            ${reasoning ? `<p>${escapeHtml(reasoning)}</p>` : absence("No settlement reasoning stands in the ledger for this criterion.")}
          </div>
        </details>`;
}

// Collapsed first, always: kind, role, time, one line. The row carries no tone
// of its own — only a settled verdict earns one — so a receipt cannot look like
// a finding just by being long and confident.
function renderReceiptRow(receipt) {
  const kind = receiptKind(receipt.kind);
  const body = typeof receipt.body === "string" ? receipt.body : "";
  const hand = receipt.performedBy ? ` · hand ${receipt.performedBy}` : "";
  const submitted = `${receipt.submittedByRoom ?? "unknown room"} · ${receipt.submittedBySpirit ?? "unknown spirit"}${hand}`;

  const flags = [`<span data-tone="quiet">${escapeHtml(receiptRole(receipt.authoredRole))}</span>`];
  if (kind.callerNamed) flags.push('<span data-tone="attention">caller-named kind</span>');

  return `
        <details class="mechanics-row evidence-row">
          <summary>
            <span class="mechanics-row-title"><strong>${escapeHtml(`${kind.label} — ${oneLine(body, LEDGER_LINE_LIMIT) || "empty body"}`)}</strong><small>${escapeHtml(submitted)}</small></span>
            <span class="mechanics-row-value"><small>Recorded</small><code>${escapeHtml(ledgerStamp(receipt.createdAt))}</code></span>
            <span class="mechanics-row-flags">${flags.join("")}</span>
          </summary>
          <div class="mechanics-row-body">
            ${body ? `<p>${escapeHtml(body)}</p>` : absence("The Host sent this receipt with an empty body.")}
            <dl class="evidence-receipt-detail">
              <div><dt>Receipt</dt><dd><code>${escapeHtml(receipt.receiptId ?? "no id")}</code></dd></div>
              <div><dt>Attempt</dt><dd><code>${escapeHtml(receipt.attemptId ?? "no attempt")}</code></dd></div>
            </dl>
          </div>
        </details>`;
}

function acceptanceStatus(round, items, settlements) {
  const settled = items.filter(isSettledVerdict).length;

  return `${boardCount(settled)} of ${boardCount(items.length)} criteria settled · ${boardCount(settlements.size)} joined to a settlement event · read ${round.readAt} local`;
}

function receiptStatus(receipts, events) {
  return `${countedNoun(receipts.length, "receipt")} · limit ${EVIDENCE_LIMIT} · ${countedNoun(events.length, "ledger event")} in the payload, not dumped here`;
}

function evidenceMarkup(questId) {
  const round = evidence.get(questId);
  if (!round || round.status === "pending") return absence("Reading the evidence door…");
  if (round.refusal) {
    return absence(`The evidence door refused: ${round.refusal}. No verdicts or receipts are shown.`);
  }

  const payload = round.data ?? {};
  const items = Array.isArray(payload.acceptanceItems) ? payload.acceptanceItems : [];
  const receipts = Array.isArray(payload.receipts) ? payload.receipts : [];
  const events = Array.isArray(payload.events) ? payload.events : [];
  const settlements = settlementsByPosition(events);

  return `
      <section class="pulse-block" aria-label="Acceptance verdicts">
        <header class="pulse-block-lead">
          <h4>Acceptance</h4>
          <small>${escapeHtml(acceptanceStatus(round, items, settlements))}</small>
        </header>
        <div class="pulse-rows">${items.length === 0
          ? absence("This quest carries no acceptance items.")
          : items.map(item => renderCriterionRow(item, settlements.get(Number(item.position)))).join("")}</div>
      </section>
      <section class="pulse-block" aria-label="Receipts">
        <header class="pulse-block-lead">
          <h4>Receipts</h4>
          <small>${escapeHtml(receiptStatus(receipts, events))}</small>
        </header>
        <div class="pulse-rows">${receipts.length === 0
          ? absence("No receipt stands against this quest yet.")
          : receipts.map(renderReceiptRow).join("")}</div>
      </section>`;
}

// The drawer is its own disclosure inside the row: opening a quest to read its
// counts must not spend a fetch on receipts nobody asked for.
function renderEvidenceDrawer(questId) {
  if (typeof questId !== "string" || questId === "") {
    return absence("This row carries no questId, so its evidence cannot be asked for.");
  }

  const open = openDrawers.has(questId) ? " open" : "";
  return `
    <details class="evidence-drawer"${open} data-evidence-drawer="${escapeHtml(questId)}">
      <summary data-evidence-toggle>Evidence — acceptance verdicts and receipts</summary>
      <div class="evidence-drawer-body" data-evidence-body="${escapeHtml(questId)}">${evidenceMarkup(questId)}</div>
    </details>`;
}

function acceptanceChip(acceptance) {
  const met = Number(acceptance.met ?? 0);
  const notMet = Number(acceptance.notMet ?? 0);
  const pending = Number(acceptance.pending ?? 0);
  const notApplicable = Number(acceptance.notApplicable ?? 0);
  const total = met + notMet + pending + notApplicable;
  if (total === 0) return '<span data-tone="quiet">no acceptance items</span>';

  // The substrate settles on met and not_applicable; the chip counts what it
  // counts and the row body keeps the four verdicts apart.
  const tone = acceptanceTone(notMet, pending);
  return `<span data-tone="${tone}">${boardCount(met + notApplicable)}/${boardCount(total)} accepted</span>`;
}

function renderQuestRow(quest, now) {
  const acceptance = quest.acceptance ?? {};
  const claim = quest.claimantRoom
    ? `${quest.claimantRoom} · ${quest.attemptState ?? "attempt"} · epoch ${boardCount(quest.claimEpoch)}`
    : "unclaimed";

  const flags = [
    `<span data-tone="${stateTone(quest.state)}">${escapeHtml(quest.state)}</span>`,
    `<span data-tone="${importanceTone(quest.importance)}">${escapeHtml(quest.importance)}</span>`
  ];
  if (quest.deadlineAt) {
    const proximity = deadlineProximity(quest.deadlineAt, now);
    flags.push(`<span data-tone="${proximity.tone}">${escapeHtml(proximity.text)}</span>`);
  }
  flags.push(acceptanceChip(acceptance));

  const deadline = quest.deadlineAt ? deadlineLabel(quest.deadlineAt) : "no deadline";
  const body = quest.body ? `<p>${escapeHtml(quest.body)}</p>` : "";

  // The row reopens itself when its drawer is open: a poll round rebuilds this
  // markup, and a drawer painted inside a closed row would be invisible.
  const reopened = openDrawers.has(quest.questId) ? " open" : "";

  return `
    <details class="mechanics-row"${reopened}>
      <summary>
        <span class="mechanics-row-title"><strong>${escapeHtml(quest.title)}</strong><small>${escapeHtml(quest.kind)} · ${escapeHtml(claim)}</small></span>
        <span class="mechanics-row-value"><small>Deadline</small><code>${escapeHtml(deadline)}</code></span>
        <span class="mechanics-row-flags">${flags.join("")}</span>
      </summary>
      <div class="mechanics-row-body">
        <dl>
          <div><dt>Met</dt><dd>${boardCount(acceptance.met)}</dd></div>
          <div><dt>Not met</dt><dd>${boardCount(acceptance.notMet)}</dd></div>
          <div><dt>Pending</dt><dd>${boardCount(acceptance.pending)}</dd></div>
          <div><dt>Not applicable</dt><dd>${boardCount(acceptance.notApplicable)}</dd></div>
          <div><dt>Quest</dt><dd><code>${escapeHtml(quest.questId)}</code></dd></div>
        </dl>
        ${body}
        ${renderEvidenceDrawer(quest.questId)}
      </div>
    </details>`;
}

function renderHallwayChannel(entry) {
  const counts = `${boardCount(entry.unread)} unread · ${countedNoun(entry.mentions, "mention")}`;
  const excerpt = entry.latestExcerpt
    ? `${entry.latestRoom ?? "unknown room"} · ${(entry.latestCreatedAt ?? "").slice(0, 16) || "no timestamp"} — ${entry.latestExcerpt}`
    : "No messages in this Hallway yet.";

  return `
    <article class="insula-observation-channel">
      <span>${escapeHtml(entry.hallway)}</span>
      <strong>${escapeHtml(counts)}</strong>
      <p>${escapeHtml(excerpt)}</p>
    </article>`;
}

function absence(text) {
  return `<p class="board-absence">${escapeHtml(text)}</p>`;
}

function questRows(now) {
  if (source.status === "idle") return absence("Not queried yet — press Query Host.");
  if (source.status === "pending") return absence("Querying the Host…");
  if (source.board.refusal) {
    return absence(`The docket board door refused: ${source.board.refusal}. No rows are shown.`);
  }

  const quests = source.board.data?.quests ?? [];
  if (quests.length === 0) return absence("The board is empty. An empty board is an empty board.");

  return quests.map(quest => renderQuestRow(quest, now)).join("");
}

function questStatus() {
  if (source.status !== "answered") return `POST ${BOARD_ROUTE} · limit ${BOARD_LIMIT}`;
  if (source.board.refusal) return `POST ${BOARD_ROUTE} · refused`;

  const quests = source.board.data?.quests ?? [];
  return `${countedNoun(quests.length, "quest")} · Host order: deadline first, then posting time · limit ${BOARD_LIMIT}`;
}

function hallwayChannels() {
  if (source.status === "idle") return absence("Not queried yet — press Query Host.");
  if (source.status === "pending") return absence("Querying the Host…");
  if (source.hallways.refusal) {
    return absence(`The Hallway inbox door refused: ${source.hallways.refusal}. No counts are shown.`);
  }

  const hallways = source.hallways.data?.hallways ?? [];
  if (hallways.length === 0) return absence("This room can open no persistent Hallway.");

  return hallways.map(renderHallwayChannel).join("");
}

function hallwayStatus() {
  if (source.status !== "answered") return `POST ${INBOX_ROUTE} · reading clears nothing`;
  if (source.hallways.refusal) return `POST ${INBOX_ROUTE} · refused`;

  const inbox = source.hallways.data ?? {};
  const hallways = inbox.hallways ?? [];
  let unread = 0;
  let mentions = 0;
  for (const entry of hallways) {
    unread += Number(entry.unread ?? 0);
    mentions += Number(entry.mentions ?? 0);
  }

  return `${countedNoun(hallways.length, "hallway")} · ${unread} unread · ${countedNoun(mentions, "mention")} pending for ${inbox.room ?? "this room"} · reading clears nothing`;
}

// Four explicit source states, in the Pulse grammar. The board carries no
// snapshot, so a refusal leaves the surface empty and says so instead of
// dressing old numbers as current ones.
function statusChips() {
  if (source.status === "idle") {
    return `
          <span data-tone="attention">Host not queried</span>
          <span>docket/board · hallway/inbox · POST only</span>`;
  }

  if (source.status === "pending") {
    return `
          <span data-tone="quiet">Querying Host…</span>`;
  }

  return `${doorChip("docket/board", source.board)}${doorChip("hallway/inbox", source.hallways)}
          <span>Queried ${escapeHtml(source.queriedAt)} local</span>`;
}

function doorChip(door, result) {
  if (result.refusal) {
    return `
          <span data-tone="attention">${escapeHtml(door)} refused · ${escapeHtml(result.refusal)}</span>`;
  }

  return `
          <span data-tone="steady">${escapeHtml(door)} connected</span>`;
}

export function renderHouseBoard() {
  const now = Date.now();
  const queryLabel = source.status === "pending" ? "Querying…" : "Query Host";

  return `
    <section class="insula-observation house-board" data-board-surface aria-labelledby="house-board-title">
      <header class="insula-observation-lead">
        <div>
          <span class="eyebrow">Docket</span>
          <h3 id="house-board-title">Board</h3>
          <p>Quests by deadline, soonest first, in the order the Host returned them. A read: the board offers work and never assigns it.</p>
        </div>
        <div class="pulse-status">
          <div class="mechanics-snapshot-status" aria-label="Board source status">${statusChips()}</div>
          <button class="card-verb" type="button" data-board-refresh>${queryLabel}</button>
        </div>
      </header>

      <section class="pulse-block" aria-labelledby="board-quests-title">
        <header class="pulse-block-lead">
          <h4 id="board-quests-title">Quests</h4>
          <small>${escapeHtml(questStatus())}</small>
        </header>
        <div class="pulse-rows">${questRows(now)}</div>
      </section>

      <section class="pulse-block" aria-labelledby="board-hallways-title">
        <header class="pulse-block-lead">
          <h4 id="board-hallways-title">Hallways</h4>
          <small>${escapeHtml(hallwayStatus())}</small>
        </header>
        <div class="insula-observation-grid">${hallwayChannels()}</div>
      </section>
    </section>`;
}
