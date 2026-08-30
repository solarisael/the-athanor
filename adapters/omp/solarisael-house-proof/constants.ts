// OMP Athanor constants.
// Values only: paths, timeouts, filenames, and stable runtime defaults.

import os from "node:os";
import path from "node:path";

export const OBSIDIAN_ROOT = process.env.SOLARISAEL_VAULT_ROOT
  ? path.resolve(process.env.SOLARISAEL_VAULT_ROOT)
  : path.join(os.homedir(), "Solarisael");
export const DIAGNOSTIC_TIMEOUT_MS = 8000;
export const AUTOMATIC_CONTEXT_IO_TIMEOUT_MS = 30_000;
export const WRITE_TIMEOUT_MS = 90000;
export const OMP_SESSION_ID = "omp";
export const TRANSCRIPT_DEBUG_LOG = "solarisael-house-transcript-debug.jsonl";
export const HOUSE_STATE_FILENAME = "solarisael-house-state.json";
// Room-local, operation-scoped Docket write capability. Never a schema field.
export const ROOM_CAPABILITY_FILENAME = "room-capability";
