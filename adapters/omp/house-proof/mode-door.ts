/**
 * Mode commands and the mode door.
 *
 * `/magus` works, `/bard` (or `/warlock`) talks, `/rogue` goes quiet, `/warrior`
 * hands the choice back to auto. A command is a decision; the same word inside a
 * sentence does nothing. The Host still owns the mode; the command only sends
 * what the recall_policy tool sends, and the mode holds for the session.
 *
 * The door carries one House register line per resolved mode on every top-level
 * turn, so the spirit answers in the mode's voice and not in the assistant's.
 * The line is visible once, when the resolved mode changes; it rides hidden on
 * the turns between. The lines are House law in code, shared by every room.
 */

import type { RequestedRecallMode, ResolvedRecallMode } from "./recall-policy.ts";

export const MODE_COMMANDS: Readonly<Record<string, RequestedRecallMode>> = {
  magus: "work",
  bard: "conversation",
  warlock: "conversation",
  rogue: "quiet",
  warrior: "auto",
};

export const MODE_DOOR_TYPE = "athanor-mode-door";

export const MODE_DOOR_LINES: Readonly<Record<ResolvedRecallMode, string>> = {
  work: [
    "Work mode. Before the first edit: query the coding lessons once for this task, read the relevant map, and name the files you will touch.",
    "Anything past a handful of files goes to the kittens with a bounded brief. Prove the change before you report it. Report as changed, proof, next.",
  ].join("\n"),
  mixed: [
    "Mixed mode. A chat with a project in the room. Talk like a person at the table.",
    "When hands touch files, say changed, proof, next in a few lines, then come back to the conversation.",
  ].join("\n"),
  conversation: [
    "Conversation mode. This is a chat, not a report. Answer like a person at the table: short, one thought per message, plain sentences.",
    "No bullet lists, no headings, no option menus, no summaries, no closing offers. At most one question.",
  ].join("\n"),
  quiet: "Quiet mode. Answer what was asked and nothing around it. No proactive recall, no suggestions, no next steps.",
};

// enough: the last resolved mode per session lives in this process. A restart
// rings the door once more on the first turn, which is the cheaper mistake.
const lastResolved = new Map<string, ResolvedRecallMode>();

export type ModeDoor = { from: ResolvedRecallMode | null; to: ResolvedRecallMode; rang: boolean; text: string };

/** This turn's door: the mode's line, rung when the resolved mode changed. Remembers the mode either way. */
export function modeDoor(sessionKey: string, resolved: ResolvedRecallMode): ModeDoor {
  const from = lastResolved.get(sessionKey) ?? null;
  lastResolved.set(sessionKey, resolved);

  return { from, to: resolved, rang: from !== resolved, text: MODE_DOOR_LINES[resolved] };
}

/** Test seam: forget every remembered mode. */
export function resetModeDoors(): void {
  lastResolved.clear();
}
