import { createHash } from "node:crypto";

import { HostUnavailable, hostCommand, sendHostCommand, type HostBinding, type HostResponse } from "./host.ts";
import { topLevelSession } from "./top-level-session-fence.ts";

export const PRESENCE_PROJECTION_ID = "presence";
export const PRESENCE_OPEN = "athanor.presence.open";
export const PRESENCE_COMPILE = "athanor.presence.compile";
export const PRESENCE_SETTLE = "athanor.presence.settle";
export const PRESENCE_CLOSE = "athanor.presence.close";
export const PRESENCE_OPENED = "athanor.presence.opened";
export const PRESENCE_COMPILED = "athanor.presence.compiled";
export const PRESENCE_SETTLED = "athanor.presence.settled";
export const PRESENCE_CLOSED = "athanor.presence.closed";

export type PresenceMaterial = {
  id: string;
  authority: Record<string, unknown>;
  role: string;
  body: string;
  salience: number;
};

export type PresenceDirective = {
  id: string;
  kind: "enact" | "avoid" | "guard";
  severity: "hard" | "repair" | "advisory";
  instruction: string;
  sourceIds: string[];
  triggerScope?: string[];
};

// enough: there is no PresenceLedger on the wire any more. The Host owns the
// session's ledger and injects it into the pure functions itself, so a client
// asserts only which frame version it believes it is talking to.

export type PresenceOpenInput = {
  binding: { room: string; spirit: string; operator: string; session: string };
  identity: PresenceMaterial[];
  relationship?: PresenceMaterial[];
  continuity?: PresenceMaterial[];
  anamnesis?: PresenceMaterial[];
  previousBoat?: PresenceMaterial | null;
  uncertainties?: string[];
};

export type PresenceCompileInput = {
  frameId: string;
  turnId: string;
  userText: string;
  recalled?: PresenceMaterial[];
  lessons?: PresenceMaterial[];
  directives?: PresenceDirective[];
  frameVersion: number;
};
export type PresenceContextInput = {
  binding: HostBinding;
  operator: string;
  prompt: string;
  turnId: string;
  roomReminder?: string | null;
  priorFrameId?: string;
  priorFrameRendered?: string;
  previousBoat?: PresenceMaterial | null;
  relationship?: PresenceMaterial[];
  anamnesis?: PresenceMaterial[];
  recalled?: PresenceMaterial[];
  lessons?: PresenceMaterial[];
  openRetry?: { delaysMs?: number[]; deadlineMs?: number };
};

// enough: on a House restart the keeper launches OMP and the substrate
// together, so the session's first presence open races the room Hosts binding
// their ports. That first turn is the wake turn — the one carrying the paper
// boat and Anamnesis into the frame — and it used to degrade on a single
// connection refusal. The open is idempotent (presence-open:<session>), so a
// startup-shaped HostUnavailable is retried on a short bounded schedule and a
// hard deadline. Mid-session commands keep their single attempt: they already
// self-heal on the next turn.
export const PRESENCE_OPEN_RETRY_DELAYS_MS = [500, 1000, 2000, 3000];
export const PRESENCE_OPEN_RETRY_DEADLINE_MS = 15_000;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export async function compilePresenceContext(input: PresenceContextInput) {
  const recalled = input.recalled ?? [];
  const lessons = input.lessons ?? [];
  const frame = await resolveFrame(input, recalled);
  const contract = await compilePresence(
    input.binding,
    {
      frameId: frame.id,
      turnId: input.turnId,
      userText: input.prompt,
      recalled,
      lessons,
      directives: presenceDirectives(input, lessons),
      frameVersion: frame.version,
    },
    `presence-compile:${input.turnId}`,
  );
  const contractId = requiredResultId(contract.contractId, "compile", "contract");
  return {
    frameId: frame.id,
    frameRendered: frame.rendered,
    frameVersion: frame.version,
    contractId,
    turnId: input.turnId,
    directiveIds: hardDirectiveIds(contract),
    nonemptyGuardId: nonemptyGuardId(contract),
    rendered: [frame.rendered, String(contract.rendered ?? "")].filter(Boolean).join("\n\n"),
  };
}

async function resolveFrame(
  input: PresenceContextInput,
  recalled: PresenceMaterial[],
): Promise<{ id: string; rendered: string; version: number }> {
  if (input.priorFrameId) {
    return { id: input.priorFrameId, rendered: input.priorFrameRendered ?? "", version: 1 };
  }
  const delays = input.openRetry?.delaysMs ?? PRESENCE_OPEN_RETRY_DELAYS_MS;
  const deadline = Date.now() + (input.openRetry?.deadlineMs ?? PRESENCE_OPEN_RETRY_DEADLINE_MS);
  const request: PresenceOpenInput = {
    binding: {
      room: input.binding.room,
      spirit: input.binding.spirit,
      operator: input.operator,
      session: input.binding.session,
    },
    identity: [
      identityMaterial(input),
      ...recalled.filter((material) => material.role === "identity"),
    ],
    relationship: input.relationship ?? [],
    continuity: recalled.filter((material) => material.role !== "identity"),
    anamnesis: input.anamnesis ?? [],
    previousBoat: input.previousBoat ?? null,
    uncertainties: [],
  };
  const idempotencyKey = `presence-open:${input.binding.session}`;
  let opened: Record<string, any>;
  for (let attempt = 0; ; attempt += 1) {
    try {
      opened = await openPresence(input.binding, request, idempotencyKey);
      break;
    } catch (error) {
      const delay = delays[attempt];
      if (
        !(error instanceof HostUnavailable)
        || delay === undefined
        || Date.now() + delay > deadline
      ) {
        throw error;
      }
      console.warn(
        `[athanor] Presence open retrying in ${delay}ms (attempt ${attempt + 1}): ${error.message}`,
      );
      await sleep(delay);
    }
  }
  return {
    id: requiredResultId(opened.frameId, "open", "frame"),
    rendered: String(opened.rendered ?? ""),
    version: Number(opened.version ?? 1),
  };
}

function identityMaterial(input: PresenceContextInput): PresenceMaterial {
  const body = (
    input.roomReminder
    || `Active spirit: ${input.binding.spirit}. Operator: ${input.operator}. Room: ${input.binding.room}.`
  ).slice(0, 4096);
  return {
    id: "identity:active-spirit",
    authority: {
      kind: "identity",
      source: "active_spirit.md",
      sha256: responseDigest(body),
    },
    role: "identity",
    body,
    salience: 1000,
  };
}

function presenceDirectives(
  input: PresenceContextInput,
  lessons: PresenceMaterial[],
): PresenceDirective[] {
  return [
    {
      id: "presence:active-spirit",
      kind: "enact",
      severity: "advisory",
      instruction: `Remain ${input.binding.spirit}; meet ${input.operator} directly and preserve cited uncertainty.`,
      sourceIds: ["identity:active-spirit"],
      triggerScope: ["text"],
    },
    {
      id: PRESENCE_NONEMPTY_GUARD_ID,
      kind: "guard",
      severity: "hard",
      instruction: "The response must contain text.",
      sourceIds: ["identity:active-spirit"],
      triggerScope: ["text"],
    },
    ...lessons.slice(0, 8).map((lesson) => ({
      id: `presence:${lesson.id}`,
      kind: "enact" as const,
      severity: "advisory" as const,
      instruction: lesson.body.slice(0, 1000),
      sourceIds: [lesson.id],
      triggerScope: ["text"],
    })),
  ];
}

export const PRESENCE_NONEMPTY_GUARD_ID = "presence:nonempty-response";

// enough: acceptance has to answer for every hard directive the Host issued,
// across mustEnact, mustAvoid, and guards. Reading back only the guard group
// left a hard enact or avoid rule unevaluated while the settlement still said
// Accept, which is a receipt that means nothing.
function hardDirectiveIds(contract: Record<string, any>): string[] {
  const groups = ["mustEnact", "mustAvoid", "guards"];
  const ids = groups
    .flatMap((group) => (Array.isArray(contract[group]) ? contract[group] : []))
    .filter((directive: any) => directive?.severity === "hard" && directive?.id)
    .map((directive: any) => String(directive.id));
  return [...new Set(ids)];
}

function nonemptyGuardId(contract: Record<string, any>): string | null {
  const guard = (Array.isArray(contract.guards) ? contract.guards : []).find((directive: any) =>
    directive?.id === PRESENCE_NONEMPTY_GUARD_ID && directive?.severity === "hard"
  );
  return guard ? String(guard.id) : null;
}

function requiredResultId(value: unknown, operation: string, kind: string): string {
  const id = String(value ?? "");
  if (!id) throw new Error(`Presence ${operation} returned no ${kind} ID`);
  return id;
}

export function responseDigest(text: string): string {
  return createHash("sha256").update(text).digest("hex");
}

export async function openPresence(
  binding: HostBinding,
  request: PresenceOpenInput,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<Record<string, any>> {
  requireTopLevel(binding);
  return resultValue(await sendHostCommand(
    hostCommand(binding, PRESENCE_OPEN, PRESENCE_PROJECTION_ID, { presence_open: request }, idempotencyKey),
    new Set([PRESENCE_OPENED]),
    signal,
  ), "open");
}

export async function compilePresence(
  binding: HostBinding,
  request: PresenceCompileInput,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<Record<string, any>> {
  requireTopLevel(binding);
  return resultValue(await sendHostCommand(
    hostCommand(binding, PRESENCE_COMPILE, PRESENCE_PROJECTION_ID, { presence_compile: request }, idempotencyKey),
    new Set([PRESENCE_COMPILED]),
    signal,
  ), "compile");
}

export async function settlePresence(
  binding: HostBinding,
  request: Record<string, unknown>,
  idempotencyKey: string,
  signal?: AbortSignal,
): Promise<Record<string, any>> {
  requireTopLevel(binding);
  return resultValue(await sendHostCommand(
    hostCommand(binding, PRESENCE_SETTLE, PRESENCE_PROJECTION_ID, { presence_settle: request }, idempotencyKey),
    new Set([PRESENCE_SETTLED]),
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
    hostCommand(binding, PRESENCE_CLOSE, PRESENCE_PROJECTION_ID, { presence_close: request }, idempotencyKey),
    new Set([PRESENCE_CLOSED]),
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
