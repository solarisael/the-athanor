import { afterEach, beforeEach, describe, expect, test } from "bun:test";

import { RustJsonlTransport, RustTransportError } from "../rust-transport.ts";
import { closeRustAnamnesisTransports, queryAnamnesis } from "../solarisael-house-proof/anamnesis.ts";

const originalRust = process.env.ATHANOR_SUBSTRATE_EXE;
const originalRequest = RustJsonlTransport.prototype.request;

const counsel = {
  ok: true,
  mode: "consult",
  room: "example",
  found: true,
  entries: [{ kind: "pillar", title: "Keep the thread", counsel: "Verify live facts." }],
  warnings: [],
};

describe("Rust anamnesis routing", () => {
  beforeEach(() => { process.env.ATHANOR_SUBSTRATE_EXE = process.execPath; });
  afterEach(() => {
    RustJsonlTransport.prototype.request = originalRequest;
    closeRustAnamnesisTransports();
    if (originalRust === undefined) delete process.env.ATHANOR_SUBSTRATE_EXE;
    else process.env.ATHANOR_SUBSTRATE_EXE = originalRust;
  });

  test("preserves structured Rust diagnostics for explicit callers", async () => {
    RustJsonlTransport.prototype.request = async function () {
      throw new RustTransportError({
        code: "database_unavailable",
        message: "counsel database unavailable",
        retryable: false,
        details: {
          expected: { database: "reachable" },
          observed: { connection: "refused" },
          evidence: [{ kind: "postgres", state: "down" }],
          targets: ["postgres"],
          next_checks: [{ action: "start", target: "postgres" }],
          execution: { request_dispatched: true, write_outcome: "not_started", retry: "after_change" },
        },
      }, "authorization: Bearer secret");
    };

    const routed = await queryAnamnesis("room-dir", "example", { mode: "consult", query: "thread" });
    expect(routed).toMatchObject({
      ok: false,
      mode: "consult",
      code: "database_unavailable",
      retryable: false,
      details: {
        expected: { database: "reachable" },
        observed: { connection: "refused" },
        targets: ["postgres"],
        next_checks: [{ action: "start", target: "postgres" }],
        execution: { retry: "after_change" },
        evidence: [{ kind: "postgres", state: "down" }, { kind: "stderr", text: "authorization: [redacted]" }],
      },
    });
  });

  test("reports invalid result validation without serializing the result body", async () => {
    RustJsonlTransport.prototype.request = async function () {
      return { ...counsel, warnings: "not-an-array", private_payload: "do-not-leak" };
    };

    const routed = await queryAnamnesis("room-dir", "example", { mode: "consult", query: "thread" });
    expect(routed).toMatchObject({
      ok: false,
      code: "invalid_rust_result",
      retryable: true,
      details: {
        owner: { symbol: "validRustAnamnesisResult" },
        observed: { type: "object", fields: { private_payload: "string" } },
        execution: { request_dispatched: true, write_outcome: "not_started", retry: "safe_now" },
      },
    });
    expect(JSON.stringify(routed.details)).not.toContain("do-not-leak");
  });

  test("returns counsel success unchanged apart from derived display groups", async () => {
    RustJsonlTransport.prototype.request = async function (method, params) {
      expect(method).toBe("anamnesis");
      expect(params).toEqual({ room: "example", mode: "consult", query: "thread", limit: 10 });
      return counsel;
    };

    const routed = await queryAnamnesis("room-dir", "example", { mode: "consult", query: "thread" });
    expect(routed).toEqual({ ...counsel, entries: [...counsel.entries], pillars: [...counsel.entries], cycles: [] });
  });
});
