import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { describe, expect, test } from "bun:test";
import { buildDispatchReceipt } from "../../../src/routing.ts";

import {
  extractKittenLifecycleMemory,
  extractKittenQuestMemories,
  kittenLineageDisabled,
  kittenLifecycleJoinKey,
  kittenQuestIdempotencyKey,
  kittenReportPath,
  questDomain,
  readKittenReport,
} from "../kitten-lineage.ts";

describe("kitten quest lineage", () => {
  test("turns each settled task result into a standalone two-thread memory", () => {
    const input = {
      tasks: [
        {
          name: "Quill",
          agent: "kintsu-kitten",
          task: "# Target\ncrates/house-substrate/src/remember.rs\n\n# Change\nPreserve lesson thread keys.",
        },
        {
          name: "Cinder",
          agent: "kodo-kitten",
          task: "# Target\nRecovery ownership\n\n# Change\nRepair the restore filter.",
        },
      ],
    };
    const details = {
      results: [
        { index: 0, id: "Quill", agent: "kintsu-kitten", exitCode: 0, task: input.tasks[0].task, output: "Thread keys now survive writes." },
        { index: 1, id: "Cinder", agent: "kodo-kitten", exitCode: 1, task: input.tasks[1].task, output: "Restore proof found an ownership blocker." },
      ],
    };

    const records = extractKittenQuestMemories(input, details);

    expect(records).toHaveLength(2);
    expect(records[0].threads).toEqual(["kitten:quill", "domain:crates-house-substrate-src-remember-rs"]);
    expect(records[0].body).toContain(`Quest\n${input.tasks[0].task}`);
    expect(records[0].body).toContain("Report\nThread keys now survive writes.");
    expect(records[0].body).toContain("Outcome: completed; role: kintsu-kitten.");
    expect(records[1].threads).toEqual(["kitten:cinder", "domain:recovery-ownership"]);
    expect(records[1].body).toContain("Outcome: failed; role: kodo-kitten.");
  });

  test("ignores in-flight snapshots and preserves terminal errors as reports", () => {
    const input = { tasks: [{ name: "Quill", task: "# Target\nLesson retrieval" }] };
    expect(extractKittenQuestMemories(input, { progress: [{ status: "running" }] })).toEqual([]);

    const [record] = extractKittenQuestMemories(input, {
      results: [{ id: "Quill", index: 0, exitCode: 1, error: "database unavailable" }],
    });
    expect(record.body).toContain("Report\ndatabase unavailable");
    expect(record.body).toContain("Outcome: failed.");
  });

  test("turns a terminal lifecycle event and exact cached assignment into lineage", () => {
    const assignment = "# Target\nsrc/routing.ts\n\n# Change\nKeep the quest warm and exact.";
    const memory = extractKittenLifecycleMemory({
      id: "Quill",
      agent: "scout",
      task: "short task label",
      assignment,
      parentToolCallId: "call-9",
    }, {
      id: "Quill",
      agent: "scout",
      status: "completed",
      parentToolCallId: "call-9",
      sessionFile: "C:/sessions/Quill.jsonl",
    }, "The routing seam now carries the lesson bodies.");

    expect(memory?.body).toContain(`Quest\n${assignment}`);
    expect(memory?.body).toContain("Report\nThe routing seam now carries the lesson bodies.");
    expect(memory?.threads).toEqual(["kitten:quill", "domain:src-routing-ts"]);
    expect(kittenReportPath("C:/sessions/Quill.jsonl")).toBe("C:/sessions/Quill.md");
  });

  test("joins progress and lifecycle by parent tool call plus task index", () => {
    expect(kittenLifecycleJoinKey({
      agent: "scout",
      index: 2,
      parentToolCallId: "call-9",
    })).toBe("call-9:2");
    expect(kittenLifecycleJoinKey({
      id: "Quill",
      index: 2,
      parentToolCallId: "call-9",
    })).toBe("call-9:2");
    expect(kittenLifecycleJoinKey({ id: "Quill" })).toBe("Quill");
  });

  test("derives bounded domain keys and stable idempotency keys", () => {
    expect(questDomain("# Target\nC:\\work\\the-athanor\\crates\\house-substrate\\src\\lesson.rs\n# Change\nRepair it")).toBe("src-lesson-rs");
    expect(kittenQuestIdempotencyKey("call-7", "Quill")).toBe("call-7:Quill");
  });

  test("derives the stable domain from a live dispatch packet", () => {
    const receipt = buildDispatchReceipt({
      lane: "verifier",
      target: "src/routing.ts",
      task: "Inspect one exact seam.",
      acceptance: ["Return one evidence-backed finding."],
    });
    const assignment = receipt.spawnPacket?.args.tasks[0].task || "";

    expect(questDomain(assignment)).toBe("src-routing-ts");
  });

  test("reads the terminal report beside the OMP child session", async () => {
    const directory = await mkdtemp(path.join(os.tmpdir(), "kitten-report-"));
    try {
      const sessionFile = path.join(directory, "Quill.jsonl");
      await writeFile(path.join(directory, "Quill.md"), "Exact final report.\n", "utf8");
      expect(await readKittenReport(sessionFile)).toBe("Exact final report.");
    } finally {
      await rm(directory, { recursive: true, force: true });
    }
  });

  test("disables writes during replay or by explicit operator switch", () => {
    expect(kittenLineageDisabled({})).toBe(false);
    expect(kittenLineageDisabled({ SOLARISAEL_REPLAY_MODE: "1" })).toBe(true);
    expect(kittenLineageDisabled({ ATHANOR_DISABLE_KITTEN_LINEAGE: "1" })).toBe(true);
  });
});
