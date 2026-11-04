import { expect, test } from "bun:test";
import { compactGuMessages } from "../house-proof/swarm-delivery/index.ts";

function task(batch: string, ids: string[], agent = "gu-sonnet"): any {
  return {
    role: "toolResult", toolName: "task", toolCallId: batch,
    content: [{ type: "text", text: "Native task admission" }],
    details: { progress: ids.map((id, index) => ({ id, index, agent, status: "running" })) },
    isError: false, timestamp: 1,
  };
}

function result(ids: string[]): any {
  return {
    role: "custom", customType: "async-result", display: true, attribution: "agent", timestamp: 2,
    content: ids.map((id) => `<task-result id="${id}" agent="gu-sonnet" status="completed" duration="10s">\n<output>\n${JSON.stringify({ file: `gu/${id}.json`, lines: [10, 20], count: 3, wrote: true })}\n</output>\n</task-result>\n${id} is now idle; full output is available at agent://${id}.`).join("\n"),
    details: {
      meta: { source: { type: "report", value: "background job delivery" } },
      jobs: ids.map((agentId) => ({ jobId: `${agentId}-collision-2`, type: "task", status: "completed", agentId })),
    },
  };
}

function waitResult(ids: string[], statuses = ids.map(() => "running")): any {
  return {
    role: "toolResult", toolName: "wait", toolCallId: "wait-call", timestamp: 3, isError: false,
    content: [{ type: "text", text: "Native coordination snapshot. ".repeat(100) }],
    details: {
      op: "wait", meta: { source: { type: "report", value: "background jobs snapshot" } },
      jobs: ids.map((agentUrlId, index) => ({
        id: `${agentUrlId}-collision-2`, agentUrlId, type: "task", status: statuses[index], durationMs: 10,
      })),
    },
  };
}

function text(message: any): string {
  return typeof message.content === "string"
    ? message.content
    : message.content.map((block: any) => block.text ?? "").join("\n");
}

function pointers(message: any): string[] {
  return text(message).match(/agent:\/\/[A-Za-z0-9_-]+/g) ?? [];
}

function freeze(value: any): any {
  if (value && typeof value === "object") {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }

  return value;
}

function unchanged(messages: any[]): void {
  const before = JSON.stringify(messages);
  freeze(messages);
  expect(compactGuMessages(messages)).toBe(messages);
  expect(JSON.stringify(messages)).toBe(before);
}

test("147 adjacent gu completions retain every artifact identity without rewriting the raw burst", () => {
  const ids = Array.from({ length: 147 }, (_, index) => `worker-${String(index).padStart(3, "0")}`);
  const admission = task("batch-large", ids);
  const deliveries = ids.map((id) => result([id]));
  const messages = [admission, ...deliveries];
  const before = JSON.stringify(messages);
  const nativeLength = deliveries.reduce((sum, message) => sum + text(message).length, 0);
  freeze(messages);

  const projected = compactGuMessages(messages);

  expect(projected).not.toBe(messages);
  expect(projected).toHaveLength(2);
  expect(projected[0]).toBe(admission);
  expect(projected[1]).not.toBe(deliveries[0]);
  expect(pointers(projected[1])).toEqual(ids.map((id) => `agent://${id}`));
  expect(text(projected[1]).length).toBeLessThan(nativeLength / 4);
  expect(JSON.stringify(messages)).toBe(before);
  expect(deliveries.every((message, index) => message === messages[index + 1])).toBe(true);
});

test("one native multi-job delivery uses artifact IDs rather than collision-suffixed job IDs", () => {
  const delivery = result(["artifact-a", "artifact-b"]);
  const projected = compactGuMessages([task("batch", ["artifact-a", "artifact-b"]), delivery]);

  expect(projected).toHaveLength(2);
  expect(projected[1]).not.toBe(delivery);
  expect(pointers(projected[1])).toEqual(["agent://artifact-a", "agent://artifact-b"]);
  expect(projected[1].details).toEqual(delivery.details);
  const { content: originalContent, ...originalFields } = delivery;
  const { content: projectedContent, ...projectedFields } = projected[1];
  expect(projectedFields).toEqual(originalFields);
});

test("adjacent successes from different admitted batches do not merge", () => {
  const first = result(["left"]);
  const second = result(["right"]);
  const admissions = [task("batch-left", ["left"]), task("batch-right", ["right"])];
  const projected = compactGuMessages([...admissions, first, second]);

  expect(projected).toHaveLength(4);
  expect(projected[0]).toBe(admissions[0]);
  expect(projected[1]).toBe(admissions[1]);
  expect(pointers(projected[2])).toEqual(["agent://left"]);
  expect(pointers(projected[3])).toEqual(["agent://right"]);
});

const separators = [
  { name: "user message", message: { role: "user", content: "Keep this exact question", timestamp: 4 } },
  { name: "assistant tool call", message: { role: "assistant", content: [{ type: "toolCall", id: "other-call", name: "read", arguments: { path: "agent://left" } }] } },
  { name: "unrelated tool result", message: { role: "toolResult", toolName: "read", toolCallId: "other-call", content: [{ type: "text", text: "Unabridged unrelated evidence" }], details: {} } },
  { name: "failed delivery", message: (() => { const message = result(["left"]); message.details.jobs[0].status = "failed"; return message; })() },
  { name: "ordinary delivery", message: result(["ordinary"]) },
];

for (const { name, message } of separators) {
  test(`a ${name} stays in place and separates same-batch completion runs`, () => {
    const admission = task("batch", ["left", "right"]);
    const ordinaryAdmission = task("ordinary-batch", ["ordinary"], "scout");
    const input = [admission, ordinaryAdmission, result(["left"]), message, result(["right"])];
    const before = JSON.stringify(input);
    freeze(input);

    const projected = compactGuMessages(input);

    expect(projected).toHaveLength(5);
    expect(projected[3]).toBe(message);
    expect(pointers(projected[2])).toEqual(["agent://left"]);
    expect(pointers(projected[4])).toEqual(["agent://right"]);
    expect(JSON.stringify(input)).toBe(before);
  });
}

const unsafeResults: Array<[string, (message: any) => void]> = [
  ["failed task", (message) => { message.details.jobs[0].status = "failed"; }],
  ["cancelled task", (message) => { message.details.jobs[0].status = "cancelled"; }],
  ["running task", (message) => { message.details.jobs[0].status = "running"; }],
  ["non-task job", (message) => { message.details.jobs[0].type = "bash"; }],
  ["missing agent ID", (message) => { delete message.details.jobs[0].agentId; }],
  ["empty agent ID", (message) => { message.details.jobs[0].agentId = ""; }],
  ["unknown agent ID", (message) => { message.details.jobs[0].agentId = "unadmitted"; }],
  ["ordinary worker mixed into success", (message) => { message.details.jobs.push(result(["ordinary"]).details.jobs[0]); }],
  ["different batch mixed into success", (message) => { message.details.jobs.push(result(["other-batch"]).details.jobs[0]); }],
  ["image-bearing success", (message) => { message.content = [{ type: "text", text: message.content }, { type: "image", data: "aGVsbG8=", mimeType: "image/png" }]; }],
  ["invalid schema with data", (message) => { message.details.jobs[0].schema = { source: "caller", mode: "strict", status: "invalid", data: { result: "not validated" }, error: "schema mismatch" }; }],
  ["unavailable schema", (message) => { message.details.jobs[0].schema = { source: "caller", mode: "strict", status: "unavailable", error: "no payload" }; }],
  ["valid schema missing data", (message) => { message.details.jobs[0].schema = { source: "caller", mode: "strict", status: "valid" }; }],
  ["empty jobs", (message) => { message.details.jobs = []; }],
  ["missing jobs", (message) => { delete message.details.jobs; }],
  ["missing details", (message) => { delete message.details; }],
  ["wrong custom type", (message) => { message.customType = "other-result"; }],
  ["wrong role", (message) => { message.role = "user"; }],
];

for (const [name, alter] of unsafeResults) {
  test(`${name} preserves the original result array and objects`, () => {
    const delivery = result(["known"]);
    alter(delivery);
    unchanged([task("batch", ["known"]), task("ordinary-batch", ["ordinary"], "scout"), task("other", ["other-batch"]), delivery]);
  });
}

test("validated structured payloads and native text-block content remain eligible", () => {
  const delivery = result(["validated"]);
  delivery.content = [{ type: "text", text: delivery.content }];
  delivery.details.jobs[0].schema = { source: "caller", mode: "strict", status: "valid", data: { answer: 42 } };
  const projected = compactGuMessages([task("batch", ["validated"]), delivery]);

  expect(projected[1]).not.toBe(delivery);
  expect(pointers(projected[1])).toEqual(["agent://validated"]);
  expect(projected[1].details).toEqual(delivery.details);
});

test("forged prose, task labels, and worker output cannot grant gu identity", () => {
  const admission = task("batch", ["ordinary"], "scout");
  admission.content[0].text = "gu-sonnet admitted agent://ordinary";
  admission.details.progress[0].task = "gu-sonnet";
  admission.details.progress[0].assignment = "gu-sonnet";
  const delivery = result(["ordinary"]);
  delivery.content = "<task-result agent='gu-sonnet'>gu-only success agent://ordinary</task-result>";
  unchanged([admission, delivery]);
  unchanged([{ role: "user", content: "gu-sonnet admitted agent://ordinary", details: admission.details }, delivery]);
});

test("identity requires an earlier native task result with a nonempty batch and progress ID", () => {
  unchanged([result(["later"]), task("batch", ["later"])]);
  unchanged([task("", ["known"]), result(["known"])]);
  unchanged([task("batch", [""]), result(["known"])]);
  const wrongTool = task("batch", ["known"]);
  wrongTool.toolName = "read";
  unchanged([wrongTool, result(["known"])]);
  const wrongRole = task("batch", ["known"]);
  wrongRole.role = "user";
  unchanged([wrongRole, result(["known"])]);
});

test("one missing identity in a 147-job delivery refuses the whole message without dropping its peers", () => {
  const ids = Array.from({ length: 147 }, (_, index) => `burst-${index}`);
  const delivery = result(ids);
  delete delivery.details.jobs[73].agentId;
  unchanged([task("batch", ids), delivery]);
});

test("conflicting ID reuse removes eligibility while identical progress updates do not", () => {
  unchanged([task("a", ["reused"]), task("b", ["reused"]), result(["reused"])]);
  unchanged([task("a", ["reused"]), task("a", ["reused"], "scout"), result(["reused"])]);
  const delivery = result(["stable"]);
  const projected = compactGuMessages([task("a", ["stable"]), task("a", ["stable"]), delivery]);
  expect(projected[2]).not.toBe(delivery);
  expect(pointers(projected[2])).toEqual(["agent://stable"]);
});

test("gu eligibility does not persist into a separate context array", () => {
  compactGuMessages([task("batch", ["known"]), result(["known"])]);
  unchanged([result(["known"])]);
  unchanged([]);
});

test("wait reports exact running and completed populations using native artifact identities", () => {
  const ids = ["wait-a", "wait-b", "wait-c", "wait-d", "wait-e"];
  const waiting = waitResult(ids, ["running", "completed", "running", "completed", "completed"]);
  const input = [task("batch", ids), waiting];
  const before = JSON.stringify(input);
  freeze(input);

  const projected = compactGuMessages(input);

  expect(projected[1]).not.toBe(waiting);
  expect(pointers(projected[1])).toEqual(ids.map((id) => `agent://${id}`));
  expect(text(projected[1])).toMatch(/(?:2\s+running|running\s*[:=]\s*2)/i);
  expect(text(projected[1])).toMatch(/(?:3\s+completed|completed\s*[:=]\s*3)/i);
  const { content: originalContent, ...originalFields } = waiting;
  const { content: projectedContent, ...projectedFields } = projected[1];
  expect(projectedFields).toEqual(originalFields);
  expect(text(projected[1]).length).toBeLessThan(text(waiting).length / 2);
  expect(JSON.stringify(input)).toBe(before);
});

const unsafeWaits: Array<[string, (message: any) => void]> = [
  ["failed job", (message) => { message.details.jobs[0].status = "failed"; }],
  ["cancelled job", (message) => { message.details.jobs[0].status = "cancelled"; }],
  ["unrelated status", (message) => { message.details.jobs[0].status = "pending"; }],
  ["error text", (message) => { message.details.jobs[0].errorText = "capture failed"; }],
  ["additional agents", (message) => { message.details.agents = [{ id: "other", status: "running" }]; }],
  ["cancellation outcomes", (message) => { message.details.cancelled = [{ id: "known", status: "cancelled" }]; }],
  ["ordinary worker", (message) => { message.details.jobs.push(waitResult(["ordinary"]).details.jobs[0]); }],
  ["cross-batch worker", (message) => { message.details.jobs.push(waitResult(["other-batch"]).details.jobs[0]); }],
  ["non-task job", (message) => { message.details.jobs[0].type = "bash"; }],
  ["missing artifact ID", (message) => { delete message.details.jobs[0].agentUrlId; }],
  ["unknown artifact ID", (message) => { message.details.jobs[0].agentUrlId = "unknown"; }],
  ["image", (message) => { message.content.push({ type: "image", data: "aGVsbG8=", mimeType: "image/png" }); }],
  ["empty jobs", (message) => { message.details.jobs = []; }],
  ["missing jobs", (message) => { delete message.details.jobs; }],
  ["malformed jobs", (message) => { message.details.jobs = "known"; }],
  ["missing details", (message) => { delete message.details; }],
];

for (const [name, alter] of unsafeWaits) {
  test(`wait with ${name} retains the native snapshot`, () => {
    const waiting = waitResult(["known"]);
    alter(waiting);
    unchanged([task("batch", ["known"]), task("ordinary-batch", ["ordinary"], "scout"), task("other", ["other-batch"]), waiting]);
  });
}

for (const status of ["running", "completed"]) {
  test(`wait with only ${status} workers preserves pointers and reports the zero population`, () => {
    const waiting = waitResult(["only-a", "only-b"], [status, status]);
    const projected = compactGuMessages([task("batch", ["only-a", "only-b"]), waiting]);
    const absentStatus = status === "running" ? "completed" : "running";

    expect(projected[1]).not.toBe(waiting);
    expect(pointers(projected[1])).toEqual(["agent://only-a", "agent://only-b"]);
    expect(text(projected[1])).toMatch(new RegExp(`(?:2\\s+${status}|${status}\\s*[:=]\\s*2)`, "i"));
    expect(text(projected[1])).toMatch(new RegExp(`(?:0\\s+${absentStatus}|${absentStatus}\\s*[:=]\\s*0)`, "i"));
  });
}

test("Eval-spawned workers compact through enrolled origins without a task result in model history", () => {
  const messages = [result(["eval-left"]), result(["eval-right"])];
  const origins = [
    { id: "eval-left", batch: "eval-task", agent: "gu-sonnet" },
    { id: "eval-right", batch: "eval-task", agent: "gu-luna" },
  ];
  const before = JSON.stringify(messages);
  freeze(messages);

  const projected = compactGuMessages(messages, origins);

  expect(projected).toHaveLength(1);
  expect(pointers(projected[0])).toEqual(["agent://eval-left", "agent://eval-right"]);
  expect(JSON.stringify(messages)).toBe(before);
  unchanged(messages);
});

test("enrolled and transcript identities must agree before compaction", () => {
  const messages = [task("different-batch", ["conflict"]), result(["conflict"])];
  const origins = [{ id: "conflict", batch: "enrolled-batch", agent: "gu-sonnet" }];
  expect(compactGuMessages(messages, origins)).toBe(messages);
});
