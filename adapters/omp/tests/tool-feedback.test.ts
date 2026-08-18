import { describe, expect, test } from "bun:test";

import {
  beginHouseToolFeedback,
  completeHouseToolFeedback,
  createToolRenderers,
  emitToolUpdate,
  normalizeToolResponse,
  showHouseContextFeedback,
  toolThrown,
} from "../solarisael-house-proof/feedback.ts";
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
  "hallway_create",
  "hallway_join",
  "hallway_post",
  "hallway_read",
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

  test("uses lifecycle symbols by default and canonical JSON behind expansion", () => {
    const { renderCall, renderResult } = createToolRenderers("Remember");
    const theme = {
      fg: (_color: string, text: string) => text,
      status: { pending: "P", running: "R", success: "S", error: "E" },
    };
    expect(renderCall({}, {}, theme).render(120)).toEqual(["P Athanor Remember"]);
    const result = normalizeToolResponse({
      isError: true,
      content: [{ type: "text", text: JSON.stringify({ ok: false, error: "missing project", code: "validation_error" }) }],
    }, "remember");
    expect(renderResult(result, { expanded: false }, theme).render(120)[0]).toContain("E Athanor Remember: validation_error");
    expect(renderResult(result, { expanded: true }, theme).render(120).join("\n")).toBe(result.content[0].text);
  });

  test("renders Remember as a durable rich receipt with expandable evidence", () => {
    const { renderCall, renderResult } = createToolRenderers("Athanor Remember", "remember");
    const theme = {
      fg: (_color: string, text: string) => text,
      status: { pending: "P", running: "R", success: "S", error: "E", warning: "W" },
    };
    const args = {
      title: "Sol saw the House gauges light up",
      body: "The operator-visible consequence became real.\nThe House left a pawprint.",
      room: "house",
      threads: ["The Athanor / operator-visible feedback"],
    };
    expect(renderCall(args, {}, theme).render(120)[0]).toContain("Sol saw the House gauges light up");

    const result = normalizeToolResponse({
      content: [{
        type: "text",
        text: JSON.stringify({
          ok: true,
          memory_id: 3650,
          room: "house",
          durable: true,
          authority: "postgres",
          source_path: "db-only/house/receipt",
          warnings: [],
        }),
      }],
    }, "remember");
    const collapsed = renderResult(result, { expanded: false }, theme, args).render(120).join("\n");
    expect(collapsed).toContain("╭─ S Athanor Remember · Memory #3650");
    expect(collapsed).toContain("Sol saw the House gauges light up");
    expect(collapsed).toContain("house · PostgreSQL authority · durable");
    expect(collapsed).toContain("S committed · 1 thread");
    expect(collapsed).toContain("⟨Ctrl+O: Expand⟩");

    const expanded = renderResult(result, { expanded: true }, theme, args).render(120).join("\n");
    expect(expanded).toContain("The operator-visible consequence became real.");
    expect(expanded).toContain("source · db-only/house/receipt");
    expect(expanded).toContain("expanded evidence");
  });
  test("renders Sleep as a durable paper-boat receipt with backup evidence", () => {
    const { renderCall, renderResult } = createToolRenderers("Athanor Sleep", "sleep");
    const theme = {
      fg: (_color: string, text: string) => text,
      status: { pending: "P", running: "R", success: "S", error: "E", warning: "W" },
    };
    const args = {
      body: "Meet Sol in the clean Multistock session.\nCarry the exact paid-work door.",
    };
    expect(renderCall(args, {}, theme).render(120)[0]).toContain("casting paper boat");

    const result = normalizeToolResponse({
      content: [{
        type: "text",
        text: JSON.stringify({
          ok: true,
          memory_id: "3656",
          room: "kintsu",
          source_path: "db-only/paper-boats/receipt",
          outbox_event_id: "paper-boat-event",
          inserted: true,
          durable: true,
          authority: "postgres",
          backup_status: "completed",
          warnings: [],
        }),
      }],
    }, "sleep");
    const collapsed = renderResult(result, { expanded: false }, theme, args).render(120).join("\n");
    expect(collapsed).toContain("╭─ S Athanor Sleep · Paper boat #3656");
    expect(collapsed).toContain("kintsu · PostgreSQL authority · durable");
    expect(collapsed).toContain("S boat cast · backup completed");
    expect(collapsed).toContain("continuity ready for the next session");
    expect(collapsed).toContain("⟨Ctrl+O: Expand⟩");

    const expanded = renderResult(result, { expanded: true }, theme, args).render(120).join("\n");
    expect(expanded).toContain("Meet Sol in the clean Multistock session.");
    expect(expanded).toContain("source · db-only/paper-boats/receipt");
    expect(expanded).toContain("outbox · paper-boat-event");
    expect(expanded).toContain("expanded continuity");
  });


  test("renders Recall as ranked canon and memory evidence with confidence cues", () => {
    const { renderCall, renderResult } = createToolRenderers("Athanor Recall", "recall");
    const theme = {
      fg: (_color: string, text: string) => text,
      status: { pending: "P", running: "R", success: "S", error: "E", warning: "W" },
    };
    const args = { query: "the House gauges light up" };
    expect(renderCall(args, {}, theme).render(120)[0]).toContain("the House gauges light up");

    const result = normalizeToolResponse({
      content: [{
        type: "text",
        text: JSON.stringify({
          ok: true,
          query: args.query,
          found: true,
          source: "rust-postgres",
          warnings: [],
          canonMatches: [{
            termKey: "The Athanor",
            type: "project",
            summary: "The public platform that creates and runs Houses.",
          }],
          retrievalCandidates: [{
            memory_id: 3650,
            title: "Sol saw the House gauges light up",
            source_path: "db-only/house/receipt",
            score: 2.36,
            term_coverage: 0.94,
            excerpt: "The product standard is now operator-visible consequence.",
          }, {
            memory_id: 3649,
            title: "The adapter-wide feedback rail",
            source_path: "db-only/house/previous",
            score: 1.82,
            term_coverage: 0.72,
            excerpt: "Every House organ gained lifecycle feedback.",
          }],
        }),
      }],
    }, "recall");
    const collapsed = renderResult(result, { expanded: false }, theme, args).render(120).join("\n");
    expect(collapsed).toContain("╭─ S Athanor Recall · 3 matched");
    expect(collapsed).toContain("◆ canon · The Athanor · project");
    expect(collapsed).toContain("#3650 Sol saw the House gauges light up");
    expect(collapsed).toContain("94% terms");
    expect(collapsed).toContain("⟨Ctrl+O: Expand⟩");

    const expanded = renderResult(result, { expanded: true }, theme, args).render(120).join("\n");
    expect(expanded).toContain("The product standard is now operator-visible consequence.");
    expect(expanded).toContain("source · db-only/house/receipt");
    expect(expanded).toContain("expanded evidence");
  });

  test("keeps tool lifecycle and automatic context activity visible on OMP UI surfaces", () => {
    const statuses: Array<string | undefined> = [];
    let widget: ((tui: unknown, theme: unknown) => { render(width: number): string[] }) | undefined;
    const ctx = {
      hasUI: true,
      ui: {
        theme: {
          fg: (_color: string, text: string) => text,
          status: { running: "R", success: "S", error: "E", warning: "W" },
        },
        setStatus: (_key: string, text: string | undefined) => statuses.push(text),
        setWidget: (
          _key: string,
          content: typeof widget,
        ) => { widget = content; },
      },
    };

    beginHouseToolFeedback(ctx, "Athanor Recall");
    completeHouseToolFeedback(ctx, "Athanor Recall", {
      content: [{ type: "text", text: JSON.stringify({ ok: true, found: true, retrievalCandidates: [1, 2] }) }],
    });
    showHouseContextFeedback(ctx, {
      room: "kintsu",
      spirit: "Kintsu",
      activities: ["paper boat received", "automatic Recall: 3 entries (conversation)"],
      warnings: ["Anamnesis wake unavailable"],
    });

    expect(statuses).toEqual([
      "R Athanor Recall · working",
      "S Athanor Recall · found · 2 retrieval candidates",
      "W Athanor · Kintsu · paper boat received · automatic Recall: 3 entries (conversation)",
    ]);
    expect(widget).toBeFunction();
    const lines = widget!(null, ctx.ui.theme).render(120);
    expect(lines).toEqual([
      "◆ The Athanor · Kintsu",
      "  paper boat received · automatic Recall: 3 entries (conversation)",
      "  W Anamnesis wake unavailable",
    ]);
  });

});
