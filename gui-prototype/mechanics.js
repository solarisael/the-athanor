import { escapeHtml } from "./text.js";
import { renderHousePulse } from "./pulse.js";
import { healthSourceLine, healthSourceTone, roomStateChannel } from "./health.js";
import { liveMechanic } from "./mechanics-live.js";

const CATEGORIES = [
  {
    id: "recall-context",
    label: "Recall & Context",
    summary: "The selected room's Recall policy and published projection cursor.",
    rows: [
      { id: "room.recall-policy", label: "Recall policy", source: "Room state", scope: "Selected room" },
      { id: "recall.projection", label: "Recall projection cursor", source: "Health", scope: "Selected room" }
    ]
  },
  {
    id: "memory-lessons",
    label: "Memory, Lessons & Anamnesis",
    summary: "Configuration availability for memory, lessons, and counsel.",
    rows: [{ id: "memory.configuration", label: "Memory and lesson settings", source: "No configuration response", scope: "Not reported" }]
  },
  {
    id: "giga-embeddings",
    label: "GIGA & Embeddings",
    summary: "Configuration availability for GIGA and embedding services.",
    rows: [{ id: "giga.configuration", label: "GIGA and embedding settings", source: "No configuration response", scope: "Not reported" }]
  },
  {
    id: "host-delivery",
    label: "Host, Delivery & Hallways",
    summary: "Host status and the reported delivery channel.",
    rows: [
      { id: "host.status", label: "Host status and API", source: "Health", scope: "Selected room" },
      { id: "delivery.channels", label: "AKASHA and broker delivery", source: "Health", scope: "Selected room" },
      { id: "delivery.receipt", label: "Latest delivery receipt", source: "Health", scope: "Selected room" },
      { id: "delivery.failure", label: "Last delivery failure", source: "Health", scope: "Selected room" }
    ]
  },
  {
    id: "rooms-sessions",
    label: "Rooms & Sessions",
    summary: "Selected room identity, policies, and reported presences.",
    rows: [
      { id: "room.state", label: "Operator and embodied spirit", source: "Room state", scope: "Selected room" },
      { id: "room.routing-mode", label: "Worker routing mode", source: "Room state", scope: "Selected room" },
      { id: "room.model-default", label: "OMP model default", source: "Room state", scope: "Selected room" },
      { id: "room.presences", label: "Room presences", source: "Room state", scope: "Selected room" },
      { id: "room.chat", label: "Room chat state", source: "Room state", scope: "Selected room" }
    ]
  },
  {
    id: "backups",
    label: "Backups",
    summary: "Backup status is not part of the current health response.",
    rows: [{ id: "backup.status", label: "Backup status and retention", source: "No backup response", scope: "Not reported" }]
  },
  {
    id: "advanced",
    label: "Advanced Guardrails",
    summary: "Insula health, returned vitals, and observed retention receipts.",
    rows: [
      { id: "insula.health", label: "Insula store health", source: "Health", scope: "Selected Host" },
      { id: "insula.vitals", label: "Insula vitals coverage", source: "Insula vitals", scope: "Selected room" },
      { id: "insula.retention", label: "Latest retention receipt", source: "Insula retention", scope: "House" }
    ]
  }
];


// Observatory view state: which category filters, what the search says, and
// where the operator's scroll rested when the slot last changed.
let category = "all";
let query = "";
let scrollTop = 0;
let timeline = null;

export function initMechanics(options) {
  timeline = options.timeline;
}

export function resetMechanicsView() {
  category = "all";
  query = "";
}

export function saveMechanicsScroll() {
  scrollTop = timeline.scrollTop;
}

export function mechanicsScrollTop() {
  return scrollTop;
}

export function handleMechanicsClick(event) {
  const button = event.target.closest("[data-mechanics-category]");
  if (!button) return false;

  category = button.dataset.mechanicsCategory;
  query = "";
  const search = timeline.querySelector("[data-mechanics-search]");
  if (search) search.value = "";
  renderMechanicsResults();
  return true;
}

export function handleMechanicsInput(event) {
  const search = event.target.closest("[data-mechanics-search]");
  if (!search) return false;

  query = search.value;
  renderMechanicsResults();
  return true;
}

function mechanicsHealthTone(health) {
  return health === "Host reported" ? "steady" : "quiet";
}

function mechanicsEntries() {
  const needle = query.trim().toLowerCase();
  const categories = needle || category === "all"
    ? CATEGORIES
    : CATEGORIES.filter(candidate => candidate.id === category);

  return categories.flatMap(group => group.rows
    .map(liveMechanic)
    .filter(row => {
      if (!needle) return true;
      return [group.label, group.summary, ...Object.values(row)].join(" ").toLowerCase().includes(needle);
    })
    .map(row => ({ category: group, row })));
}

function renderMechanicRow({ category: group, row }) {
  return `
    <details class="mechanics-row" data-mechanic-id="${escapeHtml(row.id)}">
      <summary>
        <span class="mechanics-row-title"><strong>${escapeHtml(row.label)}</strong><small>${escapeHtml(group.label)}</small></span>
        <span class="mechanics-row-value"><small>Observed</small><code>${escapeHtml(row.value)}</code></span>
        <span class="mechanics-row-flags" aria-label="${escapeHtml(`${row.health}; ${row.mutability}`)}">
          <span data-tone="${row.tone ?? mechanicsHealthTone(row.health)}">${escapeHtml(row.health)}</span>
          <span data-tone="quiet">${escapeHtml(row.mutability)}</span>
        </span>
      </summary>
      <div class="mechanics-row-body">
        <dl>
          <div><dt>Source</dt><dd>${escapeHtml(row.source)}</dd></div>
          <div><dt>Scope</dt><dd>${escapeHtml(row.scope)}</dd></div>
          <div><dt>Controls</dt><dd>Read-only</dd></div>
        </dl>
        <p>${escapeHtml(row.detail)}</p>
      </div>
    </details>`;
}

export function renderHouseMechanics() {
  const categoryButtons = [
    { id: "all", label: "All", count: CATEGORIES.reduce((sum, group) => sum + group.rows.length, 0) },
    ...CATEGORIES.map(group => ({ id: group.id, label: group.label, count: group.rows.length }))
  ];

  return `
    <section class="mechanics-observatory" aria-labelledby="mechanics-title">
      <header class="mechanics-lead">
        <span class="eyebrow">House mechanics</span>
        <h2 id="mechanics-title">Mechanical observatory</h2>
        <p>Observed Host health, selected room state, and Insula responses.</p>
        <div class="mechanics-snapshot-status" aria-label="Source status">
          <span data-tone="${healthSourceTone()}">${escapeHtml(healthSourceLine())}</span>
          <span data-tone="${roomStateChannel().tone}">${escapeHtml(roomStateChannel().full)} · ${escapeHtml(roomStateChannel().detail)}</span>
        </div>
      </header>
      ${renderHousePulse()}
      <div class="mechanics-controls">
        <label class="mechanics-search">
          <span>Search every mechanism</span>
          <input id="mechanics-search" type="search" data-mechanics-search value="${escapeHtml(query)}" placeholder="Recall, delivery, room, Insula…" autocomplete="off">
        </label>
        <nav class="mechanics-categories" aria-label="Mechanical observatory categories">
          ${categoryButtons.map(button => `
            <button id="mechanics-category-${escapeHtml(button.id)}" type="button" data-mechanics-category="${escapeHtml(button.id)}" aria-pressed="${String(category === button.id)}">
              <span>${escapeHtml(button.label)}</span><small>${button.count}</small>
            </button>`).join("")}
        </nav>
      </div>
      <p class="mechanics-results-status" role="status" aria-live="polite"></p>
      <div class="mechanics-results"></div>
      <footer>Settings changes are unavailable through the current HTTP routes.</footer>
    </section>`;
}

export function renderMechanicsResults() {
  const observatory = timeline.querySelector(".mechanics-observatory");
  if (!observatory) return;

  const entries = mechanicsEntries();
  const needle = query.trim();
  const active = CATEGORIES.find(candidate => candidate.id === category);

  observatory.querySelectorAll("[data-mechanics-category]").forEach(button => {
    button.setAttribute("aria-pressed", String(button.dataset.mechanicsCategory === category));
  });
  observatory.querySelector(".mechanics-results-status").textContent = needle
    ? `${entries.length} mechanism${entries.length === 1 ? "" : "s"} across all categories for “${needle}”`
    : `${entries.length} mechanism${entries.length === 1 ? "" : "s"} · ${active?.label ?? "All categories"}`;
  observatory.querySelector(".mechanics-results").innerHTML = entries.length > 0
    ? entries.map(renderMechanicRow).join("")
    : '<div class="mechanics-empty"><strong>No mechanism matches that search.</strong><span>Try Recall, delivery, room, or Insula.</span></div>';
}
