import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import {
  __gigaTest,
  buildGigaConversationWindow,
  closeGigaTransports,
  ingestGigaLoggedTurnsDetached,
} from "../giga.ts";

const originalEnabled = process.env.SOLARISAEL_GIGA_ENABLED;
const originalProject = process.env.SOLARISAEL_GIGA_PROJECT_KEY;

function hash(text: string): string {
  return createHash("sha256").update(text, "utf8").digest("hex");
}

function turn(sourceID: string, text: string, role: "user" | "assistant" = "user") {
  return {
    role,
    text,
    sourceID,
    contentHash: hash(text),
    sessionID: "session-1",
    sourceTimestamp: "2026-07-24T12:00:00Z",
    hasStableID: true,
  };
}

beforeEach(() => {
  delete process.env.SOLARISAEL_GIGA_PROJECT_KEY;
  __gigaTest.resetState();
});

afterEach(async () => {
  if (originalEnabled === undefined) delete process.env.SOLARISAEL_GIGA_ENABLED;
  else process.env.SOLARISAEL_GIGA_ENABLED = originalEnabled;
  if (originalProject === undefined) delete process.env.SOLARISAEL_GIGA_PROJECT_KEY;
  else process.env.SOLARISAEL_GIGA_PROJECT_KEY = originalProject;
  await closeGigaTransports();
  __gigaTest.resetState();
});

describe("GIGA OMP adapter event", () => {
  test("constructs one bounded reference-only event from durable logged turns", () => {
    const turns = [turn("turn-1", "Keep this boundary."), turn("turn-2", "Understood.", "assistant")];
    const event = buildGigaConversationWindow({ cwd: process.cwd() }, turns)!;

    expect(event.source_refs.map((source) => ({
      source_id: source.source_id,
      content_hash: source.content_hash,
    }))).toEqual([
      { source_id: "turn-1", content_hash: hash("Keep this boundary.") },
      { source_id: "turn-2", content_hash: hash("Understood.") },
    ]);
    expect(JSON.stringify(event)).not.toContain("Keep this boundary.");
    expect(JSON.stringify(event)).not.toContain("Understood.");
  });

  test("rejects stale hashes and unstable identities", () => {
    const exact = turn("turn-1", "Exact source.");
    expect(buildGigaConversationWindow(
      { cwd: process.cwd() },
      [{ ...exact, contentHash: "stale" }],
    )).toBeNull();
    expect(buildGigaConversationWindow(
      { cwd: process.cwd() },
      [{ ...exact, hasStableID: false }],
    )).toBeNull();
  });

  test("uses trusted room context and configured project scope", () => {
    process.env.SOLARISAEL_GIGA_PROJECT_KEY = "trusted-project";
    const event = buildGigaConversationWindow(
      { cwd: process.cwd() },
      [turn("turn-1", "Project rule.")],
    )!;

    expect(event.room).toBeTruthy();
    expect(event.project_keys).toEqual(["trusted-project"]);
    expect(Object.keys(event).sort()).toEqual([
      "created_at",
      "event_id",
      "event_schema_version",
      "event_type",
      "lifecycle",
      "project_keys",
      "room",
      "session_id",
      "source_refs",
    ]);
  });
});

describe("GIGA fail-open lifecycle", () => {
  test("disabled or malformed background work never throws into context generation", () => {
    process.env.SOLARISAEL_GIGA_ENABLED = "0";
    expect(() => ingestGigaLoggedTurnsDetached({ cwd: process.cwd() }, [turn("turn-1", "Exact source.")])).not.toThrow();
    process.env.SOLARISAEL_GIGA_ENABLED = "1";
    expect(() => ingestGigaLoggedTurnsDetached({ cwd: process.cwd() }, [{ ...turn("turn-1", "Exact source."), contentHash: "stale" }])).not.toThrow();
  });

  test("only a verified flat main session may enqueue GIGA work", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "omp-giga-session-"));
    try {
      const mainFile = path.join(root, "main.jsonl");
      const childDirectory = path.join(root, "main");
      const childFile = path.join(childDirectory, "Worker.jsonl");
      await writeFile(mainFile, "");
      await mkdir(childDirectory);
      await writeFile(childFile, "");

      expect(__gigaTest.isSubagentSessionContext({})).toBe(true);
      expect(__gigaTest.isSubagentSessionContext({
        sessionManager: { getSessionFile: () => mainFile },
      })).toBe(false);
      expect(__gigaTest.isSubagentSessionContext({
        sessionManager: { getSessionFile: () => childFile },
      })).toBe(true);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

});
