import { createHash } from "node:crypto";

export const HOST_SCHEMA_VERSION = 1 as const;
const DEFAULT_HOST_WS_URL = "ws://127.0.0.1:8787/athanor/v1/ws";
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


function hostUrlForRoom(room: string): string {
  const override = text(process.env.ATHANOR_HOST_WS_URL);
  if (override) return override;
  const configured = text(process.env.ATHANOR_HOST_ENDPOINTS);
  if (!configured) return DEFAULT_HOST_WS_URL;
  let endpoints: unknown;
  try {
    endpoints = JSON.parse(configured);
  } catch {
    throw new HostUnavailable("ATHANOR_HOST_ENDPOINTS is not valid JSON");
  }
  const entry = endpoints && typeof endpoints === "object" && !Array.isArray(endpoints)
    ? (endpoints as Record<string, unknown>)[room]
    : null;
  const url = entry && typeof entry === "object" && !Array.isArray(entry)
    ? text((entry as Record<string, unknown>).url)
    : "";
  if (!url) throw new HostUnavailable(`no installed Athanor Host endpoint exists for room ${room || "<empty>"}`);
  const parsed = new URL(url);
  if (parsed.protocol !== "ws:" || !["127.0.0.1", "localhost", "[::1]"].includes(parsed.hostname)) {
    throw new HostUnavailable(`installed Athanor Host endpoint for room ${room} must be loopback WebSocket`);
  }
  return url;
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
    recipient: text(process.env.ATHANOR_HOST_RECIPIENT) || "house-host",
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
): Promise<HostResponse> {
  const url = hostUrlForRoom(text(command.sender_room));
  const token = requiredEnvironment("ATHANOR_HOST_TOKEN");
  return await new Promise<HostResponse>((resolve, reject) => {
    let settled = false;
    let socket: WebSocket;
    const finish = (error: unknown, value?: HostResponse) => {
      if (settled) return;
      settled = true;
      clearTimeout(timeout);
      try { socket?.close(); } catch { /* already closed */ }
      if (error) reject(error);
      else resolve(value!);
    };
    const timeout = setTimeout(
      () => finish(new HostUnavailable(`Athanor Host timed out after ${HOST_TIMEOUT_MS}ms`)),
      HOST_TIMEOUT_MS,
    );
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
