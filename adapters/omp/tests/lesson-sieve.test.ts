import { afterEach, describe, expect, test } from "bun:test";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { createLessonSieve, LESSON_SIEVE_GRANT, selectPresenceLessons, type TtsrLesson } from "../house-proof/lesson-ttsr.ts";
import { createRecallReranker } from "../house-proof/recall-judgment.ts";
import { lessonMaterials } from "../house-proof/presence-materials.ts";

const roots: string[] = [];
afterEach(async () => {
  await Promise.all(roots.splice(0).map((root) => rm(root, { recursive: true, force: true })));
});

const baseline: TtsrLesson[] = [
  { id: 468, body: "Ponytail ladder: stop at the first rung that holds." },
  { id: 222, body: "A retrieval threshold is calibration bound to a model." },
  { id: 223, body: "Test the opposite assumption, not just the one you had in mind." },
];
const triggers: TtsrLesson[] = [{ id: 389, body: "An empty catch is a silent fallback wearing a seatbelt." }];

type Door = { mode: "shadow" | "active"; purpose?: string };
async function room(doors: { jevLessons?: Door; jevRecall?: Door }): Promise<string> {
  const root = await mkdtemp(path.join(os.tmpdir(), "athanor-lesson-sieve-"));
  roots.push(root);
  const marker: Record<string, unknown> = { room: "test-room" };
  if (doors.jevLessons) {
    marker.jevLessons = {
      mode: doors.jevLessons.mode, provider: "typesafe",
      grant: { purpose: doors.jevLessons.purpose ?? "lesson-sieve", allowPrivateLessonPackets: true, policyRevision: "sol-test-v1" },
    };
  }
  if (doors.jevRecall) {
    marker.jevRecall = {
      mode: doors.jevRecall.mode, provider: "typesafe",
      grant: { purpose: "recall-rerank", allowPrivateRecallPackets: true, policyRevision: "sol-test-v1" },
    };
  }
  await writeFile(path.join(root, ".athanor-room.json"), JSON.stringify(marker));
  return root;
}

const context = { modelRegistry: { authStorage: { getApiKey: async () => "test-key" } } };

// Scores by card text so the verdict is tied to the lesson, not to its position.
function jev(score: (text: string) => number, calls: { n: number }): typeof fetch {
  return (async (_url: unknown, init?: RequestInit) => {
    calls.n++;
    const body = JSON.parse(String(init?.body));
    const answers = Object.fromEntries(body.state.cards.map((card: { token: string; text: string }) =>
      [card.token, { type: "noul", noul: score(card.text) }]));
    return new Response(JSON.stringify({ model: "jev-latest", answers }), { status: 200 });
  }) as typeof fetch;
}

function harness(fetcher: typeof fetch) {
  const lessonReranker = createRecallReranker({ context, fetch: fetcher, grant: LESSON_SIEVE_GRANT });
  const recallReranker = createRecallReranker({ context, fetch: fetcher });
  return { lessonReranker, recallReranker, sieve: createLessonSieve(lessonReranker) };
}

async function presence(
  sieve: ReturnType<typeof harness>["sieve"],
  roomDir: string,
  turn = "turn-1",
  deadline: number | null = Date.now() + 2_000,
) {
  const sieved = await sieve({
    turn, roomDir, room: "test-room", query: "github clone counts for the repo", baseline, deadline, context,
  });
  const materials = lessonMaterials(selectPresenceLessons({ lessons: triggers, baseline: sieved.baseline }, "work"));
  return { materials, receipt: sieved.receipt, sieved };
}

const ids = (lessons: TtsrLesson[]) => lessons.map((lesson) => lesson.id);

describe("Presence lesson sieve", () => {
  test("active keeps only winners in baseline order and triggers ride untouched", async () => {
    const calls = { n: 0 };
    const { sieve, recallReranker } = harness(jev((text) => (text.includes("Ponytail") || text.includes("opposite") ? 0.9 : 0.1), calls));
    const out = await presence(sieve, await room({ jevLessons: { mode: "active" } }));
    expect(ids(out.sieved.baseline)).toEqual([468, 223]);
    expect(out.materials.map((material) => material.body)).toEqual([baseline[0].body, baseline[2].body, triggers[0].body]);
    expect(out.receipt).toMatchObject({ status: "active", scored: 3, selected: 2, fallbackUsed: false });
    expect(calls.n).toBe(1);
    expect(recallReranker.getCoverage()).toMatchObject({ turns: 0, last: null });
  });

  test("active zero winners drops the whole baseline but keeps triggers", async () => {
    const calls = { n: 0 };
    const { sieve } = harness(jev(() => 0.1, calls));
    const out = await presence(sieve, await room({ jevLessons: { mode: "active" } }));
    expect(out.sieved.baseline).toEqual([]);
    expect(out.materials.map((material) => material.body)).toEqual([triggers[0].body]);
  });

  test("shadow scores but keeps the full baseline with a receipt", async () => {
    const calls = { n: 0 };
    const { sieve } = harness(jev(() => 0.1, calls));
    const out = await presence(sieve, await room({ jevLessons: { mode: "shadow" } }));
    expect(calls.n).toBe(1);
    expect(out.sieved.baseline).toBe(baseline);
    expect(out.receipt).toMatchObject({ status: "shadow", selected: 0, fallbackUsed: true });
  });

  test("backend failure keeps the full baseline with a failed receipt", async () => {
    const calls = { n: 0 };
    const { sieve, lessonReranker, recallReranker } = harness((async () => {
      calls.n++;
      return new Response("nope", { status: 503 });
    }) as typeof fetch);
    const out = await presence(sieve, await room({ jevLessons: { mode: "active" } }));
    expect(out.sieved.baseline).toBe(baseline);
    expect(out.receipt).toMatchObject({ status: "failed", reason: "non-ok", fallbackUsed: true });
    expect(lessonReranker.getCoverage()).toMatchObject({ failed: 1 });
    expect(recallReranker.getCoverage()).toMatchObject({ turns: 0, failed: 0, last: null });
  });

  test("an exhausted budget never calls Jev and keeps the full baseline", async () => {
    const calls = { n: 0 };
    const { sieve } = harness(jev(() => 0.1, calls));
    const out = await presence(sieve, await room({ jevLessons: { mode: "active" } }), "turn-1", null);
    expect(calls.n).toBe(0);
    expect(out.sieved.baseline).toBe(baseline);
    expect(out.receipt).toMatchObject({ status: "refused", reason: "deadline", fallbackUsed: true });
  });

  test("a hung Jev is cut at the deadline and keeps the full baseline", async () => {
    const { sieve } = harness(((_url: unknown, init?: RequestInit) => new Promise((_resolve, reject) => {
      init?.signal?.addEventListener("abort", () => reject(new Error("aborted")));
    })) as typeof fetch);
    const started = Date.now();
    const out = await presence(sieve, await room({ jevLessons: { mode: "active" } }), "turn-1", Date.now() + 50);
    expect(Date.now() - started).toBeLessThan(1_000);
    expect(out.sieved.baseline).toBe(baseline);
    expect(out.receipt).toMatchObject({ status: "refused", reason: "deadline" });
  });

  test("a recall grant never opens the lesson door, and a lesson grant never opens recall", async () => {
    const calls = { n: 0 };
    const { sieve, recallReranker } = harness(jev(() => 0.1, calls));
    const recallOnly = await room({ jevRecall: { mode: "active" } });
    const out = await presence(sieve, recallOnly);
    expect(calls.n).toBe(0);
    expect(out.sieved.baseline).toBe(baseline);
    expect(out.receipt).toMatchObject({ status: "disabled", reason: "not-approved" });

    const wrongPurpose = await presence(sieve, await room({ jevLessons: { mode: "active", purpose: "recall-rerank" } }), "turn-2");
    expect(wrongPurpose.receipt).toMatchObject({ status: "disabled" });

    const lessonOnly = await room({ jevLessons: { mode: "active" } });
    expect(await recallReranker.loadPolicy(lessonOnly, "test-room")).toEqual({ mode: "off", approved: false });
    expect((await recallReranker.loadPolicy(recallOnly, "test-room")).approved).toBe(true);
    expect(calls.n).toBe(0);
  });

  test("one Jev call per turn: retries replay the same decision byte for byte", async () => {
    const calls = { n: 0 };
    let verdict = 0.9;
    const { sieve } = harness(jev(() => verdict, calls));
    const roomDir = await room({ jevLessons: { mode: "active" } });
    const [first, concurrent] = await Promise.all([presence(sieve, roomDir), presence(sieve, roomDir)]);
    verdict = 0.1;
    const retry = await presence(sieve, roomDir);
    expect(calls.n).toBe(1);
    expect(JSON.stringify(retry.materials)).toBe(JSON.stringify(first.materials));
    expect(JSON.stringify(concurrent.materials)).toBe(JSON.stringify(first.materials));
    expect(retry.receipt).toBe(first.receipt);

    const nextTurn = await presence(sieve, roomDir, "turn-2");
    expect(calls.n).toBe(2);
    expect(nextTurn.sieved.baseline).toEqual([]);
  });
});
