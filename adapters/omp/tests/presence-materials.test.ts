import { describe, expect, test } from "bun:test";

import {
  anamnesisMaterial,
  lessonMaterials,
  paperBoatMaterial,
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

  test("bounds wake materials without changing their provenance", () => {
    const paperBoat = paperBoatMaterial({ memoryId: "42", letter: "p".repeat(5000) });
    const anamnesis = anamnesisMaterial("a".repeat(5000))[0];

    expect(paperBoat).toMatchObject({
      id: "paper-boat:42",
      authority: { kind: "paper_boat", memory_id: 42 },
    });
    expect(paperBoat?.body).toBe("p".repeat(4096));
    expect(paperBoat?.body.length).toBeLessThanOrEqual(4096);
    expect(anamnesis).toMatchObject({
      id: "anamnesis:wake",
      authority: { kind: "anamnesis", source: "anamnesis:wake" },
    });
    expect(anamnesis?.body).toBe("a".repeat(4096));
    expect(anamnesis?.body.length).toBeLessThanOrEqual(4096);
  });
});
