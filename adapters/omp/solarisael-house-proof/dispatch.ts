// OMP dispatch coordinator.
// Silhouette: choose exactly one room familiar or raw worker lane, then return
// one task-tool-ready receipt shape. Spawning remains outside the adapter.

import { dispatchFamiliar } from "./familiars.ts";
import { dispatchWorker } from "./routing.ts";

function clean(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

function rejectSelector(errors: string[]) {
  return {
    ok: false,
    status: "rejected" as const,
    selector: null,
    lane: null,
    modelRole: null,
    ompAgent: null,
    familiar: null,
    errors,
    warnings: [],
    dispatcher: {
      executed: false as const,
      reason: "Select exactly one worker lane or room familiar before dispatching.",
    },
    spawnPacket: null,
    source: null,
    sourceAlias: false,
    spellbook: null,
  };
}

export async function dispatchHouse(roomDir: string, params: unknown) {
  const request = params && typeof params === "object" ? params as Record<string, unknown> : {};
  const lane = clean(request.lane);
  const familiar = clean(request.familiar);

  if (lane && familiar) return rejectSelector(["Dispatch accepts either 'lane' or 'familiar', not both."]);
  if (!lane && !familiar) return rejectSelector(["Dispatch requires either 'lane' or 'familiar'."]);

  if (familiar) {
    const receipt = await dispatchFamiliar(roomDir, { ...request, familiar });
    return {
      ...receipt,
      selector: { kind: "familiar" as const, value: familiar },
    };
  }

  const receipt = await dispatchWorker({ ...request, lane });
  return {
    ...receipt,
    selector: { kind: "lane" as const, value: lane },
    familiar: null,
    source: null,
    sourceAlias: false,
    spellbook: null,
  };
}
