import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { HEALTH_REPORT_BLOCKS, catchBoat, closePaperBoatTransports, healthDotenvPath, sleepBoat, substrateExePath, substrateHealth } from "../house-proof/substrate.ts";
import { RustJsonlTransport, RustTransportOutcomeUnknownError } from "../rust-transport.ts";

const tempRoots: string[] = [];
const substrateEnv = "ATHANOR_SUBSTRATE_ROOT";
const executableEnv = "ATHANOR_SUBSTRATE_EXE";
const fixtureEnv = "ATHANOR_TEST_SUBSTRATE_HEALTH_SCRIPT";

afterEach(async () => {
  delete process.env[substrateEnv];
  delete process.env[executableEnv];
  delete process.env[fixtureEnv];
  closePaperBoatTransports();
  await Promise.all(tempRoots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function makeSubstrate(output: string, exitCode = 0) {
  const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-substrate-"));
  tempRoots.push(dir);
  const script = [
    `console.log(${JSON.stringify(output)});`,
    `process.exitCode = ${exitCode};`,
  ].join("\n");
  const fixture = path.join(dir, "health-fixture.js");
  await writeFile(fixture, `${script}\n`, "utf8");
  process.env[substrateEnv] = dir;
  process.env[executableEnv] = process.execPath;
  process.env[fixtureEnv] = fixture;
  return dir;
}

async function makeSleepingSubstrate(milliseconds: number) {
  const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-timeout-"));
  tempRoots.push(dir);
  const fixture = path.join(dir, "health-fixture.js");
  await writeFile(
    fixture,
    `setTimeout(() => console.log("{}"), ${milliseconds});\n`,
    "utf8",
  );
  process.env[substrateEnv] = dir;
  process.env[executableEnv] = process.execPath;
  process.env[fixtureEnv] = fixture;
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

  test("reports a configured substrate with no Rust executable", async () => {
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-no-executable-"));
    tempRoots.push(dir);
    process.env[substrateEnv] = dir;
    delete process.env[executableEnv];
    delete process.env[fixtureEnv];
    const result = await substrateHealth();
    expect(result).toMatchObject({
      ok: false,
      configured: true,
      mode: "degraded",
    });
    expect(result.reason).toContain("Rust substrate executable");
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
      owner: { path: "house-proof/substrate.ts", symbol: "substrateHealth" },
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
    expect(result.reason).toContain("Rust substrate health returned malformed JSON:");
  });

  test("reports timeout as degraded without blocking Base behavior", async () => {
    await makeSleepingSubstrate(250);
    const result = await substrateHealth(20);
    expect(result).toMatchObject({ ok: false, configured: true, mode: "degraded", reason: "Rust substrate health timed out" });
  });

  test("reports substrate API mismatch instead of claiming Full mode", async () => {
    await makeSubstrate(JSON.stringify({ ok: true, mode: "full", substrateApi: 2, degradedReasons: [] }));
    const result = await substrateHealth();
    expect(result).toMatchObject({ ok: false, configured: true, mode: "degraded" });
    expect(result.reason).toContain("substrate API mismatch");
  });


});

describe("Rust Paper Boat routing", () => {
  test("routes sleep and wake through domain-prefixed Rust methods", async () => {
    process.env[executableEnv] = process.execPath;
    const originalRequest = RustJsonlTransport.prototype.request;
    const calls: Array<{ method: string; params: unknown; options: unknown }> = [];
    RustJsonlTransport.prototype.request = async function (method, params, options) {
      calls.push({ method, params, options });
      if (method === "paper_boat_sleep") {
        return {
          ok: true,
          memory_id: "17",
          room: "kintsu",
          source_path: "db-only/paper-boats/sha256-proof.md",
          outbox_event_id: "event-17",
          inserted: true,
          durable: true,
          authority: "postgres",
          backup_status: "completed",
          warnings: [],
        };
      }
      return {
        ok: true,
        found: false,
        room: "kintsu",
        id: null,
        title: null,
        body: null,
        date: null,
        source_path: null,
        created_at: null,
        unboated: [],
        unboated_truncated: false,
        warnings: [],
      };
    };
    try {
      await expect(sleepBoat("kintsu", "letter")).resolves.toMatchObject({
        ok: true,
        durable: true,
        backup_status: "completed",
      });
      await expect(catchBoat("kintsu", { timeoutMs: 2_000 })).resolves.toMatchObject({
        ok: true,
        found: false,
        room: "kintsu",
      });
      expect(calls.map(({ method }) => method)).toEqual([
        "paper_boat_sleep",
        "paper_boat_wake",
      ]);
      expect(calls[0].params).toEqual({ room: "kintsu", body: "letter", backup: true });
      expect(calls[0].options).toMatchObject({ settleDefinitively: true });
      expect(calls[1].options).toMatchObject({ timeoutMs: 2_000 });
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closePaperBoatTransports();
    }
  });

  test("returns an explicit idempotent retry receipt for an uncertain sleep", async () => {
    process.env[executableEnv] = process.execPath;
    const originalRequest = RustJsonlTransport.prototype.request;
    RustJsonlTransport.prototype.request = async function () {
      throw new RustTransportOutcomeUnknownError();
    };
    try {
      await expect(sleepBoat("kintsu", "letter")).resolves.toMatchObject({
        ok: false,
        code: "outcome_unknown",
        outcome: "unknown",
        retryable: true,
      });
    } finally {
      RustJsonlTransport.prototype.request = originalRequest;
      closePaperBoatTransports();
    }
  });

  test("visibly refuses wake and sleep when Rust is missing", async () => {
    delete process.env[executableEnv];
    await expect(catchBoat("kintsu")).resolves.toMatchObject({ ok: false });
    await expect(sleepBoat("kintsu", "letter")).resolves.toMatchObject({ ok: false });
  });
});


describe("health dotenv reaches the native Rust CLI as an argument", () => {
  const stateEnv = "ATHANOR_STATE_DIR";

  afterEach(() => { delete process.env[stateEnv]; });

  test("names <state-root>/substrate/.env when the state root is known", () => {
    process.env[stateEnv] = path.join(os.tmpdir(), "install", "state");
    expect(healthDotenvPath()).toBe(path.join(os.tmpdir(), "install", "state", "substrate", ".env"));
  });

  test("names nothing when the state root is absent or relative", () => {
    delete process.env[stateEnv];
    expect(healthDotenvPath()).toBe(null);
    process.env[stateEnv] = "relative/state";
    expect(healthDotenvPath()).toBe(null);
  });

  test("passes --env-file as a native path", async () => {
    const stateRoot = await mkdtemp(path.join(os.tmpdir(), "omp-health-state-"));
    tempRoots.push(stateRoot);
    process.env[stateEnv] = stateRoot;
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-argv-"));
    tempRoots.push(dir);
    const fixture = path.join(dir, "health-fixture.js");
    await writeFile(
      fixture,
      "const args=process.argv.slice(2); const i=args.indexOf('--env-file'); console.log(JSON.stringify({ok:true,mode:'full',substrateApi:1,degradedReasons:[],envFile:i>=0?args[i+1]:null}));\n",
      "utf8",
    );
    process.env[substrateEnv] = dir;
    process.env[executableEnv] = process.execPath;
    process.env[fixtureEnv] = fixture;

    const result = await substrateHealth(20_000);
    expect(result.envFile).toBe(path.join(stateRoot, "substrate", ".env"));
  });

  test("passes no --env-file when the state root is unknown", async () => {
    delete process.env[stateEnv];
    const dir = await mkdtemp(path.join(os.tmpdir(), "omp-health-argv-none-"));
    tempRoots.push(dir);
    const fixture = path.join(dir, "health-fixture.js");
    await writeFile(
      fixture,
      "const args=process.argv.slice(2); console.log(JSON.stringify({ok:true,mode:'full',substrateApi:1,degradedReasons:[],sawEnvFile:args.includes('--env-file')}));\n",
      "utf8",
    );
    process.env[substrateEnv] = dir;
    process.env[executableEnv] = process.execPath;
    process.env[fixtureEnv] = fixture;

    const result = await substrateHealth(20_000);
    expect(result.sawEnvFile).toBe(false);
  });
});

describe("degraded verdicts keep unreachable and schema-mismatch distinct", () => {
  // Both blocks used to vanish the moment the substrate was degraded, which is
  // exactly when a verifier reads them. Dropping `database` made an
  // unreachable server report as "schema required 16; got undefined" — the
  // schema blamed for a server nobody contacted.
  async function healthReporting(verdict: Record<string, unknown>) {
    await makeSubstrate(JSON.stringify(verdict), 1);
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
  test("every block Rust reports survives the degraded path", async () => {
    const blocks = {
      scripts: { ok: true, missing: [], owner: "rust" },
      database: { ok: false, reachable: true, schemaVersion: 11 },
      embedding: { ok: false, error: "model unavailable" },
      retrieval: { ok: null, skipped: true },
      backup: { ok: false, directory: "/install/state/substrate/backups", error: "no dump files present" },
      topology: { ok: true, stateRootSource: "installed_tree" },
    };
    await makeSubstrate(JSON.stringify({
      ok: false, mode: "degraded", substrateApi: 1,
      degradedReasons: ["PostgreSQL substrate is unavailable or incomplete"],
      ...blocks,
    }), 1);

    const result = await substrateHealth(20_000);

    expect(result.mode).toBe("degraded");
    for (const key of HEALTH_REPORT_BLOCKS) {
      expect(result[key], `degraded verdict dropped "${key}"`).toBeDefined();
    }
    expect(result.backup.error).toBe("no dump files present");
    expect(result.embedding.error).toBe("model unavailable");
    expect(result.scripts.missing).toEqual([]);
  });

  test("the adapter's own verdict fields are never shadowed by Rust", async () => {
    await makeSubstrate(JSON.stringify({
      ok: false, mode: "degraded", substrateApi: 1,
      degradedReasons: ["PostgreSQL substrate is unavailable or incomplete"],
      database: { ok: false, reachable: false, error: "refused" },
    }), 1);
    const result = await substrateHealth(20_000);

    expect(result.ok).toBe(false);
    expect(result.mode).toBe("degraded");
    expect(result.configured).toBe(true);
    expect(result.substrateApi).toBe(null);
  });
});

describe("native substrate executable selection", () => {
  test("names the executable only when it is set and absolute", () => {
    delete process.env[executableEnv];
    expect(substrateExePath()).toBe(null);
    process.env[executableEnv] = "relative/athanor-substrate.exe";
    expect(substrateExePath()).toBe(null);
    process.env[executableEnv] = "   ";
    expect(substrateExePath()).toBe(null);
    const installed = path.join(os.tmpdir(), "the-athanor", "adapters", "omp", "bin", "windows-x64", "athanor-substrate.exe");
    process.env[executableEnv] = installed;
    expect(substrateExePath()).toBe(installed);
  });
});
