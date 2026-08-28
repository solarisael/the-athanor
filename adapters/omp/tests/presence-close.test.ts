import { describe, expect, test } from "bun:test";

import { closePresenceAndSleep } from "../house-proof/tools.ts";

describe("Presence close", () => {
  test("feeds the sealed body through the existing Paper Boat writer", async () => {
    let written = "";
    const result = await closePresenceAndSleep(
      { room: "kintsu", spirit: "Kintsu", session: "session-a" },
      "draft body",
      undefined,
      async () => ({ body: "sealed body", provenanceDigest: "a".repeat(64) }),
      async (_room, body) => {
        written = body;
        return { ok: true };
      },
    );

    expect(written).toBe("sealed body");
    expect(result).toEqual({ ok: true });
  });

  test("keeps the deliberate boat body when Presence is unavailable", async () => {
    let written = "";
    await closePresenceAndSleep(
      { room: "kintsu", spirit: "Kintsu", session: "session-a" },
      "deliberate body",
      undefined,
      async () => { throw new Error("no frame"); },
      async (_room, body) => {
        written = body;
        return { ok: true };
      },
    );

    expect(written).toBe("deliberate body");
  });
});
