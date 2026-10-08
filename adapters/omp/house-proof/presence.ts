import { createHash } from "node:crypto";
import { hostCommand, sendHostCommand, type HostBinding, type HostResponse } from "./host.ts";
import { topLevelSession } from "./top-level-session-fence.ts";

export function responseDigest(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}

export async function settlePresence(
  binding: HostBinding,
  request: Record<string, unknown>,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<Record<string, any>> {
  requireTopLevel(binding);
  return resultValue(await sendHostCommand(
    hostCommand(binding, "athanor.presence.settle", "presence", { presence_settle: request }, idempotencyKey),
    new Set(["athanor.presence.settled"]),
    signal,
  ), "settle");
}

export async function closePresence(
  binding: HostBinding,
  request: Record<string, unknown>,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<Record<string, any>> {
  requireTopLevel(binding);
  return resultValue(await sendHostCommand(
    hostCommand(binding, "athanor.presence.close", "presence", { presence_close: request }, idempotencyKey),
    new Set(["athanor.presence.closed"]),
    signal,
  ), "close");
}

function requireTopLevel(binding: HostBinding): void {
  if (topLevelSession(binding.room) !== binding.session) {
    throw new Error("Presence requires the authenticated top-level OMP session");
  }
}

function resultValue(response: HostResponse, operation: string): Record<string, any> {
  const result = response.result;
  if (!result || typeof result !== "object" || Array.isArray(result)) {
    throw new Error(`Presence ${operation} returned no typed result`);
  }
  const typed = result as Record<string, any>;
  if (typed.operation !== operation || !typed.value || typeof typed.value !== "object") {
    throw new Error(`Presence ${operation} returned the wrong operation`);
  }
  return typed.value;
}
