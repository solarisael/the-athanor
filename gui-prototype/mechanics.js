// Mechanical observatory — the configuration census in House slot 2.
//
// Owns the source-census snapshot, the category/query/scroll view state, and
// every observatory render. The shell delegates its clicks and input here and
// asks for the scroll position at render time. Pulse renders inside the
// observatory frame, so this module composes it.

import { escapeHtml } from "./text.js";
import { renderHousePulse, hostLinkChip } from "./pulse.js";

// local interaction snapshot from the 2026-08-18 source census; Host authority remains disconnected
function mechanic(id, label, value, {
  defaultValue = value,
  scope = "House",
  owner = "Code",
  mutability = "Read-only",
  secrecy = "Public",
  apply = "Restart",
  health = "Defined",
  consequence
}) {
  return { id, label, value, defaultValue, scope, owner, mutability, secrecy, apply, health, consequence };
}

export const HOUSE_MECHANICS_SNAPSHOT = {
  capturedAt: "2026-08-18",
  revision: "source-census-2026-08-18",
  categories: [
    {
      id: "recall-context",
      label: "Recall & Context",
      summary: "Retrieval gates, injection bounds, context budgets, and compaction pressure.",
      rows: [
        mechanic("recall.semantic-floor", "Semantic similarity floor", "0.40", {
          health: "Calibrated",
          consequence: "Lower values admit weaker semantic matches; higher values refuse more retrieval candidates."
        }),
        mechanic("recall.content-floor", "Content similarity floor", "0.30", {
          health: "Calibrated",
          consequence: "Controls the weakest content lane match allowed into ranked Recall evidence."
        }),
        mechanic("recall.top-k", "Recall candidate breadth", "8 semantic · 8 content", {
          consequence: "Sets the pre-ranking breadth for each retrieval lane before evidence is merged."
        }),
        mechanic("recall.injection", "Automatic injection ceiling", "5 candidates · 900-char excerpts · 6000-char bodies", {
          owner: "Adapter",
          consequence: "Bounds automatic context growth and prevents one retrieval pass from flooding the conversation."
        }),
        mechanic("context.kodo-budget", "Kodo context budget", "1,000,000 tokens · compact at 90%", {
          scope: "Kodo room",
          owner: "Adapter",
          health: "Configured",
          consequence: "Defines Kodo's larger working window and the point where compaction becomes mandatory."
        }),
        mechanic("context.room-budget", "Default room context budget", "400,000 tokens · compact at 70%", {
          scope: "All other rooms",
          owner: "Adapter",
          health: "Configured",
          consequence: "Defines the ordinary room window and leaves a larger safety margin before model limits."
        }),
        mechanic("context.nudge-bands", "Context pressure bands", "40,000 tokens · warn 20 points early", {
          owner: "Adapter",
          consequence: "Controls when context-pressure nudges appear and how early the operator sees compaction risk."
        }),
        mechanic("timeout.auto-context", "Automatic context timeout", "2s", {
          owner: "Adapter",
          consequence: "A slow background context lane yields rather than blocking the conversational turn."
        }),
        mechanic("timeout.recall", "Recall and Anamnesis timeout", "120s", {
          owner: "Adapter",
          consequence: "Bounds explicit deep retrieval and counsel reads before the tool returns a timeout."
        })
      ]
    },
    {
      id: "memory-lessons",
      label: "Memory, Lessons & Anamnesis",
      summary: "Durable writes, lesson firing, counsel reads, and paper-boat limits.",
      rows: [
        mechanic("memory.write-timeout", "Durable write timeout", "90s", {
          owner: "Adapter",
          consequence: "Bounds Remember and other PostgreSQL-authoritative write receipts."
        }),
        mechanic("lesson.relevance-floor", "Lesson relevance floor", "0.15", {
          health: "Calibrated",
          consequence: "Filters weak lesson matches before the working set can influence a task."
        }),
        mechanic("lesson.working-set", "Lesson working set", "6 lessons", {
          owner: "Adapter",
          consequence: "Caps the number of lessons braided into one task context."
        }),
        mechanic("lesson.trigger-guard", "Lesson trigger guard", "32 patterns · 300ms", {
          owner: "Adapter",
          consequence: "Bounds deterministic trigger scanning and prevents pathological lesson matchers from stalling a turn."
        }),
        mechanic("anamnesis.read-limit", "Anamnesis read bounds", "10 default · 50 maximum", {
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Controls how much lived counsel one explicit Cabinet read may return."
        }),
        mechanic("paper-boat.guardrails", "Paper-boat guardrails", "64 KiB body · 64 unboated rows", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Prevents session continuity from accumulating an unbounded unsent backlog."
        })
      ]
    },
    {
      id: "giga-embeddings",
      label: "GIGA & Embeddings",
      summary: "Stage 1 gates, leases, source windows, model context, and vector health.",
      rows: [
        mechanic("giga.integration-gate", "GIGA integration gate", "Environment-gated", {
          owner: "Environment",
          mutability: "Environment-owned",
          health: "Host offline",
          consequence: "Controls whether Stage 1 processing may start at all."
        }),
        mechanic("giga.claim-owner", "GIGA claim ownership", "One owner per leased event", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Prevents two workers from processing the same event concurrently."
        }),
        mechanic("giga.source-window", "GIGA source window", "8 sources · 8 KiB each · 24 KiB total", {
          consequence: "Bounds evidence carried into one candidate-generation pass."
        }),
        mechanic("giga.lease-attempts", "GIGA lease and attempts", "3600s lease · 5 attempts · 1 candidate", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Controls recovery from abandoned work and caps retry amplification."
        }),
        mechanic("giga.model-context", "GIGA model context", "32768 tokens · 30m keep-alive", {
          owner: "Environment",
          mutability: "Environment-owned",
          consequence: "Defines the local generation window and how long the model remains warm."
        }),
        mechanic("embedding.identity", "Embedding identity", "nomic-embed-text · 768 dimensions", {
          owner: "Environment",
          mutability: "Environment-owned",
          apply: "Migration / reindex",
          health: "Configured",
          consequence: "Changing either value invalidates stored vectors and requires a complete re-embedding."
        }),
        mechanic("embedding.endpoint", "Embedding endpoint", "Configured · value redacted", {
          owner: "Environment",
          mutability: "Environment-owned",
          secrecy: "Sensitive",
          health: "Host offline",
          consequence: "Selects the local or consented remote embedding service without exposing its address here."
        })
      ]
    },
    {
      id: "host-delivery",
      label: "Host, Delivery & Hallways",
      summary: "Connection timing, delivery health, Hallway bounds, and Bell escalation policy.",
      rows: [
        mechanic("host.request-timeout", "Host request timeout", "3s", {
          owner: "Adapter",
          consequence: "Keeps ordinary Host calls from freezing the operator surface."
        }),
        mechanic("host.diagnostic-timeout", "Host diagnostic timeout", "8s", {
          owner: "Adapter",
          consequence: "Allows bounded health inspection more time than ordinary interaction calls."
        }),
        mechanic("host.identity-tuple", "Authenticated identity tuple", "House · room · spirit · session", {
          owner: "Host runtime",
          apply: "Session start",
          health: "Host offline",
          consequence: "Binds every trusted operation to the current House presence without client-supplied identity."
        }),
        mechanic("hallway.guardrails", "Hallway guardrails", "32 KiB body · 32 rooms · reads 50 / 200 max", {
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Bounds message size, membership fanout, and one read request."
        }),
        mechanic("delivery.channels", "Akasha and NATS delivery", "Unavailable · Host offline", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Reports durable authority and immediate-delivery reach when the Host connects."
        }),
        mechanic("delivery.retry-state", "Delivery instance and retries", "Unavailable", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Would expose the active delivery instance, pending retries, and last failure."
        }),
        mechanic("bell.wake-policy", "Bell and wake escalation", "Schema-compatible · policy unset", {
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Policy absent",
          consequence: "Keeps future live knocks and wake authority explicit before autonomous escalation is enabled."
        })
      ]
    },
    {
      id: "rooms-sessions",
      label: "Rooms & Sessions",
      summary: "Room identity, Recall policy, routing, model defaults, and active presences.",
      rows: [
        mechanic("room.state", "Operator and embodied spirit", "PostgreSQL room state", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Changes the room's trusted operator or embodied spirit and refreshes its live declaration."
        }),
        mechanic("room.recall-policy", "Recall policy", "Per room", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Selects whether proactive Recall resolves automatically or follows an explicit room mode."
        }),
        mechanic("room.routing-mode", "Worker routing mode", "Per room", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Live",
          health: "Host offline",
          consequence: "Controls whether bounded work defaults through House worker routing."
        }),
        mechanic("room.model-default", "OMP model default", "Per room", {
          scope: "Per room",
          owner: "PostgreSQL",
          mutability: "Host-writable",
          apply: "Next session",
          health: "Host offline",
          consequence: "Chooses the room's default OMP model selector at the next applicable session boundary."
        }),
        mechanic("room.presences", "Active room presences", "Unavailable", {
          scope: "Per room",
          owner: "PostgreSQL",
          apply: "Live",
          health: "Host offline",
          consequence: "Would show joined sessions, embodied spirits, and their delivery cursors."
        }),
        mechanic("session.delivery-cursor", "Session delivery cursor", "Per authenticated session", {
          scope: "Per session",
          owner: "PostgreSQL",
          apply: "Live",
          consequence: "Prevents the same durable Hallway attention from being injected twice into one session."
        })
      ]
    },
    {
      id: "backups",
      label: "Backups",
      summary: "Retention policy, last success, and database reachability.",
      rows: [
        mechanic("backup.retention", "Backup retention", "3 Rust · 14 shell", {
          owner: "Deployment scripts",
          mutability: "Code-owned",
          apply: "Deploy",
          health: "Divergent",
          consequence: "Two cleanup paths currently retain different counts and should converge before either becomes editable."
        }),
        mechanic("backup.last-success", "Last successful backup", "Unavailable", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Would prove the newest PostgreSQL preservation point and its age."
        }),
        mechanic("database.pool-health", "Database pool and connect health", "Unavailable", {
          owner: "Host runtime",
          apply: "Live",
          health: "Host offline",
          consequence: "Would expose pool saturation, connection reachability, and the last database failure."
        })
      ]
    },
    {
      id: "advanced",
      label: "Advanced Guardrails",
      summary: "Canonical evidence bounds, neighbor context, chunking, clustering, and secret handling.",
      rows: [
        mechanic("recall.canon-bounds", "Canon injection bounds", "6 matches · 3 files", {
          owner: "Adapter",
          consequence: "Caps authoritative canon evidence returned through one Recall pass."
        }),
        mechanic("recall.neighbor-bounds", "Thread neighbor bounds", "6 neighbors · 500 chars each", {
          owner: "Adapter",
          consequence: "Adds bounded chronology around a matched memory without loading whole threads."
        }),
        mechanic("chunk.bounds", "Memory chunk bounds", "400–1200 characters", {
          apply: "Migration / reindex",
          consequence: "Changing chunk shape alters retrieval granularity and invalidates existing embedding assumptions."
        }),
        mechanic("cluster.rebuild", "Cluster rebuild trigger", "500 new chunks or 7 days", {
          consequence: "Controls when the memory taxonomy is recomputed from accumulated semantic material."
        }),
        mechanic("secret.health-only", "Secret exposure", "Presence and health only", {
          owner: "Host runtime",
          mutability: "Never editable here",
          secrecy: "Secret-health-only",
          apply: "N/A",
          health: "Enforced",
          consequence: "Host tokens, database URLs, and passwords never cross into the GUI snapshot."
        })
      ]
    }
  ]
};


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
  if (/offline|absent|divergent|unavailable/i.test(health)) return "attention";
  if (/calibrated|configured|defined|enforced/i.test(health)) return "steady";

  return "quiet";
}

function mechanicsEntries() {
  const needle = query.trim().toLowerCase();
  const categories = needle || category === "all"
    ? HOUSE_MECHANICS_SNAPSHOT.categories
    : HOUSE_MECHANICS_SNAPSHOT.categories.filter(candidate => candidate.id === category);

  return categories.flatMap(group => group.rows
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
        <span class="mechanics-row-value"><small>Effective</small><code>${escapeHtml(row.value)}</code></span>
        <span class="mechanics-row-flags" aria-label="${escapeHtml(`${row.health}; ${row.mutability}`)}">
          <span data-tone="${mechanicsHealthTone(row.health)}">${escapeHtml(row.health)}</span>
          <span data-tone="quiet">${escapeHtml(row.mutability)}</span>
        </span>
      </summary>
      <div class="mechanics-row-body">
        <dl>
          <div><dt>Default</dt><dd>${escapeHtml(row.defaultValue)}</dd></div>
          <div><dt>Scope</dt><dd>${escapeHtml(row.scope)}</dd></div>
          <div><dt>Owner</dt><dd>${escapeHtml(row.owner)}</dd></div>
          <div><dt>Apply</dt><dd>${escapeHtml(row.apply)}</dd></div>
          <div><dt>Secrecy</dt><dd>${escapeHtml(row.secrecy)}</dd></div>
        </dl>
        <p><strong>Consequence.</strong> ${escapeHtml(row.consequence)}</p>
      </div>
    </details>`;
}

export function renderHouseMechanics() {
  const categoryButtons = [
    { id: "all", label: "All", count: HOUSE_MECHANICS_SNAPSHOT.categories.reduce((sum, group) => sum + group.rows.length, 0) },
    ...HOUSE_MECHANICS_SNAPSHOT.categories.map(group => ({ id: group.id, label: group.label, count: group.rows.length }))
  ];

  return `
    <section class="mechanics-observatory" aria-labelledby="mechanics-title">
      <header class="mechanics-lead">
        <span class="eyebrow">House mechanics</span>
        <h2 id="mechanics-title">Mechanical observatory</h2>
        <p>Effective values, ownership, health, and consequence from the current source census.</p>
        <div class="mechanics-snapshot-status" aria-label="Snapshot status">
          ${hostLinkChip()}
          <span>Source census · ${escapeHtml(HOUSE_MECHANICS_SNAPSHOT.capturedAt)}</span>
          <span>${escapeHtml(HOUSE_MECHANICS_SNAPSHOT.revision)}</span>
        </div>
      </header>
      ${renderHousePulse()}
      <div class="mechanics-controls">
        <label class="mechanics-search">
          <span>Search every mechanism</span>
          <input type="search" data-mechanics-search value="${escapeHtml(query)}" placeholder="Recall, timeout, Hallway, backup…" autocomplete="off">
        </label>
        <nav class="mechanics-categories" aria-label="Mechanical observatory categories">
          ${categoryButtons.map(button => `
            <button type="button" data-mechanics-category="${escapeHtml(button.id)}" aria-pressed="${String(category === button.id)}">
              <span>${escapeHtml(button.label)}</span><small>${button.count}</small>
            </button>`).join("")}
        </nav>
      </div>
      <p class="mechanics-results-status" role="status" aria-live="polite"></p>
      <div class="mechanics-results"></div>
      <footer>Disconnected surface · PostgreSQL-backed controls may be Host-writable later; every control remains read-only here.</footer>
    </section>`;
}

export function renderMechanicsResults() {
  const observatory = timeline.querySelector(".mechanics-observatory");
  if (!observatory) return;

  const entries = mechanicsEntries();
  const needle = query.trim();
  const active = HOUSE_MECHANICS_SNAPSHOT.categories.find(candidate => candidate.id === category);

  observatory.querySelectorAll("[data-mechanics-category]").forEach(button => {
    button.setAttribute("aria-pressed", String(button.dataset.mechanicsCategory === category));
  });
  observatory.querySelector(".mechanics-results-status").textContent = needle
    ? `${entries.length} mechanism${entries.length === 1 ? "" : "s"} across all categories for “${needle}”`
    : `${entries.length} mechanism${entries.length === 1 ? "" : "s"} · ${active?.label ?? "All categories"}`;
  observatory.querySelector(".mechanics-results").innerHTML = entries.length > 0
    ? entries.map(renderMechanicRow).join("")
    : '<div class="mechanics-empty"><strong>No mechanism matches that search.</strong><span>Try Recall, timeout, Hallway, room, or backup.</span></div>';
}
