// Load the canonical Athanor package root.
// OMP is an adapter: it normalizes OMP events, then calls the versioned core.

import { pathToFileURL } from "node:url";
import path from "node:path";
import { ATHANOR_ROOT } from "../athanor-root.ts";

const CORE_API_VERSION = 1;
const coreEntry = path.join(ATHANOR_ROOT, "index.ts");
let coreModulePromise;

export async function loadHouseCore() {
  if (!coreModulePromise) {
    coreModulePromise = import(pathToFileURL(coreEntry).href);
  }
  const core = await coreModulePromise;
  if (core.CORE_API_VERSION !== CORE_API_VERSION) {
    throw new Error(`Unsupported Athanor core API: expected ${CORE_API_VERSION}, got ${String(core.CORE_API_VERSION)}`);
  }
  for (const name of [
    "runAnamnesisQuery",
    "logUserTurn",
    "logAssistantTurn",
    "parseFamiliarSpellbook",
    "listFamiliars",
    "buildFamiliarDispatchReceipt",
  ]) {
    if (typeof core[name] !== "function") {
      throw new Error(`Athanor core API is missing ${name}`);
    }
  }
  return core;
}


export async function loadHouseLedger() {
  const core = await loadHouseCore();
  return {
    logUserTurn: core.logUserTurn,
    logAssistantTurn: core.logAssistantTurn,
  };
}


export async function loadHouseFamiliars() {
  const core = await loadHouseCore();
  return {
    FAMILIAR_SPELLBOOK_FILENAMES: core.FAMILIAR_SPELLBOOK_FILENAMES,
    parseFamiliarSpellbook: core.parseFamiliarSpellbook,
    listFamiliars: core.listFamiliars,
    buildFamiliarDispatchReceipt: core.buildFamiliarDispatchReceipt,
  };
}
export async function loadHouseRouting() {
  return await loadHouseCore();
}

export async function loadHouseQueryRouting() {
  return await loadHouseCore();
}
