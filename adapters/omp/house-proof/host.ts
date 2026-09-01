import { createHash } from "node:crypto";

export const HOST_SCHEMA_VERSION = 1 as const;
const DEFAULT_HOST_URL = "ws://127.0.0.1:8787";
const HOST_ROOM_PATH_PREFIX = "/room/";
const DEFAULT_HOST_WS_PATH = "/athanor/v1/ws";
const HOST_RECIPIENT = "house-host";
const HOST_TIMEOUT_MS = 3_000;

export type HostBinding = { room: string; spirit: string; session: string };
export type HostCommand = Record<string, unknown> & {
  schema_version: typeof HOST_SCHEMA_VERSION;
  message_id: string;
  command_or_event_type: string;
  projection_id: string;
};
export type HostResponse = Record<string, unknown> & {
  correlation_id: string;
  command_or_event_type: string;
};

export class HostUnavailable extends Error {
  constructor(message: string) {
    super(message);
    this.name = "HostUnavailable";
  }
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : value == null ? "" : String(value).trim();
}

function requiredEnvironment(name: string): string {
  const value = text(process.env[name]);
  if (!value) throw new HostUnavailable(`${name} is required for Athanor Host access`);
  return value;
}

// The House this installation serves. Docket goals and quests are scoped by it,
// and no default is invented: an unnamed House means the caller must name one.
export function hostHouseId(environ: NodeJS.ProcessEnv = process.env): string | null {
  return text(environ.ATHANOR_HOST_HOUSE_ID) || null;
}
export function hostSessionIdentity(context: {
  sessionManager?: { getSessionId?: () => unknown };
  sessionID?: unknown;
  sessionId?: unknown;
  cwd?: unknown;
}, fallback: string): string {
  try {
    const managed = text(context?.sessionManager?.getSessionId?.());
    if (managed) return managed;
  } catch {
    // A read-only harness identity is preferred, but Host calls remain fail-open.
  }
  return text(context?.sessionID)
    || text(context?.sessionId)
    || text(context?.cwd)
    || text(fallback);
}


// [host/routing] [security/loopback]
const LOOPBACK_HOSTNAMES = new Set(["127.0.0.1", "localhost", "::1", "[::1]"]);

// [protocol/room/key] [security/path]
function isSafeRoomKey(value: string): boolean {
  return value !== "house" && /^[a-z0-9]+(-[a-z0-9]+)*$/.test(value);
}

// [host/routing] [security/path]
function roomRoute(room: string): { host: string; path: string } {
  const key = text(room);
  if (!isSafeRoomKey(key)) {
    throw new HostUnavailable(`room ${key || "<empty>"} is not a safe Athanor room key`);
  }
  const base = new URL(text(process.env.ATHANOR_HOST_URL) || DEFAULT_HOST_URL);
  if (base.protocol !== "ws:" || !LOOPBACK_HOSTNAMES.has(base.hostname)) {
    throw new HostUnavailable("the installed Athanor Host URL must be a loopback WebSocket base");
  }
  return { host: base.host, path: `${HOST_ROOM_PATH_PREFIX}${key}` };
}

export function hostRoomWsUrl(room: string): string {
  const route = roomRoute(room);
  return `ws://${route.host}${route.path}${DEFAULT_HOST_WS_PATH}`;
}

export function hostHttpEndpoint(room: string, requestPath: string): { url: string; token: string } {
  const route = roomRoute(room);
  return {
    url: `http://${route.host}${route.path}${requestPath}`,
    token: requiredEnvironment("ATHANOR_HOST_TOKEN"),
  };
}

export function hostCommand(
  binding: HostBinding,
  commandType: string,
  projectionId: string,
  payload: Record<string, unknown> = {},
  idempotencyKey?: unknown,
): HostCommand {
  const requestedKey = text(idempotencyKey);
  const stableKey = requestedKey
    ? `${projectionId}:${createHash("sha256")
      .update([binding.room, binding.spirit, binding.session, commandType, requestedKey].join("\0"))
      .digest("hex")}`
    : crypto.randomUUID();
  const now = new Date();
  return {
    schema_version: HOST_SCHEMA_VERSION,
    message_id: stableKey,
    house_id: requiredEnvironment("ATHANOR_HOST_HOUSE_ID"),
    sender_room: binding.room,
    sender_spirit: binding.spirit,
    sender_session: binding.session,
    recipient: HOST_RECIPIENT,
    command_or_event_type: commandType,
    correlation_id: stableKey,
    causation_id: "",
    reply_target: binding.session,
    idempotency_key: stableKey,
    source_record_refs: [],
    scope: `room:${binding.room}:recall_policy`,
    visibility: "operator",
    authority_class: "room_state",
    created_at: now.toISOString(),
    expires_at: new Date(now.getTime() + 30_000).toISOString(),
    max_hops: 1,
    projection_id: projectionId,
    ...payload,
  };
}

export async function sendHostCommand(
  command: HostCommand,
  acceptedTypes: ReadonlySet<string>,
  signal?: AbortSignal,
  timeoutMs = HOST_TIMEOUT_MS,
): Promise<HostResponse> {
  const url = hostRoomWsUrl(text(command.sender_room));
  const token = requiredEnvironment("ATHANOR_HOST_TOKEN");
  const requestTimeoutMs = Number.isFinite(timeoutMs)
    ? Math.min(30_000, Math.max(250, Math.trunc(timeoutMs)))
    : HOST_TIMEOUT_MS;
  return await new Promise<HostResponse>((resolve, reject) => {
    let settled = false;
    let socket: WebSocket;
    let onAbort: (() => void) | undefined;
    const finish = (error: unknown, value?: HostResponse) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      if (onAbort) signal?.removeEventListener("abort", onAbort);
      try { socket?.close(); } catch { /* already closed */ }
      if (error) reject(error);
      else resolve(value!);
    };
    const timeout = setTimeout(
      () => finish(new HostUnavailable(`Athanor Host timed out after ${requestTimeoutMs}ms`)),
      requestTimeoutMs,
    );
    onAbort = () => finish(new HostUnavailable("Athanor Host request aborted"));
    if (signal?.aborted) {
      onAbort();
      return;
    }
    signal?.addEventListener("abort", onAbort, { once: true });
    try {
      const WebSocketConstructor = WebSocket as any;
      socket = new WebSocketConstructor(url, { headers: { Authorization: `Bearer ${token}` } });
    } catch (error) {
      finish(new HostUnavailable(`Athanor Host connection failed: ${text(error)}`));
      return;
    }
    socket.addEventListener("open", () => socket.send(JSON.stringify(command)));
    socket.addEventListener("error", () => finish(new HostUnavailable(`Athanor Host is unavailable at ${url}`)));
    socket.addEventListener("close", () => finish(new HostUnavailable("Athanor Host closed before replying")));
    socket.addEventListener("message", (event) => {
      let response: HostResponse;
      try {
        response = JSON.parse(String(event.data));
      } catch {
        finish(new HostUnavailable("Athanor Host returned malformed JSON"));
        return;
      }
      if (response.correlation_id !== command.message_id) return;
      const kind = text(response.command_or_event_type);
      if (kind.endsWith("command_refused") || kind.endsWith("command_failed")) {
        finish(new HostUnavailable(text(response.reason) || "Athanor Host rejected the command"));
      } else if (acceptedTypes.has(kind)) {
        finish(null, response);
      }
    });
  });
}
