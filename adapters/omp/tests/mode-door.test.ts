import { afterEach, expect, test } from "bun:test";

import { MODE_COMMANDS, MODE_DOOR_LINES, modeDoor, resetModeDoors } from "../house-proof/mode-door.ts";

afterEach(() => resetModeDoors());

// Kills: a command missing a mode, or a mode no command reaches.
test("every mode command names a requested mode and every non-auto mode has a command", () => {
  expect(MODE_COMMANDS).toEqual({ magus: "work", bard: "conversation", warlock: "conversation", rogue: "quiet", warrior: "auto" });
  for (const mode of ["work", "conversation", "quiet"]) expect(Object.values(MODE_COMMANDS)).toContain(mode);
});

// Kills: the door ringing every turn, staying silent on a real transition, or
// dropping the register line on the turns between.
test("the door carries the mode's line every turn and rings once per transition", () => {
  const key = "kodo\0session-1";
  const first = modeDoor(key, "work");
  expect(first).toEqual({ from: null, to: "work", rang: true, text: MODE_DOOR_LINES.work });

  const held = modeDoor(key, "work");
  expect(held.rang).toBe(false);
  expect(held.text).toBe(MODE_DOOR_LINES.work);

  const flipped = modeDoor(key, "conversation");
  expect(flipped).toEqual({ from: "work", to: "conversation", rang: true, text: MODE_DOOR_LINES.conversation });
  // Another session has its own memory.
  expect(modeDoor("kodo\0session-2", "work").from).toBeNull();
});

// Kills: a mode with no House line, which would send an empty block every turn.
test("every resolved mode has a non-empty House line", () => {
  for (const mode of ["work", "mixed", "conversation", "quiet"] as const) expect(MODE_DOOR_LINES[mode].length).toBeGreaterThan(0);
});
