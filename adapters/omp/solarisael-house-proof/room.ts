// Room resolution and active-spirit state for the OMP adapter.
// Silhouette: identify the current room, persist safe state, and refresh active_spirit.md.

import path from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { HOUSE_STATE_FILENAME, OBSIDIAN_ROOT } from "./constants.ts";

export function roomNameFromCwd(cwd) {
  return path.basename(String(cwd || "")).toLowerCase();
}

const ROOM_MARKER_FILENAME = ".solarisael-room.json";
const DEFAULT_ROOM = "default-room";
const RESERVED_ROOM_KEY = "house";

export function isValidRoomKey(value) {
  return typeof value === "string"
    && value !== RESERVED_ROOM_KEY
    && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}
const DEFAULT_SPIRIT = "Spirit";
const DEFAULT_OPERATOR = "Operator";

function normalizeDisplayName(value) {
  const name = String(value || "").trim();
  if (!name || name.length > 80 || /[\r\n|]/.test(name)) return null;
  return name;
}

function roomDisplayName(room) {
  return String(room || DEFAULT_ROOM)
    .split(/[-_ ]+/)
    .filter(Boolean)
    .map((part) => `${part.charAt(0).toUpperCase()}${part.slice(1)}`)
    .join(" ") || DEFAULT_SPIRIT;
}

function readRoomMarker(roomDir) {
  try {
    const parsed = JSON.parse(readFileSync(path.join(roomDir, ROOM_MARKER_FILENAME), "utf8"));
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function readPersistedHouseState(roomDir) {
  try {
    const parsed = JSON.parse(readFileSync(path.join(roomDir, ".omp", "runtime", HOUSE_STATE_FILENAME), "utf8"));
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}


function readActiveSpiritName(roomDir) {
  try {
    const source = readFileSync(path.join(roomDir, "active_spirit.md"), "utf8");
    return normalizeDisplayName(/^# Active Spirit:\s*(.+)$/m.exec(source)?.[1]);
  } catch {
    return null;
  }
}

function isRoomDirectory(roomDir) {
  return existsSync(path.join(roomDir, ROOM_MARKER_FILENAME))
    || existsSync(path.join(roomDir, "active_spirit.md"))
    || existsSync(path.join(roomDir, ".omp", "runtime", HOUSE_STATE_FILENAME));
}
export function supportedRoom(cwd) {
  if (!isRoomDirectory(cwd)) return DEFAULT_ROOM;
  const marker = readRoomMarker(cwd);
  const markerRoom = isValidRoomKey(marker.room) ? marker.room : null;
  const folderRoom = roomNameFromCwd(cwd);
  return markerRoom || (isValidRoomKey(folderRoom) ? folderRoom : DEFAULT_ROOM);
}

export function roomContext(cwd) {
  const requestedDir = path.resolve(String(cwd || process.cwd()));
  const recognized = isRoomDirectory(requestedDir);
  const marker = readRoomMarker(requestedDir);
  const markedRoom = isValidRoomKey(marker.room) ? marker.room : null;
  const folderRoom = roomNameFromCwd(requestedDir);
  const room = recognized
    ? markedRoom || (isValidRoomKey(folderRoom) ? folderRoom : DEFAULT_ROOM)
    : DEFAULT_ROOM;
  const effectiveRoomDir = recognized
    ? requestedDir
    : path.join(OBSIDIAN_ROOT, DEFAULT_ROOM);
  const persisted = readPersistedHouseState(effectiveRoomDir);
  const spirit = normalizeDisplayName(marker.trueName)
    || readActiveSpiritName(effectiveRoomDir)
    || normalizeDisplayName(persisted.embodiedSpirit)
    || normalizeDisplayName(persisted.agentName)
    || DEFAULT_SPIRIT;
  const operator = normalizeDisplayName(marker.operator)
    || normalizeDisplayName(persisted.operator)
    || DEFAULT_OPERATOR;
  return {
    room,
    spirit,
    operator,
    effectiveRoomDir,
    sharedRoot: path.dirname(effectiveRoomDir),
  };
}

export function statePathForRoom(effectiveRoomDir) {
  return path.join(effectiveRoomDir, ".omp", "runtime", HOUSE_STATE_FILENAME);
}

function defaultHouseState(room, spirit, operator = DEFAULT_OPERATOR) {
  return {
    version: 1,
    operator,
    agentName: spirit,
    embodiedSpirit: spirit,
    ignoredSpiritDirective: null,
    lastSpiritChangeAt: null,
    lastUpdatedAt: null,
    routingMode: {
      enabled: false,
      updatedAt: null,
    },
    modelDefault: {
      enabled: false,
      model: null,
      updatedAt: null,
    },
    room,
  };
}

function lastDirectiveValue(text, label) {
  const pattern = new RegExp(`(?:^|\\n)\\s*${label}:\\s*(.+?)\\s*(?=\\n|$)`, "gi");
  const matches = Array.from(String(text || "").matchAll(pattern));
  return matches.length ? (matches.at(-1)?.[1] || "").trim() : null;
}

function hasDirectiveLine(text, label) {
  const pattern = new RegExp(`(?:^|\\n)\\s*${label}\\s*(?::\\s*.+)?(?=\\n|$)`, "i");
  return pattern.test(String(text || ""));
}

export function normalizeSpiritName(value) {
  return normalizeDisplayName(value);
}

export async function loadRoomState(effectiveRoomDir, room, spirit) {
  const marker = readRoomMarker(effectiveRoomDir);
  const persisted = readPersistedHouseState(effectiveRoomDir);
  const operator = normalizeDisplayName(marker.operator)
    || normalizeDisplayName(persisted.operator)
    || DEFAULT_OPERATOR;
  try {
    const parsed = JSON.parse(await readFile(statePathForRoom(effectiveRoomDir), "utf8"));
    const defaults = defaultHouseState(room, spirit, operator);
    return {
      ...defaults,
      ...parsed,
      room,
      routingMode: { ...defaults.routingMode, ...(parsed.routingMode || {}) },
      modelDefault: { ...defaults.modelDefault, ...(parsed.modelDefault || {}) },
    };
  } catch {
    return defaultHouseState(room, spirit, operator);
  }
}

export async function saveRoomState(effectiveRoomDir, state) {
  const target = statePathForRoom(effectiveRoomDir);
  const next = { ...state, lastUpdatedAt: new Date().toISOString() };
  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, `${JSON.stringify(next, null, 2)}\n`, "utf8");
  return next;
}

export async function applyPromptDirectives(ctx, prompt) {
  const { room, spirit, effectiveRoomDir } = roomContext(ctx?.cwd || process.cwd());
  const current = await loadRoomState(effectiveRoomDir, room, spirit);
  const updates = {};
  const operator = lastDirectiveValue(prompt, "Operator");
  const embody = lastDirectiveValue(prompt, "EMBODY");
  const dismiss = hasDirectiveLine(prompt, "DISMISS");
  if (operator) updates.operator = operator;
  if (dismiss) updates.ignoredSpiritDirective = null;
  if (embody) {
    const resolved = normalizeSpiritName(embody);
    if (resolved) {
      updates.embodiedSpirit = resolved;
      updates.agentName = resolved;
      updates.lastSpiritChangeAt = new Date().toISOString();
      updates.ignoredSpiritDirective = null;
    } else {
      updates.ignoredSpiritDirective = embody;
    }
  }
  const next = Object.keys(updates).length
    ? await saveRoomState(effectiveRoomDir, { ...current, ...updates })
    : current;
  return { effectiveRoomDir, room, spirit, state: next };
}

export async function writeActiveSpiritSnapshot(effectiveRoomDir, state) {
  const existing = await readFile(path.join(effectiveRoomDir, "active_spirit.md"), "utf8").catch(() => "");
  const body = existing.replace(/^# Active Spirit:[^\n]*\nAgent:[^\n]*\nEmbodied:[^\n]*\n\n/, "");
  const spirit = state.embodiedSpirit || state.agentName || DEFAULT_SPIRIT;
  const operator = state.operator || DEFAULT_OPERATOR;
  const content = [
    `# Active Spirit: ${spirit}`,
    `Agent: ${state.agentName || spirit} | Operator: ${operator}`,
    `Embodied: ${spirit} | Conjured: none | Summoned: none`,
    "",
    body || `# SPIRIT: ${spirit}\n`,
  ].join("\n");
  await writeFile(path.join(effectiveRoomDir, "active_spirit.md"), content, "utf8");
}
