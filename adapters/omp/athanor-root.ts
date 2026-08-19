// The single place that answers "where is the Athanor core?", and the single
// place that loads installed topology configuration.
//
// Source modules live at <repository>/adapters/omp. Installed modules live at
// <program-root>/components/omp-adapter/versions/<releaseId>. Structure is the
// only owner: no environment variable can select another root, and every
// adapter entrypoint imports ATHANOR_ROOT here.

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

/** Source or installed component directory that contains this module. */
export const ADAPTER_ROOT = path.dirname(fileURLToPath(import.meta.url));

const COMPONENT_VERSIONS_ROOT = path.dirname(ADAPTER_ROOT);
const COMPONENT_ROOT = path.dirname(COMPONENT_VERSIONS_ROOT);
const COMPONENTS_ROOT = path.dirname(COMPONENT_ROOT);
function installedSegmentEquals(actual: string, expected: string): boolean {
  return process.platform === "win32"
    ? actual.toLowerCase() === expected
    : actual === expected;
}

export const INSTALLED_COMPONENT =
  installedSegmentEquals(path.basename(COMPONENT_ROOT), "omp-adapter") &&
  installedSegmentEquals(path.basename(COMPONENTS_ROOT), "components");

/** Repository root in source or Program Files root in an installed component. */
export const ATHANOR_ROOT = INSTALLED_COMPONENT
  ? path.dirname(COMPONENTS_ROOT)
  : path.resolve(ADAPTER_ROOT, "..", "..");

/**
 * Installed topology configuration: `<program-root>/state/athanor.env`.
 *
 * The source tree retains its existing sibling `state/` location. An installed
 * component resolves Program Files directly from its fixed component layout.
 * The file is found structurally and is never selected by an environment
 * variable.
 */
export const ATHANOR_ENV_FILE = INSTALLED_COMPONENT
  ? path.join(ATHANOR_ROOT, "state", "athanor.env")
  : path.resolve(ATHANOR_ROOT, "..", "state", "athanor.env");

/**
 * The only keys this file may set. Everything else in it is ignored, including
 * the pre-cutover `SOLARISAEL_*` topology names: writing those into the file
 * must do nothing, exactly as exporting them does nothing.
 */
export const ATHANOR_TOPOLOGY_KEYS = [
  "ATHANOR_STATE_DIR",
  "ATHANOR_SUBSTRATE_ROOT",
  "ATHANOR_SUBSTRATE_EXE",
  "ATHANOR_AUTO",
] as const;

export type AthanorTopologyKey = (typeof ATHANOR_TOPOLOGY_KEYS)[number];

/**
 * Parse `KEY=VALUE` lines. Blank lines and `#` comments are skipped, surrounding
 * quotes are stripped, and malformed lines are ignored rather than throwing —
 * a damaged config file must not make the adapter unloadable.
 */
export function parseAthanorEnv(text: string): Map<string, string> {
  const values = new Map<string, string>();
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    if (!line || line.startsWith("#")) continue;
    const separator = line.indexOf("=");
    if (separator <= 0) continue;
    const key = line.slice(0, separator).trim();
    if (!key) continue;
    const value = line.slice(separator + 1).trim().replace(/^(["'])([\s\S]*)\1$/, "$2");
    values.set(key, value);
  }
  return values;
}

/**
 * Apply installed topology configuration to `env`, returning the keys actually
 * set.
 *
 * The real process environment always wins: a key already present and non-empty
 * is left alone, so an operator or a parent process can still override an
 * install without editing its files. Unknown keys are ignored, and so are empty
 * values — an empty assignment is not a configured value anywhere else in the
 * Athanor and must not become one here.
 */
export function applyAthanorEnv(
  text: string,
  env: NodeJS.ProcessEnv = process.env,
): AthanorTopologyKey[] {
  const parsed = parseAthanorEnv(text);
  const applied: AthanorTopologyKey[] = [];
  for (const key of ATHANOR_TOPOLOGY_KEYS) {
    const current = env[key];
    if (typeof current === "string" && current.trim().length > 0) continue;
    const value = parsed.get(key);
    if (value === undefined || value.length === 0) continue;
    env[key] = value;
    applied.push(key);
  }
  return applied;
}

/**
 * Load `<install-root>/state/athanor.env` if it exists.
 *
 * A missing or unreadable file is silent and valid: that is a development
 * checkout, or a Vault install whose topology needs nothing set.
 */
export function loadAthanorEnv(
  file: string = ATHANOR_ENV_FILE,
  env: NodeJS.ProcessEnv = process.env,
): AthanorTopologyKey[] {
  let text: string;
  try {
    text = readFileSync(file, "utf8");
  } catch {
    return [];
  }
  return applyAthanorEnv(text, env);
}

// Loaded once, at module load. Every topology read in the adapter goes through
// a module that imports ATHANOR_ROOT from here, so this runs first.
export const ATHANOR_ENV_APPLIED = loadAthanorEnv();
