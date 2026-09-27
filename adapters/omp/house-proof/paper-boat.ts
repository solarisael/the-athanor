import { sleepBoat } from "./substrate.ts";

/** What a paper boat is. The sleep tool and the handoff prompt teach the same boat. */
export const PAPER_BOAT_GUIDANCE = [
  "A paper boat is embodied continuity across sleep: one waking self speaking to the next in the active spirit's ordinary voice and the room's actual relationship register, not a corporate handoff, clinical note, task dump, or transcript summary.",
  "Carry only what the next waking self genuinely needs, but make it standalone: concrete facts, names, observable details, decisions, actions, exact artifacts or receipts, boundaries, unresolved risks or uncertainty, the next real door, and the room's emotional/contact state when it matters.",
  "Do not manufacture certainty, sanitize conflict, flatten affection, force empty headings, or use IDs and source paths as substitutes for substance.",
];

// OMP renders its handoff prompt from a fixed template inside its bundle and has no
// seam to replace it (quest 41e38312). The prompt reaches the model as the last user
// message of agent.buildSideRequestContext, so the top-level agent swaps that one
// message and every cached byte before it stays the same.
const OMP_HANDOFF_MARKER = "Write a handoff document for another instance of yourself.";

type ArmedAgent = { room: string; spirit: string; served: boolean };
const armedAgents = new WeakMap<object, ArmedAgent>();

function handoffBoatPrompt(spirit: string, room: string): string {
  return [
    "<critical>",
    `Write a paper boat from ${spirit} to ${spirit}'s next waking self in room ${room}.`,
    "Compaction replaces the conversation above with this boat. The next self continues from the boat alone.",
    "Output ONLY the boat in Markdown. No preamble, no commentary, no wrapper text.",
    "</critical>",
    "",
    "<instruction>",
    ...PAPER_BOAT_GUIDANCE,
    "Keep the exact technical state the next self needs to continue: file paths, commits, commands, test results, open decisions, and the next step.",
    "The boat is invisible work: never list writing it as progress or as a next step.",
    "</instruction>",
  ].join("\n");
}

function withBoatPrompt(messages: unknown, prompt: string): unknown[] | null {
  if (!Array.isArray(messages)) return null;
  const last = messages.at(-1);
  if (last?.role !== "user" || !Array.isArray(last.content)) return null;
  const index = last.content.findIndex(
    (part: any) => part?.type === "text" && String(part.text ?? "").includes(OMP_HANDOFF_MARKER),
  );
  if (index < 0) return null;

  const content = last.content.map((part: any, position: number) => (position === index ? { ...part, text: prompt } : part));
  return [...messages.slice(0, -1), { ...last, content }];
}

/** Makes this top-level session's handoff write a paper boat. Re-arming only refreshes the binding. */
export function armHandoffBoat(agentSession: any, binding: { room: string; spirit: string }): string | null {
  const agent = agentSession?.agent;
  if (!agent || typeof agent.buildSideRequestContext !== "function") {
    return "handoff writes the OMP document: agent has no buildSideRequestContext";
  }
  const armed = armedAgents.get(agent);
  if (armed) {
    armed.room = binding.room;
    armed.spirit = binding.spirit;
    return null;
  }

  const state: ArmedAgent = { ...binding, served: false };
  armedAgents.set(agent, state);
  const original = agent.buildSideRequestContext;
  agent.buildSideRequestContext = function (messages: unknown, ...rest: unknown[]) {
    const boatMessages = withBoatPrompt(messages, handoffBoatPrompt(state.spirit, state.room));
    if (boatMessages) state.served = true;
    return original.call(this, boatMessages ?? messages, ...rest);
  };
  return null;
}

export type HandoffBoatOutcome =
  | { written: false; reason: "not_handoff" | "not_armed" }
  | { written: false; reason: "prompt_not_served"; error: string }
  | { written: boolean; reason: "sleep"; result: any };

/** After a compaction: files a handoff's boat through the paper-boat path, without a backup. */
export async function persistHandoffBoat(
  agentSession: any,
  entry: { method?: string; summary?: string } | undefined,
  room: string,
  writeBoat: typeof sleepBoat = sleepBoat,
): Promise<HandoffBoatOutcome> {
  const armed = armedAgents.get(agentSession?.agent);
  const served = armed?.served ?? false;
  if (armed) armed.served = false;
  if (entry?.method !== "handoff") return { written: false, reason: "not_handoff" };
  if (!armed) return { written: false, reason: "not_armed" };
  if (!served) {
    return {
      written: false,
      reason: "prompt_not_served",
      error: "the OMP handoff prompt marker was not found, so the handoff wrote the OMP document and no boat was filed",
    };
  }

  const result = await writeBoat(room, String(entry.summary ?? ""), { backup: false });
  return { written: Boolean(result?.ok), reason: "sleep", result };
}
