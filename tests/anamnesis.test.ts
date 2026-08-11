import { describe, expect, test } from "bun:test";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

import { runAnamnesisQuery } from "../src/anamnesis.ts";

describe("runAnamnesisQuery", () => {
  test("fails closed when room name and path disagree", async () => {
    const root = await mkdtemp(path.join(tmpdir(), "athanor-room-mismatch-"));
    const roomDir = path.join(root, "alpha-room");
    try {
      const result = await runAnamnesisQuery(roomDir, "beta-room");
      expect(result.ok).toBe(false);
      expect(result.warnings).toEqual([expect.stringContaining("room name/path mismatch")]);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
