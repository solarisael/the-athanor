import { describe, expect, test, beforeEach, afterEach } from "bun:test";
import { RustJsonlTransport, RustTransportError } from "../rust-transport.ts";
import { closeRustRecallTransports, recallWithRouting } from "../solarisael-house-proof/recall.ts";

const originalRust = process.env.ATHANOR_SUBSTRATE_EXE;
const originalSubstrateRoot = process.env.ATHANOR_SUBSTRATE_ROOT;
const originalRequest = RustJsonlTransport.prototype.request;

const result = (query: string) => ({
  ok: true,
  query,
  found: true,
  source: "rust-postgres",
  retrievalCandidates: [],
  canonMatches: [],
  semanticChunks: [],
  contentChunks: [],
  dateMatches: [],
  queryDates: [],
  taxonomy: { memoryTypes: ["memory"], threadKeys: [], namedEntities: [] },
});

describe("Rust recall routing", () => {
  beforeEach(() => {
    process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
    process.env.ATHANOR_SUBSTRATE_ROOT = "test-substrate";
  });
  afterEach(() => {
    RustJsonlTransport.prototype.request = originalRequest;
    closeRustRecallTransports();
    if (originalRust === undefined) delete process.env.ATHANOR_SUBSTRATE_EXE;
    else process.env.ATHANOR_SUBSTRATE_EXE = originalRust;
    if (originalSubstrateRoot === undefined) delete process.env.ATHANOR_SUBSTRATE_ROOT;
    else process.env.ATHANOR_SUBSTRATE_ROOT = originalSubstrateRoot;
  });

  test("sends protocol recall params and accepts an authoritative result that omits warnings", async () => {
    let observed: unknown;
    RustJsonlTransport.prototype.request = async function (method, params, options) {
      observed = { method, params, options };
      return result("alpha");
    };
    const routed = await recallWithRouting("room-dir", "example", "alpha");
    expect(routed).toEqual({ ok: true, result: result("alpha") });
    expect(observed).toMatchObject({
      method: "recall",
      params: {
        room: "example",
        query: "alpha",
        semantic_top_k: 8,
        semantic_min_similarity: 0.4,
        content_top_k: 8,
        content_min_similarity: 0.3,
      },
      options: { timeoutMs: 120000 },
    });
    expect((observed as any).params).not.toHaveProperty("temporal_decay");
  });

  test("forwards an explicit temporal decay opt-in", async () => {
    let observed: any;
    RustJsonlTransport.prototype.request = async function (method, params) {
      observed = { method, params };
      return result("alpha");
    };

    await expect(
      recallWithRouting("room-dir", "example", "alpha", { temporalDecay: true }),
    ).resolves.toEqual({ ok: true, result: result("alpha") });
    expect(observed).toMatchObject({
      method: "recall",
      params: { temporal_decay: true },
    });
  });

  test("retries automatic recall without temporal decay against the previous API-1 parser", async () => {
    const observed: any[] = [];
    RustJsonlTransport.prototype.request = async function (_method, params) {
      observed.push(params);
      if (observed.length === 1) {
        throw new RustTransportError({
          code: "invalid_params",
          message: "unknown field `temporal_decay`",
          retryable: false,
        });
      }
      return result("alpha");
    };

    await expect(
      recallWithRouting("room-dir", "example", "alpha", { temporalDecay: true }),
    ).resolves.toEqual({ ok: true, result: result("alpha") });
    expect(observed[0].temporal_decay).toBe(true);
    expect(observed[1]).not.toHaveProperty("temporal_decay");
  });

  test("passes caller cancellation alongside the bounded timeout", async () => {
    let observed: any;
    RustJsonlTransport.prototype.request = async function (method, params, options) {
      observed = { method, params, options };
      return result("alpha");
    };
    const controller = new AbortController();
    await recallWithRouting("room-dir", "example", "alpha", { signal: controller.signal });
    expect(observed.options).toMatchObject({ signal: controller.signal, timeoutMs: 120000 });
    expect(observed.options.settleDefinitively).toBeUndefined();
  });

  test("accepts the exact candidate shape consumed by the compactor", async () => {
    RustJsonlTransport.prototype.request = async function () {
      return {
        ...result("alpha"),
        retrievalCandidates: [{
          source_path: "memory/alpha.md",
          title: "Alpha",
          heading_path: "Notes",
          excerpt: "alpha excerpt",
          sources: ["semantic"],
          score: 0.8,
          term_coverage: 1,
          matched_terms: ["alpha"],
          missing_terms: [],
          reasons: ["semantic search"],
        }],
      };
    };
    await expect(recallWithRouting("room-dir", "example", "alpha")).resolves.toMatchObject({ ok: true });
  });

  test("reports invalid result validation with a safe observed shape", async () => {
    RustJsonlTransport.prototype.request = async function () {
      return { ok: true, query: "alpha", found: true, source: "rust-postgres", private_payload: "do-not-leak" };
    };
    const routed = await recallWithRouting("room-dir", "example", "alpha");
    expect(routed.ok).toBe(false);
    expect(routed.result.error).toContain("result.retrievalCandidates must be an array");
    expect(routed.result).toMatchObject({
      code: "invalid_rust_result",
      retryable: true,
      details: {
        owner: { symbol: "validRustRecallResult" },
        observed: { type: "object", fields: { private_payload: "string" } },
        execution: { request_dispatched: true, write_outcome: "not_started", retry: "safe_now" },
      },
    });
    expect(JSON.stringify(routed.result.details)).not.toContain("do-not-leak");
  });

  test("rejects warnings unless every entry is a string", async () => {
    RustJsonlTransport.prototype.request = async function () {
      return {
        ...result("alpha"),
        warnings: ["semantic retrieval disabled", { code: "embedding_unavailable" }],
      };
    };
    const routed = await recallWithRouting("room-dir", "example", "alpha");
    expect(routed.ok).toBe(false);
    expect(routed.result.error).toContain("result.warnings must be an array of strings");
  });

  test("rejects missing taxonomy", async () => {
    RustJsonlTransport.prototype.request = async function () {
      return { ...result("alpha"), taxonomy: null };
    };
    const routed = await recallWithRouting("room-dir", "example", "alpha");
    expect(routed.ok).toBe(false);
    expect(routed.result.error).toContain("result.taxonomy must be an object");
  });

  test("rejects malformed candidate and date elements", async () => {
    RustJsonlTransport.prototype.request = async function () {
      return {
        ...result("alpha"),
        retrievalCandidates: [{ matched_terms: ["alpha"], missing_terms: "beta" }],
        dateMatches: [{ body_excerpt: 42 }],
      };
    };
    const routed = await recallWithRouting("room-dir", "example", "alpha");
    expect(routed.ok).toBe(false);
    expect(routed.result.error).toContain("exact compactor candidate fields");
  });

  test("reports a transport crash with a retry-safe diagnostic and respawns the next request", async () => {
    let calls = 0;
    RustJsonlTransport.prototype.request = async function () {
      calls += 1;
      if (calls === 1) {
        this.close();
        throw new Error("worker exited");
      }
      return result("alpha");
    };
    const crash = await recallWithRouting("room-dir", "example", "alpha");
    expect(crash).toMatchObject({
      ok: false,
      result: {
        code: "rust_transport_failure",
        retryable: true,
        details: {
          category: "transport",
          observed: { error_type: "Error" },
          execution: { request_dispatched: true, write_outcome: "not_started", retry: "safe_now" },
        },
      },
    });
    await expect(recallWithRouting("room-dir", "example", "alpha")).resolves.toEqual({ ok: true, result: result("alpha") });
    expect(calls).toBe(2);
  });

  test("preserves structured Rust diagnostics instead of taking the fallback route", async () => {
    RustJsonlTransport.prototype.request = async function () {
      throw new RustTransportError({
        code: "postgres_unavailable",
        message: "database down",
        retryable: false,
        details: {
          expected: { database: "reachable" },
          observed: { connection: "refused" },
          evidence: [{ kind: "postgres", state: "down" }],
          targets: ["postgres://local"],
          next_checks: [{ action: "start", target: "postgres" }],
          execution: { request_dispatched: true, write_outcome: "not_started", retry: "after_change" },
        },
      }, "postgres password=secret unavailable");
    };
    const routed = await recallWithRouting("room-dir", "example", "alpha");
    expect(routed).toMatchObject({
      ok: false,
      result: {
        ok: false,
        query: "alpha",
        error: "database down",
        code: "postgres_unavailable",
        retryable: false,
        details: {
          expected: { database: "reachable" },
          observed: { connection: "refused" },
          targets: ["postgres://local"],
          next_checks: [{ action: "start", target: "postgres" }],
          execution: { retry: "after_change" },
          evidence: [{ kind: "postgres", state: "down" }, { kind: "stderr", text: "postgres password=[redacted] unavailable" }],
        },
      },
    });
  });
  test("preserves valid cluster telemetry while stripping malformed advisory fields", async () => {
    RustJsonlTransport.prototype.request = async function () {
      return {
        ...result("alpha"),
        clusterStaleness: { built_at: "2026-07-01T00:00:00Z", chunks_since_build: 4, fraction_unseen: 0.2 },
        clusterResonance: {
          profile: [{ cluster_id: 1, label: "alpha", member_count: 3, activation: 0.8 }],
          hot: [{ cluster_id: 1, label: "alpha", chunks: [{ source_path: "memory/a.md", heading_path: null, sim: 0.7 }] }],
        },
      };
    };
    const valid = await recallWithRouting("room-dir", "example", "alpha");
    expect(valid).toMatchObject({ ok: true, result: { clusterStaleness: { fraction_unseen: 0.2 }, clusterResonance: { profile: [{ label: "alpha" }] } } });

    RustJsonlTransport.prototype.request = async function () {
      return {
        ...result("alpha"),
        clusterStaleness: { built_at: "not-a-date", chunks_since_build: -1, fraction_unseen: 4 },
        clusterResonance: { profile: [{ label: "bad", member_count: "3", activation: NaN }], hot: [{ cluster_id: 1, label: "bad", chunks: [{ source_path: "memory/a.md", heading_path: 42, sim: 0.7 }] }] },
      };
    };
    const malformed = await recallWithRouting("room-dir", "example", "alpha");
    expect(malformed).toEqual({ ok: true, result: result("alpha") });
  });

});

test("routes the Vault profile through the database-free Rust method", async () => {
  const previousRust = process.env.ATHANOR_SUBSTRATE_EXE;
  const previousSubstrateRoot = process.env.ATHANOR_SUBSTRATE_ROOT;
  let observed: unknown;
  process.env.ATHANOR_SUBSTRATE_EXE = process.execPath;
  delete process.env.ATHANOR_SUBSTRATE_ROOT;
  closeRustRecallTransports();
  RustJsonlTransport.prototype.request = async function (method, params) {
    observed = { method, params };
    return {
      ...result("VAULT-ROUTE-91"),
      source: "vault-files",
      authority: "vault-files",
      roots: ["/rooms/vault-room"],
      scannedFiles: 2,
      indexedDocuments: 3,
      taxonomy: {
        memoryTypes: ["vault-file"],
        threadKeys: [],
        namedEntities: [],
        fileTypes: ["markdown", "json", "jsonl", "text"],
      },
    };
  };
  try {
    const routed = await recallWithRouting("/rooms/vault-room", "vault-room", "VAULT-ROUTE-91");
    expect(observed).toEqual({
      method: "vault_recall",
      params: {
        room: "vault-room",
        room_dir: "/rooms/vault-room",
        query: "VAULT-ROUTE-91",
      },
    });
    expect(routed).toMatchObject({
      ok: true,
      result: {
        ok: true,
        source: "vault-files",
        authority: "vault-files",
        found: true,
      },
    });
  } finally {
    RustJsonlTransport.prototype.request = originalRequest;
    closeRustRecallTransports();
    if (previousRust === undefined) delete process.env.ATHANOR_SUBSTRATE_EXE;
    else process.env.ATHANOR_SUBSTRATE_EXE = previousRust;
    if (previousSubstrateRoot === undefined) delete process.env.ATHANOR_SUBSTRATE_ROOT;
    else process.env.ATHANOR_SUBSTRATE_ROOT = previousSubstrateRoot;
  }
});
