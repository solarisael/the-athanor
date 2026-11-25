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
  dispatched = false;
  code = "transport_unavailable";
  constructor(message: string) {
    super(message);
    this.name = "HostUnavailable";
  }
}

// The closed list of failure classes (#3). The class decides the reaction;
// `code` keeps the exact word a caller acts on.
export const FAILURE_CLASSES = [
  "input_refused", "not_permitted", "conflict", "outcome_unknown", "no_connection", "provider_busy",
  "provider_down", "deadline", "budget_exhausted", "invalid_response", "configuration", "internal",
] as const;
export type FailureClass = typeof FAILURE_CLASSES[number];
export type FailureHop = { layer: string; file: string; line: number; op: string; observed?: string };
export type HostFailure = {
  class: FailureClass;
  code: string;
  message: string;
  hops: FailureHop[];
  details?: Record<string, unknown>;
};

const CLASS_SET = new Set<string>(FAILURE_CLASSES);
const RETRYABLE_CLASSES = new Set<FailureClass>(["no_connection", "provider_busy", "provider_down"]);

function record(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value) ? value as Record<string, unknown> : null;
}

function hop(value: unknown): FailureHop | null {
  const source = record(value);
  if (!source || typeof source.layer !== "string" || typeof source.file !== "string"
    || !Number.isSafeInteger(source.line) || typeof source.op !== "string") return null;
  return {
    layer: source.layer, file: source.file, line: source.line as number, op: source.op,
    ...(typeof source.observed === "string" ? { observed: source.observed } : {}),
  };
}

// The one decoder for a Host `failure` body (protocol::Failure). Every Host
// answer that failed reaches the adapter through it; null means the body is
// not a failure the adapter can read.
export function decodeHostFailure(value: unknown): HostFailure | null {
  const source = record(value);
  if (!source || !CLASS_SET.has(source.class as string) || typeof source.code !== "string"
    || typeof source.message !== "string") return null;
  const hops = Array.isArray(source.hops) ? source.hops.map(hop) : [];
  if (hops.some((entry) => entry === null)) return null;
  const details = source.details === undefined ? undefined : record(source.details);
  if (details === null) return null;
  return {
    class: source.class as FailureClass, code: source.code, message: source.message, hops: hops as FailureHop[],
    ...(details ? { details } : {}),
  };
}

export function failureRetryable(failure: HostFailure): boolean {
  return RETRYABLE_CLASSES.has(failure.class);
}

// The details a tool result shows: the Host's own details plus the class and
// the hops, which canonicalError passes through.
export function failureDetails(failure: HostFailure): Record<string, unknown> {
  return { ...failure.details, class: failure.class, hops: failure.hops };
}

// The Host answered and said no. Every caller that treats absence as
// degradation keeps doing so; only a retry loop needs the distinction, because
// asking again does not change a refusal.
export class HostRefused extends HostUnavailable {
  readonly failure: HostFailure;
  retryable: boolean;
  details: Record<string, unknown>;
  constructor(failure: HostFailure) {
    super(failure.message);
    this.name = "HostRefused";
    this.failure = failure;
    this.code = failure.code;
    this.retryable = failureRetryable(failure);
    this.details = failureDetails(failure);
  }
}

function hostRefusal(response: HostResponse): HostRefused {
  const failure = decodeHostFailure(response.failure);
  if (failure) return new HostRefused(failure);
  // A Host from before slice A1 sends `reason`; the adapter and Host releases disagree.
  const legacy = text(response.reason);
  return new HostRefused({
    class: "invalid_response",
    code: "failure_unreadable",
    message: `${text(response.command_or_event_type)} carried no readable failure${legacy ? `: ${legacy}` : ""}`,
    hops: [],
  });
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
// The harness conversation this context belongs to. Only a real harness ID
// counts: a cwd or a constant would let two conversations in one workspace
// collide into one Host session, so a missing ID refuses instead.
export function hostSessionIdentity(context: {
  sessionManager?: { getSessionId?: () => unknown };
  sessionID?: unknown;
  sessionId?: unknown;
}): string {
  let managed = "";
  try {
    managed = text(context?.sessionManager?.getSessionId?.());
  } catch {
    managed = "";
  }
  const identity = managed || text(context?.sessionID) || text(context?.sessionId);
  if (!identity) {
    const error = new HostUnavailable("the harness supplied no session ID, so this session cannot address the Host");
    error.code = "harness_session_missing";
    throw error;
  }
  return identity;
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

export type HostSpirit = { room: string; spirit: string; operator: string };

const SPIRITS_PATH = "/spirits";
// Tab completion waits for this read, so a stopped Host must fail quickly.
const SPIRITS_TIMEOUT_MS = 1_000;

// The House roster: every active room with its spirit. Any active room answers
// for the whole House (crates/host owns GET /room/<room>/spirits).
export async function fetchHostSpirits(room: string): Promise<HostSpirit[]> {
  const endpoint = hostHttpEndpoint(room, SPIRITS_PATH);
  let response: Response;
  try {
    response = await fetch(endpoint.url, {
      headers: { authorization: `Bearer ${endpoint.token}` },
      signal: AbortSignal.timeout(SPIRITS_TIMEOUT_MS),
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new HostUnavailable(`the Host roster at ${endpoint.url} is unreachable: ${detail}`);
  }
  if (!response.ok) {
    throw new HostUnavailable(`the Host roster at ${endpoint.url} answered HTTP ${response.status}`);
  }

  const payload = await response.json().catch(() => {
    throw new HostUnavailable(`the Host roster at ${endpoint.url} answered with unreadable JSON`);
  }) as { ok?: unknown; spirits?: unknown };

  if (payload?.ok !== true || !Array.isArray(payload.spirits)) {
    throw new HostUnavailable(`the Host roster at ${endpoint.url} answered without a spirits list`);
  }

  return payload.spirits.map((entry: any) => {
    const spirit = { room: text(entry?.room), spirit: text(entry?.spirit), operator: text(entry?.operator) };
    if (!isSafeRoomKey(spirit.room) || !spirit.spirit) {
      throw new HostUnavailable(`the Host roster at ${endpoint.url} named an invalid room entry`);
    }
    return spirit;
  });
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
  { settleDefinitively = false }: { settleDefinitively?: boolean } = {},
): Promise<HostResponse> {
  const url = hostRoomWsUrl(text(command.sender_room));
  const token = requiredEnvironment("ATHANOR_HOST_TOKEN");
  const requestTimeoutMs = Number.isFinite(timeoutMs)
    ? Math.min(command.command_or_event_type === "athanor.organ.call" ? 300_000 : 30_000, Math.max(250, Math.trunc(timeoutMs)))
    : HOST_TIMEOUT_MS;
  return await new Promise<HostResponse>((resolve, reject) => {
    let settled = false;
    let dispatched = false;
    let socket: WebSocket;
    let onAbort: (() => void) | undefined;
    const finish = (error: unknown, value?: HostResponse) => {
      if (settled) return;
      if (error instanceof HostUnavailable) error.dispatched = dispatched;
      settled = true;
      clearTimeout(timeout);
      if (onAbort) signal?.removeEventListener("abort", onAbort);
      try { socket?.close(); } catch { /* already closed */ }
      if (error) reject(error);
      else resolve(value!);
    };
    const unavailable = (message: string, code: string) => {
      const error = new HostUnavailable(message);
      error.code = code;
      return error;
    };
    const timeout = setTimeout(
      () => finish(unavailable(`Athanor Host timed out after ${requestTimeoutMs}ms`, "request_timeout")),
      requestTimeoutMs,
    );
    onAbort = () => {
      if (settleDefinitively && dispatched) return;
      finish(unavailable("Athanor Host request aborted", "request_aborted"));
    };
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
    socket.addEventListener("open", () => {
      if (settled) return;
      try {
        socket.send(JSON.stringify(command));
        dispatched = true;
      } catch {
        finish(new HostUnavailable("Athanor Host could not send the command"));
      }
    });
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
        finish(hostRefusal(response));
      } else if (acceptedTypes.has(kind)) {
        finish(null, response);
      }
    });
  });
}
