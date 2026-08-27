import { createHash } from "node:crypto";

import { hostCommand, sendHostCommand, type HostBinding, type HostResponse } from "./host.ts";
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

export type PresenceLedger = {
  recentRegisters?: string[];
  formsOfAddress?: string[];
  repairRuleIds?: string[];
  unresolvedThreads?: string[];
  relationshipClaims?: PresenceMaterial[];
  frameVersion: number;
  contractVersion: number;
};

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
  sessionLedger: PresenceLedger;
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
  anamnesis?: PresenceMaterial[];
  recalled?: PresenceMaterial[];
  lessons?: PresenceMaterial[];
};

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
      sessionLedger: { frameVersion: 1, contractVersion: 1 },
    },
    `presence-compile:${input.turnId}`,
  );
  const contractId = requiredResultId(contract.contractId, "compile", "contract");
  return {
    frameId: frame.id,
    frameRendered: frame.rendered,
    frameVersion: Number(contract.version ?? 1),
    contractId,
    turnId: input.turnId,
    directiveIds: contractDirectiveIds(contract),
    rendered: [frame.rendered, String(contract.rendered ?? "")].filter(Boolean).join("\n\n"),
  };
}

async function resolveFrame(
  input: PresenceContextInput,
  recalled: PresenceMaterial[],
): Promise<{ id: string; rendered: string }> {
  if (input.priorFrameId) {
    return { id: input.priorFrameId, rendered: input.priorFrameRendered ?? "" };
  }
  const opened = await openPresence(
    input.binding,
    {
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
      relationship: [],
      continuity: recalled.filter((material) => material.role !== "identity"),
      anamnesis: input.anamnesis ?? [],
      previousBoat: input.previousBoat ?? null,
      uncertainties: [],
    },
    `presence-open:${input.binding.session}`,
  );
  return {
    id: requiredResultId(opened.frameId, "open", "frame"),
    rendered: String(opened.rendered ?? ""),
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
      id: "presence:nonempty-response",
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

function contractDirectiveIds(contract: Record<string, any>): string[] {
  return (Array.isArray(contract.guards) ? contract.guards : [])
    .filter((directive: any) =>
      directive?.id === "presence:nonempty-response"
      && directive?.severity === "hard"
    )
    .map((directive: any) => directive.id);
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
