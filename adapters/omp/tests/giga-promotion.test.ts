import { afterAll, beforeAll, beforeEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { __gigaPromotionTest, registerSolarisaelTools } from "../solarisael-house-proof/tools.ts";
import { RustTransportError, type TransportDiagnostic } from "../rust-transport.ts";

const calls = {
  listRooms: [] as string[],
  resolved: [] as Array<Record<string, unknown>>,
  promotions: [] as Array<Record<string, unknown>>,
};
let currentCandidate: Record<string, unknown>;
let sourceFailure: Error | null = null;


type Schema = {
  describe(description: string): Schema;
  regex(pattern: RegExp): Schema;
  optional(): Schema;
  default(value: unknown): Schema;
};

type CapturedTool = {
  name: string;
  execute: (...args: unknown[]) => Promise<{ isError?: boolean; content: Array<{ text: string }>; details: unknown }>;
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

function candidate(kind: "memory" | "coding_lesson" | "project_lesson", projectKeys: string[] = []) {
  const project = projectKeys[0] ?? null;
  return {
    candidate_id: `candidate-${kind}`,
    event_id: "event-1",
    room: "trusted-room",
    session_id: "trusted-session",
    kind,
    review_state: "in_review",
    source_refs: [{
      source_type: "turn",
      source_id: "trusted-source",
      role: "user",
      timestamp: "2026-07-24T12:00:00.000Z",
      content_hash: "a".repeat(64),
      room: "trusted-room",
      project,
    }],
    project_keys: projectKeys,
    scope: {
      room: "trusted-room",
      project,
      visibility: "private",
      publication_review_required: true,
    },
  };
}

function parsed(result: { content: Array<{ text: string }> }) {
  return JSON.parse(result.content[0]!.text);
}

let roomRoot: string;
let promoteMemory: CapturedTool;
let promoteCodingLesson: CapturedTool;
let promoteProjectLesson: CapturedTool;

beforeAll(async () => {
  roomRoot = await mkdtemp(path.join(tmpdir(), "omp-giga-promotion-"));
  await writeFile(path.join(roomRoot, ".solarisael-room.json"), `${JSON.stringify({
    version: 1,
    room: "trusted-room",
    trueName: "Trusted Reviewer",
    operator: "Trusted Operator",
  })}\n`, "utf8");
  const tools: CapturedTool[] = [];
  registerSolarisaelTools({ zod, registerTool(tool: CapturedTool) { tools.push(tool); } });
  promoteMemory = tools.find((tool) => tool.name === "giga_promote_memory")!;
  promoteCodingLesson = tools.find((tool) => tool.name === "giga_promote_coding_lesson")!;
  promoteProjectLesson = tools.find((tool) => tool.name === "giga_promote_project_lesson")!;
});

afterAll(async () => {
  __gigaPromotionTest.resetOperations();
  await rm(roomRoot, { recursive: true, force: true });
});

beforeEach(() => {
  calls.listRooms.length = 0;
  calls.resolved.length = 0;
  calls.promotions.length = 0;
  sourceFailure = null;
  currentCandidate = candidate("memory");
  __gigaPromotionTest.setOperations({
    async requestGigaCandidateList(room: string) {
      calls.listRooms.push(room);
      return { candidates: [currentCandidate] } as any;
    },
    async resolveGigaSourceRefsFromLedger(
      ctx: unknown,
      room: string,
      sessionId: string,
      sourceRefs: unknown[],
      projectKeys: string[],
    ) {
      calls.resolved.push({ ctx, room, sessionId, sourceRefs, projectKeys });
      if (sourceFailure) throw sourceFailure;
      return sourceRefs as any;
    },
    async requestGigaPromote(request: any) {
      calls.promotions.push(request);
      const common = {
        candidate_id: request.candidate_id,
        review_state: "promoted",
        durable: true,
        authority: "full",
        warnings: [],
        reviewer_id: request.reviewer_id,
        operator_identity: request.operator_identity,
        reviewed_at: request.reviewed_at,
        committed_at: "2026-07-24T12:00:01.000Z",
      };
      if (request.target.kind === "memory") {
        return { kind: "memory", ...common, memory_id: 47, room: request.room } as any;
      }
      if (request.target.kind === "coding_lesson") {
        return { kind: "coding_lesson", ...common, coding_lesson_id: 48, scope: "global" } as any;
      }
      if (request.target.kind === "project_lesson") {
        return {
          kind: "project_lesson",
          ...common,
          project_lesson_id: 49,
          project: request.target.payload.project,
        } as any;
      }
      throw new Error(`unexpected promotion target: ${String(request.target.kind)}`);
    },
  });
});

describe("exact GIGA promotion tools preserve trusted authority", () => {
  test("derives room, reviewer, operator, source identity, and kind instead of accepting spoofed fields", async () => {
    const result = await promoteMemory.execute(
      "promote-memory",
      {
        candidate_id: currentCandidate.candidate_id,
        title: "Operator-edited title",
        body: "Operator-edited durable body",
        threads: ["boundary"],
        room: "spoofed-room",
        reviewer_id: "Spoofed Reviewer",
        operator_identity: "Spoofed Operator",
        source_refs: [{ source_id: "invented-source" }],
      },
      undefined,
      undefined,
      { cwd: roomRoot },
    );

    expect(result.isError).toBeUndefined();
    expect(parsed(result)).toEqual(result.details);
    expect(calls.listRooms).toEqual(["trusted-room"]);
    expect(calls.resolved).toHaveLength(1);
    expect(calls.resolved[0]).toMatchObject({
      room: "trusted-room",
      sessionId: "trusted-session",
      sourceRefs: currentCandidate.source_refs,
      projectKeys: [],
    });
    expect(calls.promotions).toHaveLength(1);
    expect(calls.promotions[0]).toEqual({
      candidate_id: "candidate-memory",
      room: "trusted-room",
      reviewer_id: "Trusted Reviewer",
      operator_identity: "Trusted Operator",
      authorization_basis: "omp_room_binding",
      source_refs: currentCandidate.source_refs,
      reviewed_at: expect.any(String),
      target: {
        kind: "memory",
        payload: {
          title: "Operator-edited title",
          body: "Operator-edited durable body",
          threads: ["boundary"],
        },
      },
      publication_consent: null,
    });
    expect(parsed(result)).toEqual({
      ok: true,
      kind: "memory",
      candidate_id: "candidate-memory",
      review_state: "promoted",
      durable: true,
      authority: "full",
      warnings: [],
      reviewer_id: "Trusted Reviewer",
      operator_identity: "Trusted Operator",
      reviewed_at: expect.any(String),
      committed_at: "2026-07-24T12:00:01.000Z",
      memory_id: 47,
      room: "trusted-room",
    });
  });

  test("refuses a stale source hash and never dispatches promotion", async () => {
    const sourceDiagnostic: TransportDiagnostic = {
      category: "operation",
      stage: "validation",
      operation: "resolve_giga_source_refs",
      owner: {
        component: "solarisael-house-omp",
        path: "giga.ts",
        symbol: "resolveGigaSourceRefsFromLedger",
      },
      expected: { source_hash: "stored candidate source hash matches ledger content" },
      observed: { source_hash: "stale candidate source hash" },
      targets: ["giga.ts#resolveGigaSourceRefsFromLedger"],
      next_checks: [{ action: "refresh_candidate_sources", target: "candidate-memory" }],
      execution: {
        request_dispatched: false,
        write_outcome: "not_started",
        retry: "after_change",
      },
    };
    sourceFailure = new RustTransportError(
      {
        code: "GigaSourceHashMismatchError",
        message: "stored source hash no longer matches the ledger",
        retryable: false,
      },
      "",
      sourceDiagnostic,
    );

    const result = await promoteMemory.execute(
      "promote-stale",
      {
        candidate_id: currentCandidate.candidate_id,
        title: "Title",
        body: "Body",
      },
      undefined,
      undefined,
      { cwd: roomRoot },
    );
    const output = parsed(result);

    expect(result.isError).toBe(true);
    expect(result.details).toEqual(output);
    expect(output).toMatchObject({
      ok: false,
      status: "error",
      code: "GigaSourceHashMismatchError",
      retryable: false,
    });
    expect(output.details).toMatchObject({
      category: "operation",
      stage: "validation",
      operation: "giga_promote_memory",
      expected: sourceDiagnostic.expected,
      observed: sourceDiagnostic.observed,
    });
    expect(output.details.owner).toEqual(sourceDiagnostic.owner);
    expect(output.details.targets).toEqual(sourceDiagnostic.targets);
    expect(output.details.next_checks).toEqual([{
      action: "refresh_candidate_sources",
      target: "candidate-memory",
      operation: "giga_promote_memory",
    }]);
    expect(output.details.execution).toEqual(sourceDiagnostic.execution);
    expect(calls.resolved).toHaveLength(1);
    expect(calls.promotions).toHaveLength(0);
  });

  test("routes the exact coding-lesson variant without memory or project fields", async () => {
    currentCandidate = candidate("coding_lesson");
    const result = await promoteCodingLesson.execute(
      "promote-coding",
      {
        candidate_id: currentCandidate.candidate_id,
        title: "Sanitize inherited variables",
        body: "Clear inherited state before invoking the tool.",
        shape: "process",
        proof_pattern: "The clean invocation passed.",
        trigger_context: "When spawning a child process.",
        tags: ["environment"],
      },
      undefined,
      undefined,
      { cwd: roomRoot },
    );

    expect(result.isError).toBeUndefined();
    expect(calls.promotions).toHaveLength(1);
    expect(calls.promotions[0]).toEqual({
      candidate_id: "candidate-coding_lesson",
      room: "trusted-room",
      reviewer_id: "Trusted Reviewer",
      operator_identity: "Trusted Operator",
      authorization_basis: "omp_room_binding",
      source_refs: currentCandidate.source_refs,
      reviewed_at: expect.any(String),
      target: {
        kind: "coding_lesson",
        payload: {
          title: "Sanitize inherited variables",
          body: "Clear inherited state before invoking the tool.",
          shape: "process",
          proof_pattern: "The clean invocation passed.",
          trigger_context: "When spawning a child process.",
          language_keys: [],
          technology_keys: [],
          thread_keys: [],
          tags: ["environment"],
        },
      },
      publication_consent: null,
    });
    expect(parsed(result)).toEqual({
      ok: true,
      kind: "coding_lesson",
      candidate_id: "candidate-coding_lesson",
      review_state: "promoted",
      durable: true,
      authority: "full",
      warnings: [],
      reviewer_id: "Trusted Reviewer",
      operator_identity: "Trusted Operator",
      reviewed_at: expect.any(String),
      committed_at: "2026-07-24T12:00:01.000Z",
      coding_lesson_id: 48,
      scope: "global",
    });
  });

  test("refuses an exact promotion tool that does not match the stored candidate", async () => {
    const result = await promoteCodingLesson.execute(
      "promote-kind-mismatch",
      {
        candidate_id: currentCandidate.candidate_id,
        title: "Wrong variant",
        body: "Must not route.",
      },
      undefined,
      undefined,
      { cwd: roomRoot },
    );

    expect(result.isError).toBe(true);
    expect(parsed(result).message).toContain("must match");
    expect(calls.resolved).toHaveLength(0);
    expect(calls.promotions).toHaveLength(0);
  });

  test("requires explicit project publication consent and derives the stored project", async () => {
    currentCandidate = candidate("project_lesson", ["solarisael-house"]);

    const refused = await promoteProjectLesson.execute(
      "promote-project-refused",
      {
        candidate_id: currentCandidate.candidate_id,
        title: "Stable project rule",
        body: "The reviewed project-wide rule.",
        publication_approved: false,
      },
      undefined,
      undefined,
      { cwd: roomRoot },
    );
    const refusal = parsed(refused);
    expect(refused.isError).toBe(true);
    expect(refusal).toMatchObject({
      ok: false,
      status: "error",
      code: "giga_promotion_refused",
      retryable: false,
      details: {
        room: "trusted-room",
        candidate_id: "candidate-project_lesson",
      },
    });
    expect(refusal.message).toContain("explicit publication approval");
    expect(calls.resolved).toHaveLength(0);
    expect(calls.promotions).toHaveLength(0);

    const accepted = await promoteProjectLesson.execute(
      "promote-project-approved",
      {
        candidate_id: currentCandidate.candidate_id,
        title: "Stable project rule",
        body: "The reviewed project-wide rule.",
        proof_pattern: "Observed in the exact source.",
        trigger_context: "When publishing this project rule.",
        tags: ["project-rule"],
        publication_approved: true,
      },
      undefined,
      undefined,
      { cwd: roomRoot },
    );

    expect(accepted.isError).toBeUndefined();
    expect(parsed(accepted)).toEqual(accepted.details);
    expect(calls.promotions).toHaveLength(1);
    expect(calls.promotions[0]).toEqual({
      candidate_id: "candidate-project_lesson",
      room: "trusted-room",
      reviewer_id: "Trusted Reviewer",
      operator_identity: "Trusted Operator",
      authorization_basis: "omp_room_binding",
      source_refs: currentCandidate.source_refs,
      reviewed_at: expect.any(String),
      target: {
        kind: "project_lesson",
        payload: {
          title: "Stable project rule",
          body: "The reviewed project-wide rule.",
          project: "solarisael-house",
          proof_pattern: "Observed in the exact source.",
          trigger_context: "When publishing this project rule.",
          language_keys: [],
          technology_keys: [],
          thread_keys: [],
          tags: ["project-rule"],
        },
      },
      publication_consent: { operator_approved: true, reviewer_approved: true },
    });
    expect(parsed(accepted)).toEqual({
      ok: true,
      kind: "project_lesson",
      candidate_id: "candidate-project_lesson",
      review_state: "promoted",
      durable: true,
      authority: "full",
      warnings: [],
      reviewer_id: "Trusted Reviewer",
      operator_identity: "Trusted Operator",
      reviewed_at: expect.any(String),
      committed_at: "2026-07-24T12:00:01.000Z",
      project_lesson_id: 49,
      project: "solarisael-house",
    });
  });
});
