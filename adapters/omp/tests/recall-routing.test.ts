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
        ok: false,
        query: "alpha",
        code: "rust_transport_failure",
        retryable: true,
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
        stderr: "postgres password=[redacted] unavailable",
        details: {
          category: "operation",
          expected: { database: "reachable" },
          observed: { connection: "refused" },
          targets: ["postgres://local"],
          next_checks: [{ action: "start", target: "postgres" }],
          execution: { request_dispatched: true, write_outcome: "not_started", retry: "after_change" },
        },
      },
    });
    expect((routed.result as any).details.evidence[0]).toEqual({ kind: "postgres", state: "down" });
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
