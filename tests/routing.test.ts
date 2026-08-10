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
      target: "src/example.ts",
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
      target: "src/example.ts",
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
      lessonBodies: ["Subagents are kittens: affection is part of the dispatch contract."],
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
            name: "Chisel",
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
    expect(assignment).toContain("[Quest Received] [Chisel]");
    expect(assignment).toContain("# Target\nsrc/routing.ts\n[Quest Received] [Chisel]");
    expect(assignment).toContain("Questions, limits, disagreement, and refusal are valid yields.");
    expect(assignment).toContain("// 0%");
    expect(assignment.split(/\s+/).length).toBeLessThanOrEqual(350);
    expect(receipt.spawnPacket?.args.context).toContain("Model role: pi/smol");
    expect(receipt.spawnPacket?.args.context).toContain("mode=exact");
    expect(receipt.spawnPacket?.args.context).toContain("[Codex — supplied lessons ride free");
    expect(receipt.spawnPacket?.args.context).toContain("Subagents are kittens: affection is part of the dispatch contract.");
  });

  test("keeps targetless multiline work intact and closes the quest frame on one line", () => {
    const receipt = buildDispatchReceipt({
      lane: "smol-scout",
      task: "Inspect the routing seam.\nReturn one evidence-backed finding.",
    });
    const assignment = receipt.spawnPacket?.args.tasks[0].task || "";
    expect(assignment).toContain("# Target\ngeneral\n[Quest Received] [Quill] [TARGET: general]");
    expect(assignment).toContain("The House opens one bounded door for you:\nInspect the routing seam.\nReturn one evidence-backed finding.");
    expect(receipt.warnings).toContain("smol-scout has no explicit target; lineage uses the general domain.");
    expect(assignment).not.toContain("[TARGET: Inspect the routing seam.\n");
  });

  test("renders each multiline target and acceptance line exactly once", () => {
    const receipt = buildDispatchReceipt({
      lane: "verifier",
      target: "src/routing.ts\nsrc/familiars.ts",
      task: "Inspect both named seams.",
      acceptance: ["- The routing seam is checked. // 0%\n* The familiar seam is checked."],
    });
    const assignment = receipt.spawnPacket?.args.tasks[0].task || "";

    expect(assignment).toContain("# Target\nsrc/routing.ts\n[Quest Received] [Mirror] [TARGET: src/routing.ts]\n\nsrc/familiars.ts");
    expect(assignment.match(/^src\/routing\.ts$/gm)).toHaveLength(1);
    expect(assignment).toContain("- The routing seam is checked. // 0%");
    expect(assignment).toContain("- The familiar seam is checked. // 0%");
    expect(assignment.match(/\/\/ 0%/g)).toHaveLength(2);
  });

  test("keeps bracketed targets parseable and requires edit targets", () => {
    const bracketed = buildDispatchReceipt({
      lane: "verifier",
      target: "src/routes/[id].ts",
      task: "Inspect the dynamic route.",
      acceptance: ["The frame remains parseable."],
    });
    const assignment = bracketed.spawnPacket?.args.tasks[0].task || "";
    expect(assignment).toContain("# Target\nsrc/routes/[id].ts\n[Quest Received] [Mirror] [TARGET: src/routes/(id).ts]");
    expect(bracketed.warnings).toContain("Quest frame target brackets were rendered as parentheses.");

    const missing = buildDispatchReceipt({
      lane: "smol-executor",
      task: "Edit one seam.",
      acceptance: ["The seam changes."],
      context: [{ mode: "exact", source: "src/routing.ts" }],
    });
    expect(missing.errors).toContain("smol-executor requires an exact target.");
    expect(missing.spawnPacket).toBeNull();
  });
});
