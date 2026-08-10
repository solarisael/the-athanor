// WSL/substrate interop for the OMP adapter.
// Silhouette: call the house Python scripts and return small JSON-ish results.

import path from "node:path";
import { access, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { spawn } from "node:child_process";
import {
  DESIGN_DOCS_SCRIPT,
  DESIGN_DOC_WRITE_SCRIPT,
  DIAGNOSTIC_TIMEOUT_MS,
  LESSONS_SCRIPT,
  WRITE_TIMEOUT_MS,
} from "./constants.ts";
import { ATHANOR_ROOT } from "../athanor-root.ts";

const DIAGNOSTIC_OWNER = {
  component: "solarisael-house-omp",
  path: "solarisael-house-proof/substrate.ts",
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


export function windowsPathToWsl(value) {
  const source = String(value || "").replace(/\\/g, "/");
  const match = /^([A-Za-z]):\/(.*)$/.exec(source);
  if (!match) return source;
  return `/mnt/${match[1].toLowerCase()}/${match[2]}`;
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

/**
 * The substrate a write is about to use, or the reason there is none.
 *
 * Unlike {@link substrateConfigurationError} this treats "unset" as a refusal:
 * a writer with nowhere to write must say so rather than guess a directory.
 */
export function requireSubstrate() {
  const dir = configuredSubstrateRoot();
  if (!dir) {
    return { error: "ATHANOR_SUBSTRATE_ROOT is not configured; substrate operations are unavailable" };
  }
  const shapeError = substrateConfigurationError();
  if (shapeError) return { error: shapeError };
  return { dir };
}

/**
 * The dotenv health.py should read: `<state-root>/substrate/.env`.
 *
 * `null` when this process does not know the state root, which is the
 * development case — health.py then resolves it structurally from its own
 * location, and that is the honest answer there.
 *
 * Only an absolute state root is used. A relative one would resolve against
 * whatever the WSL side's working directory happens to be, which is exactly
 * the silent-wrong-path class this cutover removed.
 */
export function healthDotenvPath() {
  const stateDir = String(process.env.ATHANOR_STATE_DIR || "").trim();
  if (!stateDir || !isAbsolutePath(stateDir)) return null;
  return path.join(stateDir, "substrate", ".env");
}

/**
 * The substrate executable health.py should report on.
 *
 * Forwarded for DIAGNOSTICS only. This adapter is not a configuration owner
 * for the executable \u2014 discovery.ts already resolved it, and health.py has no
 * say in the matter. It is passed because ATHANOR_SUBSTRATE_EXE is a Windows
 * process variable that does not cross into WSL, so without it health.py falls
 * back to `<product>/target/release/athanor-substrate` and reports an installed
 * binary at `adapters/omp/bin/<platform>/athanor-substrate.exe` as missing.
 *
 * `null` when unset or relative, exactly as with the dotenv: health.py then
 * resolves structurally, which is the honest answer in a development checkout.
 */
export function substrateExePath() {
  const executable = String(process.env.ATHANOR_SUBSTRATE_EXE || "").trim();
  if (!executable || !isAbsolutePath(executable)) return null;
  return executable;
}

export function substratePaths(dir) {
  return {
    dir,
    health: path.join(dir, "health.py"),
    recordMemory: path.join(dir, "record_memory.py"),
    catchBoat: path.join(dir, "catch_boat.py"),
  };
}

/**
 * The diagnostic blocks health.py gathers. Every one of them is a fact in its
 * own right, not a consequence of the substrate being healthy, and a verifier
 * needs them most precisely when it is degraded.
 *
 * Twice now a block was dropped here and a downstream assertion evaluated
 * against `undefined` and blamed the wrong subsystem: `topology` first, then
 * `database`, which turned an unreachable server into "schema required 13;
 * got undefined". Naming them as a set ends that pattern instead of waiting
 * for the third one.
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
    // Report blocks first: the adapter's own verdict fields below are
    // authoritative and must never be shadowed by health.py's payload.
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

  const { dir, health } = substratePaths(configuredPath);
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
  const healthError = await pathAccessError(health);
  if (healthError) {
    const missing = healthError.code === "ENOENT";
    const reason = missing ? `configured substrate health script is missing: ${health}` : `configured substrate health script is unavailable: ${health} (${errorMessage(healthError)})`;
    return degraded({
      dir,
      reason,
      diagnostic: healthDiagnostic({
        category: "filesystem",
        expected: { file: health, accessible: true },
        observed: { file: health, accessible: false, error: errorMessage(healthError) },
        evidence: [{ source: "filesystem", code: healthError.code || "unknown", target: health }],
        targets: [{ kind: "file", path: health }, configTarget],
        nextChecks: [{ action: missing ? "restore_health_script" : "repair_filesystem_access", target: { path: health } }],
      }),
    });
  }

  // health.py runs inside WSL. ATHANOR_STATE_DIR is a Windows process
  // variable and does NOT cross that boundary without a global WSLENV
  // dependency, so the dotenv travels as an argv value instead — the one form
  // that always survives the hop. When the state root is unknown to this
  // process we pass nothing and let health.py resolve structurally, which is
  // the correct answer for a development checkout.
  const argv = ["--cd", "~", "python3", windowsPathToWsl(health)];
  const stateDotenv = healthDotenvPath();
  if (stateDotenv) argv.push("--env-file", windowsPathToWsl(stateDotenv));
  const executable = substrateExePath();
  if (executable) argv.push("--substrate-exe", windowsPathToWsl(executable));
  let probe;
  try {
    probe = await runWslDiagnostic({ argv, stdin: "", timeoutMs });
  } catch (error) {
    probe = { spawnError: errorMessage(error), timedOut: false, code: null, stdout: "", stderr: "" };
  }
  if (probe.timedOut || probe.spawnError) {
    const reason = probe.timedOut ? "health.py timed out" : `health.py launch failed: ${probe.spawnError}`;
    return degraded({
      dir,
      reason,
      diagnostic: healthDiagnostic({
        category: "operation",
        stage: "startup",
        expected: { command: "python3 health.py", timeoutMs },
        observed: { timedOut: Boolean(probe.timedOut), spawned: !probe.spawnError, exitCode: probe.code },
        evidence: [{ source: "process", stderr: redactText(String(probe.stderr || "")).slice(0, 512) }],
        targets: [{ kind: "script", path: health }, { kind: "service", name: "wsl.exe" }],
        nextChecks: [{ action: "run_health_command", target: { argv } }, { action: "verify_python_runtime", target: { command: "python3" } }],
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
      reason: `health.py returned malformed JSON: ${errorMessage(error)}`,
      diagnostic: healthDiagnostic({
        category: "protocol",
        stage: "response_encode",
        expected: { json: "health verdict object" },
        observed: { stdoutBytes: raw.length, exitCode: probe.code },
        evidence: [{ source: "process", stderr: redactText(String(probe.stderr || "")).slice(0, 512) }],
        targets: [{ kind: "script", path: health }],
        nextChecks: [{ action: "run_health_command", target: { path: health } }, { action: "validate_health_json", target: { path: health } }],
      }),
    });
  }
  if (!verdict || typeof verdict !== "object" || Array.isArray(verdict)) {
    return degraded({
      dir,
      reason: "health.py returned an invalid JSON verdict",
      diagnostic: healthDiagnostic({
        category: "protocol",
        stage: "response_encode",
        expected: { type: "object" },
        observed: { type: Array.isArray(verdict) ? "array" : typeof verdict },
        evidence: [{ source: "health.py", exitCode: probe.code }],
        targets: [{ kind: "script", path: health }],
        nextChecks: [{ action: "validate_health_json", target: { path: health } }],
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
  if (!apiCompatible) reason = `substrate API mismatch: health.py reported ${String(verdict.substrateApi)}, expected 1`;
  else if (!reason && verdict.mode !== "full") reason = `health.py reported mode ${String(verdict.mode)}, expected full`;
  else if (!reason && verdict.ok !== true) reason = "health.py reported an unhealthy substrate";
  else if (!reason) reason = "health.py returned an incomplete full-mode verdict";
  const lower = reason.toLowerCase();
  const category = /embed|model|vector/.test(lower) ? "embedding" : /database|postgres|sqlite|sql/.test(lower) ? "database" : !apiCompatible ? "protocol" : "operation";
  const stage = category === "embedding" ? "embedding_request" : category === "database" ? "database_connect" : category === "protocol" ? "validation" : "startup";
  return degraded({
    dir,
    reason,
    degradedReasons: reportedReasons.length ? reportedReasons : [reason],
    // Every diagnostic block health.py gathered, carried through unchanged.
    // See HEALTH_REPORT_BLOCKS for why this is a set rather than a hand-picked
    // field or two.
    report: healthReport(verdict),
    diagnostic: healthDiagnostic({
      category,
      stage,
      expected: { ok: true, mode: "full", substrateApi: 1 },
      observed: { ok: verdict.ok === true, mode: verdict.mode, substrateApi: verdict.substrateApi, degradedReasons: reportedReasons },
      evidence: [{ source: "health.py", exitCode: probe.code, reason: redactText(reason) }],
      targets: [{ kind: "script", path: health }, category === "database" ? { kind: "service", name: "database" } : category === "embedding" ? { kind: "service", name: "embedding" } : { kind: "contract", path: "compatibility.json" }],
      nextChecks: [
        { action: category === "database" ? "verify_database_connectivity" : category === "embedding" ? "verify_embedding_provider" : "validate_health_contract", target: { path: health } },
        { action: "rerun_substrate_health", target: { path: health } },
      ],
      retry: "after_change",
    }),
  });
}


function wslPathToWindows(value) {
  const source = String(value || "");
  const match = /^\/mnt\/([a-z])\/(.*)$/i.exec(source);
  if (!match) return source;
  return `${match[1].toUpperCase()}:/${match[2]}`;
}

function topologyEnvironment(environ = process.env) {
  return ["ATHANOR_STATE_DIR", "ATHANOR_SUBSTRATE_ROOT"].flatMap((key) => {
    const value = String(environ[key] || "").trim();
    return value && isAbsolutePath(value) ? [[key, value]] : [];
  });
}

function injectWslTopology(argv, environ = process.env) {
  const pythonIndex = argv.indexOf("python3");
  const assignments = topologyEnvironment(environ)
    .map(([key, value]) => `${key}=${windowsPathToWsl(value)}`);
  if (pythonIndex < 0 || assignments.length === 0) return [...argv];
  return [
    ...argv.slice(0, pythonIndex),
    "env",
    ...assignments,
    ...argv.slice(pythonIndex),
  ];
}


export function diagnosticInvocation(argv, environ = process.env) {
  if (environ.SOLARISAEL_TEST_NATIVE_PYTHON !== "1") {
    return { command: "wsl.exe", args: injectWslTopology(argv, environ) };
  }
  const pythonIndex = argv.indexOf("python3");
  if (pythonIndex < 0 || pythonIndex === argv.length - 1) {
    throw new Error("native Python test seam requires a python3 script invocation");
  }
  return {
    command: "python",
    args: argv.slice(pythonIndex + 1).map(wslPathToWindows),
    env: Object.fromEntries(topologyEnvironment(environ)),
  };
}

export function runWslDiagnostic({ argv, stdin, timeoutMs = DIAGNOSTIC_TIMEOUT_MS }) {
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
    let invocation;
    try {
      invocation = diagnosticInvocation(argv);
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

export async function writeSessionMemory({ room, title, body, backup, type = "session", sourcePath, threads = [], continues = [], supersedes = [], timeoutMs = WRITE_TIMEOUT_MS }) {
  const substrate = requireSubstrate();
  if (substrate.error) return { ok: false, error: substrate.error };
  const { recordMemory } = substratePaths(substrate.dir);
  const resolvedSourcePath = sourcePath || memorySourcePath(title);
  const argv = [
    "--cd", "~",
    "python3", windowsPathToWsl(recordMemory),
    "--room", room,
    "--type", String(type || "session"),
    "--title", String(title || "OMP memory"),
    "--source-path", resolvedSourcePath,
    "--body-stdin",
  ];
  for (const thread of Array.isArray(threads) ? threads : []) argv.push("--thread", String(thread));
  for (const memoryId of Array.isArray(supersedes) ? supersedes : []) argv.push("--supersedes", String(memoryId));
  for (const continuation of Array.isArray(continues) ? continues : []) {
    argv.push("--continues", JSON.stringify(continuation));
  }
  if (!backup) argv.push("--no-backup");
  const probe = await runWslDiagnostic({ argv, stdin: body, timeoutMs });
  if (probe.timedOut) return { ok: false, error: "record_memory timed out" };
  if (probe.spawnError) return { ok: false, error: probe.spawnError };
  if (probe.code !== 0) return { ok: false, error: String(probe.stderr || "").trim() || `record_memory exited ${probe.code}` };
  const summary = String(probe.stdout || "").trim();
  const idMatch = /id=(\d+)/.exec(summary);
  return { ok: true, id: idMatch ? Number(idMatch[1]) : null, summary, sourcePath: resolvedSourcePath };
}

// Lesson-store write path (remember store routing). All lesson scripts share
// the shape: --title inline, lesson text on stdin (--lesson-stdin), optional
// scalar/tag flags built by stores.buildStoreArgs. Body goes via stdin, never
// inline argv — cross-shell-boundary payloads break inline (lesson 163).
export async function writeLessonStore({ store, title, body, extraArgs = [], timeoutMs = WRITE_TIMEOUT_MS }) {
  const substrate = requireSubstrate();
  if (substrate.error) return { ok: false, error: substrate.error };
  const { dir } = substratePaths(substrate.dir);
  const script = path.join(dir, store.script);
  const argv = [
    "--cd", "~",
    "python3", windowsPathToWsl(script),
    "--title", String(title || "OMP lesson"),
    "--lesson-stdin",
    ...extraArgs,
  ];
  if (store.noBackup) argv.push("--no-backup");
  const probe = await runWslDiagnostic({ argv, stdin: body, timeoutMs });
  if (probe.timedOut) return { ok: false, error: `${store.script} timed out` };
  if (probe.spawnError) return { ok: false, error: probe.spawnError };
  if (probe.code !== 0) return { ok: false, error: String(probe.stderr || "").trim() || `${store.script} exited ${probe.code}` };
  const summary = String(probe.stdout || "").trim();
  const idMatch = /id=\s*(\d+)/.exec(summary);
  return { ok: true, id: idMatch ? Number(idMatch[1]) : null, summary };
}

export async function catchBoat(room) {
  const substrate = requireSubstrate();
  if (substrate.error) return { ok: false, error: substrate.error };
  const { catchBoat: script } = substratePaths(substrate.dir);
  const argv = ["--cd", "~", "python3", windowsPathToWsl(script), "--room", room];
  const stateDotenv = healthDotenvPath();
  if (stateDotenv) argv.push("--env-file", windowsPathToWsl(stateDotenv));
  const probe = await runWslDiagnostic({ argv, stdin: "" });
  if (probe.timedOut) return { ok: false, error: "catch_boat timed out" };
  if (probe.spawnError) return { ok: false, error: probe.spawnError };
  if (probe.code !== 0) return { ok: false, error: String(probe.stderr || "").trim() || `catch_boat exited ${probe.code}` };
  try {
    return { ok: true, ...JSON.parse(String(probe.stdout || "{}")) };
  } catch (err) {
    return { ok: false, error: err?.message || String(err), stdout: String(probe.stdout || "").slice(0, 1200) };
  }
}

export function formatUnboatedWarning(boat) {
  const orphans = Array.isArray(boat?.unboated) ? boat.unboated : [];
  if (orphans.length === 0) return null;
  const plural = orphans.length === 1 ? "memory was" : "memories were";
  return [
    `STALE BOAT: ${orphans.length} ${plural} written AFTER this boat was cast.`,
    "A previous session wrote memories and ended without calling sleep, so this",
    "boat does NOT describe the most recent session. Do not treat it as current.",
    "Recover the missing session by recalling these before answering:",
    ...orphans.map((m) => `  - [${m?.id}] ${String(m?.title || "untitled").trim()}`),
  ].join("\n");
}

export function formatWakeContext(boat) {
  const body = String(boat?.body || "").trim();
  if (!body) return "";
  const clipped = body.length > 6000 ? `${body.slice(0, 6000).trimEnd()}\n...[paper boat clipped ${body.length - 6000} chars]` : body;
  const warning = formatUnboatedWarning(boat);
  return [
    "<system-reminder>",
    warning,
    warning ? "" : null,
    "Automatic wake: latest paper boat for this room.",
    "Receive it as lived continuity from the room's previous waking self: keep its voice, relationships, uncertainty, and concrete state intact; orient from it without turning it into a script or status report.",
    boat?.title ? `Title: ${boat.title}` : null,
    boat?.source_path ? `Source: ${boat.source_path}` : null,
    "",
    clipped,
    "</system-reminder>",
  ].filter((line) => line !== null).join("\n");
}

export async function runLessons(effectiveRoomDir, room, filters) {
  const argv = [
    "--cd", "~",
    "python3", windowsPathToWsl(LESSONS_SCRIPT),
    "--room-dir", windowsPathToWsl(effectiveRoomDir),
    "--room", room,
    "--type", String(filters.type),
    "--limit", String(filters.limit ?? 12),
  ];
  for (const key of ["shape", "project", "register", "stage", "query"]) {
    const value = filters[key];
    if (typeof value === "string" && value.trim()) {
      argv.push(`--${key}`, value.trim());
    }
  }
  for (const value of filters.languageKeys || []) {
    if (typeof value === "string" && value.trim()) argv.push("--language-key", value.trim());
  }
  for (const value of filters.technologyKeys || []) {
    if (typeof value === "string" && value.trim()) argv.push("--technology-key", value.trim());
  }
  const probe = await runWslDiagnostic({ argv, stdin: "" });
  if (probe.timedOut) return { ok: false, lessons: [], taxonomy: [], error: "lessons query timed out" };
  if (probe.spawnError) return { ok: false, lessons: [], taxonomy: [], error: probe.spawnError };
  if (probe.code !== 0) {
    return { ok: false, lessons: [], taxonomy: [], error: String(probe.stderr || "").trim() || `lessons query exited ${probe.code}` };
  }
  try {
    const parsed = JSON.parse(String(probe.stdout || "{}"));
    if (parsed?.ok !== true) {
      return { ok: false, lessons: [], taxonomy: [], error: parsed?.error || "lessons query refused" };
    }
    return {
      ...parsed,
      ok: true,
      lessons: Array.isArray(parsed.lessons) ? parsed.lessons : [],
      taxonomy: Array.isArray(parsed.taxonomy) ? parsed.taxonomy : [],
    };
  } catch (error) {
    return { ok: false, lessons: [], taxonomy: [], error: `lessons query returned invalid JSON: ${error?.message || String(error)}` };
  }
}

export async function runDesignDocs(effectiveRoomDir, filters) {
  const argv = [
    "--cd", "~",
    "python3", windowsPathToWsl(DESIGN_DOCS_SCRIPT),
    "--room-dir", windowsPathToWsl(effectiveRoomDir),
    "--system", String(filters.system),
    "--limit", String(filters.limit ?? 12),
  ];
  for (const [key, flag] of [
    ["docType", "--doc-type"],
    ["name", "--name"],
    ["group", "--group"],
    ["query", "--query"],
  ]) {
    const value = filters[key];
    if (typeof value === "string" && value.trim()) argv.push(flag, value.trim());
  }
  if (filters.includeSuperseded === true) argv.push("--include-superseded");

  const probe = await runWslDiagnostic({ argv, stdin: "" });
  if (probe.timedOut) return { ok: false, documents: [], taxonomy: [], error: "design document query timed out" };
  if (probe.spawnError) return { ok: false, documents: [], taxonomy: [], error: probe.spawnError };
  if (probe.code !== 0) {
    return { ok: false, documents: [], taxonomy: [], error: String(probe.stderr || "").trim() || `design document query exited ${probe.code}` };
  }
  try {
    const parsed = JSON.parse(String(probe.stdout || "{}"));
    if (parsed?.ok !== true) {
      return { ok: false, documents: [], taxonomy: [], error: parsed?.error || "design document query refused" };
    }
    return {
      ...parsed,
      ok: true,
      documents: Array.isArray(parsed.documents) ? parsed.documents : [],
      taxonomy: Array.isArray(parsed.taxonomy) ? parsed.taxonomy : [],
    };
  } catch (error) {
    return { ok: false, documents: [], taxonomy: [], error: `design document query returned invalid JSON: ${error?.message || String(error)}` };
  }
}

export async function writeDesignDoc({
  effectiveRoomDir,
  system,
  docType,
  name,
  group,
  values,
  body,
  provenance,
  tags,
  supersedes,
  allowIdentityChange,
  timeoutMs = WRITE_TIMEOUT_MS,
}) {
  const argv = [
    "--cd", "~",
    "python3", windowsPathToWsl(DESIGN_DOC_WRITE_SCRIPT),
    "--room-dir", windowsPathToWsl(effectiveRoomDir),
    "--system", String(system),
    "--doc-type", String(docType),
    "--name", String(name),
  ];
  if (typeof group === "string" && group.trim()) argv.push("--group", group.trim());
  if (values !== undefined) argv.push("--values", JSON.stringify(values));
  if (provenance !== undefined) argv.push("--provenance", JSON.stringify(provenance));
  for (const tag of Array.isArray(tags) ? tags : []) argv.push("--tag", String(tag));
  if (supersedes !== undefined) argv.push("--supersedes", String(supersedes));
  if (allowIdentityChange === true) argv.push("--allow-identity-change");
  const hasBody = body !== undefined;
  if (hasBody) argv.push("--body-stdin");

  const probe = await runWslDiagnostic({ argv, stdin: hasBody ? String(body) : "", timeoutMs });
  if (probe.timedOut) return { ok: false, error: "design document write timed out" };
  if (probe.spawnError) return { ok: false, error: probe.spawnError };
  if (probe.code !== 0) return { ok: false, error: String(probe.stderr || "").trim() || `design document write exited ${probe.code}` };
  try {
    const parsed = JSON.parse(String(probe.stdout || "{}"));
    return parsed?.ok === true ? { ...parsed, ok: true } : { ...parsed, ok: false };
  } catch (err) {
    return { ok: false, error: err?.message || String(err), stdout: String(probe.stdout || "").slice(0, 1200) };
  }
}

export async function deleteLesson({ effectiveRoomDir, kind, id, expectedTitle, timeoutMs = WRITE_TIMEOUT_MS }) {
  const script = path.join(ATHANOR_ROOT, "src", "delete-lesson.py");
  const argv = [
    "--cd", "~",
    "python3", windowsPathToWsl(script),
    "--room-dir", windowsPathToWsl(effectiveRoomDir),
    "--kind", String(kind),
    "--id", String(id),
    "--expected-title", String(expectedTitle),
  ];
  const probe = await runWslDiagnostic({ argv, stdin: "", timeoutMs });
  if (probe.timedOut) return { ok: false, deleted: false, error: "delete-lesson timed out" };
  if (probe.spawnError) return { ok: false, deleted: false, error: probe.spawnError };
  if (probe.code !== 0) return { ok: false, deleted: false, error: String(probe.stderr || "").trim() || `delete-lesson exited ${probe.code}` };
  try {
    const parsed = JSON.parse(String(probe.stdout || "{}"));
    if (parsed?.ok !== true || parsed?.deleted !== true) return { ok: false, deleted: false, ...parsed };
    return { ...parsed, ok: true, deleted: true };
  } catch (err) {
    return { ok: false, deleted: false, error: err?.message || String(err), stdout: String(probe.stdout || "").slice(0, 1200) };
  }
}
export async function updateLesson({
  effectiveRoomDir,
  kind,
  id,
  expectedTitle,
  patch = {},
  timeoutMs = WRITE_TIMEOUT_MS,
}) {
  const patchKeys = ["title", "body", "shape", "triggerContext", "tags", "threadKeys", "voice", "scope", "project", "proofPattern", "languageKeys", "technologyKeys", "register", "exampleText", "writers", "negationOf"];
  if (!patchKeys.some((key) => Object.prototype.hasOwnProperty.call(patch, key) && patch[key] !== undefined)) {
    return { ok: false, updated: false, error: "at least one update field is required" };
  }
  const script = path.join(ATHANOR_ROOT, "src", "update-lesson.py");
  const argv = [
    "--cd", "~",
    "python3", windowsPathToWsl(script),
    "--room-dir", windowsPathToWsl(effectiveRoomDir),
    "--kind", String(kind),
    "--id", String(id),
    "--expected-title", String(expectedTitle),
  ];
  const values = [
    ["title", "--title"],
    ["shape", "--shape"],
    ["triggerContext", "--trigger-context"],
    ["voice", "--voice"],
    ["scope", "--scope"],
    ["project", "--project"],
    ["proofPattern", "--proof-pattern"],
    ["exampleText", "--example-text"],
    ["negationOf", "--negation-of"],
  ];
  for (const [key, flag] of values) {
    if (Object.prototype.hasOwnProperty.call(patch, key) && patch[key] !== null && patch[key] !== undefined) {
      argv.push(flag, String(patch[key]));
    }
  }
  for (const [key, flag] of [["register", "--register"], ["writers", "--writer"], ["languageKeys", "--language-key"], ["technologyKeys", "--technology-key"], ["threadKeys", "--thread-key"]]) {
    if (Object.prototype.hasOwnProperty.call(patch, key)) {
      for (const value of Array.isArray(patch[key]) ? patch[key] : []) argv.push(flag, String(value));
    }
  }
  if (Object.prototype.hasOwnProperty.call(patch, "negationOf") && patch.negationOf === null) {
    argv.push("--clear-negation-of");
  }
  if (Object.prototype.hasOwnProperty.call(patch, "tags")) {
    for (const tag of Array.isArray(patch.tags) ? patch.tags : []) argv.push("--tag", String(tag));
  }
  const hasBody = Object.prototype.hasOwnProperty.call(patch, "body");
  if (hasBody) argv.push("--lesson-stdin");
  const probe = await runWslDiagnostic({ argv, stdin: hasBody ? String(patch.body ?? "") : "", timeoutMs });
  if (probe.timedOut) return { ok: false, updated: false, error: "update-lesson timed out" };
  if (probe.spawnError) return { ok: false, updated: false, error: probe.spawnError };
  if (probe.code !== 0) return { ok: false, updated: false, error: String(probe.stderr || "").trim() || `update-lesson exited ${probe.code}` };
  try {
    const parsed = JSON.parse(String(probe.stdout || "{}"));
    if (parsed?.ok !== true || parsed?.updated !== true) return { ...parsed, ok: false, updated: false };
    return { ...parsed, ok: true, updated: true };
  } catch (err) {
    return { ok: false, updated: false, error: err?.message || String(err), stdout: String(probe.stdout || "").slice(0, 1200) };
  }
}

async function runCabinetWriter({ room, payload, append = false, timeoutMs = WRITE_TIMEOUT_MS }) {
  const substrate = requireSubstrate();
  if (substrate.error) return { ok: false, error: substrate.error };
  const { dir } = substratePaths(substrate.dir);
  const script = path.join(dir, "record_cabinet_entry.py");
  const temp = await mkdtemp(path.join(tmpdir(), "anamnesis-"));
  const files = new Map();
  try {
    const argv = ["--cd", "~", "python3", windowsPathToWsl(script), "--room", String(room), append ? "append-rep" : "add"];
    const add = (flag, value) => { if (value !== undefined && value !== null && String(value) !== "") argv.push(flag, String(value)); };
    const addFile = async (key, flag, value) => {
      if (value === undefined || value === null || String(value) === "") return;
      const target = path.join(temp, `${key}.txt`);
      await writeFile(target, String(value), "utf8");
      files.set(key, target);
      argv.push(flag, windowsPathToWsl(target));
    };
    if (append) {
      add("--title", payload?.title);
      add("--rep-number", payload?.repNumber);
      add("--occurred-on", payload?.occurredOn);
      await addFile("how-it-went", "--how-it-went-file", payload?.howItWent);
      await addFile("portal-pull", "--portal-pull-file", payload?.portalPull);
      await addFile("lighter", "--lighter-file", payload?.lighter);
      for (const source of Array.isArray(payload?.sourcePaths) ? payload.sourcePaths : []) add("--source-path", source);
    } else {
      add("--kind", payload?.kind);
      add("--fidelity", payload?.fidelity);
      add("--activation", payload?.activation);
      if (payload?.dormant) argv.push("--dormant");
      add("--title", payload?.title); add("--shape", payload?.shape);
      if (payload?.allowEmptyCycle) argv.push("--allow-empty-cycle");
      await addFile("ramp", "--ramp-file", payload?.ramp);
      await addFile("counsel", "--counsel-file", payload?.counsel);
      await addFile("peak", "--peak-file", payload?.peak);
      await addFile("beginning", "--beginning-file", payload?.beginning);
      await addFile("verify-note", "--verify-note-file", payload?.verifyNote);
      for (const value of Array.isArray(payload?.canon) ? payload.canon : []) add("--canon", value);
      for (const value of Array.isArray(payload?.sourcePaths) ? payload.sourcePaths : []) add("--source-path", value);
      for (const value of Array.isArray(payload?.tags) ? payload.tags : []) add("--tag", value);
      if (payload?.seedRep) {
        add("--seed-rep-number", payload.seedRep.number);
        add("--seed-rep-on", payload.seedRep.occurredOn);
        await addFile("seed-rep-how", "--seed-rep-how-file", payload.seedRep.howItWent);
        await addFile("seed-rep-portal", "--seed-rep-portal-file", payload.seedRep.portalPull);
        await addFile("seed-rep-lighter", "--seed-rep-lighter-file", payload.seedRep.lighter);
      }
    }
    const probe = await runWslDiagnostic({ argv, stdin: "", timeoutMs });
    if (probe.timedOut) return { ok: false, error: "record_cabinet_entry timed out" };
    if (probe.spawnError) return { ok: false, error: probe.spawnError };
    if (probe.code !== 0) return { ok: false, error: String(probe.stderr || "").trim() || `record_cabinet_entry exited ${probe.code}` };
    const summary = String(probe.stdout || "").trim();
    const idPattern = append ? /cabinet rep:\s*id=(\d+)/i : /cabinet add:\s*id=(\d+)/i;
    const idMatch = idPattern.exec(summary);
    return {
      ok: true,
      ...(idMatch ? { id: Number(idMatch[1]) } : {}),
      summary: summary.slice(0, 1200),
    };
  } catch (err) {
    return { ok: false, error: err?.message || String(err) };
  } finally {
    await rm(temp, { recursive: true, force: true }).catch(() => {});
  }
}

export function writeAnamnesisDrawer({ room, payload, timeoutMs = WRITE_TIMEOUT_MS }) {
  return runCabinetWriter({ room, payload, timeoutMs });
}

export function appendAnamnesisRep({ room, payload, timeoutMs = WRITE_TIMEOUT_MS }) {
  return runCabinetWriter({ room, payload, append: true, timeoutMs });
}
