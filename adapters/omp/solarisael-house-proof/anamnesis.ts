import { RustJsonlTransport, RustTransportError } from "../rust-transport.ts";
import { discoverRustExecutable } from "../discovery.ts";

const rustAnamnesisTransports = new Map<string, RustJsonlTransport>();
const ANAMNESIS_DEFAULT_LIMIT = 10;
const ANAMNESIS_MAX_LIMIT = 50;
const ANAMNESIS_TIMEOUT_MS = 120_000;
const ANAMNESIS_VALIDATOR_SYMBOL = "validRustAnamnesisResult";

function observedShape(value: unknown): Record<string, unknown> {
  if (value === null) return { type: "null" };
  if (Array.isArray(value)) return { type: "array", length: value.length };
  if (typeof value !== "object") return { type: typeof value };
  const record = value as Record<string, unknown>;
  const entries = Object.keys(record).sort().slice(0, 32).map((key) => {
    const field = record[key];
    return [key, field === null ? "null" : Array.isArray(field) ? "array" : typeof field] as const;
  });
  return {
    type: "object",
    fields: Object.fromEntries(entries),
    ...(Object.keys(record).length > entries.length ? { fields_truncated: true } : {}),
  };
}

function diagnosticDetails({
  category,
  stage,
  operation,
  owner,
  expected,
  observed,
  evidence,
  targets,
  nextChecks,
  execution,
}: {
  category: string;
  stage: string;
  operation: string;
  owner: Record<string, string>;
  expected: Record<string, unknown>;
  observed: Record<string, unknown>;
  evidence: Record<string, unknown>[];
  targets: string[];
  nextChecks: Record<string, string>[];
  execution: Record<string, unknown>;
}) {
  return {
    category,
    stage,
    operation,
    owner,
    expected,
    observed,
    evidence,
    targets,
    next_checks: nextChecks,
    execution,
  };
}

function boundedStderr(stderr: unknown): string {
  return String(stderr || "")
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^\s@/]+@/gi, "$1[redacted]@")
    .replace(/(\b(?:token|password|authorization)\s*[=:]\s*)(?:Bearer\s+)?\S+/gi, "$1[redacted]")
    .slice(0, 2000);
}

function invalidRustAnamnesisFailure(validationError: string, value: unknown) {
  return {
    ok: false,
    error: `invalid Rust anamnesis result: ${validationError}`,
    code: "invalid_rust_result",
    retryable: true,
    details: diagnosticDetails({
      category: "protocol",
      stage: "validation",
      operation: "anamnesis",
      owner: {
        component: "solarisael-house-omp",
        path: "solarisael-house-proof/anamnesis.ts",
        symbol: ANAMNESIS_VALIDATOR_SYMBOL,
      },
      expected: { validator: ANAMNESIS_VALIDATOR_SYMBOL, result: "valid Rust anamnesis response" },
      observed: observedShape(value),
      evidence: [{ kind: "validator_failure", symbol: ANAMNESIS_VALIDATOR_SYMBOL, reason: validationError }],
      targets: ["solarisael-house-proof/anamnesis.ts#validRustAnamnesisResult"],
      nextChecks: [{ action: "inspect", target: "solarisael-house-proof/anamnesis.ts#validRustAnamnesisResult" }],
      execution: { request_dispatched: true, write_outcome: "not_started", retry: "safe_now" },
    }),
  };
}

function rustAnamnesisTransport(): RustJsonlTransport | null {
  const executable = discoverRustExecutable();
  if (!executable) return null;
  let transport = rustAnamnesisTransports.get(executable);
  if (!transport) {
    transport = new RustJsonlTransport({ executable });
    rustAnamnesisTransports.set(executable, transport);
  }
  return transport;
}

function evictRustAnamnesisTransport(executable: string, transport: RustJsonlTransport): void {
  if (rustAnamnesisTransports.get(executable) !== transport) return;
  rustAnamnesisTransports.delete(executable);
  void transport.close().catch(() => {});
}

export function closeRustAnamnesisTransports(): void {
  for (const [executable, transport] of rustAnamnesisTransports) {
    rustAnamnesisTransports.delete(executable);
    void transport.close().catch(() => {});
  }
}

function rustFailure(error: unknown, transport: RustJsonlTransport) {
  const stderr = boundedStderr(error instanceof RustTransportError ? error.stderr : transport.stderrDiagnostics);
  if (error instanceof RustTransportError) {
    const details = error.details && typeof error.details === "object" && !Array.isArray(error.details)
      ? {
        ...error.details,
        ...(stderr ? {
          evidence: [
            ...(Array.isArray(error.details.evidence) ? error.details.evidence : []),
            { kind: "stderr", text: stderr },
          ],
        } : {}),
      }
      : stderr ? diagnosticDetails({
        category: "transport",
        stage: "request_parse",
        operation: "anamnesis",
        owner: { component: "solarisael-house-omp", path: "solarisael-house-proof/anamnesis.ts", symbol: "rustFailure" },
        expected: { transport: "a structured Rust response or transport error" },
        observed: { transport_details: observedShape(error.details) },
        evidence: [{ kind: "stderr", text: stderr }],
        targets: ["rust-transport.ts#RustJsonlTransport.request"],
        nextChecks: [{ action: "inspect", target: "rust-transport.ts#RustJsonlTransport.request" }],
        execution: { request_dispatched: true, write_outcome: "not_started", retry: error.retryable ? "safe_now" : "after_change" },
      })
      : error.details;
    return { ok: false, error: error.message, code: error.code, retryable: error.retryable, ...(details === undefined ? {} : { details }) };
  }
  return {
    ok: false,
    error: "Rust transport request failed",
    code: "rust_transport_failure",
    retryable: true,
    details: diagnosticDetails({
      category: "transport",
      stage: "request_parse",
      operation: "anamnesis",
      owner: { component: "solarisael-house-omp", path: "solarisael-house-proof/anamnesis.ts", symbol: "rustFailure" },
      expected: { transport: "a structured Rust response or transport error" },
      observed: { error_type: error instanceof Error ? error.name : typeof error },
      evidence: stderr ? [{ kind: "stderr", text: stderr }] : [],
      targets: ["rust-transport.ts#RustJsonlTransport.request"],
      nextChecks: [{ action: "inspect", target: "rust-transport.ts#RustJsonlTransport.request" }],
      execution: { request_dispatched: true, write_outcome: "not_started", retry: "safe_now" },
    }),
  };
}


function validRustAnamnesisResult(value: unknown, mode: string, room: string): string | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return "result must be an object";
  const result = value as Record<string, unknown>;
  if (result.ok !== true || result.mode !== mode || result.room !== room || typeof result.room !== "string" || typeof result.found !== "boolean" || !Array.isArray(result.entries) || !Array.isArray(result.warnings)) {
    return "result must contain ok=true, mode, requested room, found, entries, and warnings";
  }
  if (result.found && !result.entries.every((entry) => entry && typeof entry === "object" && !Array.isArray(entry))) {
    return "found results must contain entry objects";
  }
  if (!result.warnings.every((warning) => typeof warning === "string")) return "result.warnings must contain strings";
  return null;
}

const EMPTY = { entries: [], warnings: [] };

export async function queryAnamnesis(_effectiveRoomDir, room, options = {}) {
  const mode = options?.mode === "consult" ? "consult" : "wake";
  const query = String(options?.query || "").trim();
  if (mode === "consult" && !query) return { ok: false, mode, ...EMPTY, error: "consult requires a non-empty query" };
  const executable = discoverRustExecutable();
  const transport = rustAnamnesisTransport();
  if (transport) {
    const requestedLimit = options?.limit === undefined ? ANAMNESIS_DEFAULT_LIMIT : Number(options.limit);
    const limit = Number.isFinite(requestedLimit) ? Math.max(1, Math.min(ANAMNESIS_MAX_LIMIT, Math.trunc(requestedLimit))) : ANAMNESIS_DEFAULT_LIMIT;
    const params = {
      room,
      mode,
      ...(mode === "consult" ? { query } : {}),
      limit,
    };
    try {
      const result = await transport.request("anamnesis", params, { timeoutMs: ANAMNESIS_TIMEOUT_MS });
      const validationError = validRustAnamnesisResult(result, mode, room);
      if (validationError) {
        evictRustAnamnesisTransport(executable, transport);
        return { mode, ...EMPTY, ...invalidRustAnamnesisFailure(validationError, result) };
      }
      return {
        ...result,
        entries: [...result.entries],
        pillars: result.entries.filter((entry) => (entry as Record<string, unknown>).kind === "pillar"),
        cycles: result.entries.filter((entry) => (entry as Record<string, unknown>).kind === "cycle"),
      };
    } catch (error) {
      if (!transport.usable) evictRustAnamnesisTransport(executable, transport);
      return { mode, ...EMPTY, ...rustFailure(error, transport) };
    }
  }
  return {
    ok: false,
    mode,
    ...EMPTY,
    error: "Rust anamnesis transport unavailable",
    code: "rust_transport_unavailable",
    retryable: true,
    details: diagnosticDetails({
      category: "transport",
      stage: "startup",
      operation: "anamnesis",
      owner: { component: "solarisael-house-omp", path: "solarisael-house-proof/anamnesis.ts", symbol: "rustAnamnesisTransport" },
      expected: { transport: "an available Rust JSONL transport" },
      observed: { executable_configured: Boolean(executable), transport_available: false },
      evidence: [],
      targets: ["solarisael-house-proof/anamnesis.ts#rustAnamnesisTransport"],
      nextChecks: [{ action: "inspect", target: "solarisael-house-proof/anamnesis.ts#rustAnamnesisTransport" }],
      execution: { request_dispatched: false, write_outcome: "not_started", retry: "safe_now" },
    }),
  };
}

function list(value) { return Array.isArray(value) ? value.filter(Boolean).map(String) : []; }
function text(value) { return String(value || "").trim(); }

export function formatAnamnesisContext(result, { automatic = false } = {}) {
  if (!result?.ok || !Array.isArray(result.entries) || !result.entries.length) return "";
  const lines = ["<system-reminder>", automatic ? "Automatic Anamnesis counsel (not present-state truth)." : "Anamnesis Cabinet counsel."];
  if (automatic) {
    lines.push("The Cabinet is counsel, not present-state truth.", "Pillars are standing places.", "Active cycles are prior patterns to verify against the live turn.", "Never assert a cycle is active merely because it loaded.");
  }
  lines.push("Fidelity: record=true-as-said; raw-material=true-as-reforged.", "Source paths are citations.", "");
  for (const entry of result.entries) {
    const kind = text(entry.kind) || "entry";
    const fidelity = text(entry.fidelity);
    const activation = text(entry.activation);
    const state = entry.active === true ? "active" : entry.active === false ? "inactive" : "unspecified";
    lines.push(`[${kind}${fidelity ? `; fidelity=${fidelity}` : ""}${activation ? `; activation=${activation}` : ""}; state=${state}] ${text(entry.title) || "(untitled)"}`);
    for (const [label, value] of [["Shape", entry.shape], ["Peak", entry.peak], ["Beginning", entry.beginning], ["Ramp", entry.ramp], ["Counsel", entry.counsel], ["Verify", entry.verify_note]]) {
      if (text(value)) lines.push(`${label}: ${text(value)}`);
    }
    const tags = list(entry.tags); if (tags.length) lines.push(`Tags: ${tags.join(", ")}`);
    const canon = list(entry.canon_links); if (canon.length) lines.push(`Canon: ${canon.join(", ")}`);
    const sources = list(entry.source_paths); if (sources.length) lines.push(`Sources: ${sources.join(", ")}`);
    for (const rep of Array.isArray(entry.reps) ? entry.reps : []) {
      lines.push(`Rep ${text(rep.rep_number) || "?"}${text(rep.occurred_on) ? ` (${text(rep.occurred_on)})` : ""}: ${text(rep.how_it_went)}`);
      if (text(rep.portal_pull)) lines.push(`Portal pull: ${text(rep.portal_pull)}`);
      if (text(rep.lighter)) lines.push(`Lighter: ${text(rep.lighter)}`);
      if (text(rep.source_path)) lines.push(`Rep source: ${text(rep.source_path)}`);
    }
    lines.push("");
  }
  if (Array.isArray(result.warnings) && result.warnings.length) lines.push(`Warnings: ${result.warnings.map(String).join(" | ")}`);
  lines.push("</system-reminder>");
  const output = lines.join("\n");
  if (automatic && output.length > 8000) {
    const suffix = "\n...[anamnesis context clipped]\n</system-reminder>";
    return `${output.slice(0, 8000 - suffix.length).trimEnd()}${suffix}`;
  }
  return output;
}
