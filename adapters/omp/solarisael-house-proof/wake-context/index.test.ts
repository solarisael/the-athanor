import { describe, expect, test } from "bun:test";

import { receiveAutomaticWake } from "./index.ts";

describe("automatic wake", () => {
  test("gives a cold paper-boat read its dedicated startup budget", async () => {
    let options: { timeoutMs: number } | undefined;
    const result = await receiveAutomaticWake("kintsu", async (_room, received) => {
      options = received;
      return {
        ok: true,
        found: true,
        wake_context: "letter",
        title: "paper boat",
        source_path: "db-only/paper-boat",
      };
    });

    expect(options).toEqual({ timeoutMs: 15_000 });
    expect(result).toEqual({
      answered: true,
      letter: "letter",
      title: "paper boat",
      source: "db-only/paper-boat",
      warning: null,
    });
  });

  test("turns a failure receipt into a visible bounded warning", async () => {
    const result = await receiveAutomaticWake("kintsu", async () => ({
      ok: false,
      code: "timeout",
      error: "private transport detail",
    }));

    expect(result).toEqual({
      answered: false,
      letter: "",
      title: null,
      source: null,
      warning: "paper boat unavailable (timeout)",
    });
  });

  test("keeps a thrown transport failure fail-open and visible", async () => {
    const result = await receiveAutomaticWake("kintsu", async () => {
      throw new Error("private transport detail");
    });

    expect(result).toEqual({
      answered: false,
      letter: "",
      title: null,
      source: null,
      warning: "paper boat unavailable",
    });
  });
});
