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

async function queryRoomStateHost() {
  if (roomRound.status === "pending") return;
  roomRound = { ...roomRound, status: "pending" };
  requestRender();
  try {
    const response = await fetch("/live/room/state", {
      method: "POST", headers: { "content-type": "application/json" }, body: "{}"
    });
    // A reached Host with no door is a missing fact, never an unreachable Host;
    // the footer must not say connected while this line says offline.
    if (!response.ok) {
      const missing = new Error(`this Host answers no room-state door (${response.status})`);
      missing.reached = true;
      throw missing;
    }
    const room = await response.json();
    if (!room || typeof room.room !== "string" || !Array.isArray(room.presences)) {
      throw new Error("Host answered without room state");
    }
    roomRound = { ...room, status: "live", queriedAt: new Date().toTimeString().slice(0, 5) };
  } catch (error) {
    roomRound = {
      ...roomRound,
      status: "failed",
      reached: error instanceof Error && error.reached === true,
      reason: error instanceof Error ? error.message : "no route to Host"
    };
  }
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

export async function queryHealthHost() {
  void queryRoomStateHost();
  if (round.status === "pending") return;
  round = { status: "pending" };
  requestRender();

  try {
    const response = await fetch("/live/health", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: "{}"
    });
    if (!response.ok) throw new Error(`Host answered ${response.status}`);
    const health = await response.json();
    if (!health || typeof health !== "object" || typeof health.status !== "string") {
      throw new Error("Host answered without a health status");
    }
    round = { status: "live", health, queriedAt: new Date().toTimeString().slice(0, 5) };
  } catch (error) {
    round = {
      status: "failed",
      reason: error instanceof Error ? error.message : "no route to Host"
    };
  }
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
    failed: reason => chip("Host unreachable", "Host off", "attention", `The health read failed: ${reason}.`)
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
  if (round.status === "failed") return `Host unreachable · ${round.reason}`;
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
      { label: "Surface", value: "Local only · Host unreachable" },
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
