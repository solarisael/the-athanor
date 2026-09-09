import { escapeHtml } from "./text.js";
import { healthState, roomState, roomStateChannel, healthSourceLine } from "./health.js";

const ABSENT = "not reported by the Host";
const value = input => input == null ? ABSENT : typeof input === "object" ? JSON.stringify(input) : String(input);
const fields = (object, names) => names.filter(name => object?.[name] != null).map(name => `${name}: ${value(object[name])}`).join(" · ") || ABSENT;

export function liveMechanic(row) {
  if (row.health !== "Host offline") return row;
  const health = healthState();
  const state = roomState();
  const room = state.status === "live" ? state : null;
  const database = health?.substrate?.database;
  const backup = health?.backup;
  const delivery = health?.akasha_delivery;
  let current = ABSENT;
  let attention = false;
  switch (row.id) {
    case "room.state": current = room ? `operator: ${value(room.operator)} · spirit: ${value(room.spirit)}` : ABSENT; break;
    case "room.recall-policy": current = value(room?.recallPolicy); break;
    case "room.routing-mode": current = value(room?.routingMode); break;
    case "room.model-default": current = value(room?.modelDefault); break;
    case "room.presences": current = room ? `${room.presences.length} sessions${room.presences.length ? ` · ${room.presences.map(p => p.session).join(" · ")}` : ""}` : ABSENT; break;
    case "host.identity-tuple": current = room ? fields(room, ["room", "operator", "spirit"]) : ABSENT; break;
    case "database.pool-health":
      current = fields(database, ["reachable", "schemaVersion", "pendingMigrations", "pool", "poolSize", "poolIdle", "poolMax", "lastError"]);
      attention = database?.reachable === false || database?.pendingMigrations > 0; break;
    case "backup.last-success":
      current = fields(backup, ["newest", "ageHours", "bytes"]);
      attention = backup?.ageHours > 24; break;
    case "delivery.channels":
      current = fields(delivery, ["akasha_enabled", "broker_configured", "broker_status", "last_error"]);
      attention = delivery?.broker_status != null && delivery.broker_status !== "connected"; break;
    case "delivery.retry-state": current = fields(delivery, ["instance", "pendingRetries", "last_error"]); attention = !!delivery?.last_error; break;
  }
  return { ...row, value: current, defaultValue: ABSENT, mutability: "Read-only", health: current === ABSENT ? ABSENT : "Host reported", tone: current === ABSENT ? "quiet" : attention ? "attention" : "steady" };
}

export function renderDirectStatus(item) {
  const state = roomState();
  const channel = roomStateChannel();
  const health = healthState();
  const lead = `<section class="state-section"><h2>State of ${escapeHtml(item.name)}</h2><p>${escapeHtml(healthSourceLine())}</p><p data-tone="${channel.tone}">${escapeHtml(channel.full)} · ${escapeHtml(channel.detail)}</p></section>`;
  if (state.room && state.room !== item.room) return `<div class="specimen-stack">${lead}<p>${escapeHtml(`This Host serves ${state.room}; ${item.room} is not reported here`)}</p></div>`;
  if (state.status !== "live") return `<div class="specimen-stack">${lead}<p>Room state ${ABSENT}.</p></div>`;
  const cards = [
    ["Runtime", [["Room", state.room], ["Operator", state.operator], ["Spirit", state.spirit], ["Presences", `${state.presences.length} sessions${state.presences.length ? ` · ${state.presences.map(p => p.session).join(" · ")}` : ""}`], ["Host uptime (seconds)", health?.uptimeSeconds]]],
    ["Attention", [["Recall policy", state.recallPolicy], ["Routing mode", state.routingMode]]],
    ["Context", [["Model default", state.modelDefault], ["Context used", null], ["Active instructions", null]]],
    ["Substrate", [["Database", health?.substrate?.database], ["Backup age (hours)", health?.backup?.ageHours], ["Backup", health?.backup?.newest], ["Delivery", health?.akasha_delivery?.broker_status]]]
  ];
  return `<div class="specimen-stack">${lead}${cards.map(([title, facts]) => `<section class="state-section"><span class="eyebrow">${title}</span><dl class="context-list">${facts.map(([label, fact]) => `<dt>${label}</dt><dd>${escapeHtml(value(fact))}</dd>`).join("")}</dl></section>`).join("")}</div>`;
}
