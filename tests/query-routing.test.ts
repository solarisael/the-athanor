import { describe, expect, test } from "bun:test";
import { classifyRetrievalQuery, parseRetrievalQuery, shouldAutoRecall } from "../src/query-routing.ts";

describe("parseRetrievalQuery", () => {
  test("extracts raw path/code tokens while indexing their searchable parts", () => {
    const parsed = parseRetrievalQuery(
      "Please inspect project-atlas/src/query-routing.ts and src/retrieval-candidates.ts before changing QueryRouteV1.",
    );

    expect(parsed.codeTokens).toEqual([
      "project-atlas/src/query-routing.ts",
      "src/retrieval-candidates.ts",
    ]);
    expect(parsed.terms).toEqual(expect.arrayContaining([
      "project",
      "atlas",
      "src",
      "query",
      "routing",
      "retrieval",
      "candidates",
    ]));
  });

  test("preserves original casing for entity hints", () => {
    const parsed = parseRetrievalQuery(
      "Compare AtlasStore and CedarIndex with the adapter design.",
    );

    expect(parsed.entityHints).toEqual(expect.arrayContaining([
      "AtlasStore",
      "CedarIndex",
    ]));
    expect(parsed.entityHints).not.toContain("atlasstore");
    expect(parsed.entityHints).not.toContain("cedarindex");
  });
});

describe("classifyRetrievalQuery", () => {
  test("suppresses low-information acknowledgment, meta, and operational prompts", () => {
    for (const prompt of [
      "Okay, noted.",
      "The status is unchanged.",
      "Please proceed with the same approach.",
    ]) {
      const route = classifyRetrievalQuery(prompt);
      expect(route.shouldAutoRecall).toBe(false);
      expect(route.intent).toBe("casual_contact");
      expect(route.lanes.candidates).toBe(false);
      expect(route.reasons.length).toBeGreaterThan(0);
    }
  });

  test("keeps date lookup out of technical-project intent and semantic scoring", () => {
    const route = classifyRetrievalQuery("what happened on 2026-07-04");

    expect(route.intent).toBe("date_lookup");
    expect(route.dateTokens).toEqual(["2026-07-04"]);
    expect(route.reasons).toContain("date-token");
    expect(route.shouldAutoRecall).toBe(true);
    expect(route.lanes.date).toBe(true);
    expect(route.lanes.semantic).toBe(false);
    expect(route.lanes.content).toBe(true);
    expect(route.lanes.projectLessons).toBe(false);
  });

  test("preserves substantive technical prompts", () => {
    const route = classifyRetrievalQuery(
      "How should the query-routing adapter rank retrieval candidates for test coverage?",
    );

    expect(route.intent).toBe("technical_project");
    expect(route.shouldAutoRecall).toBe(true);
    expect(route.lanes.semantic).toBe(false);
    expect(route.lanes.content).toBe(true);
    expect(route.lanes.candidates).toBe(true);
    expect(route.lanes.projectLessons).toBe(true);
  });

  test("preserves explicit memory and technical controls", () => {
    const personal = classifyRetrievalQuery("what did we intend to do today?");
    expect(personal.shouldAutoRecall).toBe(true);
    expect(personal.intent).toBe("memory_lookup");
    expect(personal.reasons).toContain("personal-canon-lookup-language");

    const technical = classifyRetrievalQuery(
      "How should database indexing improve retrieval candidate ranking?",
    );
    expect(technical.shouldAutoRecall).toBe(true);
    expect(technical.intent).toBe("technical_project");
  });

  test("routes two recognized synthetic entities to entity lookup", () => {
    const route = classifyRetrievalQuery(
      "Compare AtlasStore with CedarIndex.",
      { recognizedEntities: ["AtlasStore", "CedarIndex"] },
    );

    expect(route.intent).toBe("entity_lookup");
    expect(route.shouldAutoRecall).toBe(true);
    expect(route.reasons).toContain("recognized-entity-signals");
    expect(route.recallQuery).toBe("AtlasStore CedarIndex");
    expect(route.lanes.semantic).toBe(true);
  });

  test("does not treat capitalization alone as entity recognition", () => {
    const route = classifyRetrievalQuery("Review AtlasStore and CedarIndex.");
    expect(route.intent).not.toBe("entity_lookup");
    expect(route.shouldAutoRecall).toBe(false);
  });

  test("routes one recognized entity with generic lookup language", () => {
    const route = classifyRetrievalQuery(
      "Find details about AtlasStore.",
      { recognizedEntities: ["AtlasStore"] },
    );

    expect(route.intent).toBe("entity_lookup");
    expect(route.shouldAutoRecall).toBe(true);
    expect(route.recallQuery).toBe("AtlasStore");
  });

  test("shouldAutoRecall follows information strength", () => {
    expect(shouldAutoRecall("Sure, continue.")).toBe(false);
    expect(shouldAutoRecall("Explain database indexing tradeoffs.")).toBe(true);
  });
});
