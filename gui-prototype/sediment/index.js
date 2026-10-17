// The House and room shelves show only rows from the Host.
// Search and filters apply to loaded pages. The Host owns scope and cursors.

import { escapeHtml } from "../text.js";
import { askHost } from "../health.js";

const MEMORY_TIMELINE_ROUTE = "/live/memory/timeline";
const MEMORY_READ_ROUTE = "/live/memory/read";
const LESSON_TIMELINE_ROUTE = "/live/lesson/timeline";
const PAGE_LIMIT = 25;
const EXCERPT_LINE_LIMIT = 220;

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

export function initSediment(options) {
  requestRender = options.requestRender;
  renderLead = options.renderLead;
  renderShelfControls = options.renderShelfControls;
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
  const mark = event.target.closest("[data-durable-mark]");
  if (mark) {
    const shelf = shelfFromControl(mark);
    if (!shelf || !["all", "Memory", "Lesson"].includes(mark.dataset.durableMark)) return false;
    shelf.mark = mark.dataset.durableMark;
    paintShelfResults(shelf, mark);
    return true;
  }

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

export function handleSedimentInput(event) {
  const search = event.target.closest("[data-durable-search]");
  const shelf = search && shelfFromControl(search);
  if (!shelf) return false;

  shelf.query = search.value;
  paintShelfResults(shelf, search);
  return true;
}

function shelfFromControl(control) {
  return shelfByKey(control.closest("[data-sediment-surface]")?.dataset.sedimentShelf);
}

function paintShelfResults(shelf, control) {
  const surface = control.closest("[data-sediment-surface]");
  surface.querySelector("[data-sediment-results]").innerHTML = renderColumns(shelf);
  const status = surface.querySelector("[data-durable-status]");
  if (status) status.textContent = shelfFilterStatus(shelf);
  surface.querySelectorAll("[data-durable-mark]").forEach(button => {
    button.setAttribute("aria-pressed", String(button.dataset.durableMark === shelf.mark));
  });
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

  const shelf = shelfByKey(row.dataset.sedimentShelf);
  const source = shelf?.columns.find(column => column.id === "memories")?.rows
    .find(memory => memoryKey(shelf, memory) === key);
  if (!source) return false;

  openBodies.add(key);
  if (bodies.get(key)?.status !== "pending") askMemoryBody(key, source);
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

// The House uses the supported House filter, not the combined room-and-House view.
function buildShelf(key, item) {
  const roomFilter = { room: item.kind === "house" ? "house" : item.room ?? item.id };
  const columns = [openColumn("memories", "Memories", MEMORY_TIMELINE_ROUTE, "memories", roomFilter, "createdAt")];
  if (item.kind === "house") {
    columns.push(openColumn("lessons", "Lessons", LESSON_TIMELINE_ROUTE, "lessons", {}, "updatedAt"));
  }

  return { key, status: "idle", queriedAt: null, query: "", mark: "all", columns };
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
  if (!shelf || shelf.status === "querying" || shelf.columns.some(column => column.status === "pending")) return;

  shelf.status = "querying";
  const pending = Promise.all(shelf.columns.map(column => askColumn(column, { append: false })));
  requestRender();
  await pending;

  shelf.status = "answered";
  shelf.queriedAt = new Date().toTimeString().slice(0, 5);
  requestRender();
}

async function walkOlder(shelf, columnId) {
  const column = shelf?.columns.find(candidate => candidate.id === columnId);
  if (!column || shelf.status === "querying" || column.exhausted || !column.cursor) return;

  const pending = askColumn(column, { append: true });
  requestRender();
  await pending;
  requestRender();
}

// The keyset walk: the next page is asked for by the oldest row already held,
// never by an offset, so rows written between rounds cannot shift the window
// and hand back a row twice.
async function askColumn(column, { append }) {
  if (column.status === "pending") return;

  column.status = "pending";
  column.refusal = null;
  const body = { ...column.filter, limit: PAGE_LIMIT };
  if (append && column.cursor) body.before = column.cursor;

  const answer = await askDoor(column.route, body);
  column.status = answer.status;
  column.refusal = answer.refusal ?? null;
  if (answer.refusal) return;

  const rows = readRows(answer.data, column.collection);
  if (rows === null) {
    column.status = "failed";
    column.refusal = `Host answer carried no valid ${column.collection} array`;
    return;
  }

  if (append) column.rows.push(...rows);
  else column.rows = rows;
  column.exhausted = rows.length < PAGE_LIMIT;
  column.cursor = keysetCursor(column.rows.at(-1), column.cursorStamp);
}

async function askMemoryBody(key, source) {
  bodies.set(key, { status: "pending" });
  paintMemoryBody(key);

  const answer = await askDoor(MEMORY_READ_ROUTE, { id: Number(source.id) });
  if (!answer.refusal) {
    const memory = answer.data?.memory;
    if (String(memory?.id) !== String(source.id) || memory?.room !== source.room) {
      answer.status = "failed";
      answer.refusal = "The memory read answer does not match the selected timeline row.";
    }
  }

  bodies.set(key, { readAt: new Date().toTimeString().slice(0, 5), ...answer });
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

// The shared request door supplies the selected room header.
async function askDoor(route, body) {
  try {
    return { status: "live", data: await askHost(route, body) };
  } catch (error) {
    const status = error?.status;
    const detail = error instanceof Error ? error.message : error?.detail ?? "no route to Host";
    return {
      status: status >= 400 && status < 500 ? "refused" : "failed",
      refusal: status ? `HTTP ${status} · ${detail}` : detail
    };
  }
}

// A malformed response must not become an empty shelf.
function readRows(payload, collection) {
  const rows = payload?.[collection];
  if (!Array.isArray(rows)) return null;

  return rows.every(row => row !== null && typeof row === "object" && !Array.isArray(row)) ? rows : null;
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

// Source states remain separate from the number of loaded rows.
function doorChip(column) {
  if (column.status === "idle") {
    return `<span data-tone="quiet">${escapeHtml(`${column.label} · not queried`)}</span>`;
  }
  if (column.status === "pending") {
    return `<span data-tone="quiet">${escapeHtml(`${column.label} · loading`)}</span>`;
  }
  if (!column.refusal) {
    return `<span data-tone="steady">${escapeHtml(`${column.label} · ${countedNoun(column.rows.length, "loaded row")}`)}</span>`;
  }

  return `<span data-tone="attention">${escapeHtml(`${column.label} · ${column.status} · ${column.refusal}`)}</span>`;
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
    return absence(`The memory read ${read.status}: ${read.refusal}`);
  }

  const payload = read.data ?? {};
  const record = payload.memory;
  const body = fieldText(record?.body);
  const threads = Array.isArray(record?.threads) ? record.threads : null;
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

// Only a loaded timeline row can open the read door.
function memoryKey(shelf, row) {
  const id = Number(row.id);
  return Number.isSafeInteger(id) && id > 0 ? `${shelf.key}:${id}` : "";
}

function renderMemoryRow(shelf, row) {
  const id = fieldText(row.id);
  const key = memoryKey(shelf, row);
  const excerpt = fieldText(row.excerpt);
  const facts = [fieldText(row.room) || "no room", fieldText(row.type) || "no type"].join(" · ");

  // The read door requires a positive numeric id.
  if (!key) {
    return `
        <article class="durable-entry sediment-row">
          <time>${escapeHtml(stampLabel(row.createdAt))}</time>
          <strong>${escapeHtml(oneLine(row.title, EXCERPT_LINE_LIMIT) || "no title")}</strong>
          <small>${escapeHtml(`${facts} · no valid id; the memory body is unavailable`)}</small>
        </article>`;
  }

  const open = openBodies.has(key) ? " open" : "";
  return `
        <details class="mechanics-row sediment-row"${open} data-sediment-memory="${escapeHtml(key)}" data-sediment-shelf="${escapeHtml(shelf.key)}">
          <summary data-sediment-body-toggle>
            <span class="mechanics-row-title"><strong>${escapeHtml(oneLine(row.title, EXCERPT_LINE_LIMIT) || "no title")}</strong><small>${escapeHtml(`#${id} · ${facts}`)}</small></span>
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

function matchingRows(shelf, column) {
  const mark = column.id === "lessons" ? "Lesson" : "Memory";
  if (shelf.mark !== "all" && shelf.mark !== mark) return [];
  const terms = shelf.query.trim().toLowerCase().split(/\s+/).filter(Boolean);
  if (terms.length === 0) return column.rows;

  return column.rows.filter(row => {
    const text = [
      `#${fieldText(row.id)}`, row.title, mark, row.room, row.type,
      row.kindPath, row.createdAt, row.updatedAt, row.excerpt
    ].map(fieldText).join(" ").toLowerCase();
    return terms.every(term => text.includes(term));
  });
}

function shelfFilterStatus(shelf) {
  const loaded = shelf.columns.reduce((count, column) => count + column.rows.length, 0);
  const shown = shelf.columns.reduce((count, column) => count + matchingRows(shelf, column).length, 0);
  return `${shelfCount(shown)} of ${countedNoun(loaded, "loaded row")} · Search and filters cover loaded pages only.`;
}

function renderColumnRows(shelf, column) {
  let notice = "";
  if (column.status === "idle") notice = absence(`The ${column.label} door has not been queried.`);
  else if (column.status === "pending") notice = absence(`Loading ${column.label.toLowerCase()}…`);
  else if (column.refusal) notice = absence(`${column.label} ${column.status}: ${column.refusal}`);
  else if (column.rows.length === 0) notice = absence(`The ${column.label} door answered with no rows.`);

  const matches = matchingRows(shelf, column);
  if (column.rows.length > 0 && matches.length === 0) {
    return `${notice}${absence(`No loaded ${column.label.toLowerCase()} match the search and filter.`)}`;
  }
  const rows = column.id === "lessons"
    ? matches.map(renderLessonRow).join("")
    : matches.map(row => renderMemoryRow(shelf, row)).join("");
  return `${notice}${rows}`;
}

function renderWalkControl(shelf, column) {
  if (column.status === "idle" || column.status === "pending" || column.rows.length === 0) return "";
  if (column.exhausted) return absence(`No older ${column.label.toLowerCase()} remain in this scope.`);
  if (!column.cursor) {
    return absence(`The last ${column.label} row has no valid cursor. Older rows are unavailable.`);
  }

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
        ${column.id === "lessons" ? absence("Lesson bodies are unavailable. This route returns timeline metadata only.") : ""}
      </section>`;
}

function renderColumns(shelf) {
  return shelf.columns
    .filter(column => shelf.mark === "all" || shelf.mark === (column.id === "lessons" ? "Lesson" : "Memory"))
    .map(column => renderColumn(shelf, column)).join("");
}

// Counts cover loaded live rows, including an observed empty response.
// The overview must not open a door to obtain its count.
export function liveShelfCounts(item) {
  const shelf = shelves.get(shelfKey(item));
  if (!shelf) return null;

  return {
    status: shelf.status,
    ...Object.fromEntries(shelf.columns.map(column => [column.id, column.rows.length])),
    columns: Object.fromEntries(shelf.columns.map(column => [column.id, {
      status: column.status,
      count: column.rows.length,
      refusal: column.refusal
    }]))
  };
}

function renderShelf(item, eyebrow, title, blurb) {
  const shelf = shelfFor(item);

  return `
    <div class="specimen-stack house-record-stack">
      ${renderLead(eyebrow, title)}
      <section class="insula-observation house-sediment" data-sediment-surface data-sediment-shelf="${escapeHtml(shelf.key)}" aria-label="${escapeHtml(`${title} durable doors`)}">
        <header class="insula-observation-lead">
          <div>
            <span class="eyebrow">Durable</span>
            <h3>${escapeHtml(title)}</h3>
            <p>${escapeHtml(blurb)}</p>
          </div>
          <div class="pulse-status">
            <div class="mechanics-snapshot-status" aria-label="Durable source status">${shelfStatusChips(shelf)}</div>
            <button class="card-verb" type="button" data-sediment-refresh data-sediment-shelf="${escapeHtml(shelf.key)}"${shelf.status === "querying" || shelf.columns.some(column => column.status === "pending") ? " disabled" : ""}>${shelf.status === "querying" || shelf.columns.some(column => column.status === "pending") ? "Loading…" : "Query Host"}</button>
          </div>
        </header>
        ${renderShelfControls(item, { query: shelf.query, mark: shelf.mark, status: shelfFilterStatus(shelf) })}
        <div class="sediment-shelves" data-sediment-results>${renderColumns(shelf)}</div>
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
