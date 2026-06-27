// Pins the package-free smoke trigger for process lessons. The test command
// should be safe inside room/vault directories that intentionally have no package.json.

import { describe, expect, test } from "bun:test";
import { PROCESS_SHAPE_TRIGGERS } from "../paths.ts";

describe("PROCESS_SHAPE_TRIGGERS", () => {
  test("provides a package-free smoke trigger", () => {
    expect(PROCESS_SHAPE_TRIGGERS.find((trigger) => trigger.pattern.test(": solarisael-process-lesson-smoke"))?.name || null).toBe("process-lesson-smoke");
  });

  test("keeps npm dev detection for real package commands", () => {
    expect(PROCESS_SHAPE_TRIGGERS.find((trigger) => trigger.pattern.test("npm run dev --help"))?.name || null).toBe("package-script-dev");
  });
});
