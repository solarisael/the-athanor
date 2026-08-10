import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { dispatchHouse } from "../solarisael-house-proof/dispatch.ts";

let roomDir = "";

beforeEach(async () => {
  roomDir = await mkdtemp(path.join(os.tmpdir(), "athanor-dispatch-"));
  const familiarDir = path.join(roomDir, "familiars");
  await mkdir(familiarDir);
  await writeFile(path.join(familiarDir, "spellbook.json"), JSON.stringify({
    version: 1,
    collective: "familiars",
    collectiveAliases: ["kittens"],
    spellbookAliases: ["litters.json"],
    familiars: [
      {
        id: "cisma",
        name: "Cisma",
        aliases: ["scout-kitten"],
        lane: "smol-scout",
        description: "A bounded scout.",
      },
      {
        id: "tempera",
        name: "Têmpera",
        aliases: ["test-kitten"],
        lane: "tester",
        description: "A focused test author.",
      },
    ],
  }), "utf8");
});

afterEach(async () => {
  await rm(roomDir, { recursive: true, force: true });
});

describe("unified house dispatch", () => {
  test("packages a raw lane as direct task-tool arguments", async () => {
    const result = await dispatchHouse(roomDir, {
      lane: "verifier",
      task: "Check the exact receipt.",
      acceptance: ["Report whether the claim holds."],
    });

    expect(result).toMatchObject({
      ok: true,
      selector: { kind: "lane", value: "verifier" },
      lane: "verifier",
      familiar: null,
      spawnPacket: {
        tool: "task",
        args: { tasks: [{ name: "Mirror", agent: "reviewer" }] },
      },
    });
  });

  test("resolves a room familiar through the same dispatch surface", async () => {
    const result = await dispatchHouse(roomDir, {
      familiar: "scout-kitten",
      task: "Map the exact target.",
      context: [{ mode: "exact", source: "src/example.ts" }],
      acceptance: ["Report exact symbols."],
    });

    expect(result).toMatchObject({
      ok: true,
      selector: { kind: "familiar", value: "scout-kitten" },
      lane: "smol-scout",
      familiar: { id: "cisma", name: "Cisma" },
      spawnPacket: {
        tool: "task",
        args: { tasks: [{ name: "Cisma", agent: "scout" }] },
      },
    });
  });

  test("packages the default-worker tester familiar without an agent override", async () => {
    const requestedTask = "Preserve this exact tester assignment.";
    const result = await dispatchHouse(roomDir, {
      familiar: "test-kitten",
      task: requestedTask,
      acceptance: ["Return a task-tool-ready packet."],
    });

    expect(result).toMatchObject({
      ok: true,
      selector: { kind: "familiar", value: "test-kitten" },
      lane: "tester",
      familiar: { id: "tempera", name: "Têmpera" },
      spawnPacket: { tool: "task" },
    });
    expect(result.spawnPacket?.args.tasks).toEqual([{
      name: "Tempera",
      task: expect.stringContaining(requestedTask),
    }]);
  });

  test("requires exactly one selector", async () => {
    const both = await dispatchHouse(roomDir, {
      lane: "smol-scout",
      familiar: "cisma",
      task: "Do not run.",
    });
    const neither = await dispatchHouse(roomDir, { task: "Do not run." });

    expect(both).toMatchObject({ ok: false, selector: null, spawnPacket: null });
    expect(both.errors).toEqual(["Dispatch accepts either 'lane' or 'familiar', not both."]);
    expect(neither.errors).toEqual(["Dispatch requires either 'lane' or 'familiar'."]);
  });
});
