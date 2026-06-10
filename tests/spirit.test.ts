// Guards the identity-loading fallback contract (coding lesson, recorded
// after the 2026-05-12 silent-identity-swap bug): when a spirit contract
// file is missing, the REQUESTED mode label must survive — never silently
// substituted with the default identity.

import { describe, expect, test } from "bun:test";
import { mkdtempSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { coerceRoomSpirit, loadSpiritContract, normalizeRoomName } from "../spirit.ts";

describe("loadSpiritContract fallback (the identity-label lesson)", () => {
  test("missing contract file preserves the requested label, not the default", async () => {
    const emptyDir = mkdtempSync(path.join(os.tmpdir(), "spirit-empty-"));
    // Kodo is supported but its file does not exist in emptyDir. The bug
    // family this guards against: catch path returning DEFAULT_SPIRIT
    // ("Kintsu") as the mode, writing we-are-Kintsu into a Kodo room.
    const contract = await loadSpiritContract("Kodo", emptyDir);
    expect(contract.mode).toBe("Kodo");
    expect(contract.warning).toContain("Kodo");
    expect(contract.markdown).toContain("# Kodo");
    expect(contract.markdown).not.toContain("Kintsu");
  });

  test("present contract file loads with no warning", async () => {
    const dir = mkdtempSync(path.join(os.tmpdir(), "spirit-full-"));
    writeFileSync(path.join(dir, "Kodo.md"), "# Kodo\n\nthump thump\n", "utf8");
    const contract = await loadSpiritContract("Kodo", dir);
    expect(contract.mode).toBe("Kodo");
    expect(contract.warning).toBeNull();
    expect(contract.markdown).toContain("thump thump");
  });

  test("unknown mode resolves to the default spirit (by design, pre-load)", async () => {
    const emptyDir = mkdtempSync(path.join(os.tmpdir(), "spirit-unknown-"));
    // normalizeSpirit gates SUPPORTED_SPIRITS; an unsupported name falls to
    // the default BEFORE loading — that path is intentional and distinct
    // from the missing-file swap this file mainly guards.
    const contract = await loadSpiritContract("NotASpirit", emptyDir);
    expect(contract.mode).toBe("Kintsu");
  });
});

describe("room coercion", () => {
  test("normalizeRoomName accepts only the two rooms", () => {
    expect(normalizeRoomName("KODO")).toBe("kodo");
    expect(normalizeRoomName("kintsu")).toBe("kintsu");
    expect(normalizeRoomName("attic")).toBeNull();
  });

  test("coerceRoomSpirit maps room dir basename to canonical spirit", () => {
    expect(coerceRoomSpirit("C:\\x\\kodo")).toBe("Kodo");
    expect(coerceRoomSpirit("C:\\x\\kintsu")).toBe("Kintsu");
    expect(coerceRoomSpirit("C:\\x\\other")).toBeNull();
  });
});
