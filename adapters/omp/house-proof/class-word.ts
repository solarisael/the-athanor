/**
 * Class words: one silly word at the start of Sol's message sets the room's
 * Recall mode for the session. `magus` works, `bard` (or `warlock`) talks,
 * `rogue` goes quiet, `warrior` hands the choice back to auto. The Host still
 * owns the mode; the word only sends the same command the recall_policy tool
 * sends. The ultra-words (ultrathink and kin) stay in the Host as per-turn
 * directives: they add, these switch.
 *
 * The mode door rings once when the resolved mode changes, by word or by auto
 * flipping on tool evidence. What it says comes from the room's `mode-door.md`,
 * so each room can keep its own macro: "query lessons, read the map, kittens".
 */

import { readFileSync } from "node:fs";
import path from "node:path";
import type { RequestedRecallMode, ResolvedRecallMode } from "./recall-policy.ts";

export const CLASS_WORDS: Readonly<Record<string, RequestedRecallMode>> = {
  magus: "work",
  bard: "conversation",
  warlock: "conversation",
  rogue: "quiet",
  warrior: "auto",
};

const CLASS_WORD_PATTERN = new RegExp(`^\\s*(${Object.keys(CLASS_WORDS).join("|")})\\b`, "i");

export type ClassWord = { word: string; mode: RequestedRecallMode };

/** The class word that opens the prompt, if any. Only the first word counts: a bard in a story is not a command. */
export function classWord(prompt: string): ClassWord | null {
  const match = CLASS_WORD_PATTERN.exec(prompt);
  if (!match) return null;
  const word = match[1].toLowerCase();
  return { word, mode: CLASS_WORDS[word] };
}

export const MODE_DOOR_FILE = "mode-door.md";

export type ModeDoorLines = Record<ResolvedRecallMode, string>;

export const HOUSE_MODE_DOOR_LINES: ModeDoorLines = {
  work: [
    "Work mode is on. Before the first edit: query the coding lessons once for this task, read the relevant map, and name the files you will touch.",
    "Anything past a handful of files goes to the kittens with a bounded brief. Prove the change before you report it.",
  ].join("\n"),
  mixed: "",
  conversation: "",
  quiet: "",
};

/**
 * The room's door lines. `## work`, `## mixed`, `## conversation`, and `## quiet`
 * each replace one House line; a missing section keeps the House line, and an
 * empty section keeps the door silent for that mode.
 */
export function readModeDoorLines(roomDir: string): { lines: ModeDoorLines; problem?: string } {
  let source: string;
  try {
    source = readFileSync(path.join(roomDir, MODE_DOOR_FILE), "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return { lines: HOUSE_MODE_DOOR_LINES };
    return { lines: HOUSE_MODE_DOOR_LINES, problem: `cannot read it (${(error as Error).message}).` };
  }

  const lines = { ...HOUSE_MODE_DOOR_LINES };
  const unknown: string[] = [];
  for (const section of source.split(/^## /m).slice(1)) {
    const newline = section.indexOf("\n");
    const heading = (newline < 0 ? section : section.slice(0, newline)).trim().toLowerCase();
    const body = newline < 0 ? "" : section.slice(newline + 1).trim();
    if (heading in lines) lines[heading as ResolvedRecallMode] = body;
    else unknown.push(heading);
  }

  if (unknown.length === 0) return { lines };
  return { lines, problem: `unknown sections ${unknown.map((name) => `"${name}"`).join(", ")}.` };
}

// enough: the last resolved mode per session lives in this process. A restart
// rings the door once more on the first turn, which is the cheaper mistake.
const lastResolved = new Map<string, ResolvedRecallMode>();

export type ModeDoor = { from: ResolvedRecallMode | null; to: ResolvedRecallMode; text: string };

/** The door line for this turn when the resolved mode changed, else null. Remembers the mode either way. */
export function modeDoor(sessionKey: string, resolved: ResolvedRecallMode, lines: ModeDoorLines): ModeDoor | null {
  const from = lastResolved.get(sessionKey) ?? null;
  lastResolved.set(sessionKey, resolved);
  if (from === resolved) return null;
  const text = lines[resolved] ?? "";
  if (!text) return null;
  return { from, to: resolved, text };
}

/** Test seam: forget every remembered mode. */
export function resetModeDoors(): void {
  lastResolved.clear();
}
