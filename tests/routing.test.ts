// Guards the worker-routing seam: only dispatchable lanes are exposed, lane
// names map to real OMP agents, and accepted receipts fit the task tool exactly.

import { describe, expect, test } from "bun:test";
import { buildDispatchReceipt, getWorkerLane, listWorkerLanes } from "../src/routing.ts";

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

  test("routes the scout lane to the current OMP scout agent", () => {
    expect(getWorkerLane("smol-scout")).toMatchObject({
      name: "smol-scout",
      ompAgent: "scout",
      modelRole: "pi/smol",
      canEdit: false,
    });
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

});

describe("buildDispatchReceipt", () => {
  test("rejects invalid lanes before creating a spawn packet", () => {
    const receipt = buildDispatchReceipt({
      lane: "advisor",
      task: "Review this implementation.",
    });

    expect(receipt).toMatchObject({
      ok: false,
      status: "rejected",
      lane: null,
      modelRole: null,
      ompAgent: null,
      spawnPacket: null,
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
      modelRole: "pi/smol",
      ompAgent: "sonic",
      spawnPacket: null,
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
      modelRole: "pi/smol",
      ompAgent: "sonic",
      spawnPacket: null,
    });
    expect(receipt.errors).toEqual(["smol-executor does not allow context mode 'image-ok'."]);
  });

  test("packages valid dispatches as direct task-tool arguments", () => {
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
      modelRole: "pi/smol",
      ompAgent: "sonic",
      dispatcher: { executed: false },
      spawnPacket: {
        tool: "task",
        args: {
          tasks: [{
            name: "SmolExecutor",
            agent: "sonic",
          }],
        },
      },
    });
    expect(receipt.warnings).toEqual([]);
    expect(receipt.dispatcher.reason).toContain("spawnPacket.args");
    expect(receipt.spawnPacket?.args.tasks).toHaveLength(1);

    const assignment = receipt.spawnPacket?.args.tasks[0].task || "";
    expect(assignment).toContain("# Target");
    expect(assignment).toContain("# Change");
    expect(assignment).toContain("# Acceptance");
    expect(receipt.spawnPacket?.args.context).toContain("Model role: pi/smol");
    expect(receipt.spawnPacket?.args.context).toContain("mode=exact");
  });
});
