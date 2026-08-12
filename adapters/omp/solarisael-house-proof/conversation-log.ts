// OMP conversation capture.
// Silhouette: reduce OMP's loose message shapes to visible text and hand the
// window to the Host. Turn identity, freshness, dedupe, transcript shape, and
// durability all belong to Rust.

import { OMP_SESSION_ID } from "./constants.ts";
import { hostCommand, sendHostCommand, type HostBinding } from "./host.ts";
import { conversationText } from "./text.ts";

const CONVERSATION_LOG = "athanor.shell.conversation_log";
const SHELL_RESULT = "athanor.shell.result";
const ACCEPTED = new Set([SHELL_RESULT]);

export type LoggedTurn = {
  role: string;
  text: string;
  sourceID: string;
  contentHash: string;
  sessionID: string;
  sourceTimestamp: string;
  hasStableID: boolean;
};

export type ConversationCapture = {
  turns: number;
  fresh: boolean;
  appended: number;
  skipped: number;
  errors: Array<Record<string, unknown>>;
  loggedTurns: LoggedTurn[];
};

function harnessTimestamp(message: any): string | undefined {
  const value = message?.timestamp ?? message?.createdAt ?? message?.info?.timestamp ?? message?.info?.createdAt;
  if (value === undefined || value === null || value === "") return undefined;
  const parsed = value instanceof Date ? value : new Date(value);
  return Number.isNaN(parsed.getTime()) ? undefined : parsed.toISOString();
}

function harnessId(message: any): string | undefined {
  const value = message?.id ?? message?.messageID ?? message?.info?.id;
  if (value === undefined || value === null || value === "") return undefined;
  return String(value);
}

function visibleMessages(messages: unknown) {
  return (Array.isArray(messages) ? messages : []).map((message: any) => ({
    role: message?.role === "user" || message?.role === "assistant" ? message.role : "",
    id: harnessId(message),
    text: conversationText(message),
    timestamp: harnessTimestamp(message),
  }));
}

function conversationSessionId(ctx: any): string {
  return String(ctx?.sessionID || ctx?.sessionId || OMP_SESSION_ID);
}

export async function logConversationWindow(
  binding: HostBinding,
  roomDir: string,
  ctx: any,
  messages: unknown,
  source: string,
  operator: string,
  spirit: string,
  persist: boolean,
): Promise<ConversationCapture> {
  const command = hostCommand(binding, CONVERSATION_LOG, "shell", {
    conversation_request: {
      roomDir,
      sessionId: conversationSessionId(ctx),
      operator,
      spirit,
      source,
      persist,
      messages: visibleMessages(messages),
    },
  });
  const response = await sendHostCommand(command, ACCEPTED);
  if (!response.result || typeof response.result !== "object") {
    throw new Error("Athanor Host conversation response omitted result");
  }
  return response.result as ConversationCapture;
}
