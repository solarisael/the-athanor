import { describe, expect, test } from "bun:test";

import { emitToolUpdate, createToolRenderers, normalizeToolResponse, toolThrown } from "../solarisael-house-proof/feedback.ts";
import { registerSolarisaelTools } from "../solarisael-house-proof/tools.ts";

type Schema = {
  describe(description: string): Schema;
  regex(pattern: RegExp): Schema;
  optional(): Schema;
  default(value: unknown): Schema;
};

type CapturedTool = {
  name: string;
  label: string;
  execute: (...args: unknown[]) => Promise<unknown>;
  renderCall: (...args: unknown[]) => { render(width: number): string[] };
  renderResult: (...args: unknown[]) => { render(width: number): string[] };
};

function schema(): Schema {
  return {
    describe() { return this; },
    regex() { return this; },
    optional() { return this; },
    default() { return this; },
  };
}

const zod = {
  string: schema,
  boolean: schema,
  number: schema,
  enum: (_values: string[]) => schema(),
  array: (_element: Schema) => schema(),
  object: (_shape: Record<string, Schema>) => schema(),
  literal: (_value: string | boolean) => schema(),
  discriminatedUnion: (_key: string, _variants: Schema[]) => schema(),
};

const HOUSE_TOOL_NAMES = [
  "recall",
  "canon_read",
  "canon_write",
  "remember",
  "delete_lesson",
  "update_lesson",
  "wake",
  "room_state",
  "set_room_state",
  "lessons",
  "design_doc",
  "design_doc_write",
  "sleep",
  "house_lane_status",
  "familiar_status",
  "familiar_dispatch",
  "house_dispatch",
  "house_routing_mode",
  "kitten_lineage_status",
  "recall_policy",
  "house_model_default",
  "anamnesis",
  "anamnesis_write",
  "giga_candidate_list",
  "giga_health",
  "giga_queue_maintenance",
  "giga_review",
  "giga_promote_memory",
  "giga_promote_coding_lesson",
  "giga_promote_project_lesson",
];

function registerTools(): CapturedTool[] {
  const tools: CapturedTool[] = [];
  registerSolarisaelTools({
    zod,
    registerTool(tool: CapturedTool) { tools.push(tool); },
  });
  return tools;
}

function parsed(result: { content: Array<{ text: string }> }) {
  return JSON.parse(result.content[0].text);
}

describe("House tool feedback", () => {
  test("wraps every registered tool family with generic rendering and canonical success JSON", () => {
    const tools = registerTools();
    expect(tools.map((tool) => tool.name)).toEqual(HOUSE_TOOL_NAMES);

    for (const tool of tools) {
      expect(tool.renderCall).toBeFunction();
      expect(tool.renderResult).toBeFunction();
      const success = {
        ok: true,
        family: tool.name,
        warnings: ["degraded cache only"],
        ...(tool.name === "house_lane_status" ? { substrate: { mode: "degraded", degradedReasons: ["health check unavailable"] } } : {}),
      };
      const result = normalizeToolResponse({ content: [{ type: "text", text: JSON.stringify(success) }] }, tool.name);
      expect(result).not.toHaveProperty("isError");
      expect(parsed(result)).toEqual(success);
      expect(result.details).toEqual(success);
    }
  });

  test("preserves structured Rust diagnostics and redacts secrets without changing the error contract", () => {
    for (const operation of HOUSE_TOOL_NAMES) {
      const result = normalizeToolResponse({
        isError: true,
        content: [{
          type: "text",
          text: JSON.stringify({
            ok: false,
            error: "DATABASE_URL=postgres://operator:ultra-secret@db.example/house Bearer bearer-secret",
            code: "database_unavailable",
            retryable: true,
            details: {
              category: "database",
              causes: [{ code: "connect_failed", message: "connection refused" }],
              evidence: [{ source: "rust_stderr", text: "DATABASE_URL=postgres://operator:ultra-secret@db.example/house; {\"payload\":\"private-payload\"}" }],
              requestBody: "private-request-body",
              warnings: ["degraded replica"],
            },
          }),
        }],
      }, operation);
      const output = parsed(result);
      expect(result.isError).toBe(true);
      expect(result.details).toEqual(output);
      expect(output).toMatchObject({
        ok: false,
        status: "error",
        code: "database_unavailable",
        retryable: true,
        details: {
          category: "database",
          operation,
          causes: [{ code: "connect_failed", message: "connection refused" }],
          warnings: ["degraded replica"],
        },
      });
      expect(output.error).toBe(output.message);
      expect(output.details.evidence).toHaveLength(1);
      expect(output.details.requestBody).toEqual({ redacted: true, present: true });
      expect(JSON.stringify(output)).not.toContain("ultra-secret");
      expect(JSON.stringify(output)).not.toContain("bearer-secret");
      expect(JSON.stringify(output)).not.toContain("private-payload");
      expect(JSON.stringify(output)).not.toContain("private-request-body");
    }
  });

  test("keeps code, retryability, causes, and bounded Rust stderr when an execution throws", () => {
    const error = Object.assign(new Error("transport failed"), {
      code: "rust_transport_failed",
      retryable: true,
      details: {
        causes: [{ code: "socket_closed", message: "worker closed its pipe" }],
        evidence: [{ source: "rust", kind: "protocol-response" }],
      },
      stderr: "Authorization: Bearer stderr-secret",
    });
    const result = toolThrown(error, "recall");
    const output = parsed(result);
    expect(result.details).toEqual(output);
    expect(output).toMatchObject({
      code: "rust_transport_failed",
      retryable: true,
      details: {
        operation: "recall",
        causes: [{ code: "socket_closed", message: "worker closed its pipe" }],
        evidence: [
          { source: "rust", kind: "protocol-response" },
          { source: "rust_stderr" },
        ],
      },
    });
    expect(output.error).toBe(output.message);
    expect(JSON.stringify(output)).not.toContain("stderr-secret");
    const opaqueDetails = normalizeToolResponse({
      isError: true,
      content: [{ type: "text", text: JSON.stringify({ ok: false, error: "failed", details: ["root-cause"] }) }],
    }, "recall");
    expect(parsed(opaqueDetails).details.upstream_details).toEqual(["root-cause"]);
  });

  test("makes unknown write outcomes reconciliation-first and streams canonical progress", () => {
    const updates: unknown[] = [];
    emitToolUpdate((update) => updates.push(update), "remember");
    expect(updates).toHaveLength(1);
    const update = updates[0] as { content: Array<{ text: string }>; details: unknown };
    expect(parsed(update)).toEqual(update.details);
    expect(update.details).toMatchObject({
      status: "running",
      operation: "remember",
      details: { execution: { write_outcome: "not_started" } },
    });

    for (const operation of ["remember", "delete_lesson", "update_lesson", "set_room_state", "sleep", "house_routing_mode", "recall_policy", "house_model_default", "anamnesis_write", "giga_promote_memory", "giga_promote_coding_lesson", "giga_promote_project_lesson"]) {
      const result = normalizeToolResponse({
        isError: true,
        content: [{
          type: "text",
          text: JSON.stringify({
            ok: false,
            error: "authoritative receipt was lost after dispatch",
            code: "outcome_unknown",
            outcome: "unknown",
            retryable: true,
            reconciled: true,
            committed: null,
            details: { evidence: [{ source: "transport", id: "request-17" }] },
          }),
        }],
      }, operation);
      const output = parsed(result);
      expect(output.details.execution).toEqual({
        request_dispatched: true,
        write_outcome: "unknown",
        retry: "reconcile_first",
      });
      expect(output.details.next_checks[0]).toMatchObject({ action: "reconcile", operation, retry: "reconcile_first" });
      expect(output.details.observed).toMatchObject({ outcome: "unknown", reconciled: true, committed: null });
    }
  });

  test("uses compact rendering by default and canonical JSON behind expansion", () => {
    const { renderCall, renderResult } = createToolRenderers("Remember");
    expect(renderCall({}, {}, { fg: (_color, text) => text }).render(120)).toEqual(["Athanor Remember"]);
    const result = normalizeToolResponse({
      isError: true,
      content: [{ type: "text", text: JSON.stringify({ ok: false, error: "missing project", code: "validation_error" }) }],
    }, "remember");
    expect(renderResult(result, { expanded: false }, { fg: (_color, text) => text }).render(120)[0]).toContain("validation_error");
    expect(renderResult(result, { expanded: true }, { fg: (_color, text) => text }).render(120).join("\n")).toBe(result.content[0].text);
  });

});
