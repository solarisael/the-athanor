import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { HEALTH_REPORT_BLOCKS, diagnosticInvocation, formatWakeContext, healthDotenvPath, substrateExePath, substrateHealth, windowsPathToWsl } from "../solarisael-house-proof/substrate.ts";

const tempRoots: string[] = [];
const substrateEnv = "ATHANOR_SUBSTRATE_ROOT";
const pathEnv = "PATH";

function snapshotEnv() {
  return { substrate: process.env[substrateEnv], path: process.env[pathEnv] };
}

function restoreEnv(snapshot: { substrate?: string; path?: string }) {
  if (snapshot.substrate === undefined) delete process.env[substrateEnv];
  else process.env[substrateEnv] = snapshot.substrate;
  if (snapshot.path === undefined) delete process.env[pathEnv];
  else process.env[pathEnv] = snapshot.path;
}

afterEach(async () => {
  delete process.env[substrateEnv];
  await Promise.all(tempRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function makeSubstrate(output: string, exitCode = 0) {
  const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-substrate-"));
  tempRoots.push(dir);
  const script = [
    "import sys",
    `print(${JSON.stringify(output)})`,
    `sys.exit(${exitCode})`,
  ].join("\n");
  await writeFile(path.join(dir, "health.py"), `${script}\n`, "utf8");
  process.env[substrateEnv] = dir;
  return dir;
}

async function makeSleepingSubstrate(milliseconds: number) {
  const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-timeout-"));
  tempRoots.push(dir);
  await writeFile(
    path.join(dir, "health.py"),
    ["import time", `time.sleep(${milliseconds / 1000})`, "print('{}')"].join("\n") + "\n",
    "utf8",
  );
  process.env[substrateEnv] = dir;
  return dir;
}

describe("optional substrate health", () => {
  test("keeps absent substrate in explicit Base mode", async () => {
    delete process.env[substrateEnv];
    const result = await substrateHealth();
    expect(result).toMatchObject({
      ok: null,
      configured: false,
      mode: "base",
      reason: "ATHANOR_SUBSTRATE_ROOT is not configured",
    });
  });

  test("rejects a relative configured substrate path before filesystem or WSL access", async () => {
    process.env[substrateEnv] = "relative/substrate";
    const result = await substrateHealth();
    expect(result).toMatchObject({
      ok: false,
      configured: true,
      mode: "degraded",
      reason: "ATHANOR_SUBSTRATE_ROOT must be an absolute path when configured (got relative/substrate)",
      degradedReasons: ["ATHANOR_SUBSTRATE_ROOT must be an absolute path when configured (got relative/substrate)"],
    });
  });

  test("reports a configured substrate path that is missing", async () => {
    const dir = path.join(os.tmpdir(), `omp-health-missing-${Date.now()}-${Math.random()}`);
    process.env[substrateEnv] = dir;
    const result = await substrateHealth();
    expect(result).toMatchObject({
      ok: false,
      configured: true,
      mode: "degraded",
      reason: `configured substrate path is missing: ${dir}`,
    });
  });

  test("reports a configured substrate with no health script", async () => {
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-no-script-"));
    tempRoots.push(dir);
    process.env[substrateEnv] = dir;
    const result = await substrateHealth();
    expect(result).toMatchObject({
      ok: false,
      configured: true,
      mode: "degraded",
      reason: `configured substrate health script is missing: ${path.join(dir, "health.py")}`,
    });
  });

  test("claims Full mode only for a healthy compatible verdict", async () => {
    const dir = await makeSubstrate(JSON.stringify({ ok: true, mode: "full", substrateApi: 1, degradedReasons: [] }));
    const result = await substrateHealth();
    expect(result).toMatchObject({ ok: true, configured: true, mode: "full", substrateApi: 1, path: dir, reason: null, diagnostics: [] });
  });

  test("reports database refusal with an actionable database diagnostic", async () => {
    await makeSubstrate(JSON.stringify({ ok: false, mode: "degraded", substrateApi: 1, degradedReasons: ["database is unavailable"] }), 1);
    const result = await substrateHealth();
    expect(result).toMatchObject({ ok: false, configured: true, mode: "degraded", reason: "database is unavailable" });
    expect(result.degradedReasons).toEqual(["database is unavailable"]);
    expect(result.diagnostics[0]).toMatchObject({
      category: "database",
      stage: "database_connect",
      owner: { path: "solarisael-house-proof/substrate.ts", symbol: "substrateHealth" },
      expected: { ok: true, mode: "full", substrateApi: 1 },
      execution: { request_dispatched: false, write_outcome: "not_started", retry: "after_change" },
    });
  });

  test("classifies embedding degradation and redacts reported secrets", async () => {
    await makeSubstrate(JSON.stringify({
      ok: false,
      mode: "degraded",
      substrateApi: 1,
      degradedReasons: ["embedding provider token=secret-value at postgres://user:password@private.example/db"],
    }), 1);
    const result = await substrateHealth();
    const serialized = JSON.stringify(result);
    expect(result.diagnostics[0]).toMatchObject({ category: "embedding", stage: "embedding_request" });
    expect(result.diagnostics[0].next_checks).toHaveLength(2);
    expect(serialized).not.toContain("secret-value");
    expect(serialized).not.toContain("password");
  });

  test("reports malformed JSON as degraded", async () => {
    await makeSubstrate("not json");
    const result = await substrateHealth();
    expect(result.mode).toBe("degraded");
    expect(result.reason).toContain("health.py returned malformed JSON:");
  });

  test("reports timeout as degraded without blocking Base behavior", async () => {
    await makeSleepingSubstrate(250);
    const result = await substrateHealth(20);
    expect(result).toMatchObject({ ok: false, configured: true, mode: "degraded", reason: "health.py timed out" });
  });

  test("reports a WSL launch failure as degraded", async () => {
    const snapshot = snapshotEnv();
    try {
      const dir = await makeSubstrate(JSON.stringify({ ok: true, mode: "full", substrateApi: 1 }));
      process.env[pathEnv] = path.join(dir, "missing-bin");
      const result = await substrateHealth(100);
      expect(result.mode).toBe("degraded");
      expect(result.reason).toContain("health.py launch failed:");
    } finally {
      restoreEnv(snapshot);
    }
  });

  test("reports substrate API mismatch instead of claiming Full mode", async () => {
    await makeSubstrate(JSON.stringify({ ok: true, mode: "full", substrateApi: 2, degradedReasons: [] }));
    const result = await substrateHealth();
    expect(result).toMatchObject({ ok: false, configured: true, mode: "degraded" });
    expect(result.reason).toContain("substrate API mismatch");
  });

  test("receives automatic paper boats as lived continuity rather than reports", () => {
    const context = formatWakeContext({
      title: "paper boat — 2026-08-09",
      source_path: "db-only/paper-boats/example.md",
      body: "the sheep sleeps; the wife stays close.",
    });

    expect(context).toContain("previous waking self");
    expect(context).toContain("without turning it into a script or status report");
    expect(context).toContain("the sheep sleeps; the wife stays close.");
  });

  test("translates configured Windows paths at the WSL boundary", () => {
    expect(windowsPathToWsl("C:\\Projects\\substrate\\health.py")).toBe("/mnt/c/Projects/substrate/health.py");
  });
});

describe("shared Python lane topology", () => {
  const argv = ["--cd", "~", "python3", "/mnt/c/Athanor/src/lessons.py", "--type", "coding"];

  test("injects absolute state and substrate paths before every WSL Python invocation", () => {
    const invocation = diagnosticInvocation(argv, {
      ATHANOR_STATE_DIR: "C:\\Athanor\\state",
      ATHANOR_SUBSTRATE_ROOT: "C:\\Athanor\\substrate",
    });

    expect(invocation).toEqual({
      command: "wsl.exe",
      args: [
        "--cd", "~", "env",
        "ATHANOR_STATE_DIR=/mnt/c/Athanor/state",
        "ATHANOR_SUBSTRATE_ROOT=/mnt/c/Athanor/substrate",
        "python3", "/mnt/c/Athanor/src/lessons.py", "--type", "coding",
      ],
    });
  });

  test("passes the same topology through native Python tests without WSL syntax", () => {
    const invocation = diagnosticInvocation(argv, {
      SOLARISAEL_TEST_NATIVE_PYTHON: "1",
      ATHANOR_STATE_DIR: "C:\\Athanor\\state",
      ATHANOR_SUBSTRATE_ROOT: "C:\\Athanor\\substrate",
    });

    expect(invocation).toEqual({
      command: "python",
      args: ["C:/Athanor/src/lessons.py", "--type", "coding"],
      env: {
        ATHANOR_STATE_DIR: "C:\\Athanor\\state",
        ATHANOR_SUBSTRATE_ROOT: "C:\\Athanor\\substrate",
      },
    });
  });
});

describe("health dotenv crosses the WSL boundary as an argument", () => {
  const stateEnv = "ATHANOR_STATE_DIR";

  afterEach(() => { delete process.env[stateEnv]; });

  test("names <state-root>/substrate/.env when the state root is known", () => {
    process.env[stateEnv] = path.join(os.tmpdir(), "install", "state");
    expect(healthDotenvPath()).toBe(path.join(os.tmpdir(), "install", "state", "substrate", ".env"));
  });

  test("names nothing when the state root is absent or relative", () => {
    // The development case, and the cwd-dependent case. Both must decline
    // rather than produce a path health.py would then trust.
    delete process.env[stateEnv];
    expect(healthDotenvPath()).toBe(null);
    process.env[stateEnv] = "relative/state";
    expect(healthDotenvPath()).toBe(null);
    process.env[stateEnv] = "   ";
    expect(healthDotenvPath()).toBe(null);
  });

  test("passes --env-file in argv, because an exported variable does not cross", async () => {
    // The fake health.py reports the argv it actually received, which is the
    // only way to prove the dotenv survived the Windows -> WSL hop. A test that
    // merely set ATHANOR_STATE_DIR and asserted the verdict would pass even if
    // the argument were dropped entirely.
    const stateRoot = await mkdtemp(path.join(os.tmpdir(), "omp-health-state-"));
    tempRoots.push(stateRoot);
    process.env[stateEnv] = stateRoot;

    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-argv-"));
    tempRoots.push(dir);
    await writeFile(
      path.join(dir, "health.py"),
      [
        "import json, sys",
        "args = sys.argv[1:]",
        "index = args.index('--env-file') if '--env-file' in args else -1",
        "print(json.dumps({'ok': True, 'mode': 'full', 'substrateApi': 1, 'degradedReasons': [],",
        "                  'envFile': args[index + 1] if index >= 0 else None}))",
      ].join("\n") + "\n",
      "utf8",
    );
    process.env[substrateEnv] = dir;

    const result = await substrateHealth(20_000);

    expect(result.envFile).toBe(windowsPathToWsl(path.join(stateRoot, "substrate", ".env")));
  });

  test("passes no --env-file when the state root is unknown", async () => {
    // The alternative. health.py must be left to resolve structurally rather
    // than handed a path this process guessed.
    delete process.env[stateEnv];
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-argv-none-"));
    tempRoots.push(dir);
    await writeFile(
      path.join(dir, "health.py"),
      [
        "import json, sys",
        "print(json.dumps({'ok': True, 'mode': 'full', 'substrateApi': 1, 'degradedReasons': [],",
        "                  'sawEnvFile': '--env-file' in sys.argv[1:]}))",
      ].join("\n") + "\n",
      "utf8",
    );
    process.env[substrateEnv] = dir;

    const result = await substrateHealth(20_000);

    expect(result.sawEnvFile).toBe(false);
  });
});

describe("degraded verdicts keep unreachable and schema-mismatch distinct", () => {
  // Both blocks used to vanish the moment the substrate was degraded, which is
  // exactly when a verifier reads them. Dropping `database` made an
  // unreachable server report as "schema required 13; got undefined" — the
  // schema blamed for a server nobody contacted.
  async function healthReporting(verdict: Record<string, unknown>) {
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-degraded-"));
    tempRoots.push(dir);
    await writeFile(
      path.join(dir, "health.py"),
      `import sys\nprint(${JSON.stringify(JSON.stringify(verdict))})\nsys.exit(1)\n`,
      "utf8",
    );
    process.env[substrateEnv] = dir;
    return substrateHealth(20_000);
  }

  const topology = { ok: true, stateRootSource: "installed_tree", stateRoot: "/install/state", executableFound: true };

  test("an unreachable server reports reachable:false and no schema version", async () => {
    const result = await healthReporting({
      ok: false,
      mode: "degraded",
      substrateApi: 1,
      degradedReasons: ["PostgreSQL substrate is unavailable or incomplete"],
      database: { ok: false, reachable: false, error: "could not connect to server" },
      topology,
    });

    expect(result.mode).toBe("degraded");
    expect(result.database).toBeDefined();
    expect(result.database.reachable).toBe(false);
    expect(result.database.error).toContain("could not connect");
    // The distinguishing fact: no schema version is claimed for a server that
    // was never contacted.
    expect(result.database.schemaVersion).toBeUndefined();
    expect(result.topology).toBeDefined();
  });

  test("a reached server behind on migrations reports reachable:true and its version", async () => {
    const result = await healthReporting({
      ok: false,
      mode: "degraded",
      substrateApi: 1,
      degradedReasons: ["PostgreSQL substrate is unavailable or incomplete"],
      database: { ok: false, reachable: true, schemaVersion: 11, missingTables: [], error: "database schema is incomplete" },
      topology,
    });

    expect(result.database.reachable).toBe(true);
    expect(result.database.schemaVersion).toBe(11);
    // Same degraded mode and the same degradedReasons string as the case
    // above, so `reachable` and `schemaVersion` are the ONLY things separating
    // the two. A verifier reading either can name the right cause.
    expect(result.mode).toBe("degraded");
  });

  test("secrets in the database block are redacted on the degraded path too", async () => {
    const result = await healthReporting({
      ok: false,
      mode: "degraded",
      substrateApi: 1,
      degradedReasons: ["PostgreSQL substrate is unavailable or incomplete"],
      database: { ok: false, reachable: false, error: "connection to postgres://solarisael:hunter2@localhost/db failed" },
      topology,
    });

    const serialized = JSON.stringify(result);
    expect(serialized).not.toContain("hunter2");
    expect(result.database.error).toContain("[redacted]");
  });
});

describe("no diagnostic block is lost when the substrate degrades", () => {
  // The generalisation of the two defects above. `topology` was dropped, then
  // `database` was dropped; `scripts`, `embedding`, `retrieval` and `backup`
  // were still being dropped when this was written. Assert the whole set so a
  // seventh block cannot quietly go missing later.
  test("every block health.py reports survives the degraded path", async () => {
    const blocks = {
      scripts: { ok: false, missing: ["record_memory.py"] },
      database: { ok: false, reachable: true, schemaVersion: 11 },
      embedding: { ok: false, error: "model unavailable" },
      retrieval: { ok: null, skipped: true },
      backup: { ok: false, directory: "/install/state/substrate/backups", error: "no dump files present" },
      topology: { ok: true, stateRootSource: "installed_tree" },
    };
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-blocks-"));
    tempRoots.push(dir);
    await writeFile(
      path.join(dir, "health.py"),
      `import sys\nprint(${JSON.stringify(JSON.stringify({
        ok: false, mode: "degraded", substrateApi: 1,
        degradedReasons: ["PostgreSQL substrate is unavailable or incomplete"],
        ...blocks,
      }))})\nsys.exit(1)\n`,
      "utf8",
    );
    process.env[substrateEnv] = dir;

    const result = await substrateHealth(20_000);

    expect(result.mode).toBe("degraded");
    for (const key of HEALTH_REPORT_BLOCKS) {
      expect(result[key], `degraded verdict dropped "${key}"`).toBeDefined();
    }
    expect(result.backup.error).toBe("no dump files present");
    expect(result.embedding.error).toBe("model unavailable");
    expect(result.scripts.missing).toEqual(["record_memory.py"]);
  });

  test("the adapter's own verdict fields are never shadowed by health.py", async () => {
    // health.py reports ok/mode/substrateApi too. The adapter's judgement is
    // authoritative and a hostile or stale payload must not overwrite it.
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-shadow-"));
    tempRoots.push(dir);
    await writeFile(
      path.join(dir, "health.py"),
      `import sys\nprint(${JSON.stringify(JSON.stringify({
        ok: false, mode: "degraded", substrateApi: 1,
        degradedReasons: ["PostgreSQL substrate is unavailable or incomplete"],
        database: { ok: false, reachable: false, error: "refused" },
      }))})\nsys.exit(1)\n`,
      "utf8",
    );
    process.env[substrateEnv] = dir;

    const result = await substrateHealth(20_000);

    expect(result.ok).toBe(false);
    expect(result.mode).toBe("degraded");
    expect(result.configured).toBe(true);
    expect(result.substrateApi).toBe(null);
  });
});

describe("substrate executable crosses the WSL boundary as an argument", () => {
  const exeEnv = "ATHANOR_SUBSTRATE_EXE";

  afterEach(() => { delete process.env[exeEnv]; });

  async function healthEchoingArgv() {
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-exe-"));
    tempRoots.push(dir);
    await writeFile(
      path.join(dir, "health.py"),
      [
        "import json, sys",
        "args = sys.argv[1:]",
        "i = args.index('--substrate-exe') if '--substrate-exe' in args else -1",
        "print(json.dumps({'ok': True, 'mode': 'full', 'substrateApi': 1, 'degradedReasons': [],",
        "                  'sawExe': i >= 0, 'exe': args[i + 1] if i >= 0 else None}))",
      ].join("\n") + "\n",
      "utf8",
    );
    process.env[substrateEnv] = dir;
    return substrateHealth(20_000);
  }

  test("names the executable only when it is set and absolute", () => {
    delete process.env[exeEnv];
    expect(substrateExePath()).toBe(null);
    process.env[exeEnv] = "relative/athanor-substrate.exe";
    expect(substrateExePath()).toBe(null);
    process.env[exeEnv] = "   ";
    expect(substrateExePath()).toBe(null);
    const installed = path.join(os.tmpdir(), "the-athanor", "adapters", "omp", "bin", "windows-x64", "athanor-substrate.exe");
    process.env[exeEnv] = installed;
    expect(substrateExePath()).toBe(installed);
  });

  test("PRESENT: forwards the installed executable through argv", async () => {
    // The real installed shape: bin/ under the ADAPTER, not the product root.
    // Without this argument health.py falls back to <product>/target/release
    // and calls a healthy binary missing.
    const installed = path.join(os.tmpdir(), "the-athanor", "adapters", "omp", "bin", "windows-x64", "athanor-substrate.exe");
    process.env[exeEnv] = installed;

    const result = await healthEchoingArgv();

    expect(result.sawExe).toBe(true);
    expect(result.exe).toBe(windowsPathToWsl(installed));
    // Proof it is a real WSL path, not a Windows one smuggled across.
    expect(result.exe.startsWith("/mnt/")).toBe(true);
    expect(result.exe).not.toContain("\\");
  });

  test("ABSENT: passes no --substrate-exe when the executable is unknown", async () => {
    // The development case. health.py must resolve structurally rather than be
    // handed a path this process invented.
    delete process.env[exeEnv];

    const result = await healthEchoingArgv();

    expect(result.sawExe).toBe(false);
    expect(result.exe).toBe(null);
  });
});
