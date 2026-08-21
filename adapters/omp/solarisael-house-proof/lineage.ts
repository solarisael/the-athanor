// OMP client for the Athanor Host lineage family.
// Quest lineage — what settles, what it says, and its write-once key — is
// decided in Rust. This module only translates OMP's task event shapes.

import { hostCommand, sendHostCommand, type HostBinding } from "./host.ts";

const LINEAGE_NORMALIZE = "athanor.lineage.normalize";
const LINEAGE_LIFECYCLE = "athanor.lineage.lifecycle";
const LINEAGE_NORMALIZED = "athanor.lineage.normalized";
const ACCEPTED = new Set([LINEAGE_NORMALIZED]);

export type QuestMemory = {
  resultId: string;
  idempotencyKey: string;
  title: string;
  body: string;
  threads: [string, string];
};

function records(value: unknown, key: string): Array<Record<string, unknown>> {
  if (!value || typeof value !== "object") return [];
  const candidate = (value as Record<string, unknown>)[key];
  return Array.isArray(candidate)
    ? candidate.filter((item): item is Record<string, unknown> => Boolean(item && typeof item === "object"))
    : [];
}

function text(value: unknown): string | undefined {
  if (value === undefined || value === null) return undefined;
  const rendered = typeof value === "string" ? value : String(value);
  return rendered.length ? rendered : undefined;
}

function batch(toolCallId: string, input: unknown, details: unknown) {
  return {
    toolCallId,
    tasks: records(input, "tasks").map((task) => ({
      name: task.name,
      agent: task.agent,
      task: task.task,
    })),
    results: records(details, "results").map((result) => ({
      index: result.index,
      id: result.id,
      agent: result.agent,
      task: result.task,
      output: result.output,
      stderr: result.stderr,
      exitCode: result.exitCode,
      aborted: result.aborted === true,
      abortReason: result.abortReason,
      error: result.error,
    })),
  };
}

export type QuestLineage = { settled: boolean; memories: QuestMemory[] };

async function lineage(
  binding: HostBinding,
  commandType: string,
  payload: Record<string, unknown>,
  idempotencyKey?: string,
): Promise<QuestLineage> {
  const command = hostCommand(binding, commandType, "lineage", payload, idempotencyKey);
  const response = await sendHostCommand(command, ACCEPTED);
  if (!Array.isArray(response.memories)) {
    throw new Error("Athanor Host lineage response omitted memories");
  }
  return { settled: response.settled === true, memories: response.memories };
}

export async function normalizeQuestMemories(
  binding: HostBinding,
  toolCallId: string,
  input: unknown,
  details: unknown,
  idempotencyKey?: string,
): Promise<QuestMemory[]> {
  return (await lineage(
    binding,
    LINEAGE_NORMALIZE,
    { lineage_request: batch(toolCallId, input, details) },
    idempotencyKey,
  )).memories;
}

export async function settleQuestLifecycle(
  binding: HostBinding,
  toolCallId: string,
  progress: Record<string, unknown>,
  lifecycle: Record<string, unknown>,
  idempotencyKey?: string,
): Promise<QuestLineage> {
  return await lineage(
    binding,
    LINEAGE_LIFECYCLE,
    {
      lineage_lifecycle: {
        toolCallId,
        id: text(lifecycle.id ?? progress.id),
        agent: text(lifecycle.agent ?? progress.agent),
        task: text(progress.assignment ?? progress.task),
        status: text(lifecycle.status),
        sessionFile: text(lifecycle.sessionFile ?? progress.sessionFile),
        // Stamped at dispatch when the room was standing inside a claimed
        // quest. Absent otherwise, and the Host reads it as optional.
        ...(text(progress.attemptId) ? { attemptId: text(progress.attemptId) } : {}),
      },
    },
    idempotencyKey,
  );
}
