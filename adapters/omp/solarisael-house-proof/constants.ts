// OMP Athanor constants.
// Values only: paths, timeouts, filenames, and stable runtime defaults.

import os from "node:os";
import path from "node:path";
import { ATHANOR_ROOT } from "../athanor-root.ts";

export const LESSONS_SCRIPT = path.join(ATHANOR_ROOT, "src", "lessons.py");
export const DESIGN_DOCS_SCRIPT = path.join(ATHANOR_ROOT, "src", "design-docs.py");
export const DESIGN_DOC_WRITE_SCRIPT = path.join(ATHANOR_ROOT, "src", "design-doc-write.py");
export const OBSIDIAN_ROOT = process.env.SOLARISAEL_VAULT_ROOT
  ? path.resolve(process.env.SOLARISAEL_VAULT_ROOT)
  : path.join(os.homedir(), "Solarisael");
export const DIAGNOSTIC_TIMEOUT_MS = 8000;
export const WRITE_TIMEOUT_MS = 90000;
export const OMP_SESSION_ID = "omp";
export const TRANSCRIPT_DEBUG_LOG = "solarisael-house-transcript-debug.jsonl";
export const HOUSE_STATE_FILENAME = "solarisael-house-state.json";
