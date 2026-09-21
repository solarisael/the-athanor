import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import {
  buildVerdictPacket,
  createVerdictScorer,
  recallTitlesFromWorkingSet,
} from "../house-proof/turn-verdict.ts";
import { previousTurnForVerdict } from "../index.ts";

const roots: string[] = [];

afterEach(async () => {
  await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true })));
});

async function room(verdict: Record<string, unknown> | null, roomName = "test-room"): Promise<string> {
  const root = await mkdtemp(path.join(os.tmpdir(), "athanor-jev-verdict-"));
  roots.push(root);
  await writeFile(path.join(root, ".athanor-room.json"), JSON.stringify({
    room: roomName,
    ...(verdict ? { jevVerdict: verdict } : {}),
  }));
  return root;
}

const grant = { purpose: "turn-verdict", allowPrivateConversationPackets: true, policyRevision: "sol-2026-09-21-v1" };
const typesafeMarker = { mode: "active", provider: "typesafe", grant };
const layaMarker = { mode: "active", provider: "laya", endpoint: "http://127.0.0.1:8790/v1/systemone", grant };

function context(onAuth: () => void = () => undefined, key: string | null = "test-key") {
  return {
    modelRegistry: {
      authStorage: {
        async getApiKey(provider: string) {
          onAuth();
          expect(provider).toBe("typesafe");
          return key;
        },
      },
    },
  };
}

function answering(answers: Record<string, unknown>, model = "jev-latest") {
  const requests: Array<{ url: string; init: RequestInit }> = [];
  const fetcher = (async (url: string, init: RequestInit) => {
    requests.push({ url, init });
    return new Response(JSON.stringify({ model, answers }), { status: 200 });
  }) as unknown as typeof fetch;
  return { fetcher, requests };
}

const turnInput = {
  operatorMessage: "which photo do i send him, green shirt or the studio one?",
  operatorReply: "no dummy you're seeing it the wrong way around òwó",
  assistantTurn: "Assuming you mean the green-shirt photo is the better one, give him the actual differences.",
};

describe("verdict policy", () => {
  test("stays off without a marker, a grant, or a known provider", async () => {
    const scorer = createVerdictScorer();
    expect(await scorer.loadPolicy(await room(null), "test-room")).toEqual({ mode: "off", approved: false });
    expect(await scorer.loadPolicy(await room({ mode: "active", provider: "typesafe" }), "test-room")).toMatchObject({ mode: "off" });
    expect(await scorer.loadPolicy(await room({ ...typesafeMarker, provider: "openai" }), "test-room")).toMatchObject({ mode: "off" });
    expect(await scorer.loadPolicy(await room(typesafeMarker, "other-room"), "test-room")).toMatchObject({ mode: "off" });
  });

  test("typesafe uses the fixed endpoint; laya must name a loopback one", async () => {
    const scorer = createVerdictScorer();
    expect(await scorer.loadPolicy(await room(typesafeMarker), "test-room")).toMatchObject({
      mode: "active", provider: "typesafe", endpoint: "https://api.typesafe.ai/v1/systemone", revision: grant.policyRevision,
    });
    expect(await scorer.loadPolicy(await room(layaMarker), "test-room")).toMatchObject({
      mode: "active", provider: "laya", endpoint: layaMarker.endpoint,
    });
    expect(await scorer.loadPolicy(
      await room({ ...layaMarker, endpoint: "http://laya.example.com/v1/systemone" }),
      "test-room",
    )).toMatchObject({ mode: "off" });
  });
});

describe("verdict packet", () => {
  test("clips the message and reply heads and the assistant tail, and asks recall only when memories were injected", () => {
    const message = "M".repeat(1000);
    const reply = "R".repeat(1000);
    const assistant = "A".repeat(1000) + "TAIL";
    const bare = buildVerdictPacket({ operatorMessage: message, operatorReply: reply, assistantTurn: assistant }, "typesafe");
    expect(bare.state.schemaVersion).toBe("jev-verdict.v2");
    expect(bare.state.operatorMessage).toHaveLength(300);
    expect(bare.state.operatorReply).toHaveLength(400);
    expect(bare.state.assistantTurn).toHaveLength(600);
    expect(bare.state.assistantTurn.endsWith("TAIL")).toBe(true);
    expect(bare.state.recalledMemories).toBeUndefined();
    expect(Object.keys(bare.questions)).toEqual(["turn"]);
    expect(bare.model).toBe("jev-latest");

    const withRecall = buildVerdictPacket(
      { ...turnInput, recallTitles: ["canon: The Athanor", "The deploy landed"] },
      "laya",
    );
    expect(withRecall.state.recalledMemories).toBe("- canon: The Athanor\n- The deploy landed");
    expect(Object.keys(withRecall.questions)).toEqual(["turn", "recallFit", "recallUse"]);
    expect("model" in withRecall).toBe(false);
  });

  test("tells the judge what it is grading on every question", () => {
    const packet = buildVerdictPacket({ ...turnInput, recallTitles: ["The deploy landed"] }, "typesafe");
    const instructions = (key: string) => (packet.questions[key] as { instructions: string }).instructions;
    expect(instructions("turn")).toMatch(/judge the assistant's turn by how the operator reacts/i);
    expect(instructions("recallFit")).toMatch(/memory system retrieved .* for the operator's message/i);
    expect(instructions("recallFit")).toMatch(/judge the retrieval, not the assistant/i);
    expect(instructions("recallUse")).toMatch(/judge the assistant, not the retrieval/i);

    const empty = buildVerdictPacket({ ...turnInput, operatorMessage: "  " }, "typesafe");
    expect(empty.state.operatorMessage).toBeUndefined();
  });

  test("reads titles out of a real recall working set and nothing out of other blocks", () => {
    const content = [
      "<athanor-memories>",
      "Room-local Athanor Recall working set (mixed; resolved-mode-change).",
      JSON.stringify({
        ok: true,
        canonMatches: [{ termKey: "The Athanor", id: 207 }],
        retrievalCandidates: [{ title: "The deploy landed", excerpt: "private" }, { memory_id: 3 }],
      }, null, 2),
      "</athanor-memories>",
    ].join("\n");
    expect(recallTitlesFromWorkingSet(content)).toEqual(["canon: The Athanor", "The deploy landed"]);
    expect(recallTitlesFromWorkingSet("<athanor-memories>\nno json here\n</athanor-memories>")).toBeNull();
    expect(recallTitlesFromWorkingSet(undefined)).toBeNull();
  });
});

describe("verdict scoring", () => {
  test("disabled policy never touches credential or network", async () => {
    let calls = 0;
    const scorer = createVerdictScorer({
      context: context(() => calls++),
      fetch: (async () => { calls++; throw new Error("must not call"); }) as typeof fetch,
    });
    await scorer.loadPolicy(await room(null), "test-room");
    expect(await scorer.score(turnInput)).toMatchObject({ status: "disabled", reason: "not_approved" });
    expect(calls).toBe(0);
  });

  test("scores a turn and both recall axes through typesafe with the bearer key", async () => {
    const { fetcher, requests } = answering({
      turn: { type: "choice", choice: "corrected", probabilities: { corrected: 0.9, continued: 0.08, gold: 0.02 } },
      recallFit: { type: "choice", choice: "relevant", probabilities: { relevant: 0.8, unneeded: 0.15, wrong: 0.05 } },
      recallUse: { type: "choice", choice: "ignored", probabilities: { used: 0.1, ignored: 0.85, misled: 0.05 } },
    });
    const scorer = createVerdictScorer({ context: context(), fetch: fetcher });
    await scorer.loadPolicy(await room(typesafeMarker), "test-room");

    const result = await scorer.score({ ...turnInput, recallTitles: ["The deploy landed"], sessionId: "s1" });

    expect(result).toMatchObject({
      status: "scored", provider: "typesafe", model: "jev-latest", turn: "corrected", recall: { fit: "relevant", use: "ignored" },
    });
    expect(requests).toHaveLength(1);
    expect(requests[0]!.url).toBe("https://api.typesafe.ai/v1/systemone");
    expect((requests[0]!.init.headers as Record<string, string>).authorization).toBe("Bearer test-key");
    const body = JSON.parse(String(requests[0]!.init.body));
    expect(body.state.operatorMessage).toBe(turnInput.operatorMessage);
    expect(body.state.operatorReply).toBe(turnInput.operatorReply);
    expect(body.state.recalledMemories).toBe("- The deploy landed");
  });

  test("laya goes to the loopback endpoint without a credential", async () => {
    let authCalls = 0;
    const { fetcher, requests } = answering({ turn: { type: "choice", choice: "continued" } }, "laya-typed-decisions");
    const scorer = createVerdictScorer({ context: context(() => authCalls++), fetch: fetcher });
    await scorer.loadPolicy(await room(layaMarker), "test-room");

    const result = await scorer.score(turnInput);

    expect(result).toMatchObject({ status: "scored", provider: "laya", model: "laya-typed-decisions", turn: "continued", recall: null });
    expect(authCalls).toBe(0);
    expect(requests[0]!.url).toBe(layaMarker.endpoint);
    expect((requests[0]!.init.headers as Record<string, string>).authorization).toBeUndefined();
  });

  test("refuses a secret-like packet before the network and reports a missing credential", async () => {
    let calls = 0;
    const scorer = createVerdictScorer({
      context: context(undefined, null),
      fetch: (async () => { calls++; throw new Error("must not call"); }) as typeof fetch,
    });
    await scorer.loadPolicy(await room(typesafeMarker), "test-room");

    expect(await scorer.score({ ...turnInput, operatorReply: "here: api_key = sk-abcdefghijklmnopqrstuvwxyz" }))
      .toMatchObject({ status: "refused", reason: "secret_like" });
    expect(await scorer.score({ ...turnInput, operatorReply: "   " })).toMatchObject({ status: "refused", reason: "empty_turn" });
    expect(await scorer.score(turnInput)).toMatchObject({ status: "unavailable", reason: "credential_unavailable" });
    expect(calls).toBe(0);
  });

  test("rejects an answer set that does not match the questions asked", async () => {
    const unasked = answering({
      turn: { type: "choice", choice: "gold" },
      recallFit: { type: "choice", choice: "relevant" },
    });
    const scorer = createVerdictScorer({ context: context(), fetch: unasked.fetcher });
    await scorer.loadPolicy(await room(typesafeMarker), "test-room");
    expect(await scorer.score(turnInput)).toMatchObject({ status: "failed", reason: "invalid_response" });

    const halfAnswered = answering({
      turn: { type: "choice", choice: "gold" },
      recallFit: { type: "choice", choice: "relevant" },
    });
    const half = createVerdictScorer({ context: context(), fetch: halfAnswered.fetcher });
    await half.loadPolicy(await room(typesafeMarker), "test-room");
    expect(await half.score({ ...turnInput, recallTitles: ["The deploy landed"] }))
      .toMatchObject({ status: "failed", reason: "invalid_response" });

    for (const choice of ["meh", "constructor", "toString"]) {
      const wrongLabel = answering({ turn: { type: "choice", choice } });
      const second = createVerdictScorer({ context: context(), fetch: wrongLabel.fetcher });
      await second.loadPolicy(await room(typesafeMarker), "test-room");
      expect(await second.score(turnInput)).toMatchObject({ status: "failed", reason: "invalid_response" });
    }
  });

  test("a slow judge fails on the deadline and aborts the request", async () => {
    let aborted = false;
    const scorer = createVerdictScorer({
      context: context(),
      fetch: ((_url: string, init: RequestInit) => new Promise<Response>((_resolve, reject) => {
        init.signal?.addEventListener("abort", () => { aborted = true; reject(new Error("aborted")); });
      })) as unknown as typeof fetch,
    });
    await scorer.loadPolicy(await room(typesafeMarker), "test-room");

    const result = await scorer.score({ ...turnInput, deadline: Date.now() + 20 });

    expect(result).toMatchObject({ status: "failed", reason: "deadline" });
    expect(aborted).toBe(true);
  });
});

describe("previous turn selection", () => {
  const user = (id: string, text: string) => ({ role: "user", id, content: text });
  const assistant = (text: string) => ({ role: "assistant", content: [{ type: "text", text }, { type: "toolCall", name: "read" }] });

  test("takes the last assistant text before the prompt, the message it answered, and that turn's recall memo", () => {
    const first = user("u1", "kodo. it's time.");
    const second = user("u2", "no dummy you're seeing it the wrong way around");
    const messages = [first, assistant("Same face, very different presentation."), second];
    const turnKeys = new Map<any, string>([[first, "id:u1"], [second, "id:u2"]]);
    const memo = new Map([["id:u1", [
      { customType: "athanor-presence-context", content: "Presence v1" },
      { customType: "athanor-recall-context", content: JSON.stringify({ retrievalCandidates: [{ title: "Tania's four audios" }] }) },
    ]]]);

    expect(previousTurnForVerdict(messages, second, turnKeys, memo)).toEqual({
      operatorMessage: "kodo. it's time.",
      assistantTurn: "Same face, very different presentation.",
      recallTitles: ["Tania's four audios"],
    });
  });

  test("yields nothing on a first turn or when no assistant text precedes the prompt", () => {
    const only = user("u1", "hello");
    expect(previousTurnForVerdict([only], only, new Map([[only, "id:u1"]]), new Map())).toBeNull();

    const first = user("u1", "hello");
    const second = user("u2", "again");
    expect(previousTurnForVerdict([first, second], second, new Map(), new Map())).toBeNull();
  });
});
