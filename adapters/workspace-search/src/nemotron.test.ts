import assert from "node:assert/strict";
import { test } from "node:test";
import { createNemotronModel } from "./nemotron.ts";

const text = { kind: "text" as const, text: "shell persistence" };
const vector = () => Array(2048).fill(0.1);

test("retrieval roles and refusal of truncation reach the provider", async (t) => {
  const requests: Record<string, unknown>[] = [];
  t.mock.method(globalThis, "fetch", async (_url: string, init: RequestInit) => {
    requests.push(JSON.parse(String(init.body)));
    return Response.json({ embeddings: [vector()] });
  });
  const model = createNemotronModel();
  await model.embed([text], { purpose: "query" });
  await model.embed([text], { purpose: "document" });
  assert.deepEqual(requests.map((request) => request.input), [
    ["query: shell persistence"], ["passage: shell persistence"],
  ]);
  assert.ok(requests.every((request) => request.truncate === false));
});

test("invalid provider vectors cannot enter an index", async (t) => {
  let payload: unknown;
  t.mock.method(globalThis, "fetch", async () => Response.json(payload));
  const model = createNemotronModel();
  payload = { embeddings: [] };
  await assert.rejects(model.embed([text]), /embedding count/);
  payload = { embeddings: [[1, 2]] };
  await assert.rejects(model.embed([text]), /dimensions/);
  payload = { embeddings: [Array(2048).fill(0)] };
  await assert.rejects(model.embed([text]), /zero vector/);
  payload = { embeddings: [vector().with(3, null)] };
  await assert.rejects(model.embed([text]), /non-finite/);
});

test("Ollama context overflow remains an observable error", async (t) => {
  t.mock.method(globalThis, "fetch", async () =>
    new Response("input length exceeds maximum context length", { status: 400 }));
  await assert.rejects(createNemotronModel().embed([text]), /HTTP 400:.*exceeds maximum/);
});

test("cancellation while reading the response preserves its reason", async (t) => {
  const controller = new AbortController();
  const reason = new Error("index cancelled");
  t.mock.method(globalThis, "fetch", async () => {
    controller.abort(reason);
    return { ok: true, json: async () => { throw new Error("body interrupted"); } };
  });
  await assert.rejects(
    createNemotronModel().embed([text], { signal: controller.signal }),
    (error: unknown) => error === reason,
  );
});
