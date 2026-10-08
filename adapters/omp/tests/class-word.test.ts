import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { classWord, HOUSE_MODE_DOOR_LINES, modeDoor, readModeDoorLines, resetModeDoors } from "../house-proof/class-word.ts";

afterEach(() => resetModeDoors());

// Kills: a word matched anywhere in the prompt, or a case-sensitive match.
test("only the first word of the prompt is a class word", () => {
  expect(classWord("magus let's fix the ledger uwu")).toEqual({ word: "magus", mode: "work" });
  expect(classWord("  Bard, how was the gym?")).toEqual({ word: "bard", mode: "conversation" });
  expect(classWord("WARLOCK")).toEqual({ word: "warlock", mode: "conversation" });
  expect(classWord("rogue")).toEqual({ word: "rogue", mode: "quiet" });
  expect(classWord("warrior owo")).toEqual({ word: "warrior", mode: "auto" });
  expect(classWord("the bard in chapter three sings")).toBeNull();
  expect(classWord("magusfoo")).toBeNull();
  expect(classWord("")).toBeNull();
});

// Kills: the door ringing every turn, or staying silent on a real transition.
test("the mode door rings once per transition and stays silent while the mode holds", () => {
  const key = "kodo\0session-1";
  const first = modeDoor(key, "work", HOUSE_MODE_DOOR_LINES);
  expect(first?.from).toBeNull();
  expect(first?.to).toBe("work");
  expect(first?.text).toContain("coding lessons");
  expect(modeDoor(key, "work", HOUSE_MODE_DOOR_LINES)).toBeNull();
  // Conversation has no House line, so the door stays silent and still remembers the mode.
  expect(modeDoor(key, "conversation", HOUSE_MODE_DOOR_LINES)).toBeNull();
  expect(modeDoor(key, "work", HOUSE_MODE_DOOR_LINES)?.from).toBe("conversation");
  // Another session has its own memory.
  expect(modeDoor("kodo\0session-2", "work", HOUSE_MODE_DOOR_LINES)?.from).toBeNull();
});

// Kills: a room section ignored, an empty section falling back to the House line,
// or an unknown heading passing without a warning.
test("the room's mode-door.md replaces House lines per section", () => {
  const dir = mkdtempSync(path.join(tmpdir(), "mode-door-"));
  try {
    expect(readModeDoorLines(dir)).toEqual({ lines: HOUSE_MODE_DOOR_LINES });

    writeFileSync(path.join(dir, "mode-door.md"), [
      "# Kodo's doors",
      "## work",
      "Query lessons. Read the map. Kittens past three files.",
      "",
      "## conversation",
      "Hands off the keyboard; just be here.",
      "## quiet",
      "",
      "## dance",
      "nope",
    ].join("\n"));
    const { lines, problem } = readModeDoorLines(dir);
    expect(lines.work).toBe("Query lessons. Read the map. Kittens past three files.");
    expect(lines.conversation).toBe("Hands off the keyboard; just be here.");
    expect(lines.quiet).toBe("");
    expect(lines.mixed).toBe(HOUSE_MODE_DOOR_LINES.mixed);
    expect(problem).toContain('"dance"');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
