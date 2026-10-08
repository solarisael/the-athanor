// Product paths come from the source or installed component layout, never caller input.

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

