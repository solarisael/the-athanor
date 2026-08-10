import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { tmpdir } from "node:os";
import {
  recallTelemetryEnabled,
  recallTelemetryPath,
  recordRecallTelemetry,
} from "../solarisael-house-proof/recall-telemetry.ts";

const roots: string[] = [];
const originalEnv = process.env.SOLARISAEL_RECALL_TELEMETRY;

afterEach(async () => {
  if (originalEnv === undefined) delete process.env.SOLARISAEL_RECALL_TELEMETRY;
  else process.env.SOLARISAEL_RECALL_TELEMETRY = originalEnv;
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

async function room(marker: Record<string, unknown> = {}) {
  const root = await mkdtemp(path.join(tmpdir(), "solarisael-recall-telemetry-"));
  roots.push(root);
  await writeFile(path.join(root, ".solarisael-room.json"), JSON.stringify({ version: 1, room: "test", ...marker }), "utf8");
  return root;
}

describe("recall telemetry", () => {
  test("is opt-in through the room marker", async () => {
    const disabled = await room();
    const enabled = await room({ recallTelemetry: true });
    expect(await recallTelemetryEnabled(disabled)).toBe(false);
    expect(await recallTelemetryEnabled(enabled)).toBe(true);
  });

  test("an explicit environment setting overrides the marker", async () => {
    const root = await room({ recallTelemetry: true });
    process.env.SOLARISAEL_RECALL_TELEMETRY = "0";
    expect(await recallTelemetryEnabled(root)).toBe(false);
    process.env.SOLARISAEL_RECALL_TELEMETRY = "1";
    expect(await recallTelemetryEnabled(root)).toBe(true);
  });

  test("records a local viewport without duplicating raw prompt text", async () => {
    const root = await room({ recallTelemetry: true });
    const prompt = "Do you remember the shoreline promise?";
    expect(await recordRecallTelemetry({
      effectiveRoomDir: root,
      sessionId: "session-1",
      room: "test",
      prompt,
      route: { intent: "memory_lookup" },
      status: "injected",
      viewport: { found: true, retrievalCandidates: [{ source_path: "memory/example.md" }] },
      viewportDiagnostics: { kept: 1, suppressed: 0, reasons: {} },
      capturedAt: "2026-07-20T00:00:00.000Z",
    })).toBe(true);
    const source = await readFile(recallTelemetryPath(root), "utf8");
    expect(source).not.toContain(prompt);
    const entry = JSON.parse(source.trim());
    expect(entry).toMatchObject({
      schema_version: 1,
      session_id: "session-1",
      room: "test",
      prompt_chars: prompt.length,
      status: "injected",
      viewport_diagnostics: { kept: 1, suppressed: 0, reasons: {} },
    });
    expect(entry.prompt_sha256).toHaveLength(64);
  });

  test("does not create a file when telemetry is disabled", async () => {
    const root = await room();
    expect(await recordRecallTelemetry({
      effectiveRoomDir: root,
      sessionId: "session-2",
      room: "test",
      prompt: "hello",
      route: null,
      status: "skipped",
    })).toBe(false);
    expect(await Bun.file(recallTelemetryPath(root)).exists()).toBe(false);
  });
});
