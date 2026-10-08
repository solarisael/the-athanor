import { describe, expect, test } from "bun:test";
import { automaticRecallAllowed, parentContextRequest } from "../house-proof/recall.ts";

describe("child Recall ownership", () => {
  test("a depth-zero child still skips automatic Recall", () => {
    const ctx = { agent: { kind: "sub", name: "sub", depth: 0, parentId: "Main" } };
    expect(automaticRecallAllowed(ctx)).toBe(false);
    expect(parentContextRequest(ctx)).toContain("agent://Main");
  });

  test("a nested worker asks its own parent instead of the main session", () => {
    const ctx = { agent: { kind: "sub", name: "task", parentId: "Builder" } };
    expect(parentContextRequest(ctx)).toContain("agent://Builder");
    expect(parentContextRequest(ctx)).not.toContain("agent://Main");
  });

  test("an absent parent never becomes an invented destination", () => {
    const request = parentContextRequest({ agent: { kind: "sub", name: "task" } });
    expect(request).not.toBeNull();
    expect(request).not.toContain("agent://");
  });

  test("the explicit research role permits manual retrieval only", () => {
    const researcher = { agent: { kind: "sub", name: "memory-research", parentId: "Main" } };
    expect(parentContextRequest(researcher)).toBeNull();
    expect(automaticRecallAllowed(researcher)).toBe(false);
    expect(parentContextRequest({ agent: { kind: "sub", name: "memory-research-helper", parentId: "Main" } })).not.toBeNull();
  });

  test("top-level and legacy main contexts keep Recall", () => {
    for (const ctx of [{}, { agent: { kind: "main", name: "main" } }]) {
      expect(automaticRecallAllowed(ctx)).toBe(true);
      expect(parentContextRequest(ctx)).toBeNull();
    }
  });
});
