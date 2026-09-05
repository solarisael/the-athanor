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
const TRACE_LIMIT = 100;

// Why a lane may have no trace to open. The Host's trace route reads one exact
// trace id; nothing it serves lists the spans behind a lane, and a minute
// rollup is an aggregate over many traces. So the drawer names the missing
// identity rather than inventing a uuid to ask with.
const ROLLUP_NO_TRACE = "insula.vitals.minute v1 rollups aggregate many spans and carry no trace id. The Host's trace route reads one exact trace id, and it serves no route that lists a lane's spans, so this lane has no trace to open.";
const SNAPSHOT_NO_TRACE = "This lane comes from the stamped PostgreSQL snapshot, which recorded rollups only. Query the Host first — and a live lane still needs a trace id the Host does not publish.";

// Pulse-local source state: idle → pending → live | failed. Nothing outside
// this module reads it; the shell sees rendered markup and a render request.
let live = { status: "idle" };

// The drilldown drawer, one lane at a time: idle → pending → live | failed |
// unavailable. The open lane lives here rather than in the <details> element,
// so an async trace answer can re-render without closing the drawer it fills.
let trace = { key: null, status: "idle" };
let requestRender = () => {};

export function initPulse(options) {
  requestRender = options.requestRender;
}

export function ensurePulseQueried() {
  if (live.status === "idle") queryPulseHost();
}

export function handlePulseClick(event) {
  if (event.target.closest("[data-pulse-refresh]")) {
    queryPulseHost();
    return true;
  }

  const summary = event.target.closest("[data-pulse-lane]");
  if (!summary) return false;

  // The drawer owns the open state, so the native <details> toggle stands down.
  event.preventDefault();
  const key = summary.dataset.pulseLane;
  if (trace.key === key) {
    trace = { key: null, status: "idle" };
    renderWithLaneFocus(key);
    return true;
  }

  openLaneTrace(key, summary.dataset.pulseTrace || null);
  return true;
}

// The shell re-renders the whole panel, so the lane the operator activated has
// to be handed its focus back or a keyboard drawer cycle would strand it. The
// double frame outlasts the shell's own scheduled focus.
function renderWithLaneFocus(key) {
  requestRender();
  window.requestAnimationFrame(() => window.requestAnimationFrame(() => {
    document.querySelector(`[data-pulse-lane="${CSS.escape(key)}"]`)?.focus({ preventScroll: true });
  }));
}

// One lane's spans, read from the Host's trace route. A missing identity, a
// refusal, an unparsable body, and an empty answer are four different states,
// and each one says which it is instead of borrowing another's words.
async function openLaneTrace(key, traceId) {
  if (!traceId) {
    trace = {
      key,
      status: "unavailable",
      reason: live.status === "live" ? ROLLUP_NO_TRACE : SNAPSHOT_NO_TRACE
    };
    renderWithLaneFocus(key);
    return;
  }

  trace = { key, status: "pending", traceId };
  renderWithLaneFocus(key);

  try {
    const response = await fetch("/live/insula/trace", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ traceId, limit: TRACE_LIMIT })
    });
    const raw = await response.text();
    let answer = null;
    try {
      answer = JSON.parse(raw);
    } catch {
      throw new Error(`Host answered an unparsable body: ${raw.slice(0, 80)}`);
    }
    if (!response.ok) throw new Error(`Host refused: ${answer?.error ?? response.status}`);
    if (!Array.isArray(answer.rows)) throw new Error("Host answered without a rows collection");

    trace = {
      key,
      status: "live",
      traceId,
      rows: answer.rows,
      truncated: answer.truncated === true,
      queryName: `${answer.queryName} v${answer.queryVersion}`,
      queriedAt: new Date().toTimeString().slice(0, 5)
    };
  } catch (error) {
    trace = {
      key,
      status: "failed",
      traceId,
      reason: error instanceof Error ? error.message : "no route to Host"
    };
  }
  renderWithLaneFocus(key);
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
      note: lane.operation === "knock_claim" ? KNOCK_NOTE : undefined,
      // Rollups carry no trace identity; the drawer says so rather than guessing.
      traceId: null
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
    channels: { settled, window, tokensIn, tokensOut, usagePoints, drops, rows: vitals.rows.length, retentionReceipts: retention.rows.length }
  };
}

// Zero drops and unknown drops are different answers. drop_count_sum only means
// zero when there were rollup rows to sum; with no rows the honest reading is
// that this window reports nothing about loss.
function lossValue(facts) {
  if (facts.rows === 0) return "Not known";
  return facts.drops === 0 ? "0 dropped" : `${pulseCount(facts.drops)} dropped`;
}

function lossDetail(facts) {
  if (facts.rows === 0) {
    return "No rollup rows in window · nothing to sum, so drops are unknown, not zero";
  }
  if (facts.drops === 0) {
    return `No writer drops · drop_count_sum summed to zero across ${pulseCount(facts.rows)} rollup rows`;
  }
  return `writer-side drop_count_sum across ${pulseCount(facts.rows)} rollup rows · duplicates live in raw rows`;
}

function liveChannels() {
  const facts = live.channels;
  const retentionValue = facts.retentionReceipts === 0
    ? "No receipts yet"
    : `${pulseCount(facts.retentionReceipts)} receipts`;

  return [
    { label: "Settled events", value: `${pulseCount(facts.settled)} events`, detail: `${live.room} room · last 24 h · ${facts.window}` },
    { label: "Tokens", value: `${pulseCount(facts.tokensIn)} in`, detail: `${pulseCount(facts.tokensOut)} out · ${pulseCount(facts.usagePoints)} provider usage points` },
    { label: "Loss", value: lossValue(facts), detail: lossDetail(facts) },
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
    { label: "Loss", value: totals.drops === 0 ? "0 dropped" : `${pulseCount(totals.drops)} dropped`, detail: `${totals.drops === 0 ? "No writer drops in the captured window" : "writer-side drop receipts"} · ${pulseCount(totals.duplicates)} duplicate deliveries collapsed at ingest` },
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

// A lane is keyed by the pair its rollup was grouped on, so the same lane keeps
// its drawer across a re-query.
function laneKey(lane) {
  return `${lane.component} ${lane.operation}`;
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
  const key = laneKey(lane);
  const open = trace.key === key ? " open" : "";
  // Present only when the lane's source actually carries a trace identity, so
  // the door is never advertised where there is nothing behind it.
  const identity = lane.traceId ? ` data-pulse-trace="${escapeHtml(lane.traceId)}"` : "";

  return `
    <details class="mechanics-row"${open}>
      <summary data-pulse-lane="${escapeHtml(key)}"${identity}>
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
        ${renderLaneTrace(key)}
      </div>
    </details>`;
}

// The drawer under the open lane: exactly what the Host said about its spans.
function renderLaneTrace(key) {
  if (trace.key !== key) return "";

  if (trace.status === "pending") {
    return traceDrawer(
      '<span data-tone="quiet">Reading trace…</span>',
      `<p>Asked the Host for trace ${escapeHtml(trace.traceId)}.</p>`
    );
  }
  if (trace.status === "unavailable") {
    return traceDrawer(
      '<span data-tone="attention">No trace to open</span>',
      `<p>${escapeHtml(trace.reason)}</p>`
    );
  }
  if (trace.status === "failed") {
    return traceDrawer(
      '<span data-tone="attention">Trace refused</span>',
      `<p>${escapeHtml(trace.reason)}</p><p>Requested trace ${escapeHtml(trace.traceId)}.</p>`
    );
  }
  if (trace.rows.length === 0) {
    return traceDrawer(
      '<span data-tone="quiet">No spans</span>',
      `<p>${escapeHtml(trace.queryName)} answered zero rows for trace ${escapeHtml(trace.traceId)} inside this room's scope.</p>`
    );
  }

  const truncation = trace.truncated ? ` · truncated at ${TRACE_LIMIT}` : "";
  return traceDrawer(
    `<span data-tone="steady">${pulseCount(trace.rows.length)} spans</span>`,
    `<p>${escapeHtml(trace.queryName)} · trace ${escapeHtml(trace.traceId)} · queried ${escapeHtml(trace.queriedAt)} local${escapeHtml(truncation)}</p>
        <div class="pulse-trace-spans">${trace.rows.map(renderTraceSpan).join("")}</div>`
  );
}

function traceDrawer(chip, body) {
  return `
        <section class="pulse-trace" aria-label="Lane trace">
          <header class="pulse-block-lead">
            <h5>Trace</h5>
            <div class="mechanics-snapshot-status">${chip}</div>
          </header>
          ${body}
        </section>`;
}

// Spans render flat rather than as nested disclosures: a trace is read in
// order, and a nested toggle would lose its state on the next render.
function renderTraceSpan(row) {
  const duration = row.durationUs == null ? "open" : formatMicroseconds(row.durationUs);
  const errorFlag = row.errorClass
    ? `<span data-tone="attention">${escapeHtml(row.errorClass)}</span>`
    : "";
  const dropFlag = row.dropCount > 0
    ? `<span data-tone="attention">${pulseCount(row.dropCount)} dropped</span>`
    : "";

  return `
            <div class="pulse-trace-span">
              <span class="mechanics-row-title"><strong>${escapeHtml(row.operation)}</strong><small>${escapeHtml(row.component)} · ${escapeHtml(row.phase)} · ${escapeHtml(row.observedAt)}</small><code>${escapeHtml(row.spanId)}</code></span>
              <span class="mechanics-row-value"><small>Duration</small><code>${escapeHtml(duration)}</code></span>
              <span class="mechanics-row-flags"><span data-tone="${outcomeTone(row.outcomeClass)}">${escapeHtml(row.outcomeClass)}</span>${errorFlag}${dropFlag}</span>
            </div>`;
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
