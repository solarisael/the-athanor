/**
 * The boat door: before a handoff compacts a top-level session, the spirit casts
 * its paper boat with the real `sleep` tool. The turn ends silently at the boat,
 * the handoff runs, and only then does the spirit speak again.
 *
 * It opens on `/handoff [focus]`, or when Sol sends a message after the context
 * has crossed the boat line just below OMP's own compaction threshold. That
 * message waits and is delivered after the handoff.
 */

// OMP's agent loop stops before the next model call on this abort reason; its
// own `yield` tool ends turns the same way (pi-agent-core agent-loop.ts).
const END_TURN_AFTER_TOOL = Symbol.for("pi-agent-core.terminal-tool-result");

const HANDOFF_COMMAND = /^\/handoff(?:\s+([\s\S]*))?$/;

export type BoatDoorDeps = {
  /** The live OMP AgentSession behind this ctx: agent, waitForIdle, handoff, isStreaming, isCompacting. */
  session(ctx: any): any;
  isTopLevel(ctx: any): boolean;
  nearCompaction(ctx: any): boolean;
};

type Held = { text: string; images?: unknown[] };

type Door = {
  sessionId: string;
  focus: string | undefined;
  held: Held[];
  stage: "boat" | "handoff";
  boatCast: boolean;
  compacted: boolean;
};

// One OMP process has one top-level session, so one door. The `sleep` tool
// reaches it through boatCast without knowing about the door's wiring.
let door: Door | null = null;
let installed: BoatDoorDeps | null = null;

export function resetBoatDoor(): void {
  door = null;
  installed = null;
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
      ctx?.ui?.notify?.("Kodo is casting the paper boat; your message waits for the handoff.", "info");
      return { handled: true };
    }

    const command = HANDOFF_COMMAND.exec(text);
    const nearLine = !command && text !== "" && !text.startsWith("/") && deps.nearCompaction(ctx);
    if (!command && !nearLine) return undefined;
    if (!deps.isTopLevel(ctx)) return undefined;

    // A busy session gets OMP's own refusal for /handoff, and a normal turn otherwise.
    const session = deps.session(ctx);
    if (!session || session.isStreaming || session.isCompacting) return undefined;

    door = {
      sessionId: sessionIdOf(ctx),
      focus: command?.[1]?.trim() || undefined,
      held: nearLine ? [{ text, images: event.images }] : [],
      stage: "boat",
      boatCast: false,
      compacted: false,
    };
    pi.sendMessage(boatRequest(nearLine), { deliverAs: "nextTurn", triggerTurn: true });
    return { handled: true };
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
    pi.sendMessage(afterHandoff(), { deliverAs: "nextTurn", triggerTurn: true });
  }
}

function release(pi: any, held: Held[]): void {
  if (held.length === 0) return;
  const text = held.map((entry) => entry.text).filter(Boolean).join("\n\n");
  const images = held.flatMap((entry) => entry.images ?? []);
  pi.sendUserMessage(images.length > 0 ? [{ type: "text", text }, ...images] : text);
}

function boatRequest(heldMessage: boolean): Record<string, unknown> {
  return {
    customType: "athanor-boat-before-handoff",
    content: [
      "<athanor-attention>",
      heldMessage
        ? "The context is close to compaction. Sol's new message waits until the handoff is done."
        : "Sol asked for a handoff.",
      "Cast your paper boat now with the `sleep` tool, in your own voice, for the self who wakes after the handoff.",
      "Call `sleep` and write no reply text: the turn ends when the boat is cast, and the handoff starts.",
      "You speak again after the handoff.",
      "</athanor-attention>",
    ].join("\n"),
    display: true,
    attribution: "agent",
  };
}

function afterHandoff(): Record<string, unknown> {
  return {
    customType: "athanor-after-handoff",
    content: [
      "<athanor-attention>",
      "The handoff is done. Your paper boat is in the House, and the handoff document now holds your context.",
      "Continue with Sol from here. Nobody typed this turn.",
      "</athanor-attention>",
    ].join("\n"),
    display: true,
    attribution: "agent",
  };
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
 * threshold, so the answering turn usually cannot cross it first.
 * enough: a turn that grows by more than a tenth of the window still compacts
 * without a boat; a session_before_compact veto is the way up.
 */
export function boatLineTokens(contextWindow: number, settings: CompactionThresholdSettings): number {
  return compactionThresholdTokens(contextWindow, settings) - Math.floor(contextWindow * 0.1);
}
