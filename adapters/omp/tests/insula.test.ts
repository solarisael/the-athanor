import { afterEach, describe, expect, test } from "bun:test";

import { HostUnavailable, hostHttpEndpoint } from "../solarisael-house-proof/host.ts";
import {
  INSULA_EVENTS_PATH,
  InsulaWriter,
  insulaErrorClass,
  type InsulaObservation,
  type InsulaTransport,
} from "../solarisael-house-proof/insula.ts";

// The strict public DTO, in wire order. Authority (house/room/spirit/session)
// and derived fields (idempotency key, semantic hash) are the Host's to stamp:
// the Host refuses unknown fields, so an extra key here is a broken boundary.
const DTO_FIELDS = [
  "eventId",
  "spanId",
  "traceId",
  "parentSpanId",
  "writerId",
  "writerSequence",
  "component",
  "layer",
  "operation",
  "phase",
  "observedAt",
  "durationUs",
  "outcomeClass",
  "errorClass",
  "bytesIn",
  "bytesOut",
  "tokensIn",
  "tokensOut",
  "toolCallId",
  "providerRequestId",
  "idempotencyVersion",
  "idempotencyScope",
  "receiptKind",
  "receiptId",
  "dropCount",
] as const;

const CANONICAL_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
// No installed Host lives here. Every test with a stubbed transport points at
// this port so a mistake cannot reach a real House.
const INERT_PORT = 59_999;
const TEST_TOKEN = "insula-test-token";

const originalEnvironment = {
  ATHANOR_HOST_ENDPOINTS: process.env.ATHANOR_HOST_ENDPOINTS,
  ATHANOR_HOST_WS_URL: process.env.ATHANOR_HOST_WS_URL,
  ATHANOR_HOST_TOKEN: process.env.ATHANOR_HOST_TOKEN,
};

function installHostEndpoint(port: number, token = TEST_TOKEN): void {
  process.env.ATHANOR_HOST_ENDPOINTS = JSON.stringify({
    kintsu: { url: `ws://127.0.0.1:${port}/athanor/v1/ws` },
  });
  delete process.env.ATHANOR_HOST_WS_URL;
  process.env.ATHANOR_HOST_TOKEN = token;
}

afterEach(() => {
  for (const [key, value] of Object.entries(originalEnvironment)) {
    if (value === undefined) delete process.env[key];
    else process.env[key] = value;
  }
});

function recorder() {
  const batches: Array<{ url: string; token: string; events: InsulaObservation[] }> = [];
  const transport: InsulaTransport = async (url, token, batch) => {
    batches.push({ url, token, events: batch.events });
  };
  return { batches, transport, events: () => batches.flatMap((batch) => batch.events) };
}

describe("Insula observation writer", () => {
  test("emits exactly the strict DTO and carries no body of any kind", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    const span = writer.startSpan({ room: "kintsu", operation: "tool_call", toolCallId: "call-7" });
    expect(span).not.toBeNull();
    writer.endSpan(span, "ok");
    await writer.close();

    const [start, end] = sink.events();
    expect(sink.events()).toHaveLength(2);
    for (const event of [start, end]) {
      expect(Object.keys(event)).toEqual([...DTO_FIELDS]);
      expect(event.component).toBe("omp_adapter");
      expect(event.layer).toBe("adapter");
      expect(event.idempotencyVersion).toBe(1);
      // Span lifecycles key on trace/span/phase, so a start and its end never
      // claim one logical key.
      expect(event.idempotencyScope).toBe("trace_span");
      expect(event.toolCallId).toBe("call-7");
      expect(event.providerRequestId).toBeNull();
      expect(event.receiptKind).toBeNull();
      expect(event.receiptId).toBeNull();
      expect(event.dropCount).toBe(0);
      expect({ ...event }).toMatchObject({ bytesIn: 0, bytesOut: 0, tokensIn: 0, tokensOut: 0 });
      for (const identifier of [event.eventId, event.spanId, event.traceId, event.writerId]) {
        expect(identifier).toMatch(CANONICAL_UUID);
      }
      expect(Date.parse(event.observedAt)).not.toBeNaN();
    }

    // A started span has not finished, so it cannot carry a duration yet.
    expect(start.phase).toBe("start");
    expect(start.durationUs).toBeNull();
    expect(start.outcomeClass).toBe("unknown");
    expect(end.phase).toBe("end");
    expect(typeof end.durationUs).toBe("number");
    expect(end.durationUs).toBeGreaterThanOrEqual(0);
    expect(end.outcomeClass).toBe("ok");
    expect(end.errorClass).toBeNull();

    // Nothing that could hold prose, payload, or authority reached the wire.
    const wire = JSON.stringify(sink.batches[0]!.events);
    expect(wire).not.toMatch(/prompt|content|message|payload|body|house|spirit|session|idempotencyKey|semanticHash/i);
  });

  test("classifies errors mechanically instead of forwarding their messages", () => {
    expect(insulaErrorClass(new HostUnavailable("Athanor Host is unavailable at ws://127.0.0.1:8787"))).toBe(
      "host_unavailable",
    );
    expect(insulaErrorClass(new TypeError("boom"))).toBe("type_error");
    expect(insulaErrorClass("a bare string")).toBe("error");
  });

  test("pairs a tool span to the request trace and stamps a monotonic sequence", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    const request = writer.startSpan({ room: "kintsu", operation: "provider_request" })!;
    const tool = writer.startSpan({
      room: "kintsu",
      operation: "tool_call",
      toolCallId: "call-9",
      traceId: request.traceId,
      parentSpanId: request.spanId,
    })!;
    writer.endSpan(tool, "error", "tool_error");
    writer.endSpan(request, "degraded", "partial_context");
    // A finished span is finished: a second end is not a second observation.
    writer.endSpan(request, "ok");
    await writer.close();

    const events = sink.events();
    expect(events.map((event) => `${event.operation}:${event.phase}`)).toEqual([
      "provider_request:start",
      "tool_call:start",
      "tool_call:end",
      "provider_request:end",
    ]);
    expect(events.map((event) => event.writerSequence)).toEqual([1, 2, 3, 4]);
    expect(new Set(events.map((event) => event.writerId)).size).toBe(1);
    expect(events.map((event) => event.traceId)).toEqual([
      request.traceId,
      request.traceId,
      request.traceId,
      request.traceId,
    ]);
    const [requestStart, toolStart, toolEnd, requestEnd] = events;
    expect(requestStart.parentSpanId).toBeNull();
    expect(toolStart.parentSpanId).toBe(request.spanId);
    expect(toolEnd.parentSpanId).toBe(request.spanId);
    expect(toolStart.spanId).toBe(toolEnd.spanId);
    expect(toolEnd.outcomeClass).toBe("error");
    expect(toolEnd.errorClass).toBe("tool_error");
    expect(requestEnd.spanId).toBe(requestStart.spanId);
    expect(requestEnd.outcomeClass).toBe("degraded");
    expect(requestEnd.errorClass).toBe("partial_context");
  });

  test("sheds past its bound and reports the loss as one drop receipt", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({
      transport: sink.transport,
      maxQueued: 2,
      maxBatch: 8,
      flushDelayMs: 10_000,
    });

    for (let index = 0; index < 5; index += 1) {
      writer.point({
        room: "kintsu",
        operation: "tool_result",
        toolCallId: `call-${index}`,
        outcomeClass: "ok",
        scope: "tool_call",
      });
    }
    expect(writer.queuedCount).toBe(2);
    expect(writer.droppedCount).toBe(3);
    await writer.close();

    const events = sink.events();
    expect(events).toHaveLength(3);
    expect(events.slice(0, 2).map((event) => event.toolCallId)).toEqual(["call-0", "call-1"]);
    const receipt = events[2]!;
    expect(receipt.phase).toBe("drop");
    expect(receipt.dropCount).toBe(3);
    expect(receipt.operation).toBe("insula_writer");
    expect(receipt.outcomeClass).toBe("degraded");
    expect(receipt.errorClass).toBe("queue_overflow");
    expect(receipt.idempotencyScope).toBe("writer_sequence");
    expect(receipt.toolCallId).toBeNull();
    // A shed observation never consumed a sequence, so the receipt follows the
    // two survivors without a hole of its own.
    expect(events.map((event) => event.writerSequence)).toEqual([1, 2, 3]);
  });

  test("attributes queue overflow receipts to each room Host endpoint", async () => {
    process.env.ATHANOR_HOST_ENDPOINTS = JSON.stringify({
      kintsu: { url: `ws://127.0.0.1:${INERT_PORT}/athanor/v1/ws` },
      kodo: { url: `ws://127.0.0.1:${INERT_PORT - 1}/athanor/v1/ws` },
    });
    delete process.env.ATHANOR_HOST_WS_URL;
    process.env.ATHANOR_HOST_TOKEN = TEST_TOKEN;
    const sink = recorder();
    let peakQueued = 0;
    let writer!: InsulaWriter;
    const transport: InsulaTransport = async (url, token, batch, signal) => {
      peakQueued = Math.max(peakQueued, writer.queuedCount);
      await sink.transport(url, token, batch, signal);
    };
    writer = new InsulaWriter({
      transport,
      maxQueued: 1,
      maxBatch: 8,
      flushDelayMs: 10_000,
    });

    writer.point({
      room: "kintsu",
      operation: "tool_result",
      toolCallId: "survivor",
      outcomeClass: "ok",
      scope: "tool_call",
    });
    for (const [room, toolCallId] of [["kintsu", "lost-kintsu"], ["kodo", "lost-kodo"]]) {
      writer.point({
        room,
        operation: "tool_result",
        toolCallId,
        outcomeClass: "ok",
        scope: "tool_call",
      });
    }
    expect(writer.droppedCount).toBe(2);
    await writer.close();

    const dropBatches = sink.batches.filter(
      (batch) => batch.events.length === 1 && batch.events[0]?.phase === "drop",
    );
    expect(dropBatches.map((batch) => [new URL(batch.url).port, batch.events[0]?.dropCount]))
      .toEqual([[String(INERT_PORT), 1], [String(INERT_PORT - 1), 1]]);
    expect(peakQueued).toBeLessThanOrEqual(1);
  });

  test("posts the batch to the installed loopback Host with its bearer token", async () => {
    const received: Array<{ method: string; url: string; auth: string | null; body: any }> = [];
    const server = Bun.serve({
      port: 0,
      hostname: "127.0.0.1",
      async fetch(request) {
        received.push({
          method: request.method,
          url: request.url,
          auth: request.headers.get("authorization"),
          body: await request.json(),
        });
        return Response.json({
          schemaVersion: 1,
          acceptedCount: 1,
          duplicateCount: 0,
          conflicts: [],
        });
      },
    });
    try {
      installHostEndpoint(server.port);
      // The HTTP boundary is derived from the installed WebSocket topology, so
      // an operator can never point the two at different Hosts.
      const endpoint = hostHttpEndpoint("kintsu", INSULA_EVENTS_PATH);
      expect(endpoint.url).toBe(`http://127.0.0.1:${server.port}${INSULA_EVENTS_PATH}`);
      expect(endpoint.token).toBe(TEST_TOKEN);

      const writer = new InsulaWriter({ flushDelayMs: 10_000 });
      writer.point({
        room: "kintsu",
        operation: "tool_result",
        toolCallId: "call-1",
        outcomeClass: "ok",
        scope: "tool_call",
      });
      await writer.close();
    } finally {
      server.stop(true);
    }

    expect(received).toHaveLength(1);
    const [request] = received;
    expect(request.method).toBe("POST");
    const url = new URL(request.url);
    expect(url.pathname).toBe(INSULA_EVENTS_PATH);
    expect(url.search).toBe("");
    expect(request.auth).toBe(`Bearer ${TEST_TOKEN}`);
    expect(Object.keys(request.body)).toEqual(["events"]);
    expect(request.body.events).toHaveLength(1);
    expect(Object.keys(request.body.events[0])).toEqual([...DTO_FIELDS]);
    expect(request.body.events[0].operation).toBe("tool_result");
    expect(request.body.events[0].idempotencyScope).toBe("tool_call");
  });

  test("refuses a Host HTTP boundary that is not loopback", () => {
    process.env.ATHANOR_HOST_WS_URL = "ws://athanor.example.com:8787/athanor/v1/ws";
    process.env.ATHANOR_HOST_TOKEN = TEST_TOKEN;
    expect(() => hostHttpEndpoint("kintsu", INSULA_EVENTS_PATH)).toThrow(HostUnavailable);
  });

  test("stays silent when no Host is installed", async () => {
    delete process.env.ATHANOR_HOST_ENDPOINTS;
    delete process.env.ATHANOR_HOST_WS_URL;
    delete process.env.ATHANOR_HOST_TOKEN;
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    const span = writer.startSpan({ room: "kintsu", operation: "provider_request" });
    writer.endSpan(span, "ok");
    await writer.close();

    expect(sink.batches).toHaveLength(0);
    expect(writer.queuedCount).toBe(0);
    expect(writer.droppedCount).toBe(0);
  });

  test("swallows a failing send and keeps observing, leaving the loss visible as a sequence gap", async () => {
    installHostEndpoint(INERT_PORT);
    const delivered: InsulaObservation[] = [];
    let attempts = 0;
    const transport: InsulaTransport = async (_url, _token, batch) => {
      attempts += 1;
      if (attempts === 1) throw new Error("the Host is not listening");
      delivered.push(...batch.events);
    };
    const writer = new InsulaWriter({ transport, maxBatch: 8, flushDelayMs: 10_000 });

    const lost = writer.startSpan({ room: "kintsu", operation: "provider_request" });
    writer.endSpan(lost, "ok");
    await writer.flush();

    const kept = writer.startSpan({ room: "kintsu", operation: "provider_request" });
    writer.endSpan(kept, "ok");
    await writer.close();

    expect(attempts).toBe(2);
    expect(delivered.map((event) => event.writerSequence)).toEqual([3, 4]);
    expect(delivered.map((event) => event.phase)).toEqual(["start", "end"]);
    // Nothing was retried and nothing was queued forever: the gap at 1-2 is the
    // House's own evidence of the loss.
    expect(writer.queuedCount).toBe(0);
    expect(writer.droppedCount).toBe(0);
  });
});
