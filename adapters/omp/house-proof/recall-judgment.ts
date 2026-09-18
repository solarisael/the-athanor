import { randomUUID } from "node:crypto";
import { readFile } from "node:fs/promises";

export type RecallRerankMode = "off" | "shadow" | "active";

export type RecallRerankPolicy = Readonly<{
  mode: RecallRerankMode;
  approved: boolean;
  revision?: string;
  room?: string;
}>;

export type RecallRerankInput = {
  query: string;
  retrievalCandidates: readonly any[];
  rerankCandidates?: readonly any[];
  signal?: AbortSignal;
  deadline?: number;
  sessionId?: string;
  context?: any;
};

export type RecallRerankResult = {
  retrievalCandidates: readonly any[];
  receipt: Readonly<Record<string, unknown>>;
};

export type RecallRerankerOptions = {
  fetch?: typeof globalThis.fetch;
  context?: any;
  now?: () => number;
  readFile?: typeof readFile;
};

const ENDPOINT = "https://api.typesafe.ai/v1/systemone";
const MODEL = "jev-latest";
const FIXED_TEXT = "Estimate the probability that this card is relevant to the query.";
const MAX_PACKET_BYTES = 32768;
const MAX_RESPONSE_BYTES = 65536;
const MAX_CARDS = 48;
const MAX_CARD_CODEPOINTS = 384;
const VALID_MODEL = /^(?=[A-Za-z0-9._:/-]{1,64}$)(?=.*jev)[A-Za-z0-9._:/-]+$/i;

const toText = (value: unknown): string =>
  typeof value === "string" ? value : "";

const containsSecret = (value: string): boolean =>
  /(?:sk-[A-Za-z0-9_-]{16,}|(?:api[_-]?key|password|secret|access[_-]?token|authorization|cookie)\s*["']?\s*[:=]|-----BEGIN|bearer\s+[A-Za-z0-9._-]{16,})/i.test(value);

const clipText = (value: string, length: number): string =>
  Array.from(value).slice(0, length).join("");

const getExcerpt = (candidate: any): string =>
  toText(candidate?.excerpt ?? candidate?.text ?? candidate?.body ?? candidate?.content);

type RecallCard = {
  source: any;
  i: number;
  full: string;
  token: string;
  text: string;
};

const buildPacket = (query: string, cards: readonly RecallCard[]) => {
  const state = {
    schemaVersion: "jev-recall.v1",
    query: clipText(query, 256),
    cards: cards.map(card => ({ token: card.token, text: card.text })),
  };
  const questions = Object.fromEntries(
    cards.map(card => [card.token, { type: "noul", instructions: FIXED_TEXT }]),
  );

  return { state, model: MODEL, questions };
};

const parseValidResponse = (raw: string, cards: readonly RecallCard[]) => {
  let parsed: any;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { parsed: null, reason: "malformed" as const };
  }

  const answers = parsed?.answers;
  const keys =
    answers && typeof answers === "object" && !Array.isArray(answers)
      ? Object.keys(answers)
      : [];
  const invalidAnswers =
    keys.length !== cards.length ||
    keys.some(key => !cards.some(card => card.token === key)) ||
    cards.some(card => {
      const answer = answers?.[card.token];
      return (
        !answer ||
        Object.keys(answer).length !== 2 ||
        answer.type !== "noul" ||
        typeof answer.noul !== "number" ||
        !Number.isFinite(answer.noul) ||
        answer.noul < 0 ||
        answer.noul > 1
      );
    });

  if (typeof parsed?.model !== "string" || !VALID_MODEL.test(parsed.model)) {
    return { parsed: null, reason: "model-mismatch" as const };
  }
  if (invalidAnswers) {
    return { parsed: null, reason: "invalid-response" as const };
  }
  return { parsed, reason: undefined };
};

export function createRecallReranker(options: RecallRerankerOptions = {}) {
  const fetcher = options.fetch ?? globalThis.fetch;
  const now = options.now ?? Date.now;
  const read = options.readFile ?? readFile;
  let policy: RecallRerankPolicy = { mode: "off", approved: false };
  const coverage: Record<string, number> = {
    turns: 0,
    approved: 0,
    disabled: 0,
    refused: 0,
    unavailable: 0,
    failed: 0,
    shadow: 0,
    active: 0,
    selected: 0,
  };
  let lastReceipt: Readonly<Record<string, unknown>> | null = null;

  async function loadPolicy(
    dir: string,
    room: string,
    supplied?: any,
  ): Promise<RecallRerankPolicy> {
    let marker: any;
    try {
      marker = JSON.parse(await read(`${dir}/.athanor-room.json`, "utf8"));
    } catch {
      marker = supplied;
    }

    const revision = marker?.jevRecall?.grant?.policyRevision;
    const valid =
      marker?.room === room &&
      (marker.jevRecall?.mode === "shadow" ||
        marker.jevRecall?.mode === "active") &&
      marker.jevRecall?.provider === "typesafe" &&
      marker.jevRecall?.grant?.purpose === "recall-rerank" &&
      marker.jevRecall?.grant?.allowPrivateRecallPackets === true &&
      typeof revision === "string" &&
      revision.length > 0;

    policy = valid
      ? { mode: marker.jevRecall.mode, approved: true, revision, room }
      : { mode: "off", approved: false };
    return policy;
  }

  async function rerank(input: RecallRerankInput): Promise<RecallRerankResult> {
    coverage.turns++;
    const baseline = input.retrievalCandidates;
    const policyAtStart = policy;

    const finish = (
      status: string,
      reason?: string,
      extra: Record<string, unknown> = {},
    ): RecallRerankResult => {
      const receipt = {
        schemaVersion: "jev-recall-receipt.v1",
        status,
        ...(reason ? { reason } : {}),
        provider: "typesafe",
        model: MODEL,
        policyRevision: policyAtStart.revision ?? "",
        fallbackUsed: status !== "active",
        ...extra,
      };
      lastReceipt = receipt;
      return { retrievalCandidates: baseline, receipt };
    };

    if (!policyAtStart.approved) {
      coverage.disabled++;
      return finish("disabled", "not-approved");
    }

    coverage.approved++;
    const started = now();
    const deadline = input.deadline ?? started + 5000;
    const controller = new AbortController();
    let timer: ReturnType<typeof setTimeout> | undefined;
    let rejectDeadline: ((reason: Error) => void) | undefined;
    const onAbort = () => {
      controller.abort();
      rejectDeadline?.(new Error("deadline"));
    };
    input.signal?.addEventListener("abort", onAbort, { once: true });

    const fail = (reason: string, kind = "refused") => {
      coverage[kind] = (coverage[kind] ?? 0) + 1;
      return finish(kind, reason, {
        latencyMs: Math.max(0, now() - started),
      });
    };

    try {
      if (deadline <= now() || input.signal?.aborted) {
        return fail("deadline");
      }

      const query = toText(input.query);
      const pool = input.rerankCandidates ?? baseline;
      if (!query || containsSecret(query)) {
        return fail("privacy-refused");
      }
      if (!pool.length) {
        return fail("empty-pool");
      }

      const candidates = pool.map((source, i) => ({
        source,
        i,
        full: getExcerpt(source),
      }));
      if (candidates.some(candidate => !candidate.full || containsSecret(candidate.full))) {
        return fail("privacy-refused");
      }

      const cards = candidates
        .slice(0, MAX_CARDS)
        .map(candidate => ({
          ...candidate,
          token: `c${candidate.i.toString(36)}_${randomUUID()}`,
          text: clipText(candidate.full, MAX_CARD_CODEPOINTS),
        }));
      const body = buildPacket(query, cards);
      const serializedBody = JSON.stringify(body);
      if (new TextEncoder().encode(serializedBody).byteLength > MAX_PACKET_BYTES) {
        return fail("oversize");
      }

      const deadlineFailure = new Promise<never>((_resolve, reject) => {
        rejectDeadline = reject;
        timer = setTimeout(() => {
          controller.abort();
          reject(new Error("deadline"));
        }, Math.max(1, deadline - now()));
      });
      const authRequest = (
        (input.context ?? options.context)?.modelRegistry?.authStorage?.getApiKey?.(
          "typesafe",
          input.sessionId,
          { signal: controller.signal },
        )
      );
      const auth = await Promise.race([
        Promise.resolve(authRequest),
        deadlineFailure,
      ]);
      if (!auth) {
        return fail("credential-unavailable", "unavailable");
      }
      if (deadline <= now()) {
        return fail("deadline");
      }

      const response = await Promise.race([
        fetcher(ENDPOINT, {
          method: "POST",
          redirect: "error",
          signal: controller.signal,
          headers: {
            accept: "application/json",
            "content-type": "application/json",
            authorization: `Bearer ${auth}`,
          },
          body: serializedBody,
        }),
        deadlineFailure,
      ]);
      if (!response.ok) {
        return fail("non-ok", "failed");
      }

      const raw = await Promise.race([response.text(), deadlineFailure]);
      if (new TextEncoder().encode(raw).byteLength > MAX_RESPONSE_BYTES) {
        return fail("oversize", "failed");
      }
      const validation = parseValidResponse(raw, cards);
      if (!validation.parsed) {
        return fail(validation.reason ?? "invalid-response", "failed");
      }

      const parsed = validation.parsed;

      const selected = cards
        .filter(card => parsed.answers[card.token].noul >= 0.5)
        .sort(
          (left, right) =>
            parsed.answers[right.token].noul - parsed.answers[left.token].noul ||
            left.i - right.i,
        )
        .slice(0, 8);
      coverage.selected += selected.length;

      const status = policyAtStart.mode;
      coverage[status]++;
      const receipt = {
        ...finish(status, undefined, {
          latencyMs: Math.max(0, now() - started),
          scored: cards.length,
          selected: selected.length,
          model: parsed.model,
        }).receipt,
      };
      const exact = baseline.filter(candidate => candidate?.source === "exact_id");
      const winners = selected
        .map(card => card.source)
        .filter(candidate => !exact.includes(candidate));

      return status === "shadow"
        ? { retrievalCandidates: baseline, receipt }
        : { retrievalCandidates: [...exact, ...winners], receipt };
    } catch (error) {
      const timedOut =
        input.signal?.aborted ||
        controller.signal.aborted ||
        deadline <= now();
      return fail(
        timedOut ? "deadline" : "backend-failed",
        timedOut ? "refused" : "failed",
      );
    } finally {
      if (timer) {
        clearTimeout(timer);
      }
      input.signal?.removeEventListener("abort", onAbort);
    }
  }

  return {
    loadPolicy,
    rerank,
    getCoverage: () => ({ ...coverage, last: lastReceipt }),
  };
}
