// OMP Athanor constants.
// Values only: paths, timeouts, filenames, and stable runtime defaults.

import os from "node:os";
import path from "node:path";

export const OBSIDIAN_ROOT = process.env.ATHANOR_VAULT_ROOT
  ? path.resolve(process.env.ATHANOR_VAULT_ROOT)
  : path.join(os.homedir(), "Solarisael");
export const DIAGNOSTIC_TIMEOUT_MS = 8000;
// One budget for the whole context hook; Recall gets it minus a 500 ms commit
// reserve. Measured 2026-09-22..25 (kodo, Insula): the substrate's own recall
// spends up to 3 s on an embed stall by design, then 0.8-4.1 s on the lexical
// lane, so a 4.5 s share cut 41 of 259 recalls (16%); completed recalls ran
// p50 1.1 s, p95 3.4 s. 8 s covers the substrate's designed ceiling.
export const AUTOMATIC_CONTEXT_IO_TIMEOUT_MS = process.platform === "win32" ? 8_000 : 2_000;
export const WRITE_TIMEOUT_MS = process.platform === "win32" ? 300_000 : 90_000;
export const OMP_SESSION_ID = "omp";
export const TRANSCRIPT_DEBUG_LOG = "athanor-transcript-debug.jsonl";
export const HOUSE_STATE_FILENAME = "athanor-house-state.json";
// Room-local, operation-scoped Docket write capability. Never a schema field.
export const ROOM_CAPABILITY_FILENAME = "room-capability";
