// Client for the Host's authenticated portable-session route,
// POST /room/<room>/session (crates/host/src/portable.rs owns the contract).

import { hostHttpEndpoint, HostUnavailable } from "./host.ts";

const SESSION_PATH = "/session";
const REQUEST_TIMEOUT_MS = 5_000;

type Json = Record<string, any>;

// A Host refusal keeps the Host's own code and body, so callers can act on
// revision_conflict, instance_stale or restart_pending without parsing text.
export class PortableSessionError extends HostUnavailable {
  status: number;
  detail: Json | null;
  constructor(code: string, message: string, status: number, detail: Json | null = null) {
    super(message);
    this.name = "PortableSessionError";
    this.code = code;
    this.status = status;
    this.detail = detail;
  }
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : value == null ? "" : String(value).trim();
}

export async function sessionRequest(room: string, body: Json, signal?: AbortSignal): Promise<Json> {
  const { url, token } = hostHttpEndpoint(room, SESSION_PATH);
  const timeout = AbortSignal.timeout(REQUEST_TIMEOUT_MS);
  const combined = signal ? AbortSignal.any([signal, timeout]) : timeout;
  let response: Response;
  try {
    response = await fetch(url, {
      method: "POST",
      headers: { Authorization: `Bearer ${token}`, "Content-Type": "application/json" },
      body: JSON.stringify(body),
      signal: combined,
    });
  } catch (error) {
    const reason = error instanceof Error ? error.message : String(error);
    throw new PortableSessionError("transport_unavailable", `Athanor Host session endpoint is unreachable: ${reason}`, 0);
  }
  const raw = await response.text().catch(() => "");
  let parsed: any = null;
  if (raw.trim()) {
    try {
      parsed = JSON.parse(raw);
    } catch {
      parsed = null;
    }
  }
  if (response.ok && parsed?.ok === true) return parsed;
  if (parsed?.ok === false && parsed.error && typeof parsed.error === "object") {
    const code = text(parsed.error.code) || "refused";
    throw new PortableSessionError(code, text(parsed.error.message) || `Athanor Host refused (${code})`, response.status, parsed.error);
  }
  // The router answers an unmounted room with a bare 404.
  if (response.status === 404) {
    throw new PortableSessionError("room_unknown", `the Athanor Host serves no room ${room}`, 404);
  }
  throw new PortableSessionError("invalid_response", `Athanor Host session endpoint answered HTTP ${response.status} without a session result`, response.status);
}
