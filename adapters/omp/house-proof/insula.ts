// Insula observation emitter for the OMP adapter.
// Silhouette: mechanical, body-free lifecycle observations, bounded in memory,
// posted best effort to the installed Host Insula boundary.
//
// Insula observes and commands nothing. Nothing here may block, delay, or alter
// a turn: endpoint resolution, queueing, and sending all fail silently. What a
// swallowed failure costs is still visible to the House, because the process
// writer stamps a monotonic sequence and a gap in it is evidence of loss.
//
// The wire shape is the strict public DTO and nothing else. No prose, prompt,
// payload, or content ever enters an observation, and the House/room/spirit/
// session authority plus every derived field is refused here by omission: the
// Host stamps those from its own trusted binding.
//
// Span lifecycles use the `trace_span` idempotency scope, never `tool_call`:
// the v1 tool_call recipe keys on house/room/component/toolCallId alone, so a
// start and its end would claim one logical key with two semantic hashes and
// the Host would report the pair as key reuse. `trace_span` keys on
// trace/span/phase, and toolCallId still rides along as correlation.
// A single point may instead claim `provider_request`, which keys on the
// provider's own request id and is the one scope that stays idempotent across
// processes; it is refused outright without that id.

import { hostHttpEndpoint } from "./host.ts";

export const INSULA_EVENTS_PATH = "/athanor/v1/insula/events";

const COMPONENT = "omp_adapter";
const LAYER = "adapter";
const WRITER_OPERATION = "insula_writer";

const MAX_BATCH_EVENTS = 128; // The Host refuses a larger batch.
const MAX_QUEUED_EVENTS = 512;
const MAX_DROP_COUNT = 1_000_000_000;
const MAX_DURATION_US = 86_400_000_000;
const MAX_TOKENS = 1_099_511_627_776; // The Host refuses a larger count.
const FLUSH_DELAY_MS = 200;
const POST_TIMEOUT_MS = 2_000;
const CLOSE_TIMEOUT_MS = 750;

const CANONICAL_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const MECHANICAL_NAME = /^[a-z0-9][a-z0-9_.:-]{0,63}$/;
const OPAQUE_IDENTIFIER = /^[A-Za-z0-9_.:/@-]{1,256}$/;

export type InsulaPhase = "start" | "end" | "point" | "drop";
export type InsulaOutcome =
  | "ok"
  | "refused"
  | "error"
  | "timeout"
  | "cancelled"
  | "degraded"
  | "unknown";
export type InsulaScope =
  | "writer_sequence"
  | "tool_call"
  | "provider_request"
  | "trace_span"
  | "room_operation";

/** Exactly the fields the Host's strict DTO accepts, in wire order. */
export type InsulaObservation = {
  eventId: string;
  spanId: string;
  traceId: string;
  parentSpanId: string | null;
  writerId: string;
  writerSequence: number;
  component: string;
  layer: string;
  operation: string;
  phase: InsulaPhase;
  observedAt: string;
  durationUs: number | null;
  outcomeClass: InsulaOutcome;
  errorClass: string | null;
  bytesIn: number;
  bytesOut: number;
  tokensIn: number;
  tokensOut: number;
  toolCallId: string | null;
  providerRequestId: string | null;
  idempotencyVersion: number;
  idempotencyScope: InsulaScope;
  receiptKind: string | null;
  receiptId: string | null;
  dropCount: number;
};

export type InsulaSpan = {
  readonly room: string;
  readonly traceId: string;
  readonly spanId: string;
  readonly parentSpanId: string | null;
  readonly operation: string;
  readonly toolCallId: string | null;
  readonly startedAt: number;
  readonly startedAtEpochMs: number;
  // Learned mid-span from the provider response, so it can only reach the wire
  // on the settlement: the start was already posted without it.
  providerRequestId: string | null;
  finished: boolean;
};

export type InsulaSpanRequest = {
  room: string;
  operation: string;
  toolCallId?: string | null;
  traceId?: string | null;
  parentSpanId?: string | null;
};

export type InsulaPointRequest = InsulaSpanRequest & {
  outcomeClass: InsulaOutcome;
  errorClass?: string | null;
  scope?: InsulaScope;
  durationUs?: number | null;
  tokensIn?: number | null;
  tokensOut?: number | null;
  providerRequestId?: string | null;
};

export type InsulaEndpoint = { url: string; token: string };

export type InsulaTransport = (
  url: string,
  token: string,
  batch: { events: InsulaObservation[] },
  signal: AbortSignal,
) => Promise<void>;

type ObservationDraft = {
  traceId: string;
  spanId: string;
  parentSpanId: string | null;
  operation: string;
  phase: InsulaPhase;
  outcomeClass: InsulaOutcome;
  errorClass: string | null;
  durationUs: number | null;
  tokensIn: number;
  tokensOut: number;
  toolCallId: string | null;
  providerRequestId: string | null;
  scope: InsulaScope;
  dropCount: number;
};

function mechanicalName(value: unknown): string | null {
  const candidate = String(value ?? "").trim();
  return MECHANICAL_NAME.test(candidate) ? candidate : null;
}

function opaqueIdentifier(value: unknown): string | null {
  const candidate = String(value ?? "").trim();
  return OPAQUE_IDENTIFIER.test(candidate) ? candidate : null;
}

function canonicalUuid(value: unknown): string | null {
  const candidate = String(value ?? "").trim();
  return CANONICAL_UUID.test(candidate) ? candidate : null;
}

/** A class name, never a message: `HostUnavailable` becomes `host_unavailable`. */
export function insulaErrorClass(error: unknown): string {
  const name = error instanceof Error && error.name ? error.name : "error";
  const mechanical = name
    .replace(/([a-z0-9])([A-Z])/g, "$1_$2")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+/, "")
    .slice(0, 64);
  return mechanicalName(mechanical) ?? "error";
}

function boundedDuration(value: number | null | undefined): number | null {
  if (value == null || !Number.isFinite(value)) return null;
  return Math.min(MAX_DURATION_US, Math.max(0, Math.round(value)));
}

function boundedTokens(value: number | null | undefined): number {
  if (value == null || !Number.isFinite(value)) return 0;
  return Math.min(MAX_TOKENS, Math.max(0, Math.round(value)));
}

function elapsedMicroseconds(startedAt: number): number {
  return boundedDuration((performance.now() - startedAt) * 1_000) ?? 0;
}

function bounded(timeoutMs: number): Promise<void> {
  return new Promise((resolve) => {
    const timer = setTimeout(resolve, Math.max(0, timeoutMs));
    (timer as { unref?: () => void }).unref?.();
  });
}

export async function postInsulaBatch(
  url: string,
  token: string,
  batch: { events: InsulaObservation[] },
  signal: AbortSignal,
): Promise<void> {
  const response = await fetch(url, {
    method: "POST",
    headers: { "content-type": "application/json", authorization: `Bearer ${token}` },
    body: JSON.stringify(batch),
    signal,
  });
  // The Host answers with an ingest receipt. It is drained to release the
  // socket and then forgotten: Insula never instructs the adapter.
  await response.arrayBuffer().catch(() => undefined);
  if (!response.ok) throw new Error(`Insula ingest refused with status ${response.status}`);
}

export class InsulaWriter {
  readonly writerId: string;
  readonly #transport: InsulaTransport;
  readonly #maxQueued: number;
  readonly #maxBatch: number;
  readonly #flushDelayMs: number;
  #sequence = 0;
  #buckets = new Map<string, { token: string; events: InsulaObservation[] }>();
  #queued = 0;
  #droppedByEndpoint = new Map<string, { endpoint: InsulaEndpoint; count: number }>();
  #timer: ReturnType<typeof setTimeout> | null = null;
  #inFlight: Promise<void> | null = null;

  constructor(options: {
    transport?: InsulaTransport;
    writerId?: string;
    maxQueued?: number;
    maxBatch?: number;
    flushDelayMs?: number;
  } = {}) {
    this.writerId = canonicalUuid(options.writerId) ?? crypto.randomUUID();
    this.#transport = options.transport ?? postInsulaBatch;
    this.#maxQueued = Math.max(1, Math.trunc(options.maxQueued ?? MAX_QUEUED_EVENTS));
    this.#maxBatch = Math.min(
      MAX_BATCH_EVENTS,
      Math.max(1, Math.trunc(options.maxBatch ?? MAX_BATCH_EVENTS)),
    );
    this.#flushDelayMs = Math.max(0, Math.trunc(options.flushDelayMs ?? FLUSH_DELAY_MS));
  }

  get queuedCount(): number {
    return this.#queued;
  }

  get droppedCount(): number {
    return [...this.#droppedByEndpoint.values()]
      .reduce((total, dropped) => total + dropped.count, 0);
  }

  startSpan(request: InsulaSpanRequest): InsulaSpan | null {
    const operation = mechanicalName(request?.operation);
    if (!operation) return null;
    const span: InsulaSpan = {
      room: String(request?.room ?? "").trim(),
      traceId: canonicalUuid(request?.traceId) ?? crypto.randomUUID(),
      spanId: crypto.randomUUID(),
      parentSpanId: canonicalUuid(request?.parentSpanId),
      operation,
      toolCallId: opaqueIdentifier(request?.toolCallId),
      startedAt: performance.now(),
      startedAtEpochMs: Date.now(),
      providerRequestId: null,
      finished: false,
    };
    this.#emit(span.room, {
      traceId: span.traceId,
      spanId: span.spanId,
      parentSpanId: span.parentSpanId,
      operation: span.operation,
      phase: "start",
      outcomeClass: "unknown",
      errorClass: null,
      durationUs: null,
      tokensIn: 0,
      tokensOut: 0,
      toolCallId: span.toolCallId,
      providerRequestId: null,
      scope: "trace_span",
      dropCount: 0,
    });
    return span;
  }

  endSpan(
    span: InsulaSpan | null | undefined,
    outcomeClass: InsulaOutcome,
    errorClass: string | null = null,
    durationUs?: number | null,
  ): void {
    if (!span || span.finished) return;
    span.finished = true;
    this.#emit(span.room, {
      traceId: span.traceId,
      spanId: span.spanId,
      parentSpanId: span.parentSpanId,
      operation: span.operation,
      phase: "end",
      outcomeClass,
      errorClass: mechanicalName(errorClass),
      durationUs: durationUs === undefined
        ? elapsedMicroseconds(span.startedAt)
        : boundedDuration(durationUs),
      tokensIn: 0,
      tokensOut: 0,
      toolCallId: span.toolCallId,
      providerRequestId: span.providerRequestId,
      scope: "trace_span",
      dropCount: 0,
    });
  }

  point(request: InsulaPointRequest): void {
    const operation = mechanicalName(request?.operation);
    if (!operation) return;
    const toolCallId = opaqueIdentifier(request?.toolCallId);
    const providerRequestId = opaqueIdentifier(request?.providerRequestId);
    const scope = request?.scope ?? "trace_span";
    // A scope the Host would refuse is never sent: it names a correlation this
    // observation does not carry.
    if (scope === "tool_call" && !toolCallId) return;
    if (scope === "provider_request" && !providerRequestId) return;
    this.#emit(String(request?.room ?? "").trim(), {
      traceId: canonicalUuid(request?.traceId) ?? crypto.randomUUID(),
      spanId: crypto.randomUUID(),
      parentSpanId: canonicalUuid(request?.parentSpanId),
      operation,
      phase: "point",
      outcomeClass: request.outcomeClass,
      errorClass: mechanicalName(request?.errorClass),
      durationUs: boundedDuration(request?.durationUs),
      tokensIn: boundedTokens(request?.tokensIn),
      tokensOut: boundedTokens(request?.tokensOut),
      toolCallId,
      providerRequestId,
      scope,
      dropCount: 0,
    });
  }

  flush(): Promise<void> {
    this.#clearTimer();
    const chain = (this.#inFlight ?? Promise.resolve()).then(() => this.#drain());
    this.#inFlight = chain;
    return chain;
  }

  async close(timeoutMs = CLOSE_TIMEOUT_MS): Promise<void> {
    this.#clearTimer();
    // A closing House waits a short bounded moment for its last observations
    // and then abandons them. Shutdown never waits on telemetry.
    await Promise.race([this.flush(), bounded(timeoutMs)]);
    this.#buckets.clear();
    this.#queued = 0;
    this.#droppedByEndpoint.clear();
  }

  #emit(room: string, draft: ObservationDraft): void {
    try {
      const endpoint = this.#endpoint(room);
      if (!endpoint) return;
      if (this.#queued >= this.#maxQueued) {
        // Bounded by construction: the writer sheds the newest observation and
        // attributes the later receipt to the same Host endpoint. A process can
        // serve more than one room; global drop accounting would misstamp room
        // A's loss as room B's observation.
        const dropped = this.#droppedByEndpoint.get(endpoint.url) ?? { endpoint, count: 0 };
        dropped.endpoint = endpoint;
        dropped.count = Math.min(MAX_DROP_COUNT, dropped.count + 1);
        this.#droppedByEndpoint.set(endpoint.url, dropped);
        return;
      }
      this.#enqueue(endpoint, draft);
    } catch {
      // An unobservable turn is still a correct turn.
    }
  }

  #endpoint(room: string): InsulaEndpoint | null {
    try {
      const endpoint = hostHttpEndpoint(room, INSULA_EVENTS_PATH);
      return endpoint;
    } catch {
      // No installed Host endpoint for this room: there is nowhere to observe
      // into, which is a silent absence rather than a failure.
      return null;
    }
  }

  #enqueue(endpoint: InsulaEndpoint, draft: ObservationDraft): void {
    const bucket = this.#buckets.get(endpoint.url) ?? { token: endpoint.token, events: [] };
    bucket.token = endpoint.token;
    this.#buckets.set(endpoint.url, bucket);
    this.#sequence += 1;
    bucket.events.push({
      eventId: crypto.randomUUID(),
      spanId: draft.spanId,
      traceId: draft.traceId,
      parentSpanId: draft.parentSpanId,
      writerId: this.writerId,
      writerSequence: this.#sequence,
      component: COMPONENT,
      layer: LAYER,
      operation: draft.operation,
      phase: draft.phase,
      observedAt: new Date().toISOString(),
      // A started span has not finished, so it cannot carry a duration or a
      // measured usage yet.
      durationUs: draft.phase === "start" ? null : draft.durationUs,
      outcomeClass: draft.outcomeClass,
      errorClass: draft.outcomeClass === "ok" ? null : draft.errorClass,
      bytesIn: 0,
      bytesOut: 0,
      tokensIn: draft.phase === "start" ? 0 : draft.tokensIn,
      tokensOut: draft.phase === "start" ? 0 : draft.tokensOut,
      toolCallId: draft.toolCallId,
      providerRequestId: draft.providerRequestId,
      idempotencyVersion: 1,
      idempotencyScope: draft.scope,
      receiptKind: null,
      receiptId: null,
      dropCount: draft.dropCount,
    });
    this.#queued += 1;
    if (this.#queued >= this.#maxBatch) void this.flush();
    else this.#schedule();
  }

  #emitNextDropReceipt(): boolean {
    const next = this.#droppedByEndpoint.entries().next();
    if (next.done) return false;
    const [url, { endpoint, count }] = next.value;
    this.#droppedByEndpoint.delete(url);
    if (count <= 0) return this.#emitNextDropReceipt();
    this.#enqueue(endpoint, {
      traceId: crypto.randomUUID(),
      spanId: crypto.randomUUID(),
      parentSpanId: null,
      operation: WRITER_OPERATION,
      phase: "drop",
      outcomeClass: "degraded",
      errorClass: "queue_overflow",
      durationUs: null,
      toolCallId: null,
      scope: "writer_sequence",
      dropCount: count,
    });
    return true;
  }

  async #drain(): Promise<void> {
    // Drain every normal batch before admitting exactly one drop receipt. A
    // fresh endpoint may arrive while transport awaits; #queued detects that
    // it was outside the snapshot and gives it the next pass. Receipt emission
    // therefore never exceeds the same hard queue bound it reports.
    while (true) {
      for (const [url, bucket] of [...this.#buckets]) {
        while (bucket.events.length) {
          const events = bucket.events.splice(0, this.#maxBatch);
          this.#queued = Math.max(0, this.#queued - events.length);
          try {
            await this.#transport(
              url,
              bucket.token,
              { events },
              AbortSignal.timeout(POST_TIMEOUT_MS),
            );
          } catch {
            // Lost observations stay lost. The sequence gap is the House's
            // evidence, and a retry queue would outlive the turn it describes.
          }
        }
        this.#buckets.delete(url);
      }
      if (this.#queued > 0) continue;
      if (!this.#emitNextDropReceipt()) break;
    }
    this.#clearTimer();
  }

  #schedule(): void {
    if (this.#timer) return;
    const timer = setTimeout(() => {
      this.#timer = null;
      void this.flush();
    }, this.#flushDelayMs);
    (timer as { unref?: () => void }).unref?.();
    this.#timer = timer;
  }

  #clearTimer(): void {
    if (!this.#timer) return;
    clearTimeout(this.#timer);
    this.#timer = null;
  }
}

let processWriter: InsulaWriter | null = null;

export function insulaDisabled(
  environ: Record<string, string | undefined> = process.env,
): boolean {
  return environ.ATHANOR_DISABLE_INSULA === "1" || environ.ATHANOR_REPLAY_MODE === "1";
}

function processInsulaWriter(): InsulaWriter | null {
  // No Host token means no installed Host boundary to observe into. Checking it
  // here keeps the common uninstalled case free of thrown resolution.
  if (insulaDisabled() || !String(process.env.ATHANOR_HOST_TOKEN ?? "").trim()) return null;
  processWriter ??= new InsulaWriter();
  return processWriter;
}

export function startInsulaSpan(request: InsulaSpanRequest): InsulaSpan | null {
  try {
    return processInsulaWriter()?.startSpan(request) ?? null;
  } catch {
    return null;
  }
}

/**
 * Bind a provider's own request id to an open span. The start is already gone,
 * so only the settlement and its usage point can carry the correlation.
 */
export function noteInsulaProviderRequestId(
  span: InsulaSpan | null | undefined,
  providerRequestId: unknown,
): void {
  if (!span || span.finished || span.providerRequestId) return;
  span.providerRequestId = opaqueIdentifier(providerRequestId);
}

export function endInsulaSpan(
  span: InsulaSpan | null | undefined,
  outcomeClass: InsulaOutcome,
  errorClass: string | null = null,
  durationUs?: number | null,
): void {
  try {
    processInsulaWriter()?.endSpan(span, outcomeClass, errorClass, durationUs);
  } catch {
    // Observation never propagates.
  }
}

export function recordInsulaPoint(request: InsulaPointRequest): void {
  try {
    processInsulaWriter()?.point(request);
  } catch {
    // Observation never propagates.
  }
}

export async function closeInsulaWriter(): Promise<void> {
  const active = processWriter;
  processWriter = null;
  if (!active) return;
  try {
    await active.close();
  } catch {
    // Shutdown proceeds regardless.
  }
}
