import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import {
  dispatchFamiliar,
  familiarStatus,
  loadRoomSpellbook,
} from "../solarisael-house-proof/familiars.ts";

const spellbook = {
  version: 1,
  collective: "familiars",
  collectiveAliases: ["kittens"],
  spellbookAliases: ["litters.json"],
  familiars: [{
    id: "cisma",
    name: "Cisma",
    aliases: ["scout-kitten"],
    lane: "smol-scout",
    description: "A bounded scout.",
  }],
};

let roomDir = "";

beforeEach(async () => {
  roomDir = await mkdtemp(path.join(os.tmpdir(), "athanor-familiars-"));
  await mkdir(path.join(roomDir, "familiars"));
});

afterEach(async () => {
  await rm(roomDir, { recursive: true, force: true });
});

async function writeSpellbook(filename: string) {
  await writeFile(
    path.join(roomDir, "familiars", filename),
    `${JSON.stringify(spellbook, null, 2)}\n`,
    "utf8",
  );
}

describe("room familiar spellbooks", () => {
  test("loads the canonical spellbook path", async () => {
    await writeSpellbook("spellbook.json");

    const result = await familiarStatus(roomDir);

    expect(result).toMatchObject({ ok: true, sourceAlias: false });
    expect(result.source).toBe(path.join(roomDir, "familiars", "spellbook.json"));
    expect(result.spellbook?.collectiveAliases).toEqual(["kittens"]);
  });

  test("accepts litters.json as a spellbook filename alias", async () => {
    await writeSpellbook("litters.json");

    const result = await loadRoomSpellbook(roomDir);

    expect(result).toMatchObject({ ok: true, sourceAlias: true });
    expect(result.source).toBe(path.join(roomDir, "familiars", "litters.json"));
    expect(result.spellbook?.spellbookAliases).toEqual(["litters.json"]);
  });

  test("packages a scout task through a familiar alias", async () => {
    await writeSpellbook("spellbook.json");

    const result = await dispatchFamiliar(roomDir, {
      familiar: "scout-kitten",
      task: "Map the exact target.",
      context: [{ mode: "exact", source: "src/example.ts" }],
      acceptance: ["Report exact symbols."],
      lessonBodies: ["Subagents are kittens: affection is part of the dispatch contract."],
    });

    expect(result).toMatchObject({
      ok: true,
      status: "ready",
      familiar: { id: "cisma", lane: "smol-scout" },
      modelRole: "pi/smol",
      ompAgent: "scout",
      spawnPacket: {
        tool: "task",
        args: {
          tasks: [{ name: "Cisma", agent: "scout" }],
        },
      },
    });
    expect(result.spawnPacket?.args.context).toContain("Subagents are kittens: affection is part of the dispatch contract.");
  });
});
