// Session-aware room resolution and the native room-state client.
//
// A session reaches a home in exactly two ways. An explicit `/spirit <room>`
// attachment, persisted in the session's own custom entries and authorized by
// the Host, wins. Without one, the existing home launch still works: a session
// started inside a provisioned room directory (or the keeper's projected
// ATHANOR_CLIENT_ROOM_DIR) is that room. Any other session is an ordinary
// harness session and resolves to no room at all; nothing here invents one.

import path from "node:path";
import { existsSync, readFileSync } from "node:fs";
import { HOUSE_STATE_FILENAME, ROOM_CAPABILITY_FILENAME } from "./constants.ts";
import { hostCommand, hostSessionIdentity, sendHostCommand, HostUnavailable, HostRefused, type HostBinding } from "./host.ts";
import { OrganError } from "./organ.ts";
import { homeExplicitlyOff, sessionAttachment } from "./session.ts";

export function roomNameFromCwd(cwd) {
  return path.basename(String(cwd || "")).toLowerCase();
}

const ROOM_MARKER_FILENAME = ".athanor-room.json";
const RESERVED_ROOM_KEY = "house";

export function isValidRoomKey(value) {
  return typeof value === "string"
    && value !== RESERVED_ROOM_KEY
    && /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(value);
}
const DEFAULT_SPIRIT = "Spirit";
const DEFAULT_OPERATOR = "Operator";

export type RoomContext = {
  room: string;
  spirit: string;
  operator: string;
  effectiveRoomDir: string;
  sharedRoot: string;
  // "attached": a Host-authorized `/spirit` selection. "home": a session
  // started inside its own room directory.
  source: "attached" | "home";
};

// The session persisted a home that it cannot reach right now. Every caller
// that would otherwise act as some room refuses instead: the selection is
// never silently replaced by a generic or cwd-derived identity.
export class SelectedHomeUnavailable extends HostUnavailable {
  room: string;
  constructor(room: string, reason: string) {
    super(`the selected home ${room} is unavailable: ${reason}; run /spirit ${room} to attach again or /spirit off to detach`);
    this.name = "SelectedHomeUnavailable";
    this.code = "selected_home_unavailable";
    this.room = room;
  }
}

// An ordinary harness session asked for a room operation.
export class NoHomeAttached extends HostUnavailable {
  constructor() {
    super("this session has no attached home; run /spirit <room> to attach one");
    this.name = "NoHomeAttached";
    this.code = "no_home_attached";
  }
}

function normalizeDisplayName(value) {
  const name = String(value || "").trim();
  if (!name || name.length > 80 || /[\r\n|]/.test(name)) return null;
  return name;
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

// The home a launch directory stands in, or null. The keeper's projected
// ATHANOR_CLIENT_ROOM_DIR wins over the cwd it starts the harness in.
function launchRoomDirectory(cwd) {
  const bound = process.env.ATHANOR_CLIENT_ROOM_DIR;
  if (!bound) return path.resolve(String(cwd || process.cwd()));
  if (!path.isAbsolute(bound) || !isValidRoomKey(readRoomMarker(bound).room)) {
    throw new HostUnavailable("the client room projection must name an absolute, provisioned room directory");
  }
  return path.resolve(bound);
}

export function homeLaunchRoom(cwd): { room: string; roomDir: string } | null {
  const roomDir = launchRoomDirectory(cwd);
  if (!isRoomDirectory(roomDir)) return null;
  const marker = readRoomMarker(roomDir);
  const folderRoom = roomNameFromCwd(roomDir);
  const room = isValidRoomKey(marker.room) ? marker.room : isValidRoomKey(folderRoom) ? folderRoom : null;
  return room ? { room, roomDir } : null;
}

function homeLaunchContext(cwd): RoomContext | null {
  const launch = homeLaunchRoom(cwd);
  if (!launch) return null;
  const { room, roomDir } = launch;
  const marker = readRoomMarker(roomDir);
  const persisted = readPersistedHouseState(roomDir);
  const spirit = normalizeDisplayName(persisted.embodiedSpirit)
    || normalizeDisplayName(marker.trueName)
    || readActiveSpiritName(roomDir)
    || normalizeDisplayName(persisted.agentName)
    || DEFAULT_SPIRIT;
  const operator = normalizeDisplayName(persisted.operator)
    || normalizeDisplayName(marker.operator)
    || DEFAULT_OPERATOR;
  return { room, spirit, operator, effectiveRoomDir: roomDir, sharedRoot: path.dirname(roomDir), source: "home" };
}

// The room this session acts as, or null for an ordinary harness session.
// Workers and async operations resolve through the conversation that owns
// them (session.ts keys attachments by the root conversation), so a worker
// running in another project still acts for its parent's home.
export function roomContext(ctx: any): RoomContext | null {
  const attachment = sessionAttachment(ctx);
  if (attachment?.source === "selected") {
    if (attachment.status !== "attached") {
      throw new SelectedHomeUnavailable(attachment.room, attachment.error || "attachment pending");
    }
    return {
      room: attachment.room,
      spirit: attachment.spirit,
      operator: attachment.operator,
      effectiveRoomDir: attachment.homeDir,
      sharedRoot: path.dirname(attachment.homeDir),
      source: "attached",
    };
  }
  // An explicit `/spirit off` is authoritative: no launch cwd or projected
  // ATHANOR_CLIENT_ROOM_DIR makes this conversation a home again.
  if (homeExplicitlyOff(ctx)) return null;
  // A home launch reads its identity from the room it was launched in, with
  // or without a live Host registration.
  if (attachment?.source === "home") return homeLaunchContext(attachment.workspace);
  return homeLaunchContext(ctx?.cwd);
}

export function requireRoomContext(ctx: any): RoomContext {
  const context = roomContext(ctx);
  if (!context) throw new NoHomeAttached();
  return context;
}

export function roomBinding(ctx: any): { context: RoomContext; binding: HostBinding } | null {
  const context = roomContext(ctx);
  if (!context) return null;
  return { context, binding: { room: context.room, spirit: context.spirit, session: hostSessionIdentity(ctx) } };
}

export function requireRoomBinding(ctx: any): { context: RoomContext; binding: HostBinding } {
  const context = requireRoomContext(ctx);
  return { context, binding: { room: context.room, spirit: context.spirit, session: hostSessionIdentity(ctx) } };
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
  const { binding } = requireRoomBinding(ctx);
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
