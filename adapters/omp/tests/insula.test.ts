import { afterEach, describe, expect, test } from "bun:test";

import { HostUnavailable, hostHttpEndpoint } from "../solarisael-house-proof/host.ts";
import {
  INSULA_EVENTS_PATH,
  InsulaWriter,
  insulaErrorClass,
  noteInsulaProviderRequestId,
  type InsulaObservation,
  type InsulaTransport,
} from "../solarisael-house-proof/insula.ts";
import {
  INSULA_VITALS_PATH,
  insulaCockpitLines,
  insulaUnavailableLines,
  parseInsulaVitals,
  parseInsulaVitalsRange,
  readInsulaVitals,
  summarizeInsulaVitals,
} from "../solarisael-house-proof/vitals.ts";

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

// One Vitals minute row as the Host serves it, with every field this client
// does not read still present: an extra column is the Host's business.
function vitalsRow(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  return {
    minute: "2026-08-20T12:00:00.000Z",
    houseId: "solarisael",
    room: "kintsu",
    spirit: "Kintsu",
    component: "omp_adapter",
    layer: "adapter",
    operation: "provider_request",
    phase: "end",
    outcomeClass: "ok",
    eventCount: 1,
    durationUsSum: 1_000,
    durationUsMax: 1_000,
    bytesInSum: 0,
    bytesOutSum: 0,
    tokensInSum: 0,
    tokensOutSum: 0,
    dropCountSum: 0,
    sourceFirstSequence: 1,
    sourceLastSequence: 1,
    sourceFirstObservedAt: "2026-08-20T12:00:01.000Z",
    sourceLastObservedAt: "2026-08-20T12:00:02.000Z",
    sourceCoverageHash: "0".repeat(64),
    updatedAt: "2026-08-20T12:01:00.000Z",
    ...overrides,
  };
}

function vitalsResponse(
  rows: Array<Record<string, unknown>>,
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    schemaVersion: 1,
    queryName: "insula.vitals.minute",
    queryVersion: 1,
    houseId: "solarisael",
    room: "kintsu",
    spirit: "Kintsu",
    start: "2026-08-20T12:00:00.000Z",
    end: "2026-08-20T13:00:00.000Z",
    limit: 1_000,
    truncated: false,
    rows,
    ...overrides,
  };
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

  test("emits a successful tool result point without retaining result content", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    writer.point({
      room: "kintsu",
      operation: "tool_result",
      toolCallId: "call-success",
      outcomeClass: "ok",
      scope: "tool_call",
    });
    await writer.close();

    const [result] = sink.events();
    expect(result).toMatchObject({
      operation: "tool_result",
      phase: "point",
      outcomeClass: "ok",
      errorClass: null,
      toolCallId: "call-success",
      bytesIn: 0,
      bytesOut: 0,
      tokensIn: 0,
      tokensOut: 0,
      idempotencyScope: "tool_call",
    });
    expect(JSON.stringify(result)).not.toMatch(/content|message|payload|body/i);
  });

  test("uses the provider-reported duration instead of including later tool work", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    const request = writer.startSpan({ room: "kintsu", operation: "provider_request" });
    writer.endSpan(request, "ok", null, 1_234_000);
    await writer.close();

    const end = sink.events().find((event) => event.phase === "end");
    expect(end?.durationUs).toBe(1_234_000);
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

  test("carries measured usage and the provider request id into the settlement", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    const request = writer.startSpan({ room: "kintsu", operation: "provider_request" })!;
    noteInsulaProviderRequestId(request, "req_01HZY.tail");
    writer.endSpan(request, "ok");
    writer.point({
      room: "kintsu",
      operation: "provider_usage",
      traceId: request.traceId,
      parentSpanId: request.spanId,
      providerRequestId: request.providerRequestId,
      outcomeClass: "ok",
      tokensIn: 1_024.4,
      tokensOut: Number.MAX_SAFE_INTEGER,
      scope: "provider_request",
    });
    await writer.close();

    const [start, end, usage] = sink.events();
    expect(sink.events()).toHaveLength(3);
    expect(Object.keys(usage!)).toEqual([...DTO_FIELDS]);
    // The start measured nothing and the provider had not answered yet, so it
    // can carry neither usage nor the request id.
    expect({ ...start! }).toMatchObject({ tokensIn: 0, tokensOut: 0, providerRequestId: null });
    // Usage lives on its own point, so the span never double-counts a token.
    expect({ ...end! }).toMatchObject({ tokensIn: 0, tokensOut: 0, providerRequestId: "req_01HZY.tail" });
    expect(end!.idempotencyScope).toBe("trace_span");
    expect(usage!.operation).toBe("provider_usage");
    expect(usage!.phase).toBe("point");
    expect(usage!.outcomeClass).toBe("ok");
    expect(usage!.errorClass).toBeNull();
    expect(usage!.idempotencyScope).toBe("provider_request");
    expect(usage!.providerRequestId).toBe("req_01HZY.tail");
    expect(usage!.traceId).toBe(request.traceId);
    expect(usage!.parentSpanId).toBe(request.spanId);
    expect(usage!.tokensIn).toBe(1_024);
    // Clamped to the largest count the Host accepts rather than refused.
    expect(usage!.tokensOut).toBe(1_099_511_627_776);
  });

  test("keeps unknown usage query-visible as a degraded point with no counts", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    const request = writer.startSpan({ room: "kintsu", operation: "provider_request" })!;
    writer.endSpan(request, "cancelled", "provider_aborted");
    writer.point({
      room: "kintsu",
      operation: "provider_usage",
      traceId: request.traceId,
      parentSpanId: request.spanId,
      outcomeClass: "degraded",
      errorClass: "usage_unavailable",
      tokensIn: Number.NaN,
      tokensOut: -12,
    });
    await writer.close();

    const usage = sink.events()[2]!;
    expect(usage.outcomeClass).toBe("degraded");
    expect(usage.errorClass).toBe("usage_unavailable");
    expect(usage.tokensIn).toBe(0);
    expect(usage.tokensOut).toBe(0);
    // Without a provider request id the point falls back to its own span key
    // rather than claiming a correlation it does not have.
    expect(usage.idempotencyScope).toBe("trace_span");
    expect(usage.providerRequestId).toBeNull();
  });

  test("refuses a provider-request-scoped point with no request id", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    writer.point({
      room: "kintsu",
      operation: "provider_usage",
      outcomeClass: "degraded",
      errorClass: "usage_unavailable",
      scope: "provider_request",
    });
    await writer.close();

    expect(sink.events()).toHaveLength(0);
  });

  test("never binds a request id to a span that already settled", async () => {
    installHostEndpoint(INERT_PORT);
    const sink = recorder();
    const writer = new InsulaWriter({ transport: sink.transport, flushDelayMs: 10_000 });

    const request = writer.startSpan({ room: "kintsu", operation: "provider_request" })!;
    writer.endSpan(request, "ok");
    noteInsulaProviderRequestId(request, "req_late");
    await writer.close();

    expect(request.providerRequestId).toBeNull();
    expect(sink.events()[1]!.providerRequestId).toBeNull();
  });
});

describe("Insula Vitals cockpit", () => {
  test("accepts exactly three ranges and refuses everything else", () => {
    expect(parseInsulaVitalsRange(undefined)).toBe("1h");
    expect(parseInsulaVitalsRange("")).toBe("1h");
    expect(parseInsulaVitalsRange("15m")).toBe("15m");
    expect(parseInsulaVitalsRange(" 24H ")).toBe("24h");
    expect(parseInsulaVitalsRange("2h")).toBeNull();
    expect(parseInsulaVitalsRange("all")).toBeNull();
  });

  test("reads the installed loopback Host and sends no authority field", async () => {
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
        return Response.json(vitalsResponse([vitalsRow()]));
      },
    });
    let parsed;
    try {
      installHostEndpoint(server.port);
      parsed = await readInsulaVitals("kintsu", "1h", Date.parse("2026-08-20T13:00:00.000Z"));
    } finally {
      server.stop(true);
    }

    expect(received).toHaveLength(1);
    const [request] = received;
    expect(request!.method).toBe("POST");
    const url = new URL(request!.url);
    expect(url.pathname).toBe(INSULA_VITALS_PATH);
    // The Host refuses any query string on this router.
    expect(url.search).toBe("");
    expect(request!.auth).toBe(`Bearer ${TEST_TOKEN}`);
    // House, room, spirit, and session are the Host's to stamp: the request has
    // no field to put them in, so authority is refused by omission.
    expect(Object.keys(request!.body)).toEqual(["start", "end", "limit"]);
    expect(request!.body.start).toBe("2026-08-20T12:00:00.000Z");
    expect(request!.body.end).toBe("2026-08-20T13:00:00.000Z");
    expect(request!.body.limit).toBe(1_000);
    expect(JSON.stringify(request!.body)).not.toMatch(/house|room|spirit|session|token/i);

    expect(parsed!.houseId).toBe("solarisael");
    expect(parsed!.truncated).toBe(false);
    expect(parsed!.rows).toHaveLength(1);
    expect(parsed!.rows[0]!.operation).toBe("provider_request");
  });

  test("treats a Host that refuses or answers off-shape as unavailable", async () => {
    const server = Bun.serve({
      port: 0,
      hostname: "127.0.0.1",
      fetch(request) {
        if (new URL(request.url).pathname !== INSULA_VITALS_PATH) return new Response("no", { status: 404 });
        return Response.json({ schemaVersion: 1, error: "insula_busy" }, { status: 429 });
      },
    });
    try {
      installHostEndpoint(server.port);
      await expect(readInsulaVitals("kintsu", "15m")).rejects.toThrow(HostUnavailable);
    } finally {
      server.stop(true);
    }

    // A shape the Host did not promise is absence, never a partial count.
    expect(() => parseInsulaVitals(vitalsResponse([], { truncated: "no" }))).toThrow(HostUnavailable);
    expect(() => parseInsulaVitals(vitalsResponse([], { rows: null }))).toThrow(HostUnavailable);
    expect(() => parseInsulaVitals(vitalsResponse([vitalsRow({ eventCount: "3" })]))).toThrow(HostUnavailable);
    expect(() => parseInsulaVitals(vitalsResponse([vitalsRow({ outcomeClass: "" })]))).toThrow(HostUnavailable);
    expect(() => parseInsulaVitals(vitalsResponse([], { limit: 1_001 }))).toThrow(HostUnavailable);
    expect(() => parseInsulaVitals(vitalsResponse([vitalsRow(), vitalsRow()], { limit: 1 })))
      .toThrow(HostUnavailable);
  });

  test("refuses a Vitals boundary that is not the installed loopback Host", async () => {
    process.env.ATHANOR_HOST_WS_URL = "ws://athanor.example.com:8787/athanor/v1/ws";
    process.env.ATHANOR_HOST_TOKEN = TEST_TOKEN;
    await expect(readInsulaVitals("kintsu", "1h")).rejects.toThrow(HostUnavailable);
  });

  test("aggregates settled requests, unknown usage, tool calls, and drops", () => {
    const response = parseInsulaVitals(vitalsResponse([
      vitalsRow({ eventCount: 3, durationUsSum: 3_000_000, durationUsMax: 2_000_000 }),
      vitalsRow({ outcomeClass: "degraded", durationUsSum: 500_000, durationUsMax: 500_000 }),
      vitalsRow({ outcomeClass: "error", durationUsSum: 0, durationUsMax: null }),
      vitalsRow({
        operation: "provider_usage",
        phase: "point",
        eventCount: 3,
        durationUsSum: 0,
        durationUsMax: null,
        tokensInSum: 1_200,
        tokensOutSum: 340,
      }),
      vitalsRow({
        operation: "provider_usage",
        phase: "point",
        outcomeClass: "degraded",
        durationUsSum: 0,
        durationUsMax: null,
        sourceLastObservedAt: "2026-08-20T12:30:09.000Z",
      }),
      vitalsRow({ operation: "tool_call", eventCount: 7, durationUsSum: 70_000, durationUsMax: 20_000 }),
      vitalsRow({
        operation: "insula_writer",
        phase: "drop",
        outcomeClass: "degraded",
        durationUsSum: 0,
        durationUsMax: null,
        dropCountSum: 4,
      }),
    ]));
    const summary = summarizeInsulaVitals("1h", response);

    expect(summary.requests.settled).toBe(5);
    expect(summary.requests.outcomes).toEqual([
      { outcomeClass: "ok", count: 3 },
      { outcomeClass: "degraded", count: 1 },
      { outcomeClass: "error", count: 1 },
    ]);
    expect(summary.requests.meanUs).toBe(700_000);
    expect(summary.requests.maxUs).toBe(2_000_000);
    expect(summary.usage).toEqual({ measured: 3, unknown: 1, tokensIn: 1_200, tokensOut: 340 });
    expect(summary.toolCalls).toBe(7);
    expect(summary.errorEvents).toBe(1);
    expect(summary.degradedEvents).toBe(3);
    expect(summary.drops).toBe(4);
    expect(summary.latestObservedAt).toBe("2026-08-20T12:30:09.000Z");

    const lines = insulaCockpitLines(summary);
    expect(lines[0]).toBe("window 1h · 2026-08-20T12:00Z → 2026-08-20T13:00Z");
    expect(lines[1]).toBe("stamped solarisael · kintsu/Kintsu");
    expect(lines[2]).toBe("provider requests 5 settled · ok 3 · degraded 1 · error 1");
    expect(lines[3]).toBe("latency 700ms mean · 2.0s max");
    expect(lines[4]).toBe("usage 3 measured · 1 unknown");
    expect(lines[5]).toBe("tokens in 1,200 · out 340");
    expect(lines[6]).toBe("tool calls 7 · errors 1 · degraded 3 · writer drops 4");
    expect(lines[7]).toBe("latest observation 2026-08-20T12:30:09Z");
    // Nothing was cut off, so the page never claims it was.
    expect(lines.some((line) => line.includes("row limit"))).toBe(false);
  });

  test("says empty, truncated, and unavailable in three different voices", () => {
    const empty = insulaCockpitLines(summarizeInsulaVitals("15m", parseInsulaVitals(vitalsResponse([]))));
    expect(empty).toContain("provider requests 0 settled");
    expect(empty).toContain("latency unmeasured mean · unmeasured max");
    expect(empty).toContain("no observation in this window");

    const truncated = insulaCockpitLines(
      summarizeInsulaVitals("24h", parseInsulaVitals(vitalsResponse([vitalsRow()], { truncated: true }))),
    );
    expect(truncated.at(-1)).toBe("row limit 1,000 reached · these counts are partial");

    // A Host that cannot answer never becomes a page full of zeros.
    expect(insulaUnavailableLines("1h", new HostUnavailable("closed"))).toEqual([
      "window 1h · Host Vitals unavailable (host_unavailable)",
      "no counts shown: this is absence, not zero",
    ]);
  });
});
