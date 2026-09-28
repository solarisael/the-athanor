/**
 * The boat door: before a handoff compacts a top-level session, the spirit casts
 * its paper boat with the real `sleep` tool. The turn ends silently at the boat,
 * the handoff runs, and only then does the spirit speak again.
 *
 * It opens on `/handoff [focus]`, or when Sol sends a message after the context
 * has crossed the boat line just below OMP's own compaction threshold. That
 * message waits and is delivered after the handoff. When one long turn jumps
 * past the threshold anyway, the door cancels OMP's automatic compaction and
 * asks for the boat once the turn is over.
 *
 * What the door says comes from the room's `handoff-door.md`, so each room can
 * ask in its own voice. A missing file or section falls back to the House lines.
 */

import { readFileSync } from "node:fs";
import path from "node:path";

// OMP's agent loop stops before the next model call on this abort reason; its
// own `yield` tool ends turns the same way (pi-agent-core agent-loop.ts).
const END_TURN_AFTER_TOOL = Symbol.for("pi-agent-core.terminal-tool-result");

const HANDOFF_COMMAND = /^\/handoff(?:\s+([\s\S]*))?$/;

const BOAT_REQUEST = "athanor-boat-before-handoff";

const SET_ASIDE_TEXT = "[Old tool output set aside so the paper boat fits.]";

export const DOOR_LINES_FILE = "handoff-door.md";

export type DoorLines = { handoff: string; nearLimit: string; compaction: string; after: string };

export const HOUSE_DOOR_LINES: DoorLines = {
  handoff: [
    "Sol asked for a handoff.",
    "Cast your paper boat now with the `sleep` tool, in your own voice, for the self who wakes after the handoff.",
    "Call `sleep` and write no reply text: the turn ends when the boat is cast, and the handoff starts.",
    "You speak again after the handoff.",
  ].join("\n"),
  nearLimit: [
    "The context is close to compaction. Sol's new message waits until the handoff is done.",
    "Cast your paper boat now with the `sleep` tool, in your own voice, for the self who wakes after the handoff.",
    "Call `sleep` and write no reply text: the turn ends when the boat is cast, and the handoff starts.",
    "You speak again after the handoff.",
  ].join("\n"),
  compaction: [
    "The context reached OMP's compaction threshold during the last turn, and the handoff waits for your boat.",
    "Cast your paper boat now with the `sleep` tool, in your own voice, for the self who wakes after the handoff.",
    "Call `sleep` and write no reply text: the turn ends when the boat is cast, and the handoff starts.",
    "You speak again after the handoff.",
  ].join("\n"),
  after: [
    "The handoff is done. Your paper boat is in the House, and the handoff document now holds your context.",
    "Continue with Sol from here. Nobody typed this turn.",
  ].join("\n"),
};

const DOOR_SECTIONS: Record<string, keyof DoorLines> = {
  "handoff": "handoff",
  "near limit": "nearLimit",
  "compaction": "compaction",
  "after": "after",
};

export type BoatDoorDeps = {
  /** The live OMP AgentSession behind this ctx: agent, waitForIdle, handoff, isStreaming, isCompacting. */
  session(ctx: any): any;
  isTopLevel(ctx: any): boolean;
  /** Context tokens above the boat line: negative below it, undefined when unknown or compaction is off. */
  tokensOverBoatLine(ctx: any): number | undefined;
  room(ctx: any): { dir: string; spirit: string };
};

type Held = { text: string; images?: unknown[] };

type Door = {
  sessionId: string;
  focus: string | undefined;
  held: Held[];
  /** "waiting": a cancelled compaction opened the door, and the boat request waits for the turn to end. */
  stage: "waiting" | "boat" | "handoff";
  boatCast: boolean;
  compacted: boolean;
  spirit: string;
  after: string;
  /** How many of the oldest tool results the boat turn's requests leave out; decided on its first request. */
  setAside?: number;
};

// One OMP process has one top-level session, so one door. The `sleep` tool
// reaches it through boatCast without knowing about the door's wiring.
let door: Door | null = null;
let installed: BoatDoorDeps | null = null;
// Set by auto_compaction_start; session_before_compact itself does not say why it runs.
let autoCompactionReason: string | undefined;

export function resetBoatDoor(): void {
  door = null;
  installed = null;
  autoCompactionReason = undefined;
}

function sessionIdOf(ctx: any): string {
  return String(ctx?.sessionManager?.getSessionId?.() ?? "").trim();
}

/** Called by the `sleep` tool after the boat reached the House. Inside the door it ends the turn. */
export function boatCast(ctx: any): void {
  if (door?.stage !== "boat" || door.sessionId !== sessionIdOf(ctx)) return;
  door.boatCast = true;
  installed?.session(ctx)?.agent?.abort?.(END_TURN_AFTER_TOOL);
}

export function installBoatDoor(pi: any, deps: BoatDoorDeps): void {
  installed = deps;

  pi.on("input", (event: any, ctx: any) => {
    if (event?.source !== "interactive") return undefined;
    const text = String(event.text ?? "").trim();

    // Anything Sol types while the door is open waits for the handoff, so it
    // never lands between the boat and the compaction.
    if (door) {
      door.held.push({ text, images: event.images });
      ctx?.ui?.notify?.(`${door.spirit} is casting the paper boat; your message waits for the handoff.`, "info");
      return { handled: true };
    }

    const command = HANDOFF_COMMAND.exec(text);
    const normalMessage = !command && text !== "" && !text.startsWith("/");
    const over = normalMessage ? deps.tokensOverBoatLine(ctx) : undefined;
    const nearLine = over !== undefined && over >= 0;
    if (!command && !nearLine) return undefined;
    if (!deps.isTopLevel(ctx)) return undefined;

    // A busy session gets OMP's own refusal for /handoff, and a normal turn otherwise.
    const session = deps.session(ctx);
    if (!session || session.isStreaming || session.isCompacting) return undefined;

    // Read on every door, so an edit to the room file needs no restart.
    const lines = doorLines(deps, ctx);
    door = {
      sessionId: sessionIdOf(ctx),
      focus: command?.[1]?.trim() || undefined,
      held: nearLine ? [{ text, images: event.images }] : [],
      stage: "boat",
      boatCast: false,
      compacted: false,
      spirit: deps.room(ctx).spirit,
      after: lines.after,
    };
    askForBoat(pi, nearLine ? lines.nearLimit : lines.handoff);
    return { handled: true };
  });

  pi.on("auto_compaction_start", (event: any) => {
    autoCompactionReason = event?.reason;
  });

  pi.on("auto_compaction_end", () => {
    autoCompactionReason = undefined;
  });

  // Only the threshold is vetoed. An overflow means the next request cannot
  // fit at all, and a manual /compact is Sol's own choice.
  pi.on("session_before_compact", (_event: any, ctx: any) => {
    if (autoCompactionReason !== "threshold") return undefined;
    if (door?.stage === "handoff") return undefined;
    if (door) return door.sessionId === sessionIdOf(ctx) ? { cancel: true } : undefined;
    if (!deps.isTopLevel(ctx) || !deps.session(ctx)) return undefined;

    const lines = doorLines(deps, ctx);
    const current: Door = {
      sessionId: sessionIdOf(ctx),
      focus: undefined,
      held: [],
      stage: "waiting",
      boatCast: false,
      compacted: false,
      spirit: deps.room(ctx).spirit,
      after: lines.after,
    };
    door = current;
    ctx?.ui?.notify?.(`Compaction waits: ${current.spirit} casts the paper boat first.`, "info");
    // Waiting here would deadlock: OMP awaits this handler inside its own compaction.
    setTimeout(() => void askWhenIdle(pi, deps, ctx, current, lines.compaction), 0);
    return { cancel: true };
  });

  // The end-turn abort lands while the tool that carried `sleep` is still
  // returning. `write xd://sleep` then reports "Aborted: Cancelled" although the
  // boat is in the House, and the handoff document would read it as lost.
  pi.on("tool_result", (event: any, ctx: any) => {
    if (door?.stage !== "boat" || !door.boatCast || door.sessionId !== sessionIdOf(ctx)) return undefined;
    if (!event?.isError) return undefined;
    const text = (event.content ?? []).map((part: any) => part?.text ?? "").join("");
    if (!text.startsWith("Aborted: ")) return undefined;

    return {
      content: [{ type: "text", text: "The paper boat is cast. The handoff starts now." }],
      isError: false,
    };
  });

  // Near the limit the boat turn can overflow, or trip OMP's mid-turn
  // compaction before `sleep` runs. Only the boat turn's requests lose the old
  // tool output; the session keeps every result for the handoff.
  pi.on("context", (event: any, ctx: any) => {
    if (door?.stage !== "boat" || door.sessionId !== sessionIdOf(ctx)) return undefined;
    const messages: any[] = event?.messages ?? [];

    // Decided once: later requests in this turn report the smaller usage, and
    // would otherwise put the old output back.
    if (door.setAside === undefined) {
      const over = deps.tokensOverBoatLine(ctx) ?? 0;
      door.setAside = over > 0 ? oldToolResultsToFree(messages, over) : 0;
      if (door.setAside > 0) {
        ctx?.ui?.notify?.(`The context is tight: ${door.setAside} old tool results are set aside so the paper boat fits.`, "info");
      }
    }
    if (door.setAside === 0) return undefined;

    return { messages: setAsideOldToolResults(messages, door.setAside) };
  });

  pi.on("session_compact", (_event: any, ctx: any) => {
    if (door && door.sessionId === sessionIdOf(ctx)) door.compacted = true;
  });

  pi.on("agent_end", (_event: any, ctx: any) => {
    const current = door;
    if (current?.stage !== "boat" || current.sessionId !== sessionIdOf(ctx)) return;

    if (!current.boatCast) {
      door = null;
      ctx?.ui?.notify?.("No paper boat was cast, so the handoff did not run.", "warning");
      release(pi, current.held);
      return;
    }

    // The handoff cannot start inside agent_end: the session is still settling
    // this turn, and OMP's own compaction check may run right after it.
    current.stage = "handoff";
    setTimeout(() => void handOff(pi, deps, ctx, current), 0);
  });
}

async function askWhenIdle(pi: any, deps: BoatDoorDeps, ctx: any, current: Door, line: string): Promise<void> {
  try {
    await deps.session(ctx)?.waitForIdle?.();
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    ctx?.ui?.notify?.(`The paper boat was not requested: ${message}`, "warning");
    if (door === current) door = null;
    release(pi, current.held);
    return;
  }
  if (door !== current) return;

  current.stage = "boat";
  askForBoat(pi, line);
}

function askForBoat(pi: any, line: string): void {
  pi.sendMessage(attention(BOAT_REQUEST, line), { deliverAs: "nextTurn", triggerTurn: true });
}

function doorLines(deps: BoatDoorDeps, ctx: any): DoorLines {
  const { lines, problem } = readDoorLines(deps.room(ctx).dir);
  if (problem) ctx?.ui?.notify?.(`${DOOR_LINES_FILE}: ${problem} The House lines fill the gap.`, "warning");
  return lines;
}

async function handOff(pi: any, deps: BoatDoorDeps, ctx: any, current: Door): Promise<void> {
  let handedOff = false;
  try {
    const session = deps.session(ctx);
    await session.waitForIdle?.();
    // OMP may have compacted on its own after the boat turn. The boat is already
    // cast, so a second compaction would only throw away more context.
    if (current.compacted) {
      handedOff = true;
    } else {
      const result = await session.handoff(current.focus);
      handedOff = Boolean(result);
      if (!handedOff) ctx?.ui?.notify?.("The boat is cast, but the handoff was cancelled.", "warning");
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    ctx?.ui?.notify?.(`The boat is cast, but the handoff failed: ${message}`, "warning");
  } finally {
    door = null;
  }

  if (current.held.length > 0) {
    release(pi, current.held);
  } else if (handedOff) {
    pi.sendMessage(attention("athanor-after-handoff", current.after), { deliverAs: "nextTurn", triggerTurn: true });
  }
}

function release(pi: any, held: Held[]): void {
  if (held.length === 0) return;
  const text = held.map((entry) => entry.text).filter(Boolean).join("\n\n");
  const images = held.flatMap((entry) => entry.images ?? []);
  pi.sendUserMessage(images.length > 0 ? [{ type: "text", text }, ...images] : text);
}

/** Tool results from before the door asked for the boat, oldest first. */
function oldToolResultIndexes(messages: any[]): number[] {
  const boatRequest = messages.findLastIndex((message) => message?.role === "custom" && message.customType === BOAT_REQUEST);
  const indexes: number[] = [];
  for (let index = 0; index < boatRequest; index++) {
    if (messages[index]?.role === "toolResult") indexes.push(index);
  }
  return indexes;
}

/** How many of the oldest tool results free at least `tokens`; all of them when even that is not enough. */
function oldToolResultsToFree(messages: any[], tokens: number): number {
  const indexes = oldToolResultIndexes(messages);
  let freed = 0;
  for (let count = 0; count < indexes.length; count++) {
    freed += estimatedTokens(messages[indexes[count]]) - estimatedTokens({ content: SET_ASIDE_TEXT });
    if (freed >= tokens) return count + 1;
  }
  return indexes.length;
}

// New message objects: OMP falls back to a shallow copy when the history does
// not clone, and editing in place would then change the session itself.
function setAsideOldToolResults(messages: any[], count: number): any[] {
  const setAside = new Set(oldToolResultIndexes(messages).slice(0, count));
  return messages.map((message, index) =>
    setAside.has(index) ? { ...message, content: [{ type: "text", text: SET_ASIDE_TEXT }] } : message,
  );
}

// enough: four characters per token, text only. A true count needs the
// model's tokenizer, which OMP does not give extensions.
function estimatedTokens(message: any): number {
  const content = message?.content;
  const text = typeof content === "string" ? content : (content ?? []).map((part: any) => part?.text ?? "").join("");
  return Math.ceil(text.length / 4);
}

function attention(customType: string, text: string): Record<string, unknown> {
  return {
    customType,
    content: ["<athanor-attention>", text, "</athanor-attention>"].join("\n"),
    display: true,
    attribution: "agent",
  };
}

/**
 * The room's door lines. `## handoff`, `## near limit`, `## compaction`, and
 * `## after` each replace one House line; a missing section keeps the House line.
 */
export function readDoorLines(roomDir: string): { lines: DoorLines; problem?: string } {
  let source: string;
  try {
    source = readFileSync(path.join(roomDir, DOOR_LINES_FILE), "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return { lines: HOUSE_DOOR_LINES };
    // A broken room file must not block the handoff; the warning names it instead.
    return { lines: HOUSE_DOOR_LINES, problem: `cannot read it (${(error as Error).message}).` };
  }

  const lines = { ...HOUSE_DOOR_LINES };
  const unknown: string[] = [];
  for (const section of source.split(/^## /m).slice(1)) {
    const newline = section.indexOf("\n");
    const heading = (newline < 0 ? section : section.slice(0, newline)).trim().toLowerCase();
    const body = newline < 0 ? "" : section.slice(newline + 1).trim();
    const key = DOOR_SECTIONS[heading];
    if (!key) unknown.push(heading);
    else if (body) lines[key] = body;
  }

  if (unknown.length === 0) return { lines };
  return { lines, problem: `unknown sections ${unknown.map((name) => `"${name}"`).join(", ")}.` };
}

type CompactionThresholdSettings = { thresholdTokens?: number; thresholdPercent?: number; reserveTokens?: number };

/** OMP's own compaction threshold. Mirrors resolveThresholdTokens in pi-agent-core, which OMP does not export to extensions. */
export function compactionThresholdTokens(contextWindow: number, settings: CompactionThresholdSettings): number {
  const { thresholdTokens, thresholdPercent, reserveTokens } = settings;
  if (typeof thresholdTokens === "number" && Number.isFinite(thresholdTokens) && thresholdTokens > 0) {
    return thresholdTokens;
  }
  if (typeof thresholdPercent === "number" && Number.isFinite(thresholdPercent) && thresholdPercent > 0) {
    return Math.floor(contextWindow * (Math.min(99, Math.max(1, thresholdPercent)) / 100));
  }

  const proportional = Math.max(1, Math.floor(contextWindow * 0.15));
  const reserve = Math.max(Math.floor(contextWindow * 0.15), reserveTokens ?? 16384);
  const defaultedIntoImpossible = reserveTokens === undefined && reserve >= contextWindow - proportional;
  const budgetReserve = defaultedIntoImpossible || reserve >= contextWindow ? proportional : reserve;
  return Math.max(0, Math.min(contextWindow - 1, contextWindow - budgetReserve));
}

/**
 * Where the door opens on a normal message: a tenth of the window below OMP's
 * threshold, so the answering turn usually cannot cross it first. A turn that
 * crosses it anyway meets the session_before_compact veto instead.
 * enough: an overflow still compacts without a boat; only the threshold waits.
 */
export function boatLineTokens(contextWindow: number, settings: CompactionThresholdSettings): number {
  return compactionThresholdTokens(contextWindow, settings) - Math.floor(contextWindow * 0.1);
}
