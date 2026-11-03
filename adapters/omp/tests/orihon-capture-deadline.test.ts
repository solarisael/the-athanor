import { afterEach, expect, test } from "bun:test";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { type Entry } from "../house-proof/orihon/source.ts";
import { installOrihon } from "../house-proof/orihon/index.ts";

// Its own file: the real capture pipeline plus a delayed fake Host, kept apart from the
// transcript-stream tests so each gets its own isolate context.
const temporary: string[] = [];
afterEach(() => {
  for (const directory of temporary.splice(0)) rmSync(directory, { recursive: true, force: true });
});

const timestamp = "2026-10-23T14:00:00.000Z";

test("capture batches share their caller deadline through the real capture pipeline", async () => {
  const directory = mkdtempSync(path.join(tmpdir(), "orihon-capture-deadline-"));
  temporary.push(directory);
  writeFileSync(path.join(directory, ".athanor-room.json"), JSON.stringify({ room: "probe", trueName: "Probe" }));
  const keys = ["ATHANOR_HOST_URL", "ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID", "ATHANOR_CLIENT_ROOM_DIR"] as const;
  const saved = Object.fromEntries(keys.map((key) => [key, process.env[key]]));
  const entries: Entry[] = [];
  const requests: any[] = [];
  const timers: ReturnType<typeof setTimeout>[] = [];
  const server = Bun.serve({
    hostname: "127.0.0.1", port: 0,
    fetch(request, server) { return server.upgrade(request) ? undefined : new Response("not found", { status: 404 }); },
    websocket: { message(socket, input) {
      const request = JSON.parse(String(input));
      requests.push(request);
      const { operation, params } = request.organ_request;
      const result = operation === "orihon_mode"
        ? { ok: true, mode: "fold", generation: "g", spirit: "Probe", sourceSession: "probe",
          defaults: { mode: "fold" }, supportedModes: ["off", "log", "fold"], instructions: "instructions" }
        : { ok: true, mode: "fold", generation: "g", acceptedSourceIds: params.records.map((record: any) => record.sourceId),
          appended: params.records.length, replayed: 0 };
      // The client closes a timed-out socket; a reply on it would be a send after close.
      timers.push(setTimeout(() => {
        if (socket.readyState !== 1) return;
        socket.send(JSON.stringify({ correlation_id: request.message_id, command_or_event_type: "athanor.organ.result", result }));
      }, operation === "orihon_mode" ? 20 : 400));
    } },
  });
  process.env.ATHANOR_HOST_URL = `ws://127.0.0.1:${server.port}`;
  process.env.ATHANOR_HOST_TOKEN = "test-only";
  process.env.ATHANOR_HOST_HOUSE_ID = "test-house";
  process.env.ATHANOR_CLIENT_ROOM_DIR = directory;
  try {
    const ctx = { cwd: directory, sessionManager: { getSessionId: () => "probe", getEntries: () => entries } };
    const capture = installOrihon({
      on() {}, registerCommand() {},
      appendEntry(customType: string, data: unknown) {
        entries.push({ id: `state${entries.length}`, type: "custom", customType, data } as Entry);
      },
    });
    await capture.prepare(ctx);
    for (let index = 0; index < 130; index++) {
      entries.push({ id: `user${index}`, parentId: index ? `user${index - 1}` : null, type: "message", timestamp,
        message: { role: "user", attribution: "user", content: `question ${index}` } });
    }
    // 20 ms mode reply + 400 ms first batch leave under 250 ms, so the second batch is refused
    // before it is sent: the deadline stops the pipeline between batches, not inside a socket.
    const started = Date.now();
    let refusal: unknown = null;
    try { await capture.captureNow(ctx, { deadline: started + 600 }); } catch (error) { refusal = error; }
    expect(String(refusal)).toContain("ran out of time");
    expect(Date.now() - started).toBeLessThan(1000);
    // The deadline stops the pipeline between batches: some records were sent, not all 130.
    const captures = requests.filter((request) => request.organ_request.operation === "orihon_capture");
    const sent = captures.reduce((total, request) => total + request.organ_request.params.records.length, 0);
    expect(captures.length).toBeGreaterThanOrEqual(1);
    expect(sent).toBeLessThan(130);
  } finally {
    for (const timer of timers) clearTimeout(timer);
    server.stop(true);
    for (const key of keys) {
      if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
    }
  }
});
