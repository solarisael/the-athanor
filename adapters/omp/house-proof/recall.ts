import { RustJsonlTransport, RustTransportError } from "../rust-transport.ts";
import { discoverRustExecutable } from "../discovery.ts";

const RECALL_SEMANTIC_MIN_SIM = 0.40;
const RECALL_CONTENT_MIN_SIM = 0.30;
const RECALL_TIMEOUT_MS = 120_000;
const rustRecallTransports = new Map<string, RustJsonlTransport>();

function text(value: unknown): string {
  return String(value ?? "").trim();
}

function boundedStderr(stderr: unknown): string {
  return String(stderr || "")
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s@/]+@/gi, "$1[redacted]@")
    .replace(/(\b(?:token|password|authorization)\s*[=:]\s*)(?:Bearer\s+)?\S+/gi, "$1[redacted]")
    .slice(0, 2_000);
}

function rustRecallFailure(error: unknown, transport: RustJsonlTransport) {
  const stderr = boundedStderr(error instanceof RustTransportError ? error.stderr : transport.stderrDiagnostics);
  if (error instanceof RustTransportError) {
    return {
      ok: false,
      error: error.message,
      code: error.code,
      retryable: error.retryable,
      ...(error.details === undefined ? {} : { details: error.details }),
      ...(stderr ? { stderr } : {}),
    };
  }
  return {
    ok: false,
    error: "Rust transport request failed",
    code: "rust_transport_failure",
    retryable: true,
    ...(stderr ? { stderr } : {}),
  };
}

function rustRecallTransport() {
  const executable = discoverRustExecutable();
  if (!executable) return null;
  let transport = rustRecallTransports.get(executable);
  if (!transport) {
    transport = new RustJsonlTransport({ executable });
    rustRecallTransports.set(executable, transport);
  }
  return { executable, transport };
}

function evictRustRecallTransport(executable: string, transport: RustJsonlTransport) {
  if (rustRecallTransports.get(executable) !== transport) return;
  rustRecallTransports.delete(executable);
  void transport.close().catch(() => {});
}

export function closeRustRecallTransports() {
  for (const [executable, transport] of rustRecallTransports) {
    rustRecallTransports.delete(executable);
    void transport.close().catch(() => {});
  }
}

function temporalDecayUnsupported(error: unknown) {
  return error instanceof RustTransportError
    && error.code === "invalid_params"
    && error.message.includes("temporal_decay");
}

export type RecallProjection = "auto" | "manual";

/**
 * One AKASHA/Vault recall over the Rust transport.
 *
 * `projection` is named by the caller, never inferred from the query:
 * `"auto"` (default) is the passive working set with bounded excerpts;
 * `"manual"` is an operator-visible tool read whose selected records carry
 * their complete database bodies under the substrate's record cap
 * (`hearth::MANUAL_RECORD_CAP`, 5). A manual read never downgrades: a
 * substrate that refuses the `projection` field answers with its own
 * `invalid_params` failure, which is returned whole (`ok: false`) rather than
 * retried for clipped records. Only the automatic `temporal_decay` field keeps
 * its older retry-without-field path.
 */
export async function recallWithRouting(
  effectiveRoomDir: string,
  room: string,
  query: string,
  { signal, temporalDecay = false, projection = "auto", timeoutMs = RECALL_TIMEOUT_MS }: {
    signal?: AbortSignal;
    temporalDecay?: boolean;
    projection?: RecallProjection;
    timeoutMs?: number;
  } = {},
) {
  const runtime = rustRecallTransport();
  if (!runtime) {
    return {
      ok: false,
      result: {
        ok: false,
        query,
        error: "Rust Vault/AKASHA runtime is unavailable",
        code: "rust_runtime_unavailable",
        retryable: false,
      },
    };
  }
  const { executable, transport } = runtime;
  const vaultProfile = !text(process.env.ATHANOR_SUBSTRATE_ROOT);
  const baseParams = vaultProfile
    ? { room, room_dir: effectiveRoomDir, query }
    : {
      room,
      query,
      semantic_top_k: 8,
      semantic_min_similarity: RECALL_SEMANTIC_MIN_SIM,
      content_top_k: 8,
      content_min_similarity: RECALL_CONTENT_MIN_SIM,
    };
  const decayParams = temporalDecay && !vaultProfile ? { ...baseParams, temporal_decay: true } : baseParams;
  const manual = projection === "manual" && !vaultProfile;
  const params = manual ? { ...decayParams, projection: "manual" } : decayParams;
  const requestOptions = { signal, timeoutMs };
  try {
    let result;
    try {
      result = await transport.request(vaultProfile ? "vault_recall" : "recall", params, requestOptions);
    } catch (error) {
      // A manual read keeps its projection on the retry: dropping it would
      // hand back clipped records under a request that asked for whole ones.
      if (vaultProfile || !temporalDecay || !temporalDecayUnsupported(error)) throw error;
      result = await transport.request(
        "recall",
        manual ? { ...baseParams, projection: "manual" } : baseParams,
        requestOptions,
      );
    }
    return { ok: true, result };
  } catch (error) {
    if (!transport.usable) evictRustRecallTransport(executable, transport);
    return { ok: false, result: { ok: false, query, ...rustRecallFailure(error, transport) } };
  }
}
