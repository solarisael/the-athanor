import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { createRecallReranker } from "../house-proof/recall-judgment.ts";

const roots: string[] = [];

afterEach(async () => {
  await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true })));
});

async function room(
  mode: "shadow" | "active" = "active",
  overrides: Record<string, unknown> = {},
): Promise<string> {
  const root = await mkdtemp(path.join(os.tmpdir(), "athanor-jev-recall-"));
  roots.push(root);
  await writeFile(path.join(root, ".athanor-room.json"), JSON.stringify({
    room: "test-room",
    jevRecall: {
      mode,
      provider: "typesafe",
      grant: {
        purpose: "recall-rerank",
        allowPrivateRecallPackets: true,
        policyRevision: "sol-2026-09-18-v1",
      },
    },
    ...overrides,
  }));
  return root;
}

function responseFor(
  request: RequestInit,
  probability: (index: number) => number = () => 0.8,
): Response {
  const body = JSON.parse(String(request.body));
  const tokens = Object.keys(body.questions);
  return new Response(JSON.stringify({
    model: "jev-latest",
    answers: Object.fromEntries(tokens.map((token, index) => [
      token,
      { type: "noul", noul: probability(index) },
    ])),
    usage: { input_tokens: 10, output_tokens: 2 },
  }), { status: 200, headers: { "content-type": "application/json" } });
}

function context(onAuth: () => void = () => undefined) {
  return {
    modelRegistry: {
      authStorage: {
        async getApiKey(provider: string) {
          onAuth();
          expect(provider).toBe("typesafe");
          return "test-key";
        },
      },
    },
  };
}

describe("Recall Jev reranker", () => {
  test("denies by default before credential or network access", async () => {
    let calls = 0;
    const reranker = createRecallReranker({
      context: context(() => calls++),
      fetch: (async () => {
        calls++;
        throw new Error("must not call");
      }) as typeof fetch,
    });
    const baseline = [{ memory_id: 1, excerpt: "private" }];

    const result = await reranker.rerank({
      query: "shoreline",
      retrievalCandidates: baseline,
    });

    expect(result.retrievalCandidates).toBe(baseline);
    expect(result.receipt).toMatchObject({
      status: "disabled",
      reason: "not-approved",
      fallbackUsed: true,
    });
    expect(calls).toBe(0);
    expect(reranker.getCoverage()).toMatchObject({
      disabled: 1,
      last: { status: "disabled" },
    });
  });

  test("requires a matching trusted room marker", async () => {
    const root = await room("active", { room: "another-room" });
    let calls = 0;
    const reranker = createRecallReranker({
      context: context(() => calls++),
      fetch: (async () => {
        calls++;
        throw new Error("must not call");
      }) as typeof fetch,
    });

    const policy = await reranker.loadPolicy(root, "test-room");
    const result = await reranker.rerank({
      query: "shoreline",
      retrievalCandidates: [{ excerpt: "candidate" }],
    });

    expect(policy).toEqual({ mode: "off", approved: false });
    expect(result.receipt.status).toBe("disabled");
    expect(calls).toBe(0);
  });

  test("sends only the approved System One projection in shadow mode", async () => {
    const root = await room("shadow");
    const order: string[] = [];
    let sentBody: any;
    const reranker = createRecallReranker({
      context: context(() => order.push("auth")),
      fetch: (async (url, init) => {
        order.push("fetch");
        expect(url).toBe("https://api.typesafe.ai/v1/systemone");
        expect(init?.redirect).toBe("error");
        expect(new Headers(init?.headers).get("authorization")).toBe("Bearer test-key");
        sentBody = JSON.parse(String(init?.body));
        return responseFor(init!);
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");
    const baseline = [{ memory_id: 7, source_path: "private/path", excerpt: "baseline" }];
    const longText = "relevant ".repeat(100);

    const result = await reranker.rerank({
      query: `${"q".repeat(300)} private tail`,
      retrievalCandidates: baseline,
      rerankCandidates: [{
        memory_id: 9,
        source_path: "db-only/private",
        title: "private title",
        excerpt: longText,
      }],
      sessionId: "session",
    });

    expect(order).toEqual(["auth", "fetch"]);
    expect(Object.keys(sentBody).sort()).toEqual(["model", "questions", "state"]);
    expect(sentBody.model).toBe("jev-latest");
    expect(Object.keys(sentBody.state).sort()).toEqual(["cards", "query", "schemaVersion"]);
    expect(Array.from(sentBody.state.query)).toHaveLength(256);
    expect(sentBody.state.cards).toHaveLength(1);
    expect(Object.keys(sentBody.state.cards[0]).sort()).toEqual(["text", "token"]);
    expect(Array.from(sentBody.state.cards[0].text)).toHaveLength(384);
    expect(JSON.stringify(sentBody)).not.toContain("memory_id");
    expect(JSON.stringify(sentBody)).not.toContain("private/path");
    expect(JSON.stringify(sentBody)).not.toContain("private title");
    expect(result.retrievalCandidates).toBe(baseline);
    expect(result.receipt).toMatchObject({
      provider: "typesafe",
      model: "jev-latest",
      scored: 1,
      selected: 1,
      fallbackUsed: true,
    });
  });

  test("active mode pins exact references and selects stable relevant winners", async () => {
    const root = await room("active");
    const baseline = [
      { memory_id: 90, source: "exact_id", excerpt: "exact" },
      { memory_id: 1, excerpt: "baseline" },
    ];
    const pool = [
      { memory_id: 10, excerpt: "first" },
      { memory_id: 11, excerpt: "second" },
      { memory_id: 12, excerpt: "third" },
    ];
    const reranker = createRecallReranker({
      context: context(),
      fetch: (async (_url, init) =>
        responseFor(init!, index => [0.8, 0.8, 0.49][index])
      ) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");

    const result = await reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      rerankCandidates: pool,
    });

    expect(result.retrievalCandidates).toEqual([baseline[0], pool[0], pool[1]]);
    expect(result.receipt).toMatchObject({
      status: "active",
      scored: 3,
      selected: 2,
      fallbackUsed: false,
    });
  });

  test("an in-flight rerank keeps its policy snapshot", async () => {
    const root = await room("active");
    const baseline = [{ excerpt: "baseline" }];
    const winner = { excerpt: "winner" };
    let releaseFetch!: () => void;
    let noteFetchStarted!: () => void;
    const fetchStarted = new Promise<void>(resolve => {
      noteFetchStarted = resolve;
    });
    const fetchGate = new Promise<void>(resolve => {
      releaseFetch = resolve;
    });
    const reranker = createRecallReranker({
      context: context(),
      fetch: (async (_url, init) => {
        noteFetchStarted();
        await fetchGate;
        return responseFor(init!);
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");

    const pending = reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      rerankCandidates: [winner],
    });
    await fetchStarted;
    await writeFile(
      path.join(root, ".athanor-room.json"),
      JSON.stringify({ room: "test-room" }),
    );
    await reranker.loadPolicy(root, "test-room");
    releaseFetch();
    const result = await pending;

    expect(result.retrievalCandidates).toEqual([winner]);
    expect(result.receipt).toMatchObject({
      status: "active",
      policyRevision: "sol-2026-09-18-v1",
    });
  });

  test("privacy refusal and empty pools make zero credential or network calls", async () => {
    const root = await room();
    let calls = 0;
    const reranker = createRecallReranker({
      context: context(() => calls++),
      fetch: (async () => {
        calls++;
        throw new Error("must not call");
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");
    const baseline = [{ excerpt: "baseline" }];

    const secretQuery = await reranker.rerank({
      query: "authorization=Bearer abcdefghijklmnop",
      retrievalCandidates: baseline,
      rerankCandidates: baseline,
    });
    const secretCard = await reranker.rerank({
      query: "safe",
      retrievalCandidates: baseline,
      rerankCandidates: [{ excerpt: "api_key=abcdefghijklmnop" }],
    });
    const quotedQuery = await reranker.rerank({
      query: "{\"password\":\"hunter2\"}",
      retrievalCandidates: baseline,
      rerankCandidates: baseline,
    });
    const quotedCard = await reranker.rerank({
      query: "safe",
      retrievalCandidates: baseline,
      rerankCandidates: [{ excerpt: "{\"api_key\":\"abcdefghijklmnop\"}" }],
    });
    const empty = await reranker.rerank({
      query: "safe",
      retrievalCandidates: baseline,
      rerankCandidates: [],
    });

    expect(secretQuery.receipt.reason).toBe("privacy-refused");
    expect(secretCard.receipt.reason).toBe("privacy-refused");
    expect(quotedQuery.receipt.reason).toBe("privacy-refused");
    expect(quotedCard.receipt.reason).toBe("privacy-refused");
    expect(empty.receipt.reason).toBe("empty-pool");
    expect(calls).toBe(0);
  });

  test("rejects malformed answer sets wholesale", async () => {
    const root = await room();
    const baseline = [{ excerpt: "baseline" }];
    const pool = [{ excerpt: "first" }, { excerpt: "second" }];
    const reranker = createRecallReranker({
      context: context(),
      fetch: (async (_url, init) => {
        const body = JSON.parse(String(init?.body));
        const token = Object.keys(body.questions)[0];
        return new Response(JSON.stringify({
          model: "jev-latest",
          answers: {
            [token]: { type: "noul", noul: 2 },
            extra: { type: "noul", noul: 0.9 },
          },
        }), { status: 200 });
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");

    const result = await reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      rerankCandidates: pool,
    });

    expect(result.retrievalCandidates).toBe(baseline);
    expect(result.receipt).toMatchObject({
      status: "failed",
      reason: "invalid-response",
      fallbackUsed: true,
    });
  });

  test("rejects an unexpected model identity", async () => {
    const root = await room();
    const baseline = [{ excerpt: "baseline" }];
    const reranker = createRecallReranker({
      context: context(),
      fetch: (async (_url, init) => {
        const valid = responseFor(init!);
        const body = await valid.json();
        body.model = "echoed private text";
        return new Response(JSON.stringify(body), { status: 200 });
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");

    const result = await reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      rerankCandidates: [{ excerpt: "candidate" }],
    });

    expect(result.retrievalCandidates).toBe(baseline);
    expect(result.receipt).toMatchObject({
      status: "failed",
      reason: "invalid-response",
    });
    expect(JSON.stringify(reranker.getCoverage())).not.toContain("echoed private text");
  });

  test("an in-flight deadline aborts the backend and preserves baseline", async () => {
    const root = await room();
    let fetchCalls = 0;
    const baseline = [{ excerpt: "baseline" }];
    const reranker = createRecallReranker({
      context: context(),
      fetch: (async (_url, init) => {
        fetchCalls++;
        return await new Promise<Response>((_resolve, reject) => {
          init?.signal?.addEventListener(
            "abort",
            () => reject(new DOMException("aborted", "AbortError")),
            { once: true },
          );
        });
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");

    const result = await reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      rerankCandidates: [{ excerpt: "candidate" }],
      deadline: Date.now() + 10,
    });

    expect(fetchCalls).toBe(1);
    expect(result.retrievalCandidates).toBe(baseline);
    expect(result.receipt).toMatchObject({
      status: "refused",
      reason: "deadline",
      fallbackUsed: true,
    });
  });

  test("deadline wins when credential resolution ignores cancellation", async () => {
    const root = await room();
    let fetchCalls = 0;
    const baseline = [{ excerpt: "baseline" }];
    const reranker = createRecallReranker({
      context: {
        modelRegistry: {
          authStorage: {
            getApiKey: () => new Promise<string>(() => undefined),
          },
        },
      },
      fetch: (async () => {
        fetchCalls++;
        throw new Error("must not call");
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");

    const result = await reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      rerankCandidates: [{ excerpt: "candidate" }],
      deadline: Date.now() + 10,
    });

    expect(fetchCalls).toBe(0);
    expect(result.retrievalCandidates).toBe(baseline);
    expect(result.receipt).toMatchObject({
      status: "refused",
      reason: "deadline",
    });
  });

  test("aborted and expired work preserves baseline without credential access", async () => {
    const root = await room();
    let calls = 0;
    const reranker = createRecallReranker({
      context: context(() => calls++),
      now: () => 100,
      fetch: (async () => {
        calls++;
        throw new Error("must not call");
      }) as typeof fetch,
    });
    await reranker.loadPolicy(root, "test-room");
    const controller = new AbortController();
    controller.abort();
    const baseline = [{ excerpt: "baseline" }];

    const aborted = await reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      signal: controller.signal,
      deadline: 200,
    });
    const expired = await reranker.rerank({
      query: "query",
      retrievalCandidates: baseline,
      deadline: 100,
    });

    expect(aborted.retrievalCandidates).toBe(baseline);
    expect(expired.retrievalCandidates).toBe(baseline);
    expect(aborted.receipt.reason).toBe("deadline");
    expect(expired.receipt.reason).toBe("deadline");
    expect(calls).toBe(0);
  });
});
