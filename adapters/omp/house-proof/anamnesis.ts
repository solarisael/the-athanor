import { requestOrgan, organFailure } from "./organ.ts";

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
        component: "athanor-omp",
        path: "house-proof/anamnesis.ts",
        symbol: ANAMNESIS_VALIDATOR_SYMBOL,
      },
      expected: { validator: ANAMNESIS_VALIDATOR_SYMBOL, result: "valid Rust anamnesis response" },
      observed: observedShape(value),
      evidence: [{ kind: "validator_failure", symbol: ANAMNESIS_VALIDATOR_SYMBOL, reason: validationError }],
      targets: ["house-proof/anamnesis.ts#validRustAnamnesisResult"],
      nextChecks: [{ action: "inspect", target: "house-proof/anamnesis.ts#validRustAnamnesisResult" }],
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

export async function queryAnamnesis(room, options) {
  const mode = options?.mode === "consult" ? "consult" : "wake";
  const query = String(options?.query || "").trim();
  const timeoutMs = options?.timeoutMs === undefined ? ANAMNESIS_TIMEOUT_MS : Number(options.timeoutMs);
  if (mode === "consult" && !query) return { ok: false, mode, ...EMPTY, error: "consult requires a non-empty query" };
  const requestedLimit = options?.limit === undefined ? ANAMNESIS_DEFAULT_LIMIT : Number(options.limit);
  const limit = Number.isFinite(requestedLimit) ? Math.max(1, Math.min(ANAMNESIS_MAX_LIMIT, Math.trunc(requestedLimit))) : ANAMNESIS_DEFAULT_LIMIT;
  try {
    if (options.binding.room !== room) throw new Error("foreign room binding refused");
    const result = await requestOrgan(options.binding, "anamnesis", {
      mode, ...(mode === "consult" ? { query } : {}), limit,
    }, { timeoutMs, signal: options.signal });
    const validationError = validRustAnamnesisResult(result, mode, room);
    if (validationError) return { mode, ...EMPTY, ...invalidRustAnamnesisFailure(validationError, result) };
    const entries = result.entries as Record<string, unknown>[];
    return {
      ...result,
      entries: [...entries],
      pillars: entries.filter((entry) => entry.kind === "pillar"),
      cycles: entries.filter((entry) => entry.kind === "cycle"),
    };
  } catch (error) {
    return { mode, ...EMPTY, ...organFailure(error) };
  }
}

function list(value) { return Array.isArray(value) ? value.filter(Boolean).map(String) : []; }
function text(value) { return String(value || "").trim(); }

export function formatAnamnesisContext(result, { automatic = false } = {}) {
  if (!result?.ok || !Array.isArray(result.entries) || !result.entries.length) return "";
  const lines = ["<athanor-memories>", automatic ? "Automatic Anamnesis counsel (not present-state truth)." : "Anamnesis Cabinet counsel."];
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
  lines.push("</athanor-memories>");
  const output = lines.join("\n");
  if (automatic && output.length > 8000) {
    const suffix = "\n...[anamnesis context clipped]\n</athanor-memories>";
    return `${output.slice(0, 8000 - suffix.length).trimEnd()}${suffix}`;
  }
  return output;
}
