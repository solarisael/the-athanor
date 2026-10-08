import { DIAGNOSTIC_TIMEOUT_MS, WRITE_TIMEOUT_MS } from "./constants.ts";
import { requestOrgan, organFailure } from "./organ.ts";
import { hostHouseId } from "./host.ts";

const DIAGNOSTIC_OWNER = {
  component: "athanor-omp",
  path: "house-proof/substrate.ts",
  symbol: "substrateHealth",
};


function redactText(value) {
  return String(value || "")
    .replace(/([a-z][a-z0-9+.-]*:\/\/)[^/\s@]+@/gi, "$1[redacted]@")
    .replace(/\b[\w.-]+:[^@\\/\s]+@/g, "[redacted]@")
    .replace(/\b(token|password|secret|api[_-]?key|authorization)\s*[:=]\s*\S+/gi, "$1: [redacted]");
}

function redactValue(value) {
  if (typeof value === "string") return redactText(value);
  if (Array.isArray(value)) return value.map(redactValue);
  if (!value || typeof value !== "object") return value;
  return Object.fromEntries(Object.entries(value).map(([key, item]) => [
    key,
    /(?:token|password|secret|authorization|api[_-]?key|database_url|connection_string)/i.test(key)
      ? "[redacted]"
      : redactValue(item),
  ]));
}

function diagnostic({ category, stage, expected, observed, evidence, targets, nextChecks, retry = "after_change" }) {
  return {
    category,
    stage,
    operation: "substrate_health",
    owner: DIAGNOSTIC_OWNER,
    expected: redactValue(expected),
    observed: redactValue(observed),
    evidence: redactValue(evidence),
    targets,
    next_checks: nextChecks,
    execution: {
      request_dispatched: true,
      write_outcome: "not_started",
      retry,
    },
  };
}

function healthDiagnostic({ category = "configuration", stage = "configuration_load", expected, observed, evidence, targets, nextChecks, retry }) {
  return diagnostic({
    category,
    stage,
    expected,
    observed,
    evidence,
    targets,
    nextChecks,
    retry,
  });
}





/**
 * The diagnostic blocks Rust gathers. Each remains independently useful when
 * the aggregate verdict is degraded.
 */
export const HEALTH_REPORT_BLOCKS = ["scripts", "database", "embedding", "retrieval", "backup", "topology"];

function healthReport(verdict) {
  if (!verdict || typeof verdict !== "object") return {};
  const report = {};
  for (const key of HEALTH_REPORT_BLOCKS) {
    if (verdict[key] !== undefined && verdict[key] !== null) report[key] = redactValue(verdict[key]);
  }
  return report;
}

function substrateDegraded({ configured, dir, reason, degradedReasons = [], diagnostics = [], report = {} }) {
  const safeReason = redactText(reason);
  const safeReasons = degradedReasons.map(redactText);
  return {
    // Report blocks first: the adapter's own verdict fields below remain
    // authoritative and cannot be shadowed by the Rust payload.
    ...report,
    ok: configured ? false : null,
    configured,
    mode: configured ? "degraded" : "base",
    substrateApi: null,
    path: configured ? redactText(dir) : null,
    reason: safeReason,
    degradedReasons: safeReasons,
    diagnostics,
  };
}



export async function substrateHealth(binding, timeoutMs = DIAGNOSTIC_TIMEOUT_MS) {
  try {
    const verdict = await requestOrgan(binding, "substrate_health", { skipEmbedding: true }, { timeoutMs });
    const configured = verdict.configured === true;
    const dir = (verdict.topology as Record<string, unknown> | undefined)?.substrateDir ?? null;
    const reasons = Array.isArray(verdict.degradedReasons) ? verdict.degradedReasons.filter((reason) => typeof reason === "string" && reason.trim()) : [];
    if (!configured) {
      return substrateDegraded({ configured: false, dir: null, reason: "AKASHA is not configured for this Host room", report: healthReport(verdict) });
    }
    if (verdict.ok === true && verdict.mode === "full" && verdict.substrateApi === 1) {
      return { ...redactValue(verdict), ok: true, configured: true, mode: "full", path: dir, reason: null, degradedReasons: reasons.map(redactText), diagnostics: [] };
    }
    const reason = verdict.substrateApi !== 1
      ? `substrate API mismatch: Rust reported ${String(verdict.substrateApi)}, expected 1`
      : reasons.join("; ") || "Rust substrate reported an unhealthy substrate";
    return substrateDegraded({
      configured: true, dir, reason, degradedReasons: reasons.length ? reasons : [reason],
      report: healthReport(verdict),
      diagnostics: [healthDiagnostic({
        category: "operation", stage: "startup",
        expected: { ok: true, mode: "full", substrateApi: 1 },
        observed: { ok: verdict.ok, mode: verdict.mode, substrateApi: verdict.substrateApi },
        evidence: [{ source: "athanor.organ.result", reason }],
        targets: [{ kind: "contract", path: "crates/akasha/src/health.rs" }],
        nextChecks: [{ action: "inspect", target: { path: "crates/akasha/src/health.rs" } }],
      })],
    });
  } catch (error) {
    const failure = organFailure(error);
    return substrateDegraded({
      configured: true, dir: null, reason: failure.error,
      diagnostics: failure.details ? [failure.details] : [],
    });
  }
}




export async function sleepBoat(binding, body, { signal, backup = true } = {}) {
  try {
    return await requestOrgan(binding, "paper_boat_sleep", { body, backup }, { signal, timeoutMs: WRITE_TIMEOUT_MS, write: true });
  } catch (error) {
    return organFailure(error);
  }
}

export async function catchBoat(binding, { signal, timeoutMs = WRITE_TIMEOUT_MS } = {}) {
  try {
    return await requestOrgan(binding, "paper_boat_wake", {}, { signal, timeoutMs });
  } catch (error) {
    return organFailure(error);
  }
}

export const WAKE_BOARD_STATES = ["offered", "claimed"];

export async function readQuestBoard(binding, { houseId, states = WAKE_BOARD_STATES, limit = 10, signal, timeoutMs = WRITE_TIMEOUT_MS } = {}) {
  try {
    if (houseId && houseId !== hostHouseId()) throw new Error("foreign House target refused");
    return await requestOrgan(binding, "quest_board", { states, limit }, { signal, timeoutMs });
  } catch (error) {
    return organFailure(error);
  }
}

// Empty-set silence: an unanswered board, a refused board, and an empty board
// all render nothing. The wake letter never carries a fabricated section, and a
// quest line never claims a deadline the board did not state.
export function formatQuestBoardSection(receipt) {
  if (!receipt || typeof receipt !== "object" || receipt.ok !== true) return "";
  const quests = Array.isArray(receipt.quests) ? receipt.quests : [];
  const lines = [];
  for (const quest of quests) {
    if (!quest || typeof quest !== "object") continue;
    const title = String(quest.title ?? "").replace(/\s+/g, " ").trim();
    if (!title) continue;
    const state = String(quest.state ?? "").trim() || "unknown state";
    const importance = String(quest.importance ?? "").trim() || "hint";
    // Docket receipts are camelCase end to end, unlike the older paper-boat
    // receipts beside them. Read exactly the key the board states.
    const deadline = String(quest.deadlineAt ?? "").trim();
    lines.push(`- ${title} — ${state}, ${importance}, ${deadline ? `due ${deadline}` : "no deadline"}`);
  }
  if (lines.length === 0) return "";
  return ["## Quest board", ...lines].join("\n");
}


