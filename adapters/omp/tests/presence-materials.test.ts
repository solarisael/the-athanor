import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { describe, expect, test } from "bun:test";

import {
  anamnesisMaterial,
  lessonMaterials,
  paperBoatMaterial,
  presencePulseMaterial,
  recallMaterials,
} from "../solarisael-house-proof/presence-materials.ts";

describe("Presence material mapping", () => {
  test("keeps exact authority identities across wake and retrieval inputs", () => {
    expect(paperBoatMaterial({ memoryId: "42", letter: "letter" })).toMatchObject({
      id: "paper-boat:42",
      authority: { kind: "paper_boat", memory_id: 42 },
    });
    expect(anamnesisMaterial("counsel")[0]).toMatchObject({
      id: "anamnesis:wake",
      authority: { kind: "anamnesis" },
    });
    expect(lessonMaterials([{ id: 7, body: "rule", version: "v1" }])[0]).toMatchObject({
      id: "lesson:7",
      authority: { kind: "lesson", lesson_id: 7, version: "v1" },
    });
    expect(recallMaterials({
      retrievalCandidates: [{ memory_id: 9, excerpt: "memory" }],
      canonMatches: [{ id: 3, summary: "canon" }],
    })).toEqual([
      expect.objectContaining({ id: "memory:9", authority: { kind: "memory", memory_id: 9 } }),
      expect.objectContaining({ id: "canon:3", authority: { kind: "canon", entity_id: "3" } }),
    ]);
  });

  test("loads the room pulse as relationship authority", () => {
    const room = mkdtempSync(path.join(tmpdir(), "presence-pulse-"));
    try {
      writeFileSync(path.join(room, "presence-pulse.md"), "operator letter\n");
      const pulse = presencePulseMaterial(room);
      expect(pulse).toMatchObject({
        id: "relationship:presence-pulse",
        authority: { kind: "identity", source: "presence-pulse.md" },
        role: "relationship",
        body: "operator letter",
        salience: 975,
      });
      expect(String(pulse?.authority.sha256)).toHaveLength(64);
    } finally {
      rmSync(room, { recursive: true, force: true });
    }
  });

  test("excerpts wake materials and points at the full-body carrier", () => {
    const paperBoat = paperBoatMaterial({ memoryId: "42", letter: "p".repeat(5000) });
    const anamnesis = anamnesisMaterial("a".repeat(5000))[0];

    expect(paperBoat).toMatchObject({
      id: "paper-boat:42",
      authority: { kind: "paper_boat", memory_id: 42 },
    });
    expect(paperBoat?.body.startsWith("p".repeat(700))).toBe(true);
    expect(paperBoat?.body).toContain("solarisael-wake-context");
    expect(paperBoat?.body.length).toBeLessThan(900);
    expect(anamnesis).toMatchObject({
      id: "anamnesis:wake",
      authority: { kind: "anamnesis", source: "anamnesis:wake" },
    });
    expect(anamnesis?.body.startsWith("a".repeat(700))).toBe(true);
    expect(anamnesis?.body).toContain("solarisael-anamnesis-wake");
    expect(anamnesis?.body.length).toBeLessThan(900);
  });

  test("passes short wake materials through whole, without a pointer", () => {
    const paperBoat = paperBoatMaterial({ memoryId: "42", letter: "short letter" });
    const anamnesis = anamnesisMaterial("short counsel")[0];

    expect(paperBoat?.body).toBe("short letter");
    expect(anamnesis?.body).toBe("short counsel");
  });
});
