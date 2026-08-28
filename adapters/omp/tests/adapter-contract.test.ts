import { describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ADAPTER_API_VERSION } from "../index.ts";
import { analyzeContext, parseContextAnalysisResponse } from "../house-proof/context.ts";
import { dispatchHouse, familiarStatus, laneStatus } from "../house-proof/routing.ts";
import { roomContext, supportedRoom } from "../house-proof/room.ts";

describe("OMP adapter contract", () => {
  test("exports a thin adapter API without loading a sibling TypeScript core", () => {
    expect(ADAPTER_API_VERSION).toBe(1);
    expect(typeof analyzeContext).toBe("function");
    expect(typeof laneStatus).toBe("function");
    expect(typeof familiarStatus).toBe("function");
    expect(typeof dispatchHouse).toBe("function");
  });

  test("refuses a Context Host analysis that omits its query route", () => {
    expect(() => parseContextAnalysisResponse({
      correlation_id: "message-1",
      command_or_event_type: "athanor.context.analyzed",
      analysis: { roomReminder: "still malformed" },
    })).toThrow("Context Host analysis omitted the query route");
  });

  test("uses neutral defaults for an unmarked directory", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-contract-"));
    try {
      const cwd = path.join(root, "unmarked-room");
      expect(supportedRoom(cwd)).toBe("default-room");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("rejects the reserved house room key from adapter room resolution", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-reserved-room-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".athanor-room.json"),
        `${JSON.stringify({ version: 1, room: "house", trueName: "Example Room", operator: "Example Operator" })}\n`,
        "utf8",
      );
      expect(supportedRoom(cwd)).toBe("example");
      expect(roomContext(cwd)).toMatchObject({ room: "example", operator: "Example Operator" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  test("uses a neutral operator when a marker omits one", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-operator-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".athanor-room.json"),
        `${JSON.stringify({ version: 1, room: "example", trueName: "Example Room" })}\n`,
        "utf8",
      );
      expect(roomContext(cwd)).toMatchObject({
        room: "example",
        spirit: "Example Room",
        operator: "Operator",
      });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });


  test("retains explicit persisted room markers", async () => {
    const root = await mkdtemp(path.join(os.tmpdir(), "omp-adapter-marker-"));
    try {
      const cwd = path.join(root, "example");
      await mkdir(cwd);
      await writeFile(
        path.join(cwd, ".athanor-room.json"),
        `${JSON.stringify({ version: 1, room: "custom-room", trueName: "Example Room", operator: "Example Operator" })}\n`,
        "utf8",
      );
      expect(roomContext(cwd)).toMatchObject({ room: "custom-room", spirit: "Example Room", operator: "Example Operator" });
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
  test("ships a resolvable always-loaded Chargebook in the starter room", async () => {
    const starterRoom = path.join(import.meta.dir, "..", "starter-room", "example");
    const agents = await readFile(path.join(starterRoom, "AGENTS.md"), "utf8");
    const references = [...agents.matchAll(/^- @(.+)$/gm)].map((match) => match[1]);

    expect(references).toContain("chargebook.md");
    await Promise.all(references.map((reference) => readFile(path.join(starterRoom, reference), "utf8")));

    const chargebook = await readFile(path.join(starterRoom, "chargebook.md"), "utf8");
    const headings = [...chargebook.matchAll(/^## (.+)$/gm)].map((match) => match[1]);
    expect(headings).toEqual([
      "Positive credits",
      "Zero-cost presence",
      "Behavioral charges",
      "Operation costs",
    ]);
  });

});
