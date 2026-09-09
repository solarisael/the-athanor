// The footer, Account, observatory, and Direct Status share these Host rounds.
// Missing contract fields remain not reported, even when a request fails.

// These fields feed the five footer channels, not the full health payload.
const HEALTH_FIELDS = [
  "status", "schema_version", "websocket_path", "projection_id",
  "version", "sequence", "state_hash", "akasha_delivery", "insula"
];

const NOT_REPORTED = "The room health round carries no such field.";
const CONTRACT_NOTE = `The footer reads these health fields: ${HEALTH_FIELDS.join(", ")}.`;

// Source state: idle → pending → live | failed. Nothing outside this module
// mutates it; the shell asks for rendered channel text and a render request.
let round = { status: "idle" };
let roomRound = { status: "idle" };

// The reconnect loop. A failed health round arms one retry with a growing
// delay; a live round after any retry is a recovery, and every listener asks
// its own door again. A live round asks again every 30 s, quietly, so a Host
// that dies while nobody is typing is noticed before the next send. Nothing
// here re-sends anything.
const RETRY_DELAYS_MS = [2000, 4000, 8000, 15000];
const HEARTBEAT_MS = 30000;
let retry = { timer: null, attempt: 0, delayMs: null };
const recoveredListeners = new Set();

export function onHostRecovered(listener) {
  recoveredListeners.add(listener);
}

// Chat and the other doors report a transport-class failure here so one loop
// owns the retry clock instead of every door polling a dead Host.
export function noteHostFailure(reason) {
  if (round.status === "failed" || round.status === "pending") return;
  round = { status: "failed", reason };
  scheduleRetry();
  requestRender();
}

// The one door to a /live route. Every failure comes back as one error shape:
// `status` (undefined when nothing answered), the proxy's `hop` when it names
// one, and `transport` for the class the reconnect loop owns — no answer, a
// 502 from the proxy, or a 5xx from the Host. A 4xx is the Host refusing and
// stays with the caller.
const HOP_REASONS = {
  host_unreachable: "Host not reachable",
  host_timeout: "Host did not answer in 20 s",
  host_transport: "Host connection broke",
  proxy: "local proxy failed"
};

export async function askHost(path, body = {}) {
  let response;
  try {
    response = await fetch(path, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body)
    });
  } catch (error) {
    throw Object.assign(new Error("no route to Host"), { status: undefined, hop: null, transport: true, cause: error });
  }
  if (response.ok) return response.json();
  let reason = `Host answered ${response.status}`;
  let hop = null;
  if (response.headers.get("content-type")?.includes("application/json")) {
    const result = await response.json().catch(() => null);
    if (result && typeof result.error === "string") reason = result.error;
    if (result && typeof result.hop === "string") {
      hop = result.hop;
      reason = HOP_REASONS[hop] ?? reason;
    }
  }
  throw Object.assign(new Error(reason), { status: response.status, hop, transport: response.status >= 500 });
}

function scheduleRetry() {
  clearTimeout(retry.timer);
  const delayMs = RETRY_DELAYS_MS[Math.min(retry.attempt, RETRY_DELAYS_MS.length - 1)];
  retry = { timer: setTimeout(() => { void queryHealthHost(); }, delayMs), attempt: retry.attempt + 1, delayMs };
}

function retrySuffix() {
  return retry.delayMs ? ` · retry ${retry.attempt} in ${Math.round(retry.delayMs / 1000)} s` : "";
}

export function roomState() {
  return roomRound;
}

export function healthState() {
  return round.status === "live" ? round.health : null;
}

export function roomStateChannel() {
  if (roomRound.status === "live") return chip("Room state connected", "Room", "steady", `Room ${roomRound.room} · Queried ${roomRound.queriedAt} local`);
  if (roomRound.status === "failed" && roomRound.reached) return chip("Room state not served", "Room —", "quiet", `${roomRound.reason} · not reported by the Host`);
  if (roomRound.status === "failed") return chip("Room state unreachable", "Room off", "attention", `${roomRound.reason} · not reported by the Host`);
  if (roomRound.status === "pending") return chip("Room state querying…", "Room …", "quiet", "The room state round is open.");
  return chip("Room state not queried", "Room —", "quiet", "Room state not reported by the Host.");
}

export function ensureRoomStateQueried() {
  if (roomRound.status === "idle") queryRoomStateHost();
}

let roomInFlight = false;

async function queryRoomStateHost() {
  if (roomInFlight) return;
  roomInFlight = true;
  if (roomRound.status !== "live") roomRound = { ...roomRound, status: "pending" };
  requestRender();
  try {
    const room = await askHost("/live/room/state");
    if (!room || typeof room.room !== "string" || !Array.isArray(room.presences)) {
      throw new Error("Host answered without room state");
    }
    roomRound = { ...room, status: "live", queriedAt: new Date().toTimeString().slice(0, 5) };
  } catch (error) {
    // A reached Host with no door is a missing fact, never an unreachable Host;
    // the footer must not say connected while this line says offline. A
    // transport failure is the opposite case: the Host was not reached.
    const reached = error.status !== undefined && !error.transport;
    roomRound = {
      ...roomRound,
      status: "failed",
      reached,
      reason: reached ? `this Host answers no room-state door (${error.status})` : error.message
    };
  }
  roomInFlight = false;
  requestRender();
}
let requestRender = () => {};

export function initHealth(options) {
  requestRender = options.requestRender;
}

export function ensureHealthQueried() {
  ensureRoomStateQueried();
  if (round.status === "idle") queryHealthHost();
}

let healthInFlight = false;

export async function queryHealthHost() {
  void queryRoomStateHost();
  if (healthInFlight) return;
  healthInFlight = true;
  const recovering = retry.attempt > 0;
  clearTimeout(retry.timer);
  retry = { ...retry, timer: null };
  // A live round stays live while it refreshes; only a cold or failed round
  // shows the query, so the heartbeat never flickers the footer.
  if (round.status !== "live") round = { status: "pending" };
  requestRender();

  try {
    const health = await askHost("/live/health");
    if (!health || typeof health !== "object" || typeof health.status !== "string") {
      throw new Error("Host answered without a health status");
    }
    round = { status: "live", health, queriedAt: new Date().toTimeString().slice(0, 5) };
    retry = { timer: setTimeout(() => { void queryHealthHost(); }, HEARTBEAT_MS), attempt: 0, delayMs: null };
    if (recovering) for (const listener of recoveredListeners) listener();
  } catch (error) {
    round = { status: "failed", reason: error.message };
    scheduleRetry();
  }
  healthInFlight = false;
  requestRender();
}

function count(value) {
  return typeof value === "number" ? value.toLocaleString("en-US") : "not reported";
}

// The Host names its own room inside the WebSocket path it publishes; the page
// is never told the room by anything but the Host's answer.
function hostRoom(health) {
  const parts = String(health.websocket_path ?? "").split("/");
  return parts[1] === "room" && parts[2] ? parts[2] : "unnamed room";
}

function shortHash(value) {
  return typeof value === "string" && value.length > 12 ? `${value.slice(0, 12)}…` : String(value);
}

// One channel reader per chip. Each returns the strip text, the narrow text,
// the popover title and detail, and the tone its dot carries.
const CHANNELS = {
  host: {
    idle: () => chip("Host not queried", "Host —", "quiet", "Nothing has been asked of the Host yet."),
    pending: () => chip("Host querying…", "Host …", "quiet", "The health round is open."),
    live: health => chip(
      `Host ${health.status} · ${hostRoom(health)}`,
      `Host ${health.status}`,
      health.status === "ok" ? "steady" : "attention",
      `Room ${hostRoom(health)} answered its health route with status ${health.status}, API schema version ${health.schema_version ?? "not reported"}, WebSocket path ${health.websocket_path ?? "not reported"}.`
    ),
    failed: reason => chip("Host unreachable", "Host off", "attention", `The health read failed: ${reason}${retrySuffix()}.`)
  },

  recall: {
    idle: () => chip("Recall not queried", "Recall —", "quiet", "Nothing has been asked of the Host yet."),
    pending: () => chip("Recall querying…", "Recall …", "quiet", "The health round is open."),
    live: health => chip(
      `Recall seq ${count(health.sequence)}`,
      `Recall ${count(health.sequence)}`,
      "steady",
      `The Host serves projection ${health.projection_id} at version ${count(health.version)}, sequence ${count(health.sequence)}, state hash ${shortHash(health.state_hash)}. This is the recall policy cursor the Host publishes, not a count of memories.`
    ),
    failed: reason => chip("Recall unreachable", "Recall off", "attention", `The health read failed: ${reason}.`)
  },

  // Absent from the contract, so this chip does not follow the round.
  body: {
    always: () => chip(
      "Body not reported by Host",
      "No body",
      "quiet",
      `No embodied-session or active-body field exists in the room health round, so this surface shows none rather than a zero. ${NOT_REPORTED} ${CONTRACT_NOTE}`
    )
  },

  kittens: {
    always: () => chip(
      "Kittens not reported by Host",
      "No count",
      "quiet",
      `No kitten, worker-lane, or subagent field exists in the room health round, so this surface shows no count rather than zero. ${NOT_REPORTED} ${CONTRACT_NOTE}`
    )
  },

  delivery: {
    idle: () => chip("Delivery not queried", "Delivery —", "quiet", "Nothing has been asked of the Host yet."),
    pending: () => chip("Delivery querying…", "Delivery …", "quiet", "The health round is open."),
    live: health => deliveryChip(health.akasha_delivery),
    failed: reason => chip("Delivery unreachable", "Delivery off", "attention", `The health read failed: ${reason}.`)
  }
};

function chip(full, compact, tone, detail) {
  return { full, compact, tone, detail };
}

function deliveryChip(delivery) {
  if (!delivery || typeof delivery.broker_status !== "string") {
    return chip(
      "Delivery not reported by Host",
      "No delivery",
      "quiet",
      `The round carried no AKASHA delivery block. ${NOT_REPORTED}`
    );
  }
  const status = delivery.broker_status;
  const tone = status === "connected" ? "steady" : status === "degraded" ? "attention" : "quiet";
  const parts = [
    `AKASHA ${typeof delivery.akasha_enabled === "boolean" ? delivery.akasha_enabled ? "enabled" : "disabled" : "not reported"}`,
    `broker ${typeof delivery.broker_configured === "boolean" ? delivery.broker_configured ? "configured" : "not configured" : "not reported"}`,
    `status ${status}`
  ];
  if (delivery.latest_event_id) parts.push(`latest receipt event ${delivery.latest_event_id}`);
  if (typeof delivery.latest_original_stream_sequence === "number") {
    parts.push(`stream sequence ${count(delivery.latest_original_stream_sequence)}`);
  }
  if (delivery.last_error) parts.push(`last error ${delivery.last_error}`);

  return chip(
    `Delivery ${status}`,
    status.charAt(0).toUpperCase() + status.slice(1),
    tone,
    `${parts.join(" · ")}.`
  );
}

// [gui/prototype/status] — the strip order is the footer's order.
export const STATUS_CHANNELS = ["host", "recall", "body", "kittens", "delivery"];

export function statusChannel(name) {
  const channel = CHANNELS[name];
  if (channel.always) return channel.always();
  if (round.status === "live") return channel.live(round.health);
  if (round.status === "failed") return channel.failed(round.reason);
  if (round.status === "pending") return channel.pending();
  return channel.idle();
}

// One line under every popover naming which source the number above came from.
export function healthSourceLine() {
  if (round.status === "live") {
    return `Host connected · ${hostRoom(round.health)} room health · Queried ${round.queriedAt} local`;
  }
  if (round.status === "pending") return "Querying Host…";
  if (round.status === "failed") return `Host unreachable · ${round.reason}${retrySuffix()}`;
  return "Host not queried";
}

export function healthSourceTone() {
  if (round.status === "live") return "steady";
  if (round.status === "failed") return "attention";
  return "quiet";
}

// The Account state block reads the same round. Surface states what this page
// is allowed to do, not merely whether a socket opened.
export function accountStateRows() {
  if (round.status === "live") {
    const health = round.health;
    return [
      { label: "Surface", value: "Live reads · chat writes only" },
      { label: "Host", value: `${health.status} · ${hostRoom(health)} room` },
      { label: "Persistence", value: persistenceValue(health.insula) }
    ];
  }
  if (round.status === "pending") {
    return [
      { label: "Surface", value: "Local only · querying Host" },
      { label: "Host", value: "Querying…" },
      { label: "Persistence", value: "Querying…" }
    ];
  }
  if (round.status === "failed") {
    return [
      { label: "Surface", value: `Local only · Host unreachable${retrySuffix()}` },
      { label: "Host", value: "Unreachable" },
      { label: "Persistence", value: "Unreachable" }
    ];
  }
  return [
    { label: "Surface", value: "Local only · Host not queried" },
    { label: "Host", value: "Not queried" },
    { label: "Persistence", value: "Not queried" }
  ];
}

// The Host's only persistence reading is the insula pool it writes observations
// through. Its own status word travels; nothing is upgraded to a broader claim.
function persistenceValue(insula) {
  if (!insula || typeof insula.status !== "string") return "Not reported by Host";
  return `PostgreSQL ${insula.status}`;
}

export function persistenceDetail() {
  if (round.status !== "live") return "";
  const insula = round.health.insula;
  if (!insula || typeof insula.status !== "string") {
    return `The round carried no insula block. ${NOT_REPORTED}`;
  }
  return `The Host reports its insula store as ${insula.status} after ${count(insula.successfulOperations)} successful and ${count(insula.failedOperations)} failed operations, at API schema ${insula.schemaVersion}. This is the observation pool the Host writes through; it is the only persistence reading the health round carries.`;
}
