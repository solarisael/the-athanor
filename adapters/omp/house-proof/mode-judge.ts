import { readFile } from "node:fs/promises";

// Picks the retrieval mode for the operator's next message by reading the turn
// that just closed. Sibling of turn-verdict.ts: same marker, same grant
// discipline, same two providers, one question instead of three. The verdict
// judges what already happened; this judges what to fetch next, so its result
// leaves the adapter twice — once to the Host as a proposal the policy may
// consume, once to Insula as a measurement. Neither touches context directly.
//
// enough: the guards below (secret shape, clips, caps) are copied from
// turn-verdict rather than shared. Two small judges, one owner each; fold them
// into a common packet module when a third arrives.

export type ModeProvider = "typesafe" | "laya";
/** Never `quiet`: silence is the operator's to ask for, not a judge's to infer. */
export type JudgedMode = "conversation" | "work" | "mixed";

export type ModePolicy =
  | { mode: "off"; approved: false }
  | {
    mode: "active";
    approved: true;
    provider: ModeProvider;
    endpoint: string;
    revision: string;
    room: string;
  };

export type ModeInput = {
  /** The operator's message the judged turn answered. */
  operatorMessage: string;
  assistantTurn: string;
  /** The operator's newest message, the one the next turn answers; the strongest signal for its mode. */
  operatorReply: string;
  sessionId?: string;
  signal?: AbortSignal;
  deadline?: number;
};

export type ModeResult =
  | {
    status: "scored";
    provider: ModeProvider;
    model: string;
    mode: JudgedMode;
    latencyMs: number;
    packetBytes: number;
  }
  | {
    status: "disabled" | "refused" | "unavailable" | "failed";
    provider: ModeProvider | null;
    reason: string;
    latencyMs: number;
  };

export type ModeScorerOptions = {
  fetch?: typeof fetch;
  now?: () => number;
  readFile?: (path: string, encoding: "utf8") => Promise<string>;
  /** OMP extension context; the typesafe key comes from its auth storage. */
  context?: { modelRegistry?: any };
};

const TYPESAFE_ENDPOINT = "https://api.typesafe.ai/v1/systemone";
const TYPESAFE_MODEL = "jev-latest";
const LOOPBACK_ENDPOINT = /^http:\/\/(127\.0\.0\.1|localhost|\[::1\])(:\d{1,5})?\/\S*$/;
const MAX_PACKET_BYTES = 32768;
const MAX_RESPONSE_BYTES = 65536;
const DEFAULT_DEADLINE_MS = 5000;

// enough: same clips as the verdict's matching fields, so both judges read
// the same turn. Raise them together with the sidecar's head_max_len.
const MESSAGE_CHARS = 300;
const ASSISTANT_TAIL_CHARS = 600;
const REPLY_CHARS = 400;

const MODE_INSTRUCTIONS =
  "A memory system chooses which retrieval mode to use for the operator's newest message. " +
  "Judge the exchange below, weighing that newest message most, and pick the mode that would surface the right memories.";

const MODE_CRITERIA: Record<JudgedMode, string> = {
  conversation: "The exchange is about the operator, their people, their history, feelings, or the room's shared life",
  work: "The exchange is about a project, code, files, a build, a deploy, or a technical decision",
  mixed: "Both at once, or neither clearly",
};

const containsSecret = (value: string): boolean =>
  /(?:sk-[A-Za-z0-9_-]{16,}|(?:api[_-]?key|password|secret|access[_-]?token|authorization|cookie)\s*["']?\s*[:=]|-----BEGIN|bearer\s+[A-Za-z0-9._-]{16,})/i.test(value);

const head = (value: string, length: number): string =>
  Array.from(value).slice(0, length).join("");

const tail = (value: string, length: number): string =>
  Array.from(value).slice(-length).join("");

export function buildModePacket(input: ModeInput, provider: ModeProvider) {
  const operatorMessage = head(input.operatorMessage.trim(), MESSAGE_CHARS);
  const operatorReply = head(input.operatorReply.trim(), REPLY_CHARS);
  const state: Record<string, string> = {
    schemaVersion: "jev-mode.v1",
    assistantTurn: tail(input.assistantTurn.trim(), ASSISTANT_TAIL_CHARS),
  };
  if (operatorMessage) state.operatorMessage = operatorMessage;
  if (operatorReply) state.operatorReply = operatorReply;

  const questions: Record<string, unknown> = {
    mode: { type: "choice", instructions: MODE_INSTRUCTIONS, criteria: MODE_CRITERIA },
  };

  return provider === "typesafe"
    ? { state, model: TYPESAFE_MODEL, questions }
    : { state, questions };
}

function parseModeResponse(raw: string): { model: string; mode: JudgedMode } | { reason: string } {
  let parsed: any;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { reason: "malformed" };
  }
  const model = parsed?.model;
  if (typeof model !== "string" || !model || model.length > 64) return { reason: "model_mismatch" };

  const answer = parsed?.answers?.mode;
  const mode: JudgedMode | null =
    answer?.type === "choice" && Object.hasOwn(MODE_CRITERIA, String(answer.choice)) ? answer.choice : null;
  if (!mode) return { reason: "invalid_response" };
  return { model, mode };
}

export function createModeScorer(options: ModeScorerOptions = {}) {
  const fetcher = options.fetch ?? globalThis.fetch;
  const now = options.now ?? Date.now;
  const read = options.readFile ?? readFile;
  let policy: ModePolicy = { mode: "off", approved: false };

  // Its own grant beside the verdict's: approving a judge of the last turn is
  // not approving one that steers the next retrieval.
  async function loadPolicy(dir: string, room: string): Promise<ModePolicy> {
    let marker: any;
    try {
      marker = JSON.parse(await read(`${dir}/.athanor-room.json`, "utf8"));
    } catch {
      marker = null;
    }
    const judge = marker?.jevMode;
    const revision = judge?.grant?.policyRevision;
    const provider: ModeProvider | null =
      judge?.provider === "typesafe" || judge?.provider === "laya" ? judge.provider : null;
    const endpoint = provider === "typesafe"
      ? TYPESAFE_ENDPOINT
      : typeof judge?.endpoint === "string" && LOOPBACK_ENDPOINT.test(judge.endpoint)
        ? judge.endpoint
        : null;
    const valid =
      marker?.room === room &&
      judge?.mode === "active" &&
      provider !== null &&
      endpoint !== null &&
      judge?.grant?.purpose === "recall-mode" &&
      judge?.grant?.allowPrivateConversationPackets === true &&
      typeof revision === "string" &&
      revision.length > 0;

    policy = valid
      ? { mode: "active", approved: true, provider: provider!, endpoint: endpoint!, revision, room }
      : { mode: "off", approved: false };
    return policy;
  }

  async function score(input: ModeInput): Promise<ModeResult> {
    const active = policy;
    const started = now();
    const fail = (
      status: "refused" | "unavailable" | "failed",
      reason: string,
      provider: ModeProvider | null,
    ): ModeResult => ({ status, provider, reason, latencyMs: Math.max(0, now() - started) });

    if (!active.approved) return { status: "disabled", provider: null, reason: "not_approved", latencyMs: 0 };
    // The assistant turn is the one always-present state field; without it the
    // packet is a question about nothing.
    if (!input.assistantTurn.trim()) return fail("refused", "empty_turn", active.provider);

    const packet = buildModePacket(input, active.provider);
    const serialized = JSON.stringify(packet);
    if (containsSecret(serialized)) return fail("refused", "secret_like", active.provider);
    const packetBytes = new TextEncoder().encode(serialized).byteLength;
    if (packetBytes > MAX_PACKET_BYTES) return fail("refused", "oversize", active.provider);

    const deadline = input.deadline ?? started + DEFAULT_DEADLINE_MS;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let rejectDeadline: ((reason: Error) => void) | undefined;
    const onAbort = () => {
      controller.abort();
      rejectDeadline?.(new Error("deadline"));
    };
    input.signal?.addEventListener("abort", onAbort, { once: true });

    try {
      if (deadline <= now() || input.signal?.aborted) return fail("failed", "deadline", active.provider);
      const deadlineFailure = new Promise<never>((_resolve, reject) => {
        rejectDeadline = reject;
        timer = setTimeout(() => {
          controller.abort();
          reject(new Error("deadline"));
        }, Math.max(1, deadline - now()));
      });

      const headers: Record<string, string> = {
        accept: "application/json",
        "content-type": "application/json",
      };
      if (active.provider === "typesafe") {
        const auth = await Promise.race([
          Promise.resolve(options.context?.modelRegistry?.authStorage?.getApiKey?.(
            "typesafe",
            input.sessionId,
            { signal: controller.signal },
          )),
          deadlineFailure,
        ]);
        if (!auth) return fail("unavailable", "credential_unavailable", active.provider);
        headers.authorization = `Bearer ${auth}`;
      }

      const response = await Promise.race([
        fetcher(active.endpoint, {
          method: "POST",
          redirect: "error",
          signal: controller.signal,
          headers,
          body: serialized,
        }),
        deadlineFailure,
      ]);
      if (!response.ok) return fail("failed", "non_ok", active.provider);

      const raw = await Promise.race([response.text(), deadlineFailure]);
      if (new TextEncoder().encode(raw).byteLength > MAX_RESPONSE_BYTES) {
        return fail("failed", "oversize", active.provider);
      }
      const parsed = parseModeResponse(raw);
      if ("reason" in parsed) return fail("failed", parsed.reason, active.provider);

      return {
        status: "scored",
        provider: active.provider,
        model: parsed.model,
        mode: parsed.mode,
        latencyMs: Math.max(0, now() - started),
        packetBytes,
      };
    } catch (error) {
      const timedOut = input.signal?.aborted || controller.signal.aborted || deadline <= now();
      if (timedOut) return fail("failed", "deadline", active.provider);
      return fail("failed", error instanceof Error && error.name === "TypeError" ? "network" : "transport", active.provider);
    } finally {
      if (timer) clearTimeout(timer);
      input.signal?.removeEventListener("abort", onAbort);
    }
  }

  return { loadPolicy, score, get policy() { return policy; } };
}
