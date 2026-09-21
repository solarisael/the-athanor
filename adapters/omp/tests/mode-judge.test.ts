import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { buildModePacket, createModeScorer } from "../house-proof/mode-judge.ts";
import { RecallPolicyHostClient } from "../house-proof/recall-policy.ts";
import { modeInsulaPoints } from "../index.ts";

const roots: string[] = [];

afterEach(async () => {
  await Promise.all(roots.splice(0).map(root => rm(root, { recursive: true, force: true })));
});

async function room(marker: Record<string, unknown> | null, roomName = "test-room"): Promise<string> {
  const root = await mkdtemp(path.join(os.tmpdir(), "athanor-jev-mode-"));
  roots.push(root);
  await writeFile(path.join(root, ".athanor-room.json"), JSON.stringify({
    room: roomName,
    ...(marker ?? {}),
  }));
  return root;
}

const grant = { purpose: "recall-mode", allowPrivateConversationPackets: true, policyRevision: "sol-2026-09-21-v1" };
const typesafeMarker = { jevMode: { mode: "active", provider: "typesafe", grant } };
const layaMarker = {
  jevMode: { mode: "active", provider: "laya", endpoint: "http://127.0.0.1:8790/v1/systemone", grant },
};

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
  operatorMessage: "does the deploy need the migration first, or can it ride behind the flag?",
  assistantTurn: "Run the migration first: the flag only hides the column, it does not create it.",
  operatorReply: "ok ran it. anyway how's Ys doing, did the fever break?",
};

describe("mode grant", () => {
  test("stays off without `jevMode`, and the verdict's own grant never turns it on", async () => {
    const scorer = createModeScorer();
    expect(await scorer.loadPolicy(await room(null), "test-room")).toEqual({ mode: "off", approved: false });
    expect(await scorer.loadPolicy(
      await room({ jevVerdict: { mode: "active", provider: "typesafe", grant: { ...grant, purpose: "turn-verdict" } } }),
      "test-room",
    )).toEqual({ mode: "off", approved: false });
  });

  test("refuses a mode grant written for another purpose, room, or provider", async () => {
    const scorer = createModeScorer();
    const cases: Array<Record<string, unknown>> = [
      { jevMode: { mode: "active", provider: "typesafe", grant: { ...grant, purpose: "turn-verdict" } } },
      { jevMode: { mode: "active", provider: "typesafe", grant: { ...grant, allowPrivateConversationPackets: false } } },
      { jevMode: { mode: "active", provider: "typesafe", grant: { ...grant, policyRevision: "" } } },
      { jevMode: { mode: "shadow", provider: "typesafe", grant } },
      { jevMode: { mode: "active", provider: "openai", grant } },
    ];
    for (const marker of cases) {
      expect(await scorer.loadPolicy(await room(marker), "test-room")).toMatchObject({ mode: "off" });
    }
    expect(await scorer.loadPolicy(await room(typesafeMarker, "other-room"), "test-room")).toMatchObject({ mode: "off" });
  });

  test("typesafe uses the fixed endpoint; laya must name a loopback one", async () => {
    const scorer = createModeScorer();
    expect(await scorer.loadPolicy(await room(typesafeMarker), "test-room")).toMatchObject({
      mode: "active",
      approved: true,
      provider: "typesafe",
      endpoint: "https://api.typesafe.ai/v1/systemone",
      revision: grant.policyRevision,
      room: "test-room",
    });
    expect(await scorer.loadPolicy(await room(layaMarker), "test-room")).toMatchObject({
      mode: "active", provider: "laya", endpoint: "http://127.0.0.1:8790/v1/systemone",
    });
    expect(await scorer.loadPolicy(
      await room({ jevMode: { ...layaMarker.jevMode, endpoint: "http://laya.example.com/v1/systemone" } }),
      "test-room",
    )).toMatchObject({ mode: "off" });
  });
});

describe("mode packet", () => {
  test("carries the message head and the assistant tail under one `mode` question", () => {
    const message = "M".repeat(1000);
    const assistant = "A".repeat(1000) + "TAIL";
    const reply = "R".repeat(1000);
    const packet = buildModePacket({ operatorMessage: message, assistantTurn: assistant, operatorReply: reply }, "typesafe");

    expect(packet.state.schemaVersion).toBe("jev-mode.v1");
    expect(packet.state.operatorMessage).toHaveLength(300);
    expect(packet.state.assistantTurn).toHaveLength(600);
    expect(packet.state.operatorReply).toHaveLength(400);
    expect(packet.state.assistantTurn.endsWith("TAIL")).toBe(true);
    expect(Object.keys(packet.state).sort()).toEqual(["assistantTurn", "operatorMessage", "operatorReply", "schemaVersion"]);
    expect(Object.keys(packet.questions)).toEqual(["mode"]);
    expect(packet.model).toBe("jev-latest");

    const laya = buildModePacket(turnInput, "laya");
    expect("model" in laya).toBe(false);
    expect(buildModePacket({ ...turnInput, operatorMessage: "   " }, "laya").state.operatorMessage).toBeUndefined();
    expect(buildModePacket({ ...turnInput, operatorReply: "   " }, "laya").state.operatorReply).toBeUndefined();
  });

  test("tells the judge the job it is doing and offers exactly the three modes", () => {
    const question = buildModePacket(turnInput, "typesafe").questions.mode as {
      type: string;
      instructions: string;
      criteria: Record<string, string>;
    };

    expect(question.type).toBe("choice");
    expect(question.instructions).toBe(
      "A memory system chooses which retrieval mode to use for the operator's newest message. "
      + "Judge the exchange below, weighing that newest message most, and pick the mode that would surface the right memories.",
    );
    expect(question.criteria).toEqual({
      conversation: "The exchange is about the operator, their people, their history, feelings, or the room's shared life",
      work: "The exchange is about a project, code, files, a build, a deploy, or a technical decision",
      mixed: "Both at once, or neither clearly",
    });
    // `quiet` is the operator's to ask for; the judge is never offered it.
    expect(Object.keys(question.criteria)).not.toContain("quiet");
  });
});

describe("mode scoring", () => {
  test("a room without the grant never touches credential or network", async () => {
    let calls = 0;
    const scorer = createModeScorer({
      context: context(() => calls++),
      fetch: (async () => { calls++; throw new Error("must not call"); }) as typeof fetch,
    });
    await scorer.loadPolicy(await room(null), "test-room");
    expect(await scorer.score(turnInput)).toMatchObject({ status: "disabled", reason: "not_approved" });
    expect(calls).toBe(0);
  });

  test("scores a mode through typesafe with the bearer key and the judged turn in the packet", async () => {
    const { fetcher, requests } = answering({
      mode: { type: "choice", choice: "work", probabilities: { work: 0.9, conversation: 0.06, mixed: 0.04 } },
    });
    const scorer = createModeScorer({ context: context(), fetch: fetcher });
    await scorer.loadPolicy(await room(typesafeMarker), "test-room");

    expect(await scorer.score({ ...turnInput, sessionId: "s1" })).toMatchObject({
      status: "scored", provider: "typesafe", model: "jev-latest", mode: "work",
    });
    expect(requests).toHaveLength(1);
    expect(requests[0]!.url).toBe("https://api.typesafe.ai/v1/systemone");
    expect((requests[0]!.init.headers as Record<string, string>).authorization).toBe("Bearer test-key");
    const body = JSON.parse(String(requests[0]!.init.body));
    expect(body.state.operatorMessage).toBe(turnInput.operatorMessage);
    expect(body.state.assistantTurn).toBe(turnInput.assistantTurn);
    expect(body.state.operatorReply).toBe(turnInput.operatorReply);
  });

  test("refuses an empty turn and a secret-like packet before the network", async () => {
    let calls = 0;
    const scorer = createModeScorer({
      context: context(undefined, null),
      fetch: (async () => { calls++; throw new Error("must not call"); }) as typeof fetch,
    });
    await scorer.loadPolicy(await room(typesafeMarker), "test-room");

    expect(await scorer.score({ ...turnInput, assistantTurn: "  " })).toMatchObject({ status: "refused", reason: "empty_turn" });
    expect(await scorer.score({ ...turnInput, assistantTurn: "api_key = sk-abcdefghijklmnopqrstuvwxyz" }))
      .toMatchObject({ status: "refused", reason: "secret_like" });
    expect(await scorer.score(turnInput)).toMatchObject({ status: "unavailable", reason: "credential_unavailable" });
    expect(calls).toBe(0);
  });

  test("rejects a label outside the three, including `quiet`", async () => {
    for (const choice of ["quiet", "auto", "constructor"]) {
      const { fetcher } = answering({ mode: { type: "choice", choice } });
      const scorer = createModeScorer({ context: context(), fetch: fetcher });
      await scorer.loadPolicy(await room(typesafeMarker), "test-room");
      expect(await scorer.score(turnInput)).toMatchObject({ status: "failed", reason: "invalid_response" });
    }
  });

  test("a slow judge fails on the deadline and aborts the request", async () => {
    let aborted = false;
    const scorer = createModeScorer({
      context: context(),
      fetch: ((_url: string, init: RequestInit) => new Promise<Response>((_resolve, reject) => {
        init.signal?.addEventListener("abort", () => { aborted = true; reject(new Error("aborted")); });
      })) as unknown as typeof fetch,
    });
    await scorer.loadPolicy(await room(typesafeMarker), "test-room");

    expect(await scorer.score({ ...turnInput, deadline: Date.now() + 20 })).toMatchObject({
      status: "failed", reason: "deadline",
    });
    expect(aborted).toBe(true);
  });
});

describe("mode insula points", () => {
  const parent = { room: "kodo", traceId: "trace-1", spanId: "span-1", providerRequestId: "msg_01" };

  test("names the provider and the chosen mode, and never leaves `trace_span`", () => {
    const points = modeInsulaPoints("kodo", parent, {
      status: "scored", provider: "typesafe", model: "jev-latest", mode: "work", latencyMs: 412, packetBytes: 903,
    });

    expect(points.map(point => point.operation)).toEqual(["mode_request.typesafe", "recall_mode.work"]);
    for (const point of points) {
      expect(point.scope).toBe("trace_span");
      expect(point).toMatchObject({ room: "kodo", traceId: "trace-1", parentSpanId: "span-1", providerRequestId: "msg_01" });
    }
    expect(points[0]).toMatchObject({ outcomeClass: "ok", errorClass: null, durationUs: 412_000, bytesOut: 903 });
    expect(points[1]).toMatchObject({ outcomeClass: "ok" });
  });

  test("a failure is one point with its reason; a room without the grant is silence", () => {
    const failed = modeInsulaPoints("kodo", undefined, {
      status: "failed", provider: "laya", reason: "invalid_response", latencyMs: 7,
    });
    expect(failed).toHaveLength(1);
    expect(failed[0]).toMatchObject({
      operation: "mode_request.laya", outcomeClass: "error", errorClass: "invalid_response",
      scope: "trace_span", traceId: undefined, providerRequestId: undefined, bytesOut: 0,
    });

    expect(modeInsulaPoints("kodo", parent, { status: "disabled", provider: null, reason: "not_approved", latencyMs: 0 }))
      .toEqual([]);
    expect(modeInsulaPoints("kodo", parent, { status: "refused", provider: null, reason: "secret_like", latencyMs: 1 })[0])
      .toMatchObject({ operation: "mode_request.none", outcomeClass: "refused" });
  });
});

describe("judged mode command", () => {
  const savedEnv: Record<string, string | undefined> = {};

  beforeEach(() => {
    for (const key of ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID"]) savedEnv[key] = process.env[key];
    process.env.ATHANOR_HOST_TOKEN = "test-token";
    process.env.ATHANOR_HOST_HOUSE_ID = "solarisael";
  });

  afterEach(() => {
    for (const [key, value] of Object.entries(savedEnv)) {
      if (value === undefined) delete process.env[key];
      else process.env[key] = value;
    }
  });

  test("sends `athanor.recall_policy.judged_mode` with the mode, the source, and the revision", async () => {
    const received: Array<Record<string, any>> = [];
    const server = Bun.serve({
      port: 0,
      hostname: "127.0.0.1",
      fetch(request, server) {
        if (server.upgrade(request)) return;
        return new Response("not a websocket", { status: 400 });
      },
      websocket: {
        message(socket, data) {
          const command = JSON.parse(String(data));
          received.push(command);
          socket.send(JSON.stringify({
            correlation_id: command.message_id,
            command_or_event_type: "athanor.recall_policy.snapshot",
            state: { requestedMode: "auto", resolvedMode: "work", resolutionReason: "jev-judged" },
            version: 4,
            sequence: 9,
            state_hash: "hash-4",
          }));
        },
      },
    });
    process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;

    try {
      const client = new RecallPolicyHostClient({ room: "test-room", spirit: "Kodo", session: "s1" });
      const snapshot = await client.judgedMode({ mode: "work", source: "jev", revision: grant.policyRevision });

      expect(snapshot).toMatchObject({ version: 4, sequence: 9, stateHash: "hash-4" });
      expect(snapshot.recallPolicy.resolvedMode).toBe("work");
    } finally {
      server.stop(true);
    }

    expect(received).toHaveLength(1);
    expect(received[0]).toMatchObject({
      command_or_event_type: "athanor.recall_policy.judged_mode",
      projection_id: "recall_policy",
      scope: "room:test-room:recall_policy",
      sender_room: "test-room",
      judged_mode: { mode: "work", source: "jev", revision: grant.policyRevision },
    });
    // Strict on the Host side: the payload is that one key, with no
    // base_version or mutations riding along.
    expect(Object.keys(received[0]!.judged_mode)).toEqual(["mode", "source", "revision"]);
    expect(received[0]!.base_version).toBeUndefined();
    expect(received[0]!.mutations).toBeUndefined();
  });
});
