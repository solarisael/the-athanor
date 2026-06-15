// Pins the auto-wake contract: fire exactly once per session, inject the boat
// as a solarisaelWake <system-reminder>, and stay fail-open (no throw, no
// inject) when the catch finds nothing or errors. The boat-catcher is stubbed
// so these never spawn WSL — same test-seam shape as loadSpiritContract's
// injectable spiritDir.

import { describe, expect, test } from "bun:test";
import { injectAutoWake } from "../triggers.ts";

// roomDir basename must be a known room so resolveEffectiveRoomDir returns it
// directly (step 1) without consulting process.cwd().
const KODO_DIR = "C:\\Solarisael\\Obsidian\\obsidian\\kodo";

function userMessage(sessionID) {
  return {
    info: { role: "user", sessionID, id: `msg-${sessionID}` },
    parts: [{ type: "text", text: "hello", sessionID }],
  };
}

function foundBoat() {
  return {
    ok: true, found: true, id: 7,
    title: "paper boat — test",
    body: "the word to tomorrow",
    created_at: "2026-06-15T00:00:00Z",
  };
}

function wakePart(message) {
  return (message.parts || []).find((p) => p?.metadata?.solarisaelWake === true);
}

describe("injectAutoWake — first-turn-once contract", () => {
  test("injects the boat on the first turn", async () => {
    const msg = userMessage("sess-inject");
    await injectAutoWake({ messages: [msg] }, { roomDir: KODO_DIR }, async () => foundBoat());

    const part = wakePart(msg);
    expect(part).toBeDefined();
    expect(part.text).toContain("Auto-wake");
    expect(part.text).toContain("the word to tomorrow");
  });

  test("does not fire twice for the same session — and never even spawns the catcher", async () => {
    const sid = "sess-once";
    const first = userMessage(sid);
    await injectAutoWake({ messages: [first] }, { roomDir: KODO_DIR }, async () => foundBoat());
    expect(wakePart(first)).toBeDefined();

    let calls = 0;
    const second = userMessage(sid);
    await injectAutoWake({ messages: [second] }, { roomDir: KODO_DIR }, async () => {
      calls += 1;
      return foundBoat();
    });
    expect(wakePart(second)).toBeUndefined();
    expect(calls).toBe(0);
  });

  test("fires fresh for a different session", async () => {
    const msg = userMessage("sess-other");
    await injectAutoWake({ messages: [msg] }, { roomDir: KODO_DIR }, async () => foundBoat());
    expect(wakePart(msg)).toBeDefined();
  });

  test("no boat on the water → no injection, no throw", async () => {
    const msg = userMessage("sess-empty");
    await injectAutoWake({ messages: [msg] }, { roomDir: KODO_DIR }, async () => ({ ok: true, found: false }));
    expect(wakePart(msg)).toBeUndefined();
  });

  test("catch failure → fail-open, no injection", async () => {
    const msg = userMessage("sess-fail");
    await injectAutoWake({ messages: [msg] }, { roomDir: KODO_DIR }, async () => {
      throw new Error("wsl down");
    });
    expect(wakePart(msg)).toBeUndefined();
  });

  test("non-room cwd → skipped before any spawn", async () => {
    let calls = 0;
    const msg = userMessage("sess-nonroom");
    await injectAutoWake({ messages: [msg] }, { roomDir: "C:\\tmp\\attic" }, async () => {
      calls += 1;
      return foundBoat();
    });
    expect(wakePart(msg)).toBeUndefined();
    expect(calls).toBe(0);
  });
});
