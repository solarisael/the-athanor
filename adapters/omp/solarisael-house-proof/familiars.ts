// OMP familiar adapter.
// Silhouette: load one room spellbook, then delegate validation and packet
// shaping to the pure Athanor core. No spawning happens here.

import { readFile } from "node:fs/promises";
import path from "node:path";
import { loadHouseFamiliars } from "./core.ts";

const FAMILIAR_DIRECTORY = "familiars";

function missingFile(error: unknown): boolean {
  return Boolean(error && typeof error === "object" && "code" in error && error.code === "ENOENT");
}

export async function loadRoomSpellbook(roomDir: string) {
  const core = await loadHouseFamiliars();
  const filenames = Array.isArray(core.FAMILIAR_SPELLBOOK_FILENAMES)
    ? core.FAMILIAR_SPELLBOOK_FILENAMES
    : ["spellbook.json", "litters.json"];

  for (const filename of filenames) {
    const source = path.join(roomDir, FAMILIAR_DIRECTORY, filename);
    let text: string;
    try {
      text = await readFile(source, "utf8");
    } catch (error) {
      if (missingFile(error)) continue;
      return {
        ok: false,
        errors: [`Could not read familiar spellbook '${source}': ${error instanceof Error ? error.message : String(error)}`],
        source,
        sourceAlias: filename !== filenames[0],
        spellbook: null,
      };
    }

    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch (error) {
      return {
        ok: false,
        errors: [`Familiar spellbook '${source}' is not valid JSON: ${error instanceof Error ? error.message : String(error)}`],
        source,
        sourceAlias: filename !== filenames[0],
        spellbook: null,
      };
    }

    const parsed = core.parseFamiliarSpellbook(value);
    return {
      ...parsed,
      source,
      sourceAlias: filename !== filenames[0],
    };
  }

  return {
    ok: false,
    errors: [`No familiar spellbook found in '${path.join(roomDir, FAMILIAR_DIRECTORY)}'. Tried: ${filenames.join(", ")}.`],
    source: null,
    sourceAlias: false,
    spellbook: null,
  };
}

export async function familiarStatus(roomDir: string) {
  const loaded = await loadRoomSpellbook(roomDir);
  if (!loaded.spellbook) return loaded;

  return {
    ok: true,
    errors: [],
    source: loaded.source,
    sourceAlias: loaded.sourceAlias,
    spellbook: loaded.spellbook,
  };
}

export async function dispatchFamiliar(roomDir: string, request: unknown) {
  const loaded = await loadRoomSpellbook(roomDir);
  if (!loaded.spellbook) {
    return {
      ok: false,
      status: "rejected",
      lane: null,
      modelRole: null,
      ompAgent: null,
      familiar: null,
      errors: loaded.errors,
      warnings: [],
      dispatcher: {
        executed: false,
        reason: "The room spellbook could not be loaded, so no familiar packet was built.",
      },
      spawnPacket: null,
      source: loaded.source,
      sourceAlias: loaded.sourceAlias,
      spellbook: null,
    };
  }

  const core = await loadHouseFamiliars();
  const receipt = core.buildFamiliarDispatchReceipt(loaded.spellbook, request);
  return {
    ...receipt,
    source: loaded.source,
    sourceAlias: loaded.sourceAlias,
    spellbook: {
      collective: loaded.spellbook.collective,
      collectiveAliases: loaded.spellbook.collectiveAliases,
      spellbookAliases: loaded.spellbook.spellbookAliases,
    },
  };
}
