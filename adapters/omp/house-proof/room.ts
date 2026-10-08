// OMP workspace discovery and the native room-state client.

import path from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { HOUSE_STATE_FILENAME, OBSIDIAN_ROOT, ROOM_CAPABILITY_FILENAME } from "./constants.ts";
import { hostCommand, hostSessionIdentity, sendHostCommand, HostUnavailable, HostRefused } from "./host.ts";
import { OrganError } from "./organ.ts";

export function roomNameFromCwd(cwd) {
  return path.basename(String(cwd || "")).toLowerCase();
}

const ROOM_MARKER_FILENAME = ".athanor-room.json";
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
    const markerPath = path.join(roomDir, ROOM_MARKER_FILENAME);
    const parsed = JSON.parse(readFileSync(markerPath, "utf8"));
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function readPersistedHouseState(roomDir) {
  try {
    const statePath = path.join(roomDir, ".omp", "runtime", HOUSE_STATE_FILENAME);
    const parsed = JSON.parse(readFileSync(statePath, "utf8"));
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
  const spirit = normalizeDisplayName(persisted.embodiedSpirit)
    || normalizeDisplayName(marker.trueName)
    || readActiveSpiritName(effectiveRoomDir)
    || normalizeDisplayName(persisted.agentName)
    || DEFAULT_SPIRIT;
  const operator = normalizeDisplayName(persisted.operator)
    || normalizeDisplayName(marker.operator)
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

// Operation-scoped Docket write capability.
//
// The omp environment is global across every room in the House, so an
// environment variable cannot carry a per-room secret; it is an override lane
// for tests and one-room installs and it wins when set. The durable per-room
// answer is the room-local file, read at call time so provisioning a room does
// not require restarting the harness.
//
// The secret is returned to the caller that is about to spend it and nowhere
// else: it never enters a schema, a receipt, or a log line.
export function roomCapabilityPath(effectiveRoomDir) {
  return path.join(effectiveRoomDir, ".omp", "runtime", ROOM_CAPABILITY_FILENAME);
}

export function roomCapability(effectiveRoomDir, environ = process.env) {
  const configured = String(environ.ATHANOR_ROOM_CAPABILITY || "").trim();
  if (configured) return configured;
  try {
    return readFileSync(roomCapabilityPath(effectiveRoomDir), "utf8").trim() || null;
  } catch {
    return null;
  }
}

export type RoomStatePatch = {
  operator?: string;
  embodiedSpirit?: string;
  routingModeEnabled?: boolean;
  modelDefaultEnabled?: boolean;
  modelDefaultModel?: string | null;
};

async function requestRoomState(ctx: any, request: Record<string, unknown>, signal?: AbortSignal) {
  const { room, spirit, effectiveRoomDir } = roomContext(ctx?.cwd || process.cwd());
  const binding = { room, spirit, session: hostSessionIdentity(ctx, effectiveRoomDir) };
  const mutates = request.action !== "read";
  try {
    const response = await sendHostCommand(
      hostCommand(binding, "athanor.room.state", "room", { room_request: request }),
      new Set(["athanor.room.state_result"]),
      signal,
      undefined,
      { settleDefinitively: mutates },
    );
    if (!response.result || typeof response.result !== "object" || Array.isArray(response.result)) {
      throw new OrganError("Native room state returned an invalid result",
        mutates ? "outcome_unknown" : "invalid_response", !mutates);
    }
    return response.result as Record<string, any>;
  } catch (error) {
    if (mutates && error instanceof HostUnavailable && error.dispatched && !(error instanceof HostRefused)) {
      throw new OrganError("Native room state outcome is unknown after dispatch", "outcome_unknown", false, {
        execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" },
        cause: error.message,
      });
    }
    throw error;
  }
}

export function loadRoomState(ctx: any, signal?: AbortSignal) {
  return requestRoomState(ctx, { action: "read" }, signal);
}

export function patchRoomState(ctx: any, patch: RoomStatePatch, signal?: AbortSignal) {
  return requestRoomState(ctx, { action: "patch", ...patch }, signal);
}

export function applyPromptDirectives(ctx: any, prompt: string, nativeUser: boolean, signal?: AbortSignal) {
  return requestRoomState(ctx, { action: "applyPrompt", prompt, nativeUser }, signal);
}
