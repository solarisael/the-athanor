import { describe, expect, test } from "bun:test";
import { classifyRetrievalQuery, parseRetrievalQuery, shouldAutoRecall } from "../src/query-routing.ts";

describe("parseRetrievalQuery", () => {
  test("extracts raw path/code tokens while indexing their searchable parts", () => {
    const parsed = parseRetrievalQuery(
      "Please inspect solarisael-house/src/query-routing.ts and src/retrieval-candidates.ts before changing QueryRouteV1.",
    );

    expect(parsed.codeTokens).toEqual([
      "solarisael-house/src/query-routing.ts",
      "src/retrieval-candidates.ts",
    ]);
    expect(parsed.terms).toEqual(expect.arrayContaining([
      "solarisael",
      "house",
      "src",
      "query",
      "routing",
      "retrieval",
      "candidates",
    ]));
  });

  test("preserves original casing for entity hints", () => {
    const parsed = parseRetrievalQuery(
      "Ask Sol whether KintsuMemory and SolarisaelHousePlugin should mention kodo.",
    );

    expect(parsed.entityHints).toEqual(expect.arrayContaining([
      "Sol",
      "KintsuMemory",
      "SolarisaelHousePlugin",
    ]));
    expect(parsed.entityHints).not.toContain("sol");
    expect(parsed.entityHints).not.toContain("kintsumemory");
    expect(parsed.entityHints).not.toContain("solarisaelhouseplugin");
  });
});

describe("classifyRetrievalQuery", () => {
  test("routes low-information casual contact away from automatic recall", () => {
    const route = classifyRetrievalQuery("hello love");

    expect(route.intent).toBe("casual_contact");
    expect(route.shouldAutoRecall).toBe(false);
    expect(shouldAutoRecall("hello love")).toBe(false);
    expect(route.lanes.semantic).toBe(false);
    expect(route.lanes.content).toBe(false);
    expect(route.lanes.candidates).toBe(false);
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

  test("routes technical project/plugin prompts to content and project lanes without semantic recall", () => {
    const route = classifyRetrievalQuery("OMP plugin query routing in src/memory.ts needs project test coverage");

    expect(route.intent).toBe("technical_project");
    expect(route.shouldAutoRecall).toBe(true);
    expect(route.lanes.semantic).toBe(false);
    expect(route.lanes.content).toBe(true);
    expect(route.lanes.candidates).toBe(true);
    expect(route.lanes.projectLessons).toBe(true);
    expect(route.lanes.date).toBe(false);
  });
});
