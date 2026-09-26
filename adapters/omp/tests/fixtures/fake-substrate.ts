// A stand-in substrate child for context-budget.test.ts, spoken to over the
// real RustJsonlTransport. It answers lesson queries with two always-on coding
// lessons and handles `recall` as ATHANOR_FAKE_RECALL says: `hang` never answers,
// as a stalled embed never did live, and `exit` dies mid-request.
import { createInterface } from "node:readline";

const baseline = [
  { type: "coding", id: 468, alwaysOn: true, lesson: "Ponytail ladder: stop at the first rung that holds." },
  { type: "coding", id: 223, alwaysOn: true, lesson: "Test the opposite assumption, not just the one you had in mind." },
];

function reply(id: string, result: unknown): void {
  process.stdout.write(`${JSON.stringify({ protocol: 1, id, result })}\n`);
}

for await (const line of createInterface({ input: process.stdin })) {
  if (!line.trim()) continue;
  const { id, method, params } = JSON.parse(line);
  if (method === "lesson_query") {
    reply(id, { ok: true, lessons: params?.alwaysOn ? baseline : [], taxonomy: [] });
  } else if (method === "recall" || method === "vault_recall") {
    if (process.env.ATHANOR_FAKE_RECALL === "exit") process.exit(23);
    // `hang`: the request stays open until the transport gives up on it.
  } else {
    reply(id, { ok: false, error: `fake substrate does not answer ${method}` });
  }
}
