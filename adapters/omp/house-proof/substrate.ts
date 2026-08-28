// Substrate health plus the Rust-owned Paper Boat lifecycle.

import path from "node:path";
import { access } from "node:fs/promises";
import { spawn } from "node:child_process";
import { DIAGNOSTIC_TIMEOUT_MS, WRITE_TIMEOUT_MS } from "./constants.ts";
import { discoverRustExecutable } from "../discovery.ts";
import { ATHANOR_ROOT } from "../athanor-root.ts";
import { RustJsonlTransport, RustTransportError, RustTransportOutcomeUnknownError } from "../rust-transport.ts";

const DIAGNOSTIC_OWNER = {
  component: "athanor-omp",
  path: "house-proof/substrate.ts",
  symbol: "substrateHealth",
};

const paperBoatTransports = new Map();

function paperBoatTransport() {
  const executable = discoverRustExecutable();
  if (!executable) return { executable: null, transport: null };
  let transport = paperBoatTransports.get(executable);
  if (transport && !transport.usable) {
    paperBoatTransports.delete(executable);
    void transport.close().catch(() => {});
    transport = null;
  }
  if (!transport) {
    transport = new RustJsonlTransport({ executable });
    paperBoatTransports.set(executable, transport);
  }
  return { executable, transport };
}

function paperBoatFailure(method, error) {
  if (error instanceof RustTransportError) {
    return {
      ok: false,
      error: error.message,
      code: error.code,
      retryable: error.retryable,
      details: error.details,
    };
  }
  if (error instanceof RustTransportOutcomeUnknownError) {
    return {
      ok: false,
      error: `Rust ${method} outcome is unknown after dispatch`,
      code: "outcome_unknown",
      outcome: "unknown",
      retryable: method === "paper_boat_sleep",
      details: error.details || null,
    };
  }
  return { ok: false, error: error?.message || String(error) };
}

export function closePaperBoatTransports() {
  for (const [executable, transport] of paperBoatTransports) {
    paperBoatTransports.delete(executable);
    void transport.close().catch(() => {});
  }
}

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
      request_dispatched: false,
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



function isAbsolutePath(value) {
  const source = String(value || "").trim();
  return path.posix.isAbsolute(source)
    || path.win32.isAbsolute(source)
    || /^[A-Za-z]:[\\/]/.test(source)
    || /^\\\\/.test(source);
}

// ATHANOR_SUBSTRATE_ROOT is the one and only answer to "where is the
// substrate?". There is deliberately no structural or sibling fallback:
//
//   * Vault ships no substrate at all, so "unset" is a real, valid state and
//     must stay distinguishable from "misconfigured".
//   * A fallback that happens to land on a populated directory is exactly how
//     a test, or a Vault install sitting next to a dev checkout, reaches a live
//     substrate by accident.
//
// AKASHA installers therefore set ATHANOR_SUBSTRATE_ROOT explicitly.
function configuredSubstrateRoot() {
  return String(process.env.ATHANOR_SUBSTRATE_ROOT || "").trim();
}

/** Shape validation only: `null` when unset, because unset is valid for Vault. */
export function substrateConfigurationError() {
  const configuredPath = configuredSubstrateRoot();
  if (!configuredPath || isAbsolutePath(configuredPath)) return null;
  return `ATHANOR_SUBSTRATE_ROOT must be an absolute path when configured (got ${configuredPath})`;
}


/** The dotenv passed to the native Rust health command when state is known. */
export function healthDotenvPath() {
  const stateDir = String(process.env.ATHANOR_STATE_DIR || "").trim();
  if (!stateDir || !isAbsolutePath(stateDir)) return null;
  return path.join(stateDir, "substrate", ".env");
}

/** The explicitly selected native Rust substrate executable, when absolute. */
export function substrateExePath() {
  const executable = String(process.env.ATHANOR_SUBSTRATE_EXE || "").trim();
  if (!executable || !isAbsolutePath(executable)) return null;
  return executable;
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

function errorMessage(error) {
  return error?.message || error?.code || String(error);
}

async function pathAccessError(target) {
  try {
    await access(target);
    return null;
  } catch (error) {
    return error;
  }
}

/**
 * Read the canonical public substrate health verdict.
 *
 * The optional substrate never gates Vault behavior. A missing
 * ATHANOR_SUBSTRATE_ROOT is the valid Vault state; a configured path that
 * cannot produce a healthy, compatible verdict is explicitly degraded instead.
 */
export async function substrateHealth(timeoutMs = DIAGNOSTIC_TIMEOUT_MS) {
  const configuredPath = String(process.env.ATHANOR_SUBSTRATE_ROOT || "").trim();
  const configTarget = { kind: "environment", name: "ATHANOR_SUBSTRATE_ROOT" };
  const degraded = ({ dir, reason, degradedReasons = [], diagnostic: entry, report = {} }) => substrateDegraded({
    configured: true,
    dir,
    reason,
    degradedReasons,
    diagnostics: [entry],
    report,
  });
  if (!configuredPath) {
    return substrateDegraded({
      configured: false,
      dir: null,
      reason: "ATHANOR_SUBSTRATE_ROOT is not configured",
      diagnostics: [healthDiagnostic({
        expected: { configured: false, mode: "base" },
        observed: { configured: false },
        evidence: [{ source: "environment", state: "missing", name: "ATHANOR_SUBSTRATE_ROOT" }],
        targets: [configTarget],
        nextChecks: [{ action: "configure_optional_substrate", target: configTarget }],
        retry: "after_change",
      })],
    });
  }
  const configurationError = substrateConfigurationError();
  if (configurationError) {
    return degraded({
      dir: configuredPath,
      reason: configurationError,
      degradedReasons: [configurationError],
      diagnostic: healthDiagnostic({
        expected: { path: "absolute filesystem path" },
        observed: { path: redactText(configuredPath), absolute: false },
        evidence: [{ source: "environment", name: "ATHANOR_SUBSTRATE_ROOT", state: "present" }],
        targets: [configTarget],
        nextChecks: [{ action: "set_absolute_path", target: configTarget }],
      }),
    });
  }

  const dir = configuredPath;
  const dirError = await pathAccessError(dir);
  if (dirError) {
    const missing = dirError.code === "ENOENT";
    const reason = missing ? `configured substrate path is missing: ${dir}` : `configured substrate path is unavailable: ${dir} (${errorMessage(dirError)})`;
    return degraded({
      dir,
      reason,
      diagnostic: healthDiagnostic({
        category: "filesystem",
        expected: { directory: dir, accessible: true },
        observed: { directory: dir, accessible: false, error: errorMessage(dirError) },
        evidence: [{ source: "filesystem", code: dirError.code || "unknown", target: dir }],
        targets: [{ kind: "directory", path: dir }, configTarget],
        nextChecks: [{ action: missing ? "create_or_select_substrate" : "repair_filesystem_access", target: { path: dir } }],
      }),
    });
  }
  let executable;
  try {
    executable = substrateExePath() || discoverRustExecutable();
  } catch (error) {
    const reason = `configured Rust substrate executable is invalid: ${errorMessage(error)}`;
    return degraded({
      dir,
      reason,
      diagnostic: healthDiagnostic({
        category: "filesystem",
        expected: { executable: true },
        observed: { executable: false, error: errorMessage(error) },
        evidence: [{ source: "executable-selection", error: errorMessage(error) }],
        targets: [{ kind: "environment", name: "ATHANOR_SUBSTRATE_EXE" }],
        nextChecks: [{ action: "select_substrate_executable", target: { name: "ATHANOR_SUBSTRATE_EXE" } }],
      }),
    });
  }
  const executableError = executable ? await pathAccessError(executable) : { code: "ENOENT", message: "no Rust substrate executable selected" };
  if (executableError) {
    const reason = executable
      ? `configured Rust substrate executable is unavailable: ${executable} (${errorMessage(executableError)})`
      : "Rust substrate executable is unavailable";
    return degraded({
      dir,
      reason,
      diagnostic: healthDiagnostic({
        category: "filesystem",
        expected: { executable, accessible: true },
        observed: { executable, accessible: false, error: errorMessage(executableError) },
        evidence: [{ source: "filesystem", code: executableError.code || "unknown", target: executable }],
        targets: [{ kind: "file", path: executable || "athanor-substrate" }],
        nextChecks: [{ action: "restore_substrate_executable", target: { path: executable || "athanor-substrate" } }],
      }),
    });
  }

  const argv = ["health", "--substrate-dir", dir, "--skip-embedding"];
  const stateDotenv = healthDotenvPath();
  if (stateDotenv) argv.push("--env-file", stateDotenv);
  const fixture = String(process.env.ATHANOR_TEST_SUBSTRATE_HEALTH_SCRIPT || "").trim();
  const command = fixture ? process.execPath : executable;
  const commandArgs = fixture ? [fixture, ...argv] : argv;
  let probe;
  try {
    probe = await runDiagnosticProcess({ command, args: commandArgs, stdin: "", timeoutMs });
  } catch (error) {
    probe = { spawnError: errorMessage(error), timedOut: false, code: null, stdout: "", stderr: "" };
  }
  if (probe.timedOut || probe.spawnError) {
    const reason = probe.timedOut ? "Rust substrate health timed out" : `Rust substrate health launch failed: ${probe.spawnError}`;
    return degraded({
      dir,
      reason,
      diagnostic: healthDiagnostic({
        category: "operation",
        stage: "startup",
        expected: { command: "athanor-substrate health", timeoutMs },
        observed: { timedOut: Boolean(probe.timedOut), spawned: !probe.spawnError, exitCode: probe.code },
        evidence: [{ source: "process", stderr: redactText(String(probe.stderr || "")).slice(0, 512) }],
        targets: [{ kind: "file", path: executable }],
        nextChecks: [{ action: "run_health_command", target: { executable, argv } }],
        retry: "safe_now",
      }),
    });
  }

  const raw = String(probe.stdout || "").trim();
  let verdict;
  try {
    verdict = JSON.parse(raw);
  } catch (error) {
    return degraded({
      dir,
      reason: `Rust substrate health returned malformed JSON: ${errorMessage(error)}`,
      diagnostic: healthDiagnostic({
        category: "protocol",
        stage: "response_encode",
        expected: { json: "health verdict object" },
        observed: { stdoutBytes: raw.length, exitCode: probe.code },
        evidence: [{ source: "process", stderr: redactText(String(probe.stderr || "")).slice(0, 512) }],
        targets: [{ kind: "file", path: executable }],
        nextChecks: [{ action: "run_health_command", target: { executable, argv } }, { action: "validate_health_json", target: { executable } }],
      }),
    });
  }
  if (!verdict || typeof verdict !== "object" || Array.isArray(verdict)) {
    return degraded({
      dir,
      reason: "Rust substrate health returned an invalid JSON verdict",
      diagnostic: healthDiagnostic({
        category: "protocol",
        stage: "response_encode",
        expected: { type: "object" },
        observed: { type: Array.isArray(verdict) ? "array" : typeof verdict },
        evidence: [{ source: "athanor-substrate", exitCode: probe.code }],
        targets: [{ kind: "file", path: executable }],
        nextChecks: [{ action: "validate_health_json", target: { executable } }],
      }),
    });
  }

  const reportedReasons = Array.isArray(verdict.degradedReasons)
    ? verdict.degradedReasons.filter((reason) => typeof reason === "string" && reason.trim())
    : [];
  const apiCompatible = verdict.substrateApi === 1;
  const full = verdict.ok === true && verdict.mode === "full" && apiCompatible;
  if (full) {
    return {
      ...redactValue(verdict),
      ok: true,
      configured: true,
      mode: "full",
      path: dir,
      reason: null,
      degradedReasons: reportedReasons.map(redactText),
      diagnostics: [],
    };
  }

  let reason = reportedReasons.join("; ");
  if (!apiCompatible) reason = `substrate API mismatch: Rust reported ${String(verdict.substrateApi)}, expected 1`;
  else if (!reason && verdict.mode !== "full") reason = `Rust substrate reported mode ${String(verdict.mode)}, expected full`;
  else if (!reason && verdict.ok !== true) reason = "Rust substrate reported an unhealthy substrate";
  else if (!reason) reason = "Rust substrate returned an incomplete full-mode verdict";
  const lower = reason.toLowerCase();
  const category = /embed|model|vector/.test(lower) ? "embedding" : /database|postgres|sqlite|sql/.test(lower) ? "database" : !apiCompatible ? "protocol" : "operation";
  const stage = category === "embedding" ? "embedding_request" : category === "database" ? "database_connect" : category === "protocol" ? "validation" : "startup";
  return degraded({
    dir,
    reason,
    degradedReasons: reportedReasons.length ? reportedReasons : [reason],
    // Every diagnostic block Rust gathered survives the degraded path.
    report: healthReport(verdict),
    diagnostic: healthDiagnostic({
      category,
      stage,
      expected: { ok: true, mode: "full", substrateApi: 1 },
      observed: { ok: verdict.ok === true, mode: verdict.mode, substrateApi: verdict.substrateApi, degradedReasons: reportedReasons },
      evidence: [{ source: "athanor-substrate", exitCode: probe.code, reason: redactText(reason) }],
      targets: [{ kind: "file", path: executable }, category === "database" ? { kind: "service", name: "database" } : category === "embedding" ? { kind: "service", name: "embedding" } : { kind: "contract", path: "compatibility.json" }],
      nextChecks: [
        { action: category === "database" ? "verify_database_connectivity" : category === "embedding" ? "verify_embedding_provider" : "validate_health_contract", target: { executable } },
        { action: "rerun_substrate_health", target: { executable } },
      ],
      retry: "after_change",
    }),
  });
}



export function runDiagnosticProcess({ command, args, env, stdin, timeoutMs = DIAGNOSTIC_TIMEOUT_MS }) {
  return new Promise((resolve) => {
    let stdout = "";
    let stderr = "";
    let settled = false;
    let timer;
    const finish = (value) => {
      if (settled) return;
      settled = true;
      if (timer !== undefined) clearTimeout(timer);
      resolve(value);
    };
    const invocation = {
      command,
      args: Array.isArray(args) ? args : [],
      env,
    };

    let child;
    try {
      child = spawn(invocation.command, invocation.args, {
        windowsHide: true,
        stdio: ["pipe", "pipe", "pipe"],
        ...(invocation.env ? { env: { ...process.env, ...invocation.env } } : {}),
      });
    } catch (error) {
      finish({
        timedOut: false,
        spawnError: error?.message || String(error),
        code: null,
        stdout,
        stderr,
      });
      return;
    }

    timer = setTimeout(() => {
      finish({ timedOut: true, spawnError: null, code: null, stdout, stderr });
      try {
        if (typeof child.kill === "function") child.kill();
      } catch {
        // A diagnostic timeout must still settle when a mocked or exited child cannot be killed.
      }
    }, timeoutMs);

    const subscribe = (stream, event, listener) => {
      if (typeof stream?.on === "function") stream.on(event, listener);
    };
    subscribe(child.stdout, "data", (chunk) => { stdout += chunk.toString("utf8"); });
    subscribe(child.stderr, "data", (chunk) => { stderr += chunk.toString("utf8"); });
    // Streams can report an error after the child closes; retaining listeners prevents an
    // already-settled health probe from surfacing as an unrelated asynchronous test failure.
    subscribe(child.stdin, "error", () => {});
    subscribe(child.stdout, "error", () => {});
    subscribe(child.stderr, "error", () => {});
    subscribe(child, "error", (err) => finish({
      timedOut: false,
      spawnError: err?.message || String(err),
      code: null,
      stdout,
      stderr,
    }));
    subscribe(child, "close", (code) => finish({
      timedOut: false,
      spawnError: null,
      code,
      stdout,
      stderr,
    }));

    const input = child.stdin;
    if (
      !settled
      && typeof input?.end === "function"
      && input.writableEnded !== true
      && input.destroyed !== true
      && input.writable !== false
    ) {
      try {
        input.end(String(stdin || ""));
      } catch {
        // Diagnostic only. Broken stdin should be reported through process exit.
      }
    }
  });
}

export function memorySourcePath(title, now = new Date()) {
  return `memory/omp_${now.toISOString().replace(/[:.]/g, "-")}_${String(title || "memory").toLowerCase().replace(/[^a-z0-9]+/g, "_").replace(/^_+|_+$/g, "").slice(0, 48) || "memory"}.md`;
}

export async function sleepBoat(room, body, { signal } = {}) {
  const { executable, transport } = paperBoatTransport();
  if (!transport || !executable) {
    return { ok: false, error: "Rust substrate executable is unavailable; paper boat was not written" };
  }
  try {
    const result = await transport.request("paper_boat_sleep", {
      room,
      body,
      backup: true,
    }, {
      signal: signal || undefined,
      timeoutMs: WRITE_TIMEOUT_MS,
      settleDefinitively: true,
    });
    return result;
  } catch (error) {
    if (!transport.usable) paperBoatTransports.delete(executable);
    return paperBoatFailure("paper_boat_sleep", error);
  }
}

export async function catchBoat(room, { signal, timeoutMs = WRITE_TIMEOUT_MS } = {}) {
  const { executable, transport } = paperBoatTransport();
  if (!transport || !executable) {
    return { ok: false, error: "Rust substrate executable is unavailable; paper boat wake is unavailable" };
  }
  try {
    const result = await transport.request("paper_boat_wake", { room }, {
      signal: signal || undefined,
      timeoutMs,
    });
    return result;
  } catch (error) {
    if (!transport.usable) paperBoatTransports.delete(executable);
    return paperBoatFailure("paper_boat_wake", error);
  }
}

// The wake board rides the paper-boat lane: same substrate process, same
// timeout discipline, one transport for everything the wake letter needs.
export const WAKE_BOARD_STATES = ["offered", "claimed"];

export async function readQuestBoard(binding, { houseId, states = WAKE_BOARD_STATES, limit = 10, signal, timeoutMs = WRITE_TIMEOUT_MS } = {}) {
  const { executable, transport } = paperBoatTransport();
  if (!transport || !executable) {
    return { ok: false, error: "Rust substrate executable is unavailable; the quest board is unavailable" };
  }
  try {
    return await transport.request("quest_board", {
      room: binding.room,
      spirit: binding.spirit,
      session: binding.session,
      houseId,
      states,
      limit,
    }, {
      signal: signal || undefined,
      timeoutMs,
    });
  } catch (error) {
    if (!transport.usable) paperBoatTransports.delete(executable);
    return paperBoatFailure("quest_board", error);
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


