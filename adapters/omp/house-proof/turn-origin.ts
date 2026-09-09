// Turn origins for context selection, memo identity, and anchored additions.
// Native and queued prompts use before_agent_start; idle generated prompts do not.
// Only the three named House doors carry generated-turn identity.

import { messageText } from "./text.ts";

export const GENERATED_TURN_ORIGINS: ReadonlyMap<string, string> = new Map([
  ["athanor-restart-continuation", "intentId"],
  ["athanor-chat-say", "sayId"],
  ["athanor-hallway-knock", "knockId"],
]);

export type TurnOrigin = {
  message: any;
  // A native user turn carries operator authority; a generated turn does not.
  native: boolean;
};

export function generatedTurnKey(message: any): string | null {
  if (!message || message.role !== "custom") return null;
  const customType = typeof message.customType === "string" ? message.customType : "";
  const field = GENERATED_TURN_ORIGINS.get(customType);
  if (!field) return null;
  const id = message.details?.[field];
  if (typeof id !== "string") return null;
  const trimmed = id.trim();
  if (!trimmed) return null;
  return `${customType}:${trimmed}`;
}


// Stable identity per turn origin. User keys are unchanged from before the
// generated-turn cut: `id:<id>` when OMP names the message, otherwise the
// user-turn ordinal plus a digest of the text. Generated keys are the door's
// customType plus its own id, so a retry inside the same turn recomputes the
// same key and a distinct event never collides with an earlier one.
export function turnKeysByMessage(messages: any[]): Map<any, string> {
  const keys = new Map<any, string>();
  let ordinal = 0;
  for (const message of messages) {
    if (message?.role === "user") {
      ordinal += 1;
      const identity = typeof message?.id === "string" && message.id
        ? `id:${message.id}`
        : `ord:${ordinal}:${Bun.hash(messageText(message)).toString(36)}`;
      keys.set(message, identity);
      continue;
    }
    const generated = generatedTurnKey(message);
    if (generated) keys.set(message, generated);
  }
  return keys;
}

// The text OMP hands before_agent_start for a message it is prompting with:
// a user prompt's content is [{ type: "text", text: expandedText }, ...images]
// (agent-session.ts prompt), and a custom message's text is its string content
// or its text parts joined with nothing (#getCustomMessageTextContent).
function originPromptText(message: any): string {
  if (message?.role === "custom") {
    if (typeof message.content === "string") return message.content;
    if (Array.isArray(message.content)) {
      return message.content
        .filter((part: any) => part?.type === "text" && typeof part.text === "string")
        .map((part: any) => part.text)
        .join("");
    }
    return "";
  }
  return messageText(message);
}

// The message this request's turn answers.
//
// activePrompt is the before_agent_start prompt held for this session, or
// null/undefined when the harness emitted none for the running turn. With a
// prompt: the last recognized message whose text equals it wins, and a prompt
// that matches no recognized message resolves to nothing — an older unrelated
// user turn is never adopted as this turn's authority. Without a prompt: the
// latest recognized message wins, which is the door message on OMP's idle
// agent-initiated path and the user's own message on a native turn whose
// before_agent_start was not observed. Only a role:user message is native.
export function currentTurnOrigin(messages: any[], activePrompt?: string | null): TurnOrigin | null {
  const hasActivePrompt = typeof activePrompt === "string" && activePrompt.length > 0;
  for (let index = messages.length - 1; index >= 0; index -= 1) {
    const message = messages[index];
    const native = message?.role === "user";
    if (!native && generatedTurnKey(message) === null) continue;
    if (hasActivePrompt && originPromptText(message) !== activePrompt) continue;
    return { message, native };
  }
  return null;
}

export function anchorTurnAdditions(
  messages: any[],
  turnKeys: Map<any, string>,
  memo: Map<string, Array<Record<string, any>>>,
): { messages: any[] } | undefined {
  const output: any[] = [];
  let inserted = false;
  for (const message of messages) {
    output.push(message);
    const key = turnKeys.get(message);
    if (!key) continue;
    const additions = memo.get(key);
    if (additions?.length) {
      output.push(...additions);
      inserted = true;
    }
  }
  return inserted ? { messages: output } : undefined;
}
