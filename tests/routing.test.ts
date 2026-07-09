// Guards the v0 worker-routing seam: only dispatchable lanes are exposed,
// lane names map to real OMP agents, and invalid packets stop at receipt shaping.

import { describe, expect, test } from "bun:test";
import { buildDispatchReceipt, getWorkerLane, listWorkerLanes, resolveDispatchModel } from "../src/routing.ts";

describe("listWorkerLanes", () => {
  test("exposes worker lanes without advisor or main channels", () => {
    const laneNames = listWorkerLanes().map((lane) => lane.name);

    expect(laneNames).toContain("smol-scout");
    expect(laneNames).toContain("smol-executor");
    expect(laneNames).toContain("tester");
    expect(laneNames).toContain("verifier");
    expect(laneNames).not.toContain("advisor");
    expect(laneNames).not.toContain("main");
  });
});

describe("worker lane mappings", () => {
  test("routes smol-executor to sonic while preserving smol lane metadata", () => {
    expect(getWorkerLane("smol-executor")).toMatchObject({
      name: "smol-executor",
      ompAgent: "sonic",
      modelRole: "pi/smol",
      canEdit: true,
      requiresAcceptance: true,
    });
  });

  test("routes tester to the existing OMP task agent", () => {
    expect(getWorkerLane("tester")).toMatchObject({
      name: "tester",
      ompAgent: "task",
    });
  });

  test("resolves lane model roles to OMP spawn aliases unless explicitly overridden", () => {
    const scout = getWorkerLane("smol-scout");
    const tester = getWorkerLane("tester");

    expect(scout && resolveDispatchModel(scout)).toBe("smol");
    expect(tester && resolveDispatchModel(tester)).toBe("default");
    expect(tester && resolveDispatchModel(tester, "provider/custom-model")).toBe("provider/custom-model");
  });
});

describe("buildDispatchReceipt", () => {
  test("rejects invalid lanes before creating a task packet", () => {
    const receipt = buildDispatchReceipt({
      lane: "advisor",
      task: "Review this implementation.",
    });

    expect(receipt).toMatchObject({
      ok: false,
      status: "rejected",
      lane: null,
      requestedModelRole: null,
      model: null,
      ompAgent: null,
      taskPacket: null,
    });
    expect(receipt.errors).toEqual(["Unknown worker lane: advisor"]);
  });

  test("rejects edit-capable lanes that omit explicit acceptance criteria", () => {
    const receipt = buildDispatchReceipt({
      lane: "smol-executor",
      task: "Update one exact function.",
      context: [{ mode: "exact", source: "src/example.ts" }],
    });

    expect(receipt).toMatchObject({
      ok: false,
      status: "rejected",
      lane: "smol-executor",
      requestedModelRole: "pi/smol",
      ompAgent: "sonic",
      taskPacket: null,
    });
    expect(receipt.errors).toEqual(["smol-executor requires at least one acceptance item."]);
  });

  test("rejects image-ok context for smol-executor", () => {
    const receipt = buildDispatchReceipt({
      lane: "smol-executor",
      task: "Apply this bounded change.",
      acceptance: ["The changed behavior is covered."],
      context: [{ mode: "image-ok", source: "mockup.png" }],
    });

    expect(receipt).toMatchObject({
      ok: false,
      status: "rejected",
      lane: "smol-executor",
      requestedModelRole: "pi/smol",
      ompAgent: "sonic",
      taskPacket: null,
    });
    expect(receipt.errors).toEqual(["smol-executor does not allow context mode 'image-ok'."]);
  });

  test("packages a valid smol-executor dispatch without executing it", () => {
    const receipt = buildDispatchReceipt({
      lane: "smol-executor",
      target: "src/routing.ts",
      task: "Add the requested guard.",
      acceptance: ["The guard rejects invalid packets."],
      context: [{ mode: "exact", source: "src/routing.ts", reason: "Target under edit" }],
      risk: "medium",
    });

    expect(receipt).toMatchObject({
      ok: true,
      status: "ready",
      lane: "smol-executor",
      requestedModelRole: "pi/smol",
      ompAgent: "sonic",
      dispatcher: { executed: false },
    });
    expect(receipt.taskPacket?.agent).toBe("sonic");
    expect(receipt.model).toBe("smol");
    expect(receipt.taskPacket?.model).toBe("smol");
    expect(receipt.dispatcher.reason).toContain("eval agent() helper");
    expect(receipt.dispatcher.reason).toContain("model");
    expect(receipt.dispatcher.reason).toContain("plain task tool ignores model");
    expect(receipt.taskPacket?.tasks).toHaveLength(1);

    const assignment = receipt.taskPacket?.tasks[0].assignment || "";
    expect(assignment).toContain("## Target");
    expect(assignment).toContain("## Change");
    expect(assignment).toContain("## Acceptance");
  });

  test("passes explicit model overrides through to the task packet and warns about the role replacement", () => {
    const receipt = buildDispatchReceipt({
      lane: "tester",
      task: "Exercise the observable behavior.",
      acceptance: ["The behavior fails if the wrong model is packaged."],
      context: [{ mode: "retrieve-only", source: "tests/example.test.ts" }],
      model: "provider/custom-model",
    });

    expect(receipt).toMatchObject({
      ok: true,
      status: "ready",
      lane: "tester",
      requestedModelRole: "pi/default",
      model: "provider/custom-model",
      ompAgent: "task",
    });
    expect(receipt.taskPacket?.model).toBe("provider/custom-model");
    expect(receipt.warnings).toContain(
      "Model override 'provider/custom-model' replaces lane role 'pi/default' for this dispatch.",
    );
    expect(receipt.warnings.some((warning) => warning.includes("eval agent() helper"))).toBe(true);
  });
});
