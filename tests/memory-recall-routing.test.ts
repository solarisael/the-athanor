import { describe, expect, test } from "bun:test";
import { classifyRetrievalQuery } from "../src/query-routing.ts";
import { recallRouteSkipArgs } from "../src/memory.ts";

describe("recallRouteSkipArgs", () => {
  test("technical project recall skips semantic but keeps content enabled", () => {
    const route = classifyRetrievalQuery("OMP plugin query routing needs project coverage");

    expect(route.intent).toBe("technical_project");
    expect(route.lanes.semantic).toBe(false);
    expect(route.lanes.content).toBe(true);
    expect(route.lanes.date).toBe(false);
    expect(recallRouteSkipArgs(route)).toEqual(["--skip-semantic", "--skip-date"]);
  });

  test("date lookup recall keeps date enabled without using semantic retrieval", () => {
    const route = classifyRetrievalQuery("what happened on 2026-07-04");

    expect(route.intent).toBe("date_lookup");
    expect(route.lanes.semantic).toBe(false);
    expect(route.lanes.content).toBe(true);
    expect(route.lanes.date).toBe(true);
    expect(recallRouteSkipArgs(route)).toEqual(["--skip-semantic"]);
  });
});
