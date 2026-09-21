import { readFile } from "node:fs/promises";

// Judges the previous assistant turn by the operator's reply, and the recall
// injected for that turn on two axes: fit (did Recall surface what the
// operator's message needed) and use (did the assistant draw on it). Two axes
// because one label lied in two of four cells: a perfect recall the assistant
// ignored read as Recall's miss, a bad recall the assistant parroted read as a
// hit. Results feed Insula only; nothing here touches context. The provider is
// interchangeable: typesafe (hosted Jev) and laya (loopback sidecar) speak the
// same state + questions shape, so switching is a marker edit.

export type VerdictProvider = "typesafe" | "laya";
export type TurnVerdict = "corrected" | "continued" | "gold";
export type RecallFit = "relevant" | "unneeded" | "wrong";
export type RecallUse = "used" | "ignored" | "misled";
export type RecallVerdict = { fit: RecallFit; use: RecallUse };

export type VerdictPolicy =
  | { mode: "off"; approved: false }
  | {
    mode: "active";
    approved: true;
    provider: VerdictProvider;
    endpoint: string;
    revision: string;
    room: string;
  };

export type VerdictInput = {
  /** The operator's message the judged turn answered; the one Recall ran for. */
  operatorMessage: string;
  operatorReply: string;
  assistantTurn: string;
  /** Titles of what Recall injected for that turn; empty or absent skips the recall questions. */
  recallTitles?: readonly string[];
  sessionId?: string;
  signal?: AbortSignal;
  deadline?: number;
};

export type VerdictResult =
  | {
    status: "scored";
    provider: VerdictProvider;
    model: string;
    turn: TurnVerdict;
    recall: RecallVerdict | null;
    latencyMs: number;
    packetBytes: number;
  }
  | {
    status: "disabled" | "refused" | "unavailable" | "failed";
    provider: VerdictProvider | null;
    reason: string;
    latencyMs: number;
  };

export type VerdictScorerOptions = {
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

// enough: clips sized to the Laya English root (~480 state tokens across the
// four strings). Raise them together with the sidecar's head_max_len, never
// one side alone.
const MESSAGE_CHARS = 300;
const REPLY_CHARS = 400;
const ASSISTANT_TAIL_CHARS = 600;
const RECALL_TITLES_CHARS = 600;

const TURN_INSTRUCTIONS =
  "An AI assistant answered the operator's message, then the operator replied. " +
  "Judge the assistant's turn by how the operator reacts to it.";
const RECALL_FIT_INSTRUCTIONS =
  "A memory system retrieved the listed memories for the operator's message, before the assistant answered. " +
  "Judge the retrieval, not the assistant: did it surface what that message needed?";
const RECALL_USE_INSTRUCTIONS =
  "The assistant answered with the listed memories in its context. " +
  "Judge the assistant, not the retrieval: did its turn draw on them, and to what effect?";

const TURN_CRITERIA: Record<TurnVerdict, string> = {
  corrected: "The operator corrects, disagrees, re-explains, or says the assistant got it wrong",
  continued: "The operator builds on the turn or moves on; no correction",
  gold: "The operator explicitly praises or thanks the assistant for that turn",
};

const RECALL_FIT_CRITERIA: Record<RecallFit, string> = {
  relevant: "The memories fit the operator's message; a good reply would draw on them",
  unneeded: "The message did not need memories; the retrieval was harmless noise",
  wrong: "The memories miss what the message was about; they point somewhere else",
};

const RECALL_USE_CRITERIA: Record<RecallUse, string> = {
  used: "The assistant's turn visibly draws on the recalled memories",
  ignored: "The turn does not draw on them",
  misled: "The turn followed the recalled memories into a wrong direction",
};

const containsSecret = (value: string): boolean =>
  /(?:sk-[A-Za-z0-9_-]{16,}|(?:api[_-]?key|password|secret|access[_-]?token|authorization|cookie)\s*["']?\s*[:=]|-----BEGIN|bearer\s+[A-Za-z0-9._-]{16,})/i.test(value);

const head = (value: string, length: number): string =>
  Array.from(value).slice(0, length).join("");

const tail = (value: string, length: number): string =>
  Array.from(value).slice(-length).join("");

export function buildVerdictPacket(input: VerdictInput, provider: VerdictProvider) {
  const titles = (input.recallTitles ?? []).map(title => String(title ?? "").trim()).filter(Boolean);
  const recallList = head(titles.map(title => `- ${title}`).join("\n"), RECALL_TITLES_CHARS);
  const operatorMessage = head(input.operatorMessage.trim(), MESSAGE_CHARS);
  const state: Record<string, string> = {
    schemaVersion: "jev-verdict.v2",
    assistantTurn: tail(input.assistantTurn.trim(), ASSISTANT_TAIL_CHARS),
    operatorReply: head(input.operatorReply.trim(), REPLY_CHARS),
  };
  if (operatorMessage) state.operatorMessage = operatorMessage;
  if (recallList) state.recalledMemories = recallList;

  const questions: Record<string, unknown> = {
    turn: { type: "choice", instructions: TURN_INSTRUCTIONS, criteria: TURN_CRITERIA },
  };
  if (recallList) {
    questions.recallFit = { type: "choice", instructions: RECALL_FIT_INSTRUCTIONS, criteria: RECALL_FIT_CRITERIA };
    questions.recallUse = { type: "choice", instructions: RECALL_USE_INSTRUCTIONS, criteria: RECALL_USE_CRITERIA };
  }

  return provider === "typesafe"
    ? { state, model: TYPESAFE_MODEL, questions }
    : { state, questions };
}

/** Recall titles out of one `athanor-recall-context` block; null when it is not one. */
export function recallTitlesFromWorkingSet(content: unknown): string[] | null {
  if (typeof content !== "string") return null;
  const start = content.indexOf("{");
  const end = content.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  let parsed: any;
  try {
    parsed = JSON.parse(content.slice(start, end + 1));
  } catch {
    return null;
  }
  const titles: string[] = [];
  for (const match of Array.isArray(parsed?.canonMatches) ? parsed.canonMatches : []) {
    if (typeof match?.termKey === "string") titles.push(`canon: ${match.termKey}`);
  }
  for (const candidate of Array.isArray(parsed?.retrievalCandidates) ? parsed.retrievalCandidates : []) {
    if (typeof candidate?.title === "string") titles.push(candidate.title);
  }
  return titles;
}

function parseVerdictResponse(
  raw: string,
  askedRecall: boolean,
): { model: string; turn: TurnVerdict; recall: RecallVerdict | null } | { reason: string } {
  let parsed: any;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { reason: "malformed" };
  }
  const model = parsed?.model;
  if (typeof model !== "string" || !model || model.length > 64) return { reason: "model_mismatch" };

  const answers = parsed?.answers;
  const choice = <T extends string>(answer: any, criteria: Record<T, string>): T | null =>
    answer?.type === "choice" && Object.hasOwn(criteria, String(answer.choice)) ? answer.choice : null;

  const turn = choice(answers?.turn, TURN_CRITERIA);
  if (!turn) return { reason: "invalid_response" };

  if (!askedRecall) {
    if (answers.recallFit !== undefined || answers.recallUse !== undefined) return { reason: "invalid_response" };
    return { model, turn, recall: null };
  }
  const fit = choice(answers?.recallFit, RECALL_FIT_CRITERIA);
  const use = choice(answers?.recallUse, RECALL_USE_CRITERIA);
  if (!fit || !use) return { reason: "invalid_response" };
  return { model, turn, recall: { fit, use } };
}

export function createVerdictScorer(options: VerdictScorerOptions = {}) {
  const fetcher = options.fetch ?? globalThis.fetch;
  const now = options.now ?? Date.now;
  const read = options.readFile ?? readFile;
  let policy: VerdictPolicy = { mode: "off", approved: false };

  // Same marker file and grant discipline as the recall reranker: the operator
  // turns this on per room, in writing, with a revision they can revoke.
  async function loadPolicy(dir: string, room: string): Promise<VerdictPolicy> {
    let marker: any;
    try {
      marker = JSON.parse(await read(`${dir}/.athanor-room.json`, "utf8"));
    } catch {
      marker = null;
    }
    const verdict = marker?.jevVerdict;
    const revision = verdict?.grant?.policyRevision;
    const provider: VerdictProvider | null =
      verdict?.provider === "typesafe" || verdict?.provider === "laya" ? verdict.provider : null;
    const endpoint = provider === "typesafe"
      ? TYPESAFE_ENDPOINT
      : typeof verdict?.endpoint === "string" && LOOPBACK_ENDPOINT.test(verdict.endpoint)
        ? verdict.endpoint
        : null;
    const valid =
      marker?.room === room &&
      verdict?.mode === "active" &&
      provider !== null &&
      endpoint !== null &&
      verdict?.grant?.purpose === "turn-verdict" &&
      verdict?.grant?.allowPrivateConversationPackets === true &&
      typeof revision === "string" &&
      revision.length > 0;

    policy = valid
      ? { mode: "active", approved: true, provider: provider!, endpoint: endpoint!, revision, room }
      : { mode: "off", approved: false };
    return policy;
  }

  async function score(input: VerdictInput): Promise<VerdictResult> {
    const active = policy;
    const started = now();
    const fail = (
      status: "refused" | "unavailable" | "failed",
      reason: string,
      provider: VerdictProvider | null,
    ): VerdictResult => ({ status, provider, reason, latencyMs: Math.max(0, now() - started) });

    if (!active.approved) return { status: "disabled", provider: null, reason: "not_approved", latencyMs: 0 };
    if (!input.operatorReply.trim() || !input.assistantTurn.trim()) {
      return fail("refused", "empty_turn", active.provider);
    }

    const packet = buildVerdictPacket(input, active.provider);
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
      const parsed = parseVerdictResponse(raw, "recallFit" in packet.questions);
      if ("reason" in parsed) return fail("failed", parsed.reason, active.provider);

      return {
        status: "scored",
        provider: active.provider,
        model: parsed.model,
        turn: parsed.turn,
        recall: parsed.recall,
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
