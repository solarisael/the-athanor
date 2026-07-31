import { describe, expect, test } from "bun:test";
import {
  buildFamiliarDispatchReceipt,
  getFamiliar,
  parseFamiliarSpellbook,
} from "../src/familiars.ts";

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
    temperament: "Curious and exact.",
    appearance: "A black kitten with future horns.",
  }],
};

describe("familiar spellbooks", () => {
  test("keeps generic vocabulary while exposing room aliases", () => {
    const result = parseFamiliarSpellbook(spellbook);

    expect(result).toMatchObject({ ok: true, errors: [] });
    expect(result.spellbook?.collective).toBe("familiars");
    expect(result.spellbook?.collectiveAliases).toEqual(["kittens"]);
    expect(result.spellbook?.spellbookAliases).toEqual(["litters.json"]);
    expect(result.spellbook?.familiars[0]).toMatchObject({
      temperament: "Curious and exact.",
      appearance: "A black kitten with future horns.",
    });
  });

  test("resolves a familiar by id, display name, or alias", () => {
    expect(getFamiliar(spellbook, "cisma")?.lane).toBe("smol-scout");
    expect(getFamiliar(spellbook, "CISMA")?.id).toBe("cisma");
    expect(getFamiliar(spellbook, "scout-kitten")?.name).toBe("Cisma");
  });

  test("rejects lookup aliases shared by different familiars", () => {
    const result = parseFamiliarSpellbook({
      ...spellbook,
      familiars: [
        ...spellbook.familiars,
        {
          id: "outra",
          name: "Outra",
          aliases: ["scout-kitten"],
          lane: "verifier",
          description: "A conflicting familiar.",
        },
      ],
    });

    expect(result.ok).toBe(false);
    expect(result.errors).toContain("Familiar lookup key 'scout-kitten' is already owned by 'cisma'.");
  });
});

describe("familiar dispatch", () => {
  test("binds the familiar identity to its worker lane packet", () => {
    const receipt = buildFamiliarDispatchReceipt(spellbook, {
      familiar: "scout-kitten",
      task: "Map the exact target.",
      target: "src/example.ts",
      context: [{ mode: "exact", source: "src/example.ts" }],
      acceptance: ["Report the exact symbols found."],
    });

    expect(receipt).toMatchObject({
      ok: true,
      status: "ready",
      familiar: { id: "cisma", name: "Cisma", lane: "smol-scout" },
      lane: "smol-scout",
      modelRole: "pi/smol",
      ompAgent: "scout",
      spawnPacket: {
        tool: "task",
        args: {
          tasks: [{ name: "Cisma", agent: "scout" }],
        },
      },
    });
    expect(receipt.spawnPacket?.args.context).toContain("Familiar: Cisma (cisma)");
  });

  test("rejects names absent from the spellbook", () => {
    const receipt = buildFamiliarDispatchReceipt(spellbook, {
      familiar: "missing",
      task: "Do not run.",
    });

    expect(receipt).toMatchObject({
      ok: false,
      status: "rejected",
      familiar: null,
      spawnPacket: null,
    });
    expect(receipt.errors).toEqual(["Unknown familiar: missing"]);
  });
});
