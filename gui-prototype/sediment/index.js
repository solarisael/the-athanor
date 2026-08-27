// Sediment — the durable instrument in House slot 3 and in every room's slot 3.
//
// Owns the live wires to the room's durable record through the serve.ts proxy:
// the memory timeline, one memory's full body, and the lesson timeline. It
// renders no invented rows and keeps the fixture shelf visibly named until
// live rows actually flow; then the fixtures retire, and not one round earlier.
//
// The app owns the fixture data and the fixture row markup. This module owns
// the doors, the shelf composition, and the keyset walk.

import { escapeHtml } from "../text.js";

const MEMORY_TIMELINE_ROUTE = "/live/memory/timeline";
const MEMORY_READ_ROUTE = "/live/memory/read";
const LESSON_TIMELINE_ROUTE = "/live/lesson/timeline";
const PAGE_LIMIT = 25;
const EXCERPT_LINE_LIMIT = 220;
const REFUSAL_DETAIL_LIMIT = 120;

// One shelf per subject, built on first ask and kept for the session: a shelf
// holds rows the operator has already walked back through, and throwing that
// away on every re-render would make 'load more' a treadmill.
const shelves = new Map();

// One read memory body per id, plus which of them the operator has open. Both
// survive re-render for the same reason the board's evidence drawer does:
// native <details> state dies with the markup that carried it.
const bodies = new Map();
const openBodies = new Set();

let requestRender = () => {};
let renderLead = () => "";
let renderShelfControls = () => "";
let renderFixtureRows = () => "";

export function initSediment(options) {
  requestRender = options.requestRender;
  renderLead = options.renderLead;
  renderShelfControls = options.renderShelfControls;
  renderFixtureRows = options.renderFixtureRows;
}

// Called from the slot-3 render path, so it asks each shelf once and leaves
// re-asking to the operator's own button. A render happens on every click in
// the mantle; a door does not deserve that.
export function ensureSedimentQueried(item) {
  const shelf = shelfFor(item);
  if (shelf.status !== "idle") return;

  queryShelf(shelf);
}

export function handleSedimentClick(event) {
  const refresh = event.target.closest("[data-sediment-refresh]");
  if (refresh) {
    queryShelf(shelfByKey(refresh.dataset.sedimentShelf));
    return true;
  }

  const more = event.target.closest("[data-sediment-more]");
  if (more) {
    const shelf = shelfByKey(more.dataset.sedimentShelf);
    walkOlder(shelf, more.dataset.sedimentMore);
    return true;
  }

  const bodyToggle = event.target.closest("[data-sediment-body-toggle]");
  if (bodyToggle) return toggleMemoryBody(bodyToggle.closest("[data-sediment-memory]"));

  return false;
}

// The click arrives before the browser flips <details>, so !row.open reads as
// "about to open". Nothing here calls preventDefault: the disclosure stays the
// browser's to run and the fetch only rides along.
function toggleMemoryBody(row) {
  const key = row?.dataset.sedimentMemory;
  if (!key) return false;

  if (row.open) {
    openBodies.delete(key);
    return true;
  }

  openBodies.add(key);
  if (bodies.get(key)?.status !== "pending") askMemoryBody(key);
  return true;
}

function shelfKey(item) {
  return item.kind === "house" ? "house" : `room:${item.id}`;
}

function shelfFor(item) {
  const key = shelfKey(item);
  const held = shelves.get(key);
  if (held) return held;

  const built = buildShelf(key, item);
  shelves.set(key, built);
  return built;
}

function shelfByKey(key) {
  return shelves.get(key);
}

// The House shelf reads the room's whole durable record; a room shelf asks the
// same door for that one room. Lessons are House-wide, so only the House shelf
// carries a lesson column — the same shape the fixture shelves already have.
function buildShelf(key, item) {
  const roomFilter = item.kind === "house" ? {} : { room: item.room ?? item.id };
  const columns = [openColumn("memories", "Memories", MEMORY_TIMELINE_ROUTE, "memories", roomFilter, "createdAt")];
  if (item.kind === "house") {
    columns.push(openColumn("lessons", "Lessons", LESSON_TIMELINE_ROUTE, "lessons", {}, "updatedAt"));
  }

  return { key, status: "idle", queriedAt: null, columns };
}

// One column of a shelf: its door, filter, cursor timestamp field, rows already
// landed, and keyset cursor to ask past. Rows accumulate across "load more"
// rounds; a refusal belongs to the last round alone.
function openColumn(id, label, route, collection, filter, cursorStamp) {
  return {
    id,
    label,
    route,
    collection,
    filter,
    cursorStamp,
    rows: [],
    cursor: null,
    exhausted: false,
    status: "idle",
    refusal: null
  };
}

async function queryShelf(shelf) {
  if (!shelf || shelf.status === "querying") return;

  shelf.status = "querying";
  await Promise.all(shelf.columns.map(column => askColumn(column, { append: false })));

  shelf.status = "answered";
  shelf.queriedAt = new Date().toTimeString().slice(0, 5);
  requestRender();
}

async function walkOlder(shelf, columnId) {
  const column = shelf?.columns.find(candidate => candidate.id === columnId);
  if (!column) return;

  await askColumn(column, { append: true });
  requestRender();
}

// The keyset walk: the next page is asked for by the oldest row already held,
// never by an offset, so rows written between rounds cannot shift the window
// and hand back a row twice.
async function askColumn(column, { append }) {
  if (column.status === "pending") return;

  column.status = "pending";
  const body = { ...column.filter, limit: PAGE_LIMIT };
  if (append && column.cursor) body.before = column.cursor;

  const answer = await askDoor(column.route, body);
  column.status = "answered";
  column.refusal = answer.refusal ?? null;
  if (answer.refusal) return;

  const rows = readRows(answer.data, column.collection);
  if (rows === null) {
    column.refusal = `Host answer carried no ${column.collection} array`;
    return;
  }

  column.rows = append ? [...column.rows, ...rows] : rows;
  column.exhausted = rows.length < PAGE_LIMIT;
  column.cursor = keysetCursor(column.rows.at(-1), column.cursorStamp);
}

async function askMemoryBody(key) {
  bodies.set(key, { status: "pending" });
  paintMemoryBody(key);

  const answer = await askDoor(MEMORY_READ_ROUTE, { id: parsedId(key) });

  bodies.set(key, { status: "answered", readAt: new Date().toTimeString().slice(0, 5), ...answer });
  paintMemoryBody(key);
}

// Painted into the live row by hand instead of through requestRender: a full
// re-render would rebuild the shelf and shut the disclosure the operator just
// opened. The match walks the nodes rather than building a selector, because a
// Host-supplied id reaching querySelector as text is a selector-injection door.
function paintMemoryBody(key) {
  const target = [...document.querySelectorAll("[data-sediment-body]")]
    .find(node => node.dataset.sedimentBody === key);
  if (target) target.innerHTML = memoryBodyMarkup(key);
}

// One door, one answer: the parsed payload, or the exact reason it refused.
// Kept in the board's shape because both instruments read the same proxy and a
// second refusal grammar on one page would be one grammar too many.
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

// The wire shape is fixed: an empty array is an honest empty shelf, while a
// missing or malformed collection is a protocol refusal rather than no rows.
function readRows(payload, collection) {
  const rows = payload?.[collection];
  if (!Array.isArray(rows)) return null;

  return rows.filter(row => row !== null && typeof row === "object");
}

// Each door names its own timestamp field and requires a numeric id. A row
// without either part remains readable, but cannot start another page honestly.
function keysetCursor(row, stampField) {
  const stamp = fieldText(row?.[stampField]);
  const id = Number(row?.id);
  if (!stamp || !Number.isSafeInteger(id) || id < 1) return null;

  return { [stampField]: stamp, id };
}

function fieldText(value) {
  if (typeof value === "string") return value.trim();
  if (typeof value === "number" && Number.isFinite(value)) return String(value);

  return "";
}

function parsedId(value) {
  const numeric = Number(value);
  return Number.isInteger(numeric) ? numeric : value;
}

// Number() first: a Host count never reaches markup as caller text. Duplicated
// from the board on purpose — text.js is the shared safety module and is not
// this ticket's to widen, so the two copies stay honest rather than one of them
// reaching across a boundary it was not given.
function shelfCount(value) {
  return Number(value ?? 0).toLocaleString("en-US");
}

function countedNoun(value, singular) {
  return `${shelfCount(value)} ${Number(value) === 1 ? singular : `${singular}s`}`;
}

function oneLine(value, limit) {
  const text = String(value ?? "").replace(/\s+/g, " ").trim();
  if (text.length <= limit) return text;

  return `${text.slice(0, limit - 1).trimEnd()}…`;
}

// The Host sends RFC 3339 UTC; the label stays UTC so no row claims a timezone
// the Host never sent. An unusable stamp is named, never sliced into half a date.
function stampLabel(value) {
  const stamp = fieldText(value);
  if (stamp.length < 16) return "no date";

  return `${stamp.slice(0, 10)} ${stamp.slice(11, 16)} UTC`;
}

function absence(text) {
  return `<p class="sediment-absence">${escapeHtml(text)}</p>`;
}

// An unmapped proxy path never reaches the Host. Every other failure carries
// the upstream status and exact refusal body without guessing why it failed.
function doorChip(column) {
  if (column.status !== "answered") {
    return `<span data-tone="quiet">${escapeHtml(`${column.label} · asking`)}</span>`;
  }
  if (!column.refusal) {
    return `<span data-tone="steady">${escapeHtml(`${column.label} · ${countedNoun(column.rows.length, "row")}`)}</span>`;
  }
  if (column.refusal.includes("unknown live route")) {
    return `<span data-tone="attention">${escapeHtml(`${column.label} · not mapped in this proxy · ${column.refusal}`)}</span>`;
  }

  return `<span data-tone="attention">${escapeHtml(`${column.label} · refused · ${column.refusal}`)}</span>`;
}

function shelfStatusChips(shelf) {
  const chips = shelf.columns.map(doorChip).join("");
  const queried = shelf.queriedAt
    ? `<span>Queried ${escapeHtml(shelf.queriedAt)} local</span>`
    : "<span>Not queried yet</span>";

  return `${chips}${queried}`;
}

function memoryBodyMarkup(key) {
  const read = bodies.get(key);
  if (!read || read.status === "pending") return absence("Reading the memory door…");
  if (read.refusal) {
    return absence(`The memory read door refused: ${read.refusal}. No body is shown.`);
  }

  const payload = read.data ?? {};
  const record = payload.memory ?? payload;
  const body = fieldText(payload.body) || fieldText(payload.memory?.body);
  const threads = Array.isArray(payload.threads) ? payload.threads : null;
  const threadLine = threads
    ? absence(`${countedNoun(threads.length, "thread")} returned beside this body.`)
    : "";

  if (!body) return absence("The memory door answered without a body for this row.");

  return `${authorityBanner(record)}<p>${escapeHtml(body)}</p>${threadLine}`;
}

// The read door carries supersededBy and archivedAt so that a body can never be
// read as the current record when it is not. The banner stands before the first
// line, because after the first line the operator has already read it as
// current.
function authorityBanner(record) {
  const superseded = Number.isInteger(record.supersededBy) ? record.supersededBy : null;
  const archived = fieldText(record.archivedAt);
  if (superseded === null && archived === "") return "";

  const authority = [
    superseded === null ? "" : `Superseded by memory #${superseded}`,
    archived === "" ? "" : `Archived ${stampLabel(record.archivedAt)}`
  ].filter(Boolean).join(" · ");

  return `<p class="sediment-authority">${escapeHtml(`${authority} · this row is history, not the current record.`)}</p>`;
}

// Live rows are collapsed first and carry only what the timeline door sends.
// The body is a second door, asked once, on the operator's own expand.
function renderMemoryRow(shelf, row) {
  const id = fieldText(row.id);
  const key = id || "";
  const excerpt = fieldText(row.excerpt);
  const facts = [fieldText(row.room) || "no room", fieldText(row.type) || "no type"].join(" · ");

  // A row the door sent without an id can be shown but not opened: its body
  // door takes an id, and this page will not invent one.
  if (!key) {
    return `
        <article class="durable-entry sediment-row">
          <time>${escapeHtml(stampLabel(row.createdAt))}</time>
          <strong>${escapeHtml(oneLine(row.title, EXCERPT_LINE_LIMIT) || "no title")}</strong>
          <small>${escapeHtml(`${facts} · no id, so its body cannot be asked for`)}</small>
        </article>`;
  }

  const open = openBodies.has(key) ? " open" : "";
  return `
        <details class="mechanics-row sediment-row"${open} data-sediment-memory="${escapeHtml(key)}" data-sediment-shelf="${escapeHtml(shelf.key)}">
          <summary data-sediment-body-toggle>
            <span class="mechanics-row-title"><strong>${escapeHtml(oneLine(row.title, EXCERPT_LINE_LIMIT) || "no title")}</strong><small>${escapeHtml(`#${key} · ${facts}`)}</small></span>
            <span class="mechanics-row-value"><small>Written</small><code>${escapeHtml(stampLabel(row.createdAt))}</code></span>
            <span class="mechanics-row-flags"><span data-tone="quiet">live row</span></span>
          </summary>
          <div class="mechanics-row-body">
            ${excerpt ? `<p>${escapeHtml(excerpt)}</p>` : absence("The timeline door sent no excerpt for this row.")}
            <div data-sediment-body="${escapeHtml(key)}">${memoryBodyMarkup(key)}</div>
          </div>
        </details>`;
}

// A lesson row has no body door in this contract, so it stays a plain row
// rather than a disclosure that opens onto nothing.
function renderLessonRow(row) {
  const id = fieldText(row.id);
  const kindPath = fieldText(row.kindPath) || "no kind path";

  return `
        <article class="durable-entry sediment-row">
          <time>${escapeHtml(stampLabel(row.updatedAt))}</time>
          <strong>${escapeHtml(oneLine(row.title, EXCERPT_LINE_LIMIT) || "no title")}</strong>
          <small>${escapeHtml(`${id ? `#${id}` : "no id"} · ${kindPath}`)}</small>
        </article>`;
}

function renderColumnRows(shelf, column) {
  if (column.status !== "answered") return absence(`Asking the ${column.label} door…`);
  if (column.rows.length === 0) {
    return column.refusal
      ? absence(`The ${column.label} door has not answered with rows: ${column.refusal}. Nothing is shown in its place.`)
      : absence(`The ${column.label} door answered with no rows.`);
  }

  const rows = column.id === "lessons"
    ? column.rows.map(renderLessonRow).join("")
    : column.rows.map(row => renderMemoryRow(shelf, row)).join("");
  if (!column.refusal) return rows;

  return `${rows}${absence(`Walking farther through the ${column.label} door refused: ${column.refusal}. The preceding live rows remain.`)}`;
}

function renderWalkControl(shelf, column) {
  if (column.status !== "answered" || column.refusal || column.rows.length === 0) return "";
  if (!column.cursor) {
    return absence(`These rows carry no date to walk back from, so the ${column.label} door cannot be asked for older ones.`);
  }
  if (column.exhausted) return absence(`No rows older than these stand behind the ${column.label} door.`);

  // # enough: load-more button; virtualization when rows hurt
  return `
        <button class="card-verb" type="button" data-sediment-more="${escapeHtml(column.id)}" data-sediment-shelf="${escapeHtml(shelf.key)}">Load older ${escapeHtml(column.label.toLowerCase())}</button>`;
}

function renderColumn(shelf, column) {
  const walked = column.rows.length > PAGE_LIMIT ? ` · walked back ${shelfCount(Math.ceil(column.rows.length / PAGE_LIMIT))} pages` : "";

  return `
      <section class="pulse-block" aria-label="${escapeHtml(`${column.label} timeline`)}">
        <header class="pulse-block-lead">
          <h4>${escapeHtml(column.label)}</h4>
          <small>${escapeHtml(`POST ${column.route} · newest first · limit ${PAGE_LIMIT}${walked}`)}</small>
        </header>
        <div class="pulse-rows">${renderColumnRows(shelf, column)}</div>
        ${renderWalkControl(shelf, column)}
      </section>`;
}

function liveRowsFlow(shelf) {
  return shelf.columns.some(column => column.rows.length > 0);
}

// The House overview card counts its rows from here, so the card retires its
// fixture count on the round the shelf below it retires the fixture rows: one
// retire rule, never two numbers disagreeing on one page. Nothing is built by
// asking — a card must not open a door.
export function liveShelfCounts(item) {
  const shelf = shelves.get(shelfKey(item));
  if (!shelf || !liveRowsFlow(shelf)) return null;

  return Object.fromEntries(shelf.columns.map(column => [column.id, column.rows.length]));
}

// The fixture shelf is the operator's existing surface and it keeps its search,
// its marks, and its inspector rows: it is retired only when live rows actually
// flow, and until then every row on it wears a chip saying what it is.
function renderFixtureShelf(item, shelf) {
  if (liveRowsFlow(shelf)) return "";

  return `
      <section class="pulse-block sediment-fixtures" data-fixture-shelf aria-label="Fixture shelf">
        <header class="pulse-block-lead">
          <h4>Fixtures</h4>
          <small>Written into the page, not read from any door. These rows retire the round live rows arrive.</small>
        </header>
        ${renderShelfControls(item)}
        <div data-durable-results>${renderFixtureRows(item)}</div>
      </section>`;
}

function renderShelf(item, eyebrow, title, blurb) {
  const shelf = shelfFor(item);

  return `
    <div class="specimen-stack house-record-stack">
      ${renderLead(eyebrow, title)}
      <section class="insula-observation house-sediment" data-sediment-surface aria-label="${escapeHtml(`${title} durable doors`)}">
        <header class="insula-observation-lead">
          <div>
            <span class="eyebrow">Durable</span>
            <h3>${escapeHtml(title)}</h3>
            <p>${escapeHtml(blurb)}</p>
          </div>
          <div class="pulse-status">
            <div class="mechanics-snapshot-status" aria-label="Durable source status">${shelfStatusChips(shelf)}</div>
            <button class="card-verb" type="button" data-sediment-refresh data-sediment-shelf="${escapeHtml(shelf.key)}">Query Host</button>
          </div>
        </header>
        <div class="sediment-shelves">${shelf.columns.map(column => renderColumn(shelf, column)).join("")}</div>
        ${renderFixtureShelf(item, shelf)}
      </section>
    </div>`;
}

export function renderHouseSediment(item) {
  return renderShelf(
    item,
    "House record",
    "Memories & Lessons",
    "The shared shelves as the Host holds them: memories and lessons, newest first, walked back one page at a time."
  );
}

export function renderRoomSediment(item) {
  return renderShelf(
    item,
    "Room record",
    item.name,
    `${item.name} room memory as the Host holds it, newest first. Room memory stays inside its room.`
  );
}
