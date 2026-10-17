import { escapeHtml } from "./text.js";
import { healthState, roomState, roomStateChannel, healthSourceLine } from "./health.js";
import { pulseState } from "./pulse.js";

const ABSENT = "not reported by the Host";
const value = input => input == null ? ABSENT : typeof input === "object" ? JSON.stringify(input) : String(input);
const fields = (object, names) => names
  .filter(name => object?.[name] != null)
  .map(name => `${name}: ${value(object[name])}`)
  .join(" · ") || ABSENT;

export function liveMechanic(row) {
  const health = healthState();
  const state = roomState();
  const room = state.status === "live" ? state : null;
  const pulse = pulseState();
  const delivery = health?.akasha_delivery;
  let current = ABSENT;
  let attention = false;
  let detail = "The current responses do not provide this setting.";
  switch (row.id) {
    case "room.state":
      current = fields(room, ["room", "operator", "spirit"]);
      break;
    case "room.recall-policy": current = value(room?.recallPolicy); break;
    case "room.routing-mode": current = value(room?.routingMode); break;
    case "room.model-default":
      current = room && Object.hasOwn(room, "modelDefault") && room.modelDefault === null ? "No model default set" : value(room?.modelDefault);
      break;
    case "room.presences": current = value(room?.presences); break;
    case "room.chat": current = value(room?.chat); break;
    case "recall.projection":
      current = fields(health, ["projection_id", "version", "sequence", "state_hash"]);
      break;
    case "host.status":
      current = fields(health, ["status", "schema_version", "hostApi", "websocket_path"]);
      attention = health?.status != null && health.status !== "ok";
      break;
    case "delivery.channels":
      current = fields(delivery, ["akasha_enabled", "broker_configured", "broker_status"]);
      attention = delivery?.broker_status === "degraded";
      break;
    case "delivery.receipt":
      current = fields(delivery, ["latest_event_id", "latest_original_stream_sequence"]);
      break;
    case "delivery.failure":
      current = delivery && Object.hasOwn(delivery, "last_error") && delivery.last_error === null ? "No last error reported" : value(delivery?.last_error);
      attention = !!delivery?.last_error;
      break;
    case "insula.health":
      current = fields(health?.insula, ["status", "schemaVersion", "successfulOperations", "failedOperations"]);
      attention = health?.insula?.status === "degraded";
      break;
    case "insula.vitals":
      if (pulse.status === "live") {
        current = `${pulse.queryName} · ${pulse.channels.rows} returned rows · ${pulse.room} · ${pulse.start}–${pulse.end}${pulse.truncated ? " · truncated" : ""}`;
      }
      detail = pulse.status === "failed" ? pulse.reason : `Vitals read: ${pulse.status}.`;
      attention = pulse.status === "failed";
      break;
    case "insula.retention": {
      const retention = pulse.retention;
      const latest = retention?.rows?.[0];
      current = latest ? fields(latest, ["receiptId", "retentionDays", "createdAt", "sweptThrough", "eventCount", "writerCount", "tombstoneEventCount"])
        : retention?.status === "live" ? "No receipts returned; retention days not reported" : ABSENT;
      detail = retention?.status === "failed" ? retention.reason
        : retention?.status === "live" ? `${retention.queryName} v${retention.queryVersion} · ${retention.rows.length} returned receipts${retention.truncated ? " · truncated" : ""}.`
        : "Retention has not been read.";
      attention = retention?.status === "failed";
      break;
    }
  }
  if (row.source === "Health") detail = healthSourceLine();
  if (row.source === "Room state") detail = `${roomStateChannel().full} · ${roomStateChannel().detail}`;
  return {
    ...row,
    value: current,
    mutability: "Read-only",
    health: attention ? "Read needs attention" : current === ABSENT ? "Not reported" : "Host reported",
    tone: attention ? "attention" : current === ABSENT ? "quiet" : "steady",
    detail
  };
}

export function renderDirectStatus(item) {
  const state = roomState();
  const channel = roomStateChannel();
  const health = healthState();
  const lead = `<section class="state-section"><h2>State of ${escapeHtml(item.name)}</h2><p>${escapeHtml(healthSourceLine())}</p><p data-tone="${channel.tone}">${escapeHtml(channel.full)} · ${escapeHtml(channel.detail)}</p></section>`;
  if (state.status !== "live") return `<div class="specimen-stack">${lead}<p>Room state ${ABSENT}.</p></div>`;
  if (state.room !== item.room) return `<div class="specimen-stack">${lead}<p>${escapeHtml(`The selected Host reports ${state.room}, not ${item.room}.`)}</p></div>`;
  const cards = [
    ["Room", [
      ["Room", state.room],
      ["Operator", state.operator],
      ["Spirit", state.spirit],
      ["Registered presences", Array.isArray(state.presences) ? state.presences.length : null],
      ["Chat entries", state.chat?.entries],
      ["Last chat activity", state.chat?.lastAt]
    ]],
    ["Policies", [
      ["Requested Recall mode", state.recallPolicy?.requestedMode],
      ["Resolved Recall mode", state.recallPolicy?.resolvedMode],
      ["Worker routing", state.routingMode],
      ["Model default", Object.hasOwn(state, "modelDefault") && state.modelDefault === null ? "No model default set" : state.modelDefault]
    ]],
    ["Host", [
      ["Host status", health?.status],
      ["Insula status", health?.insula?.status],
      ["Broker state", health?.akasha_delivery?.broker_status]
    ]]
  ];
  const diagnostics = {
    presences: state.presences,
    recallPolicy: state.recallPolicy,
    hostApi: health?.hostApi,
    insula: health?.insula,
    delivery: health?.akasha_delivery
  };
  const scalar = fact => fact != null && typeof fact !== "object" ? String(fact) : ABSENT;
  return `<div class="specimen-stack">${lead}${cards.map(([title, facts]) => `<section class="state-section"><span class="eyebrow">${title}</span><dl class="context-list">${facts.map(([label, fact]) => `<dt>${label}</dt><dd>${escapeHtml(scalar(fact))}</dd>`).join("")}</dl></section>`).join("")}<details class="state-section"><summary>Technical diagnostics</summary><pre>${escapeHtml(JSON.stringify(diagnostics, null, 2))}</pre></details></div>`;
}
