// Pulse reads room-scoped Insula rollups and House retention receipts.

import { escapeHtml } from "./text.js";
import { askHost } from "./health.js";

const RAW_ONLY = "raw rows only — not carried in rollups";
const TRACE_LIMIT = 100;

// The drawer shows the newest handful, not a log. The Host caps this read at
// 100; eight is what fits a lane body without becoming its own scroll surface.
const SPANS_LIMIT = 8;

// The same 24 h the lane's own rollups were summed over (see queryPulseHost),
// so the spans in the drawer belong to the population the lane counted. A
// narrower window would show an empty drawer under a lane reporting events.
const SPANS_WINDOW = "24h";

// Only settled spans. A `start` row carries no duration and no real outcome
// yet, and the drawer's two columns are duration and outcome.
const SPANS_PHASE = "end";


// Pulse-local source state: idle → pending → live | failed. Nothing outside
// this module reads it; the shell sees rendered markup and a render request.
let live = { status: "idle" };

// The drilldown, one lane at a time, in two stages. A minute rollup aggregates
// many traces and carries no trace id, so the lane cannot open a trace
// directly: `spans` lists the lane's newest settled spans from the Host, and
// picking one of those supplies the trace id `trace` then reads. Both live here
// rather than in the <details> element, so an async answer can re-render
// without closing the drawer it fills.
let spans = { key: null, status: "idle" };
let trace = { key: null, status: "idle" };
let requestRender = () => {};
let focusGeneration = 0;

export function initPulse(options) {
  requestRender = options.requestRender;
}

export function ensurePulseQueried() {
  if (live.status === "idle") queryPulseHost();
}

// Recovery reopens failed reads without repeating successful cold reads.
export function recoverPulse() {
  if (live.status === "failed" || live.retention?.status === "failed") queryPulseHost();
}

export function handlePulseClick(event) {
  if (event.target.closest("[data-pulse-refresh]")) {
    queryPulseHost();
    return true;
  }

  // A span row sits inside the open lane's drawer, so it is tested before the
  // lane it lives in or the lane would swallow every drill-down click.
  const spanRow = event.target.closest("[data-pulse-span-trace]");
  if (spanRow) {
    event.preventDefault();
    const key = spanRow.dataset.pulseSpanKey;
    const traceId = spanRow.dataset.pulseSpanTrace;
    if (trace.key === key && trace.traceId === traceId) {
      trace = { key: null, status: "idle" };
      renderWithSpanFocus(traceId);
      return true;
    }

    openLaneTrace(key, traceId);
    return true;
  }

  const summary = event.target.closest("[data-pulse-lane]");
  if (!summary) return false;

  // The drawer owns the open state, so the native <details> toggle stands down.
  event.preventDefault();
  const key = summary.dataset.pulseLane;
  if (spans.key === key) {
    spans = { key: null, status: "idle" };
    trace = { key: null, status: "idle" };
    renderWithLaneFocus(key);
    return true;
  }

  openLaneSpans(key, summary.dataset.pulseOperation);
  return true;
}

// The shell re-renders the whole panel, so the control the operator activated
// has to be handed its focus back or a keyboard drawer cycle would strand it.
// The double frame outlasts the shell's own scheduled focus.
function renderWithFocus(selector) {
  const generation = ++focusGeneration;
  const active = document.activeElement;
  const restoreFocus = active === document.body || active === document.querySelector(selector);

  requestRender();

  // A read may replace its drawer, but it does not own a different control's focus.
  if (!restoreFocus) return;

  window.requestAnimationFrame(() => window.requestAnimationFrame(() => {
    if (generation !== focusGeneration) return;

    const target = document.querySelector(selector);
    const active = document.activeElement;

    if (active !== document.body && active !== target) return;

    target?.focus({ preventScroll: true });
  }));
}

function renderWithLaneFocus(key) {
  renderWithFocus(`[data-pulse-lane="${CSS.escape(key)}"]`);
}

// Once the spans list is open, the span row is the control the operator pressed.
// Handing focus back to the lane header instead would strand a keyboard
// operator above the whole list they just chose from, with the trace drawer
// they opened below it.
function renderWithSpanFocus(traceId) {
  renderWithFocus(`[data-pulse-span-trace="${CSS.escape(traceId)}"]`);
}

// The Host scopes spans to its trusted room binding. Rollups have no trace id,
// so this read supplies the identities for the trace drawer.
async function openLaneSpans(key, operation) {
  trace = { key: null, status: "idle" };

  if (live.status !== "live") {
    spans = { key, status: "unavailable", reason: "Live vitals are not available. Query the Host before opening a lane." };
    renderWithLaneFocus(key);
    return;
  }

  const pending = { key, status: "pending", operation };
  spans = pending;
  renderWithLaneFocus(key);

  try {
    const answer = await askHost("/live/insula/spans", {
      operation,
      phase: SPANS_PHASE,
      window: SPANS_WINDOW,
      limit: SPANS_LIMIT
    });

    if (spans !== pending) return;
    if (!Array.isArray(answer?.rows)) throw new Error("Spans response has no rows collection");

    spans = {
      key,
      status: "live",
      operation,
      rows: answer.rows,
      truncated: answer.truncated === true,
      window: answer.window,
      queryName: `${answer.queryName} v${answer.queryVersion}`,
      queriedAt: new Date().toTimeString().slice(0, 5)
    };
  } catch (error) {
    if (spans !== pending) return;

    spans = {
      key,
      status: "failed",
      operation,
      reason: error instanceof Error ? error.message : "no route to Host"
    };
  }
  renderWithLaneFocus(key);
}

// One trace, read from the Host's trace route for a trace id the spans list
// supplied. A refusal, an unparsable body, and an empty answer are three
// different states, and each one says which it is instead of borrowing
// another's words. There is no missing-identity state here any more: the only
// door into this function is a span row that carries a real trace id.
async function openLaneTrace(key, traceId) {
  const pending = { key, status: "pending", traceId };
  trace = pending;
  renderWithSpanFocus(traceId);

  try {
    const answer = await askHost("/live/insula/trace", { traceId, limit: TRACE_LIMIT });

    if (trace !== pending) return;
    if (!Array.isArray(answer?.rows)) throw new Error("Trace response has no rows collection");

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
    if (trace !== pending) return;

    trace = {
      key,
      status: "failed",
      traceId,
      reason: error instanceof Error ? error.message : "no route to Host"
    };
  }
  renderWithSpanFocus(traceId);
}

function pulseCount(value) {
  return typeof value === "number" && Number.isFinite(value) ? value.toLocaleString("en-US") : "not reported";
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

    const [vitals, retention] = await Promise.all([
      askHost("/live/insula/vitals", { start: start.toISOString(), end: end.toISOString(), limit: 4000 })
        .then(answer => {
          if (!Array.isArray(answer?.rows)) throw new Error("Vitals response has no rows collection");
          return { ...answer, status: "live" };
        })
        .catch(error => ({ status: "failed", reason: `Vitals: ${error.message}` })),
      askHost("/live/insula/retention", { limit: 20 })
        .then(answer => {
          if (!Array.isArray(answer?.rows)) throw new Error("Retention response has no rows collection");
          return { ...answer, status: "live" };
        })
        .catch(error => ({ status: "failed", reason: `Retention: ${error.message}` }))
    ]);
    const queriedAt = new Date().toTimeString().slice(0, 5);
    live = vitals.status === "live" ? derivePulseLive(vitals, retention, queriedAt)
      : { status: "failed", reason: vitals.reason, retention, queriedAt };
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
  const sum = (rows, field) => rows.length && rows.every(row => typeof row[field] === "number" && Number.isFinite(row[field]))
    ? rows.reduce((total, row) => total + row[field], 0)
    : null;
  const settledRows = vitals.rows.filter(row => row.phase !== "start");
  for (const row of settledRows) {
    const key = `${row.component} ${row.operation}`;
    const lane = lanes.get(key) ?? { operation: row.operation, component: row.component, rows: [], outcomes: {}, maxUs: null };
    lane.rows.push(row);
    if (typeof row.eventCount === "number") {
      lane.outcomes[row.outcomeClass] = (lane.outcomes[row.outcomeClass] ?? 0) + row.eventCount;
    }
    if (row.durationUsMax != null) lane.maxUs = Math.max(lane.maxUs ?? row.durationUsMax, row.durationUsMax);
    lanes.set(key, lane);
  }
  const laneList = [...lanes.values()].map(lane => ({
    ...lane,
    settled: sum(lane.rows, "eventCount"),
    maxDuration: formatMicroseconds(lane.maxUs),
    errorClasses: RAW_ONLY
  }));
  laneList.sort((a, b) => (b.settled ?? -1) - (a.settled ?? -1));
  const usageRows = vitals.rows.filter(row => row.operation === "provider_usage" && row.outcomeClass === "ok");
  return {
    status: "live",
    queriedAt,
    room: vitals.room,
    spirit: vitals.spirit,
    start: vitals.start,
    end: vitals.end,
    queryName: `${vitals.queryName} v${vitals.queryVersion}`,
    truncated: vitals.truncated === true,
    lanes: laneList,
    retention,
    channels: {
      settled: sum(settledRows, "eventCount"),
      tokensIn: sum(vitals.rows, "tokensInSum"),
      tokensOut: sum(vitals.rows, "tokensOutSum"),
      usagePoints: sum(usageRows, "eventCount"),
      drops: sum(vitals.rows, "dropCountSum"),
      rows: vitals.rows.length
    }
  };
}

export function pulseState() {
  return live;
}

// Zero drops and unknown drops are different answers. drop_count_sum only means
// zero when there were rollup rows to sum; with no rows the honest reading is
// that this window reports nothing about loss.
function lossValue(facts) {
  if (facts.drops == null) return "Not reported";
  return facts.drops === 0 ? "0 dropped" : `${pulseCount(facts.drops)} dropped`;
}

function lossDetail(facts) {
  if (facts.drops == null) {
    return "No complete drop count in the returned rollup rows";
  }
  if (facts.drops === 0) {
    return `No writer drops in returned rows · drop_count_sum summed to zero across ${pulseCount(facts.rows)} rollup rows`;
  }
  return `writer-side drop_count_sum across ${pulseCount(facts.rows)} rollup rows · duplicates live in raw rows`;
}

function liveChannels() {
  const facts = live.channels;
  const retention = live.retention;
  const latest = retention.rows?.[0];
  const coverage = live.truncated ? "partial returned rows" : "returned rows";
  return [
    { label: "Settled events", value: `${pulseCount(facts.settled)} events`, detail: `${live.room} room · ${coverage} · ${live.start}–${live.end}` },
    { label: "Tokens", value: `${pulseCount(facts.tokensIn)} in`, detail: `${pulseCount(facts.tokensOut)} out · ${pulseCount(facts.usagePoints)} provider usage points · ${coverage}` },
    { label: "Loss", value: lossValue(facts), detail: lossDetail(facts) },
    {
      label: "Retention",
      value: retention.status === "failed" ? "Read failed" : `${pulseCount(retention.rows.length)} returned receipts`,
      detail: retention.status === "failed" ? retention.reason
        : `House-wide · ${retention.queryName} v${retention.queryVersion}${retention.truncated ? " · truncated" : ""} · ${latest?.retentionDays == null ? "retention days not reported" : `${latest.retentionDays} days in latest receipt`}`
    }
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
  const settled = lane.settled;
  const split = [];
  const flags = [];
  for (const [outcome, count] of Object.entries(lane.outcomes)) {
    split.push(`${pulseCount(count)} ${outcome}`);
    if (outcome !== "ok") flags.push(`<span data-tone="${outcomeTone(outcome)}">${pulseCount(count)} ${escapeHtml(outcome)}</span>`);
  }
  if (flags.length === 0) flags.push(settled == null
    ? '<span data-tone="quiet">Outcomes not fully reported</span>'
    : '<span data-tone="steady">all ok</span>');
  const key = laneKey(lane);
  const open = spans.key === key ? " open" : "";

  return `
    <details class="mechanics-row"${open}>
      <summary id="pulse-lane-${encodeURIComponent(key)}" data-pulse-lane="${escapeHtml(key)}" data-pulse-operation="${escapeHtml(lane.operation)}">
        <span class="mechanics-row-title"><strong>${escapeHtml(lane.operation)}</strong><small>${escapeHtml(lane.component)}</small></span>
        <span class="mechanics-row-value"><small>Settled</small><code>${pulseCount(settled)} events</code></span>
        <span class="mechanics-row-flags">${flags.join("")}</span>
      </summary>
      <div class="mechanics-row-body">
        <dl>
          <div><dt>Outcomes</dt><dd>${escapeHtml(split.join(" · ") || "not reported")}</dd></div>
          <div><dt>Max duration</dt><dd>${escapeHtml(lane.maxDuration || "not measured")}</dd></div>
          <div><dt>Error classes</dt><dd>${escapeHtml(lane.errorClasses || "none")}</dd></div>
          <div><dt>Source</dt><dd>${escapeHtml(live.queryName)}</dd></div>
        </dl>
        ${renderLaneSpans(key)}
        ${renderLaneTrace(key)}
      </div>
    </details>`;
}

// The first drawer under the open lane: the lane's newest settled spans, and
// the only place a trace id becomes reachable from the surface.
function renderLaneSpans(key) {
  if (spans.key !== key) return "";

  if (spans.status === "pending") {
    return spansDrawer(
      '<span data-tone="quiet">Reading spans…</span>',
      `<p>Asked the Host for the newest ${SPANS_LIMIT} settled spans of ${escapeHtml(spans.operation)} in the last ${SPANS_WINDOW}.</p>`
    );
  }
  if (spans.status === "unavailable") {
    return spansDrawer(
      '<span data-tone="attention">No spans to list</span>',
      `<p>${escapeHtml(spans.reason)}</p>`
    );
  }
  if (spans.status === "failed") {
    return spansDrawer(
      '<span data-tone="attention">Spans refused</span>',
      `<p>${escapeHtml(spans.reason)}</p><p>Requested lane ${escapeHtml(spans.operation)}.</p>`
    );
  }
  if (spans.rows.length === 0) {
    return spansDrawer(
      '<span data-tone="quiet">No spans in window</span>',
      `<p>${escapeHtml(spans.queryName)} answered zero settled spans for ${escapeHtml(spans.operation)} in the last ${escapeHtml(spans.window)} inside this room's scope.</p>`
    );
  }

  const truncation = spans.truncated ? ` · truncated at ${SPANS_LIMIT}` : "";
  return spansDrawer(
    `<span data-tone="steady">${pulseCount(spans.rows.length)} spans</span>`,
    `<p>${escapeHtml(spans.queryName)} · last ${escapeHtml(spans.window)} · queried ${escapeHtml(spans.queriedAt)} local${escapeHtml(truncation)}</p>
        <div class="pulse-trace-spans">${spans.rows.map((row) => renderSpanChoice(key, row)).join("")}</div>`
  );
}

function spansDrawer(chip, body) {
  return `
        <section class="pulse-trace" aria-label="Lane spans">
          <header class="pulse-block-lead">
            <h5>Spans</h5>
            <div class="mechanics-snapshot-status">${chip}</div>
          </header>
          ${body}
        </section>`;
}

// One offered span. It is a real <button> because it performs the drill-down
// the whole row exists for; the trace id rides a data attribute so the click
// handler never has to parse rendered text back into an identity.
function renderSpanChoice(key, row) {
  const duration = row.durationUs == null ? "not measured" : formatMicroseconds(row.durationUs);
  const errorFlag = row.errorClass
    ? `<span data-tone="attention">${escapeHtml(row.errorClass)}</span>`
    : "";
  const chosen = trace.key === key && trace.traceId === row.traceId;

  return `
            <button id="pulse-span-${encodeURIComponent(row.spanId)}" type="button" class="pulse-trace-span" aria-pressed="${chosen}"
              data-pulse-span-key="${escapeHtml(key)}" data-pulse-span-trace="${escapeHtml(row.traceId)}">
              <span class="mechanics-row-title"><strong>${escapeHtml(row.observedAt)}</strong><small>${escapeHtml(row.component)} · span ${escapeHtml(row.spanId)}</small><code>${escapeHtml(row.traceId)}</code></span>
              <span class="mechanics-row-value"><small>Duration</small><code>${escapeHtml(duration)}</code></span>
              <span class="mechanics-row-flags"><span data-tone="${outcomeTone(row.outcomeClass)}">${escapeHtml(row.outcomeClass)}</span>${errorFlag}</span>
            </button>`;
}

// The second drawer: the full causal trace behind the span the operator picked.
function renderLaneTrace(key) {
  if (trace.key !== key) return "";

  if (trace.status === "pending") {
    return traceDrawer(
      '<span data-tone="quiet">Reading trace…</span>',
      `<p>Asked the Host for trace ${escapeHtml(trace.traceId)}.</p>`
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
  const duration = row.durationUs == null ? row.phase === "start" ? "open" : "not measured" : formatMicroseconds(row.durationUs);
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
  const facts = [
    ["Receipt id", receipt.receiptId],
    ["Created", receipt.createdAt],
    ["Retention days", receipt.retentionDays],
    ["Swept through", receipt.sweptThrough],
    ["Window start", receipt.windowStart],
    ["Window end", receipt.windowEnd],
    ["Deleted events", receipt.eventCount],
    ["Writers", receipt.writerCount],
    ["Duplicate count", receipt.duplicateCountSum],
    ["Drop count", receipt.dropCountSum],
    ["Tombstone events", receipt.tombstoneEventCount],
    ["Tombstone writers", receipt.tombstoneWriterCount],
    ["Coverage hash", receipt.coverageHash],
    ["Rollup query", receipt.rollupQueryName],
    ["Rollup version", receipt.rollupQueryVersion],
    ["Rollup watermark", receipt.rollupWatermark]
  ];
  return `
    <details class="mechanics-row">
      <summary>
        <span class="mechanics-row-title"><strong>${escapeHtml(receipt.receiptKind ?? "not reported")}</strong><small>House ${escapeHtml(receipt.houseId ?? "not reported")}</small></span>
        <span class="mechanics-row-value"><small>Deleted events</small><code>${pulseCount(receipt.eventCount)}</code></span>
        <span class="mechanics-row-flags"><span data-tone="quiet">Retention receipt</span></span>
      </summary>
      <div class="mechanics-row-body"><dl class="pulse-receipt-detail">
        ${facts.map(([label, fact]) => `<div><dt>${label}</dt><dd>${escapeHtml(fact == null ? "not reported" : String(fact))}</dd></div>`).join("")}
      </dl></div>
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

  return hostLinkChip();
}

export function hostLinkChip() {
  if (live.status === "live") return '<span data-tone="steady">Vitals connected</span>';
  if (live.status === "pending") return '<span data-tone="quiet">Querying Host…</span>';
  if (live.status === "failed") {
    return `<span data-tone="attention">Pulse read failed · ${escapeHtml(live.reason)}</span>`;
  }

  return '<span data-tone="attention">Host not queried</span>';
}

export function renderHousePulse() {
  const showingLive = live.status === "live";
  const channels = showingLive ? liveChannels() : [];
  const lanes = showingLive ? live.lanes : [];
  const laneStatus = showingLive
    ? `${lanes.length} returned lanes · ${live.room} room · ${live.queryName}${live.truncated ? " · truncated" : ""}`
    : "Live vitals unavailable";
  const retention = live.retention;
  const receiptStatus = retention?.status === "live"
    ? `House-wide · ${retention.queryName} v${retention.queryVersion} · ${retention.rows.length} returned receipts${retention.truncated ? " · truncated" : ""}`
    : retention?.reason ?? (live.status === "pending" ? "Reading retention…" : "Retention has not been read");
  const empty = live.status === "pending" ? "Reading Insula…" : live.status === "failed" ? live.reason : "Insula has not been queried.";
  const queryLabel = live.status === "pending" ? "Querying…" : "Query Host";

  return `
    <section class="insula-observation" aria-labelledby="house-pulse-title">
      <header class="insula-observation-lead">
        <div>
          <span class="eyebrow">Insula</span>
          <h3 id="house-pulse-title">Pulse</h3>
          <p>Room vitals and House retention receipts from Insula.</p>
        </div>
        <div class="pulse-status">
          <div class="mechanics-snapshot-status" aria-label="Pulse source status">${statusChips()}</div>
          <button id="pulse-refresh" class="card-verb" type="button" data-pulse-refresh>${queryLabel}</button>
        </div>
      </header>

      <div class="insula-observation-grid">${channels.length ? channels.map(renderChannel).join("") : `<p>${escapeHtml(empty)}</p>`}
      </div>

      <section class="pulse-block" aria-labelledby="pulse-lanes-title">
        <header class="pulse-block-lead">
          <h4 id="pulse-lanes-title">Lanes</h4>
          <small>${escapeHtml(laneStatus)}</small>
        </header>
        <div class="pulse-rows">${lanes.length ? lanes.map(renderLane).join("") : `<p>${escapeHtml(showingLive ? "No settled lanes in the returned window." : empty)}</p>`}</div>
      </section>

      <section class="pulse-block" aria-labelledby="pulse-receipts-title">
        <header class="pulse-block-lead">
          <h4 id="pulse-receipts-title">Retention receipts</h4>
          <small>${escapeHtml(receiptStatus)}</small>
        </header>
        <div class="pulse-rows">${retention?.status === "live" ? retention.rows.length ? retention.rows.map(renderReceipt).join("") : "<p>No retention receipts returned. Retention days are not reported.</p>" : `<p>${escapeHtml(receiptStatus)}</p>`}</div>
      </section>
    </section>`;
}
