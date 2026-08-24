// Pulse — the Insula instrument in House slot 2.
//
// Owns the stamped PostgreSQL snapshot, the one read-only live wire to a room
// Host (through the serve.ts proxy, so no credential ever reaches this page),
// the derivation from vitals rollups, and the rendering. The surface renders
// the snapshot instantly; one Host query may replace channels and lanes with
// live rows wearing live chips. Receipts stay snapshot-only because no Host
// route serves receipt kinds yet.

import { escapeHtml } from "./text.js";

// One consistent capture: every number below was read from PostgreSQL insula
// rows inside a single transaction at the stamped moment. Times are −03.
const PULSE_SNAPSHOT = {
  capturedAt: "2026-08-20 22:12 −03",
  source: "PostgreSQL insula snapshot",
  vitalsQuery: "insula.vitals.minute v1",
  totals: { spans: 15853, writers: 36, components: 4, window: "16:01–22:12 −03", duplicates: 13, drops: 0 },
  tokens: { tokensIn: 95690376, tokensOut: 398925, usagePoints: 810 },
  retention: { receipts: 0, sweepRuns: 13, days: 14, firstExpiry: "2026-09-03" },
  rooms: [
    ["kintsu", 8555], ["tuner", 5329], ["kodo", 1660],
    ["house", 211], ["salvia", 49], ["hugo", 49]
  ],
  lanes: [
    { operation: "knock_claim", component: "house_host", outcomes: { ok: 2436 }, maxDuration: "28.7 ms", errorClasses: "",
      note: "Doorman claim poll, roughly one span per host every two seconds, aggregated to one lane at render." },
    { operation: "tool_call", component: "omp_adapter", outcomes: { ok: 1915, error: 29, cancelled: 15 }, maxDuration: "24.6 min", errorClasses: "tool_error 29 · session_shutdown 15" },
    { operation: "provider_request", component: "omp_adapter", outcomes: { ok: 1800, degraded: 9, cancelled: 5 }, maxDuration: "1.9 min", errorClasses: "partial_context 9 · provider_aborted 5" },
    { operation: "provider_usage", component: "omp_adapter", outcomes: { ok: 809, degraded: 3 }, maxDuration: "", errorClasses: "usage_unavailable 3" },
    { operation: "context_assembly", component: "omp_adapter", outcomes: { ok: 801, degraded: 12 }, maxDuration: "2.3 s", errorClasses: "partial_context 12" },
    { operation: "receipt_projection", component: "house_host", outcomes: { ok: 48, refused: 192 }, maxDuration: "", errorClasses: "" },
    { operation: "insula_ingest", component: "house_host", outcomes: { ok: 149 }, maxDuration: "67.1 ms", errorClasses: "" },
    { operation: "tool_result", component: "omp_adapter", outcomes: { ok: 94, error: 1 }, maxDuration: "", errorClasses: "tool_error 1" },
    { operation: "lesson_trigger_match", component: "athanor_substrate", outcomes: { ok: 86 }, maxDuration: "10.9 ms", errorClasses: "" },
    { operation: "retention_sweep", component: "athanor_substrate", outcomes: { ok: 13 }, maxDuration: "42.0 ms", errorClasses: "" },
    { operation: "hallway_projection", component: "house_host", outcomes: { ok: 7 }, maxDuration: "", errorClasses: "" },
    { operation: "recall_policy_decide", component: "house_host", outcomes: { ok: 7 }, maxDuration: "", errorClasses: "" },
    { operation: "insula_health", component: "deployment_probe", outcomes: { ok: 5 }, maxDuration: "", errorClasses: "" },
    { operation: "lesson_query", component: "athanor_substrate", outcomes: { ok: 3 }, maxDuration: "26.0 ms", errorClasses: "" },
    { operation: "hallway_read", component: "athanor_substrate", outcomes: { ok: 2 }, maxDuration: "13.0 ms", errorClasses: "" },
    { operation: "hallway_post", component: "athanor_substrate", outcomes: { ok: 2 }, maxDuration: "21.4 ms", errorClasses: "" },
    { operation: "remember", component: "athanor_substrate", outcomes: { ok: 1 }, maxDuration: "824.9 ms", errorClasses: "" },
    { operation: "entity_resolve", component: "athanor_substrate", outcomes: { ok: 1 }, maxDuration: "1.9 ms", errorClasses: "" },
    { operation: "recall", component: "athanor_substrate", outcomes: { ok: 1 }, maxDuration: "9.5 s", errorClasses: "" },
    { operation: "pg_backup", component: "athanor_substrate", outcomes: { ok: 1 }, maxDuration: "", errorClasses: "" }
  ],
  receipts: [
    { kind: "athanor.recall_policy.command_accepted", component: "house_host", count: 7, lastAt: "22:05 −03", outcome: "ok", latestId: "4772aa88-19e9-4fff-bd40-54ebb8e58a46" },
    { kind: "athanor.hallway.inbox_projected", component: "house_host", count: 7, lastAt: "22:05 −03", outcome: "ok", latestId: "9bd7575c-cd6f-4db5-bd76-33ae1ae158f1" },
    { kind: "paper_boat_receipt", component: "house_host", count: 48, lastAt: "21:45 −03", outcome: "ok", latestId: "64212b60-a5a8-445f-ad7f-b2b6b6bfe407" },
    { kind: "insula.backup", component: "athanor_substrate", count: 1, lastAt: "21:45 −03", outcome: "ok", latestId: "a61389335e22df4f0b0bfee497b0a7ff45ee925f7ea0b34c23d9a6f58f5a7d64" }
  ]
};

const KNOCK_NOTE = "Doorman claim poll, roughly one span per host every two seconds, aggregated to one lane at render.";
const RAW_ONLY = "raw rows only — not carried in rollups";

// Pulse-local source state: idle → pending → live | failed. Nothing outside
// this module reads it; the shell sees rendered markup and a render request.
let live = { status: "idle" };
let requestRender = () => {};

export function initPulse(options) {
  requestRender = options.requestRender;
}

export function ensurePulseQueried() {
  if (live.status === "idle") queryPulseHost();
}

export function handlePulseClick(event) {
  if (!event.target.closest("[data-pulse-refresh]")) return false;

  queryPulseHost();
  return true;
}

function pulseCount(value) {
  return value.toLocaleString("en-US");
}

function formatMicroseconds(us) {
  if (us == null) return "";
  if (us < 1000) return `${us} µs`;
  if (us < 1e6) return `${(us / 1000).toFixed(1)} ms`;
  if (us < 90e6) return `${(us / 1e6).toFixed(1)} s`;

  return `${(us / 6e7).toFixed(1)} min`;
}

function outcomeTone(outcome) {
  if (outcome === "error" || outcome === "refused") return "attention";
  if (outcome === "ok") return "steady";

  return "quiet";
}

async function queryPulseHost() {
  if (live.status === "pending") return;

  live = { status: "pending" };
  requestRender();

  try {
    const end = new Date();
    const start = new Date(end.getTime() - 24 * 3600 * 1000);

    const [vitalsResponse, retentionResponse] = await Promise.all([
      fetch("/live/insula/vitals", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ start: start.toISOString(), end: end.toISOString(), limit: 4000 })
      }),
      fetch("/live/insula/retention", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ limit: 20 })
      })
    ]);
    if (!vitalsResponse.ok || !retentionResponse.ok) {
      throw new Error(`Host answered ${vitalsResponse.ok ? retentionResponse.status : vitalsResponse.status}`);
    }

    const queriedAt = new Date().toTimeString().slice(0, 5);
    live = derivePulseLive(await vitalsResponse.json(), await retentionResponse.json(), queriedAt);
  } catch (error) {
    live = { status: "failed", reason: error instanceof Error ? error.message : "no route to Host" };
  }

  requestRender();
}

// Live vitals rows arrive as minute rollups; lanes are arithmetic over them.
// Raw-only facts (error classes, duplicates, writers) are named as absent
// rather than imitated.
function derivePulseLive(vitals, retention, queriedAt) {
  const lanes = new Map();
  let settled = 0;
  let tokensIn = 0;
  let tokensOut = 0;
  let usagePoints = 0;
  let drops = 0;
  let firstMinute = null;
  let lastMinute = null;

  for (const row of vitals.rows) {
    if (!firstMinute || row.minute < firstMinute) firstMinute = row.minute;
    if (!lastMinute || row.minute > lastMinute) lastMinute = row.minute;
    tokensIn += row.tokensInSum;
    tokensOut += row.tokensOutSum;
    drops += row.dropCountSum;
    if (row.operation === "provider_usage" && row.outcomeClass === "ok") usagePoints += row.eventCount;

    if (row.phase === "start") continue;

    settled += row.eventCount;
    const key = `${row.component} ${row.operation}`;
    const lane = lanes.get(key) ?? { operation: row.operation, component: row.component, outcomes: {}, maxUs: null };
    lane.outcomes[row.outcomeClass] = (lane.outcomes[row.outcomeClass] ?? 0) + row.eventCount;
    if (row.durationUsMax != null) lane.maxUs = Math.max(lane.maxUs ?? 0, row.durationUsMax);
    lanes.set(key, lane);
  }

  const laneList = [];
  for (const lane of lanes.values()) {
    let laneSettled = 0;
    for (const count of Object.values(lane.outcomes)) laneSettled += count;

    laneList.push({
      operation: lane.operation,
      component: lane.component,
      outcomes: lane.outcomes,
      settled: laneSettled,
      maxDuration: lane.maxUs == null ? "" : formatMicroseconds(lane.maxUs),
      errorClasses: RAW_ONLY,
      note: lane.operation === "knock_claim" ? KNOCK_NOTE : undefined
    });
  }
  laneList.sort((a, b) => b.settled - a.settled);

  const minuteLabel = stamp => stamp ? stamp.slice(11, 16) : "";
  const window = firstMinute
    ? `${minuteLabel(firstMinute)}–${minuteLabel(lastMinute)} UTC`
    : "no rows in window";

  return {
    status: "live",
    queriedAt,
    room: vitals.room,
    spirit: vitals.spirit,
    truncated: vitals.truncated,
    lanes: laneList,
    channels: { settled, window, tokensIn, tokensOut, usagePoints, drops, retentionReceipts: retention.rows.length }
  };
}

function liveChannels() {
  const facts = live.channels;
  const retentionValue = facts.retentionReceipts === 0
    ? "No receipts yet"
    : `${pulseCount(facts.retentionReceipts)} receipts`;

  return [
    { label: "Settled events", value: `${pulseCount(facts.settled)} events`, detail: `${live.room} room · last 24 h · ${facts.window}` },
    { label: "Tokens", value: `${pulseCount(facts.tokensIn)} in`, detail: `${pulseCount(facts.tokensOut)} out · ${pulseCount(facts.usagePoints)} provider usage points` },
    { label: "Loss", value: `${pulseCount(facts.drops)} dropped`, detail: "writer-side drop receipts · duplicates live in raw rows" },
    { label: "Retention", value: retentionValue, detail: `house-wide · insula.retention.receipts v1 · ${PULSE_SNAPSHOT.retention.days}-day law` }
  ];
}

function snapshotChannels() {
  const totals = PULSE_SNAPSHOT.totals;
  const tokens = PULSE_SNAPSHOT.tokens;
  const retention = PULSE_SNAPSHOT.retention;

  return [
    { label: "Observations", value: `${pulseCount(totals.spans)} spans`, detail: `${pulseCount(totals.writers)} writers · ${totals.components} components · ${totals.window}` },
    { label: "Tokens", value: `${pulseCount(tokens.tokensIn)} in`, detail: `${pulseCount(tokens.tokensOut)} out · ${pulseCount(tokens.usagePoints)} provider usage points` },
    { label: "Loss", value: `${pulseCount(totals.drops)} dropped`, detail: `${pulseCount(totals.duplicates)} duplicate deliveries collapsed at ingest` },
    { label: "Retention", value: "No rows expired", detail: `${pulseCount(retention.sweepRuns)} sweep runs · first expiry ${retention.firstExpiry} · ${retention.days}-day law` }
  ];
}

function renderChannel(channel) {
  return `
    <article class="insula-observation-channel">
      <span>${escapeHtml(channel.label)}</span>
      <strong>${escapeHtml(channel.value)}</strong>
      <p>${escapeHtml(channel.detail)}</p>
    </article>`;
}

function renderLane(lane) {
  let settled = 0;
  const split = [];
  const flags = [];
  for (const [outcome, count] of Object.entries(lane.outcomes)) {
    settled += count;
    split.push(`${pulseCount(count)} ${outcome}`);
    if (outcome !== "ok") flags.push(`<span data-tone="${outcomeTone(outcome)}">${pulseCount(count)} ${escapeHtml(outcome)}</span>`);
  }
  if (flags.length === 0) flags.push('<span data-tone="steady">all ok</span>');

  const note = lane.note ? `<p>${escapeHtml(lane.note)}</p>` : "";

  return `
    <details class="mechanics-row">
      <summary>
        <span class="mechanics-row-title"><strong>${escapeHtml(lane.operation)}</strong><small>${escapeHtml(lane.component)}</small></span>
        <span class="mechanics-row-value"><small>Settled</small><code>${pulseCount(settled)} events</code></span>
        <span class="mechanics-row-flags">${flags.join("")}</span>
      </summary>
      <div class="mechanics-row-body">
        <dl>
          <div><dt>Outcomes</dt><dd>${escapeHtml(split.join(" · "))}</dd></div>
          <div><dt>Max duration</dt><dd>${escapeHtml(lane.maxDuration || "not measured")}</dd></div>
          <div><dt>Error classes</dt><dd>${escapeHtml(lane.errorClasses || "none")}</dd></div>
          <div><dt>Recompute</dt><dd>${escapeHtml(PULSE_SNAPSHOT.vitalsQuery)}</dd></div>
        </dl>
        ${note}
      </div>
    </details>`;
}

function renderReceipt(receipt) {
  const plural = receipt.count === 1 ? "" : "s";

  return `
    <details class="mechanics-row">
      <summary>
        <span class="mechanics-row-title"><strong>${escapeHtml(receipt.kind)}</strong><small>${escapeHtml(receipt.component)}</small></span>
        <span class="mechanics-row-value"><small>Latest</small><code>${escapeHtml(receipt.lastAt)}</code></span>
        <span class="mechanics-row-flags">
          <span data-tone="${outcomeTone(receipt.outcome)}">${escapeHtml(receipt.outcome)}</span>
          <span data-tone="quiet">×${pulseCount(receipt.count)}</span>
        </span>
      </summary>
      <div class="mechanics-row-body">
        <dl class="pulse-receipt-detail">
          <div><dt>Today</dt><dd>${pulseCount(receipt.count)} receipt point${plural}</dd></div>
          <div><dt>Latest receipt id</dt><dd><code>${escapeHtml(receipt.latestId)}</code></dd></div>
        </dl>
      </div>
    </details>`;
}

// The chip says which source is on screen; the rest of the line says where the
// numbers beside it came from.
function statusChips() {
  if (live.status === "live") {
    const truncation = live.truncated ? " · truncated" : "";
    return `
          ${hostLinkChip()}
          <span>${escapeHtml(live.room)} Host · room-scoped vitals</span>
          <span>Queried ${escapeHtml(live.queriedAt)} local${truncation}</span>`;
  }

  return `
          ${hostLinkChip()}
          <span>${escapeHtml(PULSE_SNAPSHOT.source)}</span>
          <span>Captured ${escapeHtml(PULSE_SNAPSHOT.capturedAt)}</span>`;
}

// Slot 2 renders two headers — Pulse's own and the observatory's above it. Both
// chips come from here, so the surface can never say Host connected in one line
// and Host offline in the line above it.
export function hostLinkChip() {
  if (live.status === "live") return '<span data-tone="steady">Host connected</span>';
  if (live.status === "pending") return '<span data-tone="quiet">Querying Host…</span>';
  if (live.status === "failed") {
    return `<span data-tone="attention">Host unreachable · ${escapeHtml(live.reason)}</span>`;
  }

  return '<span data-tone="attention">Host not queried</span>';
}

export function renderHousePulse() {
  const showingLive = live.status === "live";
  const channels = showingLive ? liveChannels() : snapshotChannels();
  const lanes = showingLive ? live.lanes : PULSE_SNAPSHOT.lanes;

  const roomLine = PULSE_SNAPSHOT.rooms.map(([room, spans]) => `${room} ${pulseCount(spans)}`).join(" · ");
  const laneStatus = showingLive
    ? `${lanes.length} lanes · ${live.room} room only · live ${PULSE_SNAPSHOT.vitalsQuery}`
    : `${lanes.length} lanes · ${roomLine}`;

  let receiptPoints = 0;
  for (const receipt of PULSE_SNAPSHOT.receipts) receiptPoints += receipt.count;
  const receiptStatus = `Latest per kind · ${pulseCount(receiptPoints)} points · snapshot ${PULSE_SNAPSHOT.capturedAt} · no Host route yet`;

  const queryLabel = live.status === "pending" ? "Querying…" : "Query Host";

  return `
    <section class="insula-observation" aria-labelledby="house-pulse-title">
      <header class="insula-observation-lead">
        <div>
          <span class="eyebrow">Insula</span>
          <h3 id="house-pulse-title">Pulse</h3>
          <p>Every number recomputable from ${escapeHtml(PULSE_SNAPSHOT.vitalsQuery)} rollups and raw receipt points.</p>
        </div>
        <div class="pulse-status">
          <div class="mechanics-snapshot-status" aria-label="Pulse source status">${statusChips()}</div>
          <button class="card-verb" type="button" data-pulse-refresh>${queryLabel}</button>
        </div>
      </header>

      <div class="insula-observation-grid">${channels.map(renderChannel).join("")}
      </div>

      <section class="pulse-block" aria-labelledby="pulse-lanes-title">
        <header class="pulse-block-lead">
          <h4 id="pulse-lanes-title">Lanes</h4>
          <small>${escapeHtml(laneStatus)}</small>
        </header>
        <div class="pulse-rows">${lanes.map(renderLane).join("")}</div>
      </section>

      <section class="pulse-block" aria-labelledby="pulse-receipts-title">
        <header class="pulse-block-lead">
          <h4 id="pulse-receipts-title">Receipts</h4>
          <small>${escapeHtml(receiptStatus)}</small>
        </header>
        <div class="pulse-rows">${PULSE_SNAPSHOT.receipts.map(renderReceipt).join("")}</div>
      </section>
    </section>`;
}
