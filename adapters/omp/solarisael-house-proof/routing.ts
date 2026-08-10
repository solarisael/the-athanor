// OMP routing adapter helpers.
// Silhouette: no tool registration here, only loading shared core routing and
// shaping results for the OMP tool layer.

import { loadHouseRouting } from "./core.ts";

export async function laneStatus() {
  const routing = await loadHouseRouting();
  return {
    ok: true,
    lanes: routing.listWorkerLanes(),
    advisor: routing.ADVISOR_REVIEW_CHANNEL,
    notes: [
      "Advisor is a separate review channel, not a dispatchable worker lane.",
      "house_dispatch v0 validates and packages an OMP task packet; the main model still calls task/agent explicitly.",
    ],
  };
}

export async function dispatchWorker(params) {
  const routing = await loadHouseRouting();
  return routing.buildDispatchReceipt(params);
}
