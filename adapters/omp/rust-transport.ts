import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";

export type JsonObject = Record<string, unknown>;

export interface RustTransportOptions {
  executable: string;
  args?: string[];
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  stderrLimitBytes?: number;
  spawnProcess?: typeof spawn;
  shutdownGraceMs?: number;
  shutdownTimeoutMs?: number;
}

export interface RequestOptions {
  signal?: AbortSignal;
  timeoutMs?: number;
  /** Once dispatched, await the authoritative response; only pre-abort is honored. */
  settleDefinitively?: boolean;
}

export interface StructuredRustError {
  code: string;
  message: string;
  retryable: boolean;
  details?: unknown;
}

type DiagnosticCategory = "input" | "transport" | "protocol" | "configuration" | "database" | "embedding" | "filesystem" | "backup" | "authorization" | "operation" | "reconciliation" | "internal";
type DiagnosticStage = "validation" | "spawn" | "startup" | "request_write" | "request_parse" | "configuration_load" | "database_connect" | "database_query" | "embedding_request" | "transaction" | "backup" | "response_encode" | "reconciliation" | "shutdown";
type WriteOutcome = "not_started" | "rolled_back" | "committed" | "unknown";
type RetryAdvice = "safe_now" | "after_change" | "reconcile_first" | "never";

export interface TransportDiagnostic extends JsonObject {
  category: DiagnosticCategory;
  stage: DiagnosticStage;
  operation?: string;
  owner: { component: string; path: string; symbol: string };
  expected?: unknown;
  observed?: unknown;
  targets: unknown[];
  next_checks: unknown[];
  execution: {
    request_dispatched: boolean;
    write_outcome: WriteOutcome;
    retry: RetryAdvice;
  };
}

const OWNER = { component: "RustJsonlTransport", path: "rust-transport.ts", symbol: "RustJsonlTransport" } as const;
const MAX_DIAGNOSTIC_TEXT_BYTES = 4 * 1024;
const MAX_DIAGNOSTIC_ITEMS = 64;
const MAX_DIAGNOSTIC_DEPTH = 8;

function isObject(value: unknown): value is JsonObject {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function truncateDiagnosticText(value: string, limit = MAX_DIAGNOSTIC_TEXT_BYTES): string {
  const bytes = Buffer.from(value);
  if (bytes.length <= limit) return value;
  if (limit <= 0) return "";
  const marker = "...[truncated]";
  if (limit <= marker.length) return marker.slice(0, limit);
  const contentLimit = limit - marker.length;
  let prefix = bytes.subarray(0, contentLimit).toString("utf8");
  while (Buffer.byteLength(prefix) > contentLimit) prefix = prefix.slice(0, -1);
  return `${prefix}${marker}`;
}

function redactDiagnosticText(value: string, limit = MAX_DIAGNOSTIC_TEXT_BYTES): string {
  const redacted = value
    .replace(/\b([a-z][a-z0-9+.-]*):\/\/[^\s/@]*:[^\s/@]*@/gi, "$1://[REDACTED]@")
    .replace(/\b(authorization)\s*([:=])\s*[^\r\n]*/gi, "$1$2 [REDACTED]")
    .replace(/\b(bearer)\s+[A-Za-z0-9._~+/=-]+/gi, "$1 [REDACTED]")
    .replace(/\b(password|passwd|pwd|token|secret|api[_-]?key|access[_-]?key)\s*([:=])\s*(\"[^\"]*\"|'[^']*'|[^\s,;]+)/gi, "$1$2[REDACTED]");
  return truncateDiagnosticText(redacted, limit);
}

function sanitizeDiagnosticValue(value: unknown, depth = 0): unknown {
  if (value === null || typeof value === "boolean" || typeof value === "number") return value;
  if (typeof value === "string") return redactDiagnosticText(value);
  if (depth >= MAX_DIAGNOSTIC_DEPTH) return "[truncated diagnostic depth]";
  if (Array.isArray(value)) {
    const sanitized = value.slice(0, MAX_DIAGNOSTIC_ITEMS).map((item) => sanitizeDiagnosticValue(item, depth + 1));
    if (value.length > MAX_DIAGNOSTIC_ITEMS) sanitized.push("[truncated diagnostic items]");
    return sanitized;
  }
  if (isObject(value)) {
    const sanitized: JsonObject = {};
    const entries = Object.entries(value);
    for (const [index, [key, item]] of entries.entries()) {
      if (index >= MAX_DIAGNOSTIC_ITEMS) {
        sanitized._truncated = true;
        break;
      }
      sanitized[key] = /password|passwd|pwd|token|secret|authorization|api[_-]?key|access[_-]?key/i.test(key)
        ? "[REDACTED]"
        : sanitizeDiagnosticValue(item, depth + 1);
    }
    return sanitized;
  }
  return `[unsupported diagnostic value: ${typeof value}]`;
}

function asStringArray(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value.filter((item): item is string => typeof item === "string").map((item) => redactDiagnosticText(item, 1024));
}

function uniqueStrings(values: string[]): string[] {
  return [...new Set(values)];
}

function hasOwn(object: object, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(object, key);
}

function asError(value: unknown): Error {
  return value instanceof Error ? value : new Error(String(value));
}

function redactedCause(error: Error | undefined): Error | undefined {
  if (!error) return undefined;
  const cause = new Error(redactDiagnosticText(error.message));
  cause.name = error.name;
  return cause;
}

function isWriteOutcome(value: unknown): value is WriteOutcome {
  return value === "not_started" || value === "rolled_back" || value === "committed" || value === "unknown";
}

function isRetryAdvice(value: unknown): value is RetryAdvice {
  return value === "safe_now" || value === "after_change" || value === "reconcile_first" || value === "never";
}
function isDiagnosticCategory(value: unknown): value is DiagnosticCategory {
  return value === "input" || value === "transport" || value === "protocol" || value === "configuration"
    || value === "database" || value === "embedding" || value === "filesystem" || value === "backup"
    || value === "authorization" || value === "operation" || value === "reconciliation" || value === "internal";
}

function isDiagnosticStage(value: unknown): value is DiagnosticStage {
  return value === "validation" || value === "spawn" || value === "startup" || value === "request_write"
    || value === "request_parse" || value === "configuration_load" || value === "database_connect"
    || value === "database_query" || value === "embedding_request" || value === "transaction"
    || value === "backup" || value === "response_encode" || value === "reconciliation" || value === "shutdown";
}

function isDiagnosticOwner(value: unknown): value is TransportDiagnostic["owner"] {
  return isObject(value)
    && typeof value.component === "string"
    && typeof value.path === "string"
    && typeof value.symbol === "string";
}

function isCanonicalDiagnosticDetails(value: unknown): value is TransportDiagnostic {
  if (!isObject(value)) return false;
  const hasDiagnosticField = hasOwn(value, "category")
    || hasOwn(value, "stage")
    || hasOwn(value, "operation")
    || hasOwn(value, "owner")
    || hasOwn(value, "expected")
    || hasOwn(value, "observed")
    || hasOwn(value, "evidence")
    || hasOwn(value, "targets")
    || hasOwn(value, "next_checks")
    || hasOwn(value, "execution");
  if (!hasDiagnosticField) return false;
  if (hasOwn(value, "category") && !isDiagnosticCategory(value.category)) return false;
  if (hasOwn(value, "stage") && !isDiagnosticStage(value.stage)) return false;
  if (hasOwn(value, "operation") && typeof value.operation !== "string") return false;
  if (hasOwn(value, "owner") && !isDiagnosticOwner(value.owner)) return false;
  if (hasOwn(value, "evidence") && !Array.isArray(value.evidence)) return false;
  if (hasOwn(value, "targets") && !Array.isArray(value.targets)) return false;
  if (hasOwn(value, "next_checks") && !Array.isArray(value.next_checks)) return false;
  if (!hasOwn(value, "execution")) return true;
  if (!isObject(value.execution)) return false;
  return (!hasOwn(value.execution, "request_dispatched") || typeof value.execution.request_dispatched === "boolean")
    && (!hasOwn(value.execution, "write_outcome") || isWriteOutcome(value.execution.write_outcome))
    && (!hasOwn(value.execution, "retry") || isRetryAdvice(value.execution.retry));
}

function uniqueDiagnosticValues(values: unknown[]): unknown[] {
  const fingerprints = new Set<string>();
  return values.filter((value) => {
    const fingerprint = JSON.stringify(value) ?? String(value);
    if (fingerprints.has(fingerprint)) return false;
    fingerprints.add(fingerprint);
    return true;
  });
}

function mergeDiagnosticDetails(rawDetails: unknown, fallback: TransportDiagnostic): TransportDiagnostic {

  const sanitized = sanitizeDiagnosticValue(rawDetails);
  const raw = isObject(sanitized) ? sanitized : {};
  const rawOwner = isObject(raw.owner) ? raw.owner : {};
  const rawExecution = isObject(raw.execution) ? raw.execution : {};
  const rawObserved = isObject(raw.observed) ? raw.observed : undefined;
  const fallbackObserved = isObject(fallback.observed) ? fallback.observed : undefined;
  const rawEvidence = Array.isArray(raw.evidence) ? raw.evidence : [];
  const rawNextChecks = Array.isArray(raw.next_checks) ? raw.next_checks : [];
  const operation = typeof raw.operation === "string" ? raw.operation : fallback.operation;

  return {
    ...raw,
    category: isDiagnosticCategory(raw.category) ? raw.category : fallback.category,
    stage: isDiagnosticStage(raw.stage) ? raw.stage : fallback.stage,
    ...(operation === undefined ? {} : { operation }),
    owner: {
      component: typeof rawOwner.component === "string" ? rawOwner.component : fallback.owner.component,
      path: typeof rawOwner.path === "string" ? rawOwner.path : fallback.owner.path,
      symbol: typeof rawOwner.symbol === "string" ? rawOwner.symbol : fallback.owner.symbol,
    },
    ...(hasOwn(raw, "expected") ? { expected: raw.expected } : fallback.expected === undefined ? {} : { expected: fallback.expected }),
    observed: rawObserved || fallbackObserved ? { ...fallbackObserved, ...rawObserved } : undefined,
    evidence: hasOwn(raw, "evidence") ? uniqueDiagnosticValues(rawEvidence) : fallback.evidence,
    targets: hasOwn(raw, "targets") && Array.isArray(raw.targets) ? raw.targets : fallback.targets,
    next_checks: hasOwn(raw, "next_checks") ? rawNextChecks : fallback.next_checks,
    execution: {
      request_dispatched: typeof rawExecution.request_dispatched === "boolean"
        ? rawExecution.request_dispatched
        : fallback.execution.request_dispatched,
      write_outcome: isWriteOutcome(rawExecution.write_outcome)
        ? rawExecution.write_outcome
        : fallback.execution.write_outcome,
      retry: isRetryAdvice(rawExecution.retry) ? rawExecution.retry : fallback.execution.retry,
    },
  };
}

function standaloneDetails(error: StructuredRustError, stderr: string): TransportDiagnostic {
  const fallback: TransportDiagnostic = {
    category: "operation",
    stage: "request_parse",
    owner: { ...OWNER, symbol: "RustTransportError" },
    observed: { stderr: { bytes_collected: Buffer.byteLength(stderr), bounded: true } },
    evidence: stderr ? [{ type: "stderr", text: redactDiagnosticText(stderr) }] : [],
    targets: ["rust-transport.ts#RustTransportError"],
    next_checks: [{ action: "inspect_structured_error", target: "error.details" }],
    execution: {
      request_dispatched: true,
      write_outcome: "unknown",
      retry: error.retryable ? "safe_now" : "after_change",
    },
  };
  return mergeDiagnosticDetails(error.details, fallback);
}

export class RustTransportError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly details: TransportDiagnostic;
  readonly stderr: string;

  constructor(error: StructuredRustError, stderr = "", details?: TransportDiagnostic) {
    super(error.message);
    this.name = "RustTransportError";
    this.code = error.code;
    this.retryable = error.retryable;
    this.stderr = redactDiagnosticText(stderr);
    this.details = details ?? standaloneDetails(error, this.stderr);
  }
}

export interface TransportUnavailableErrorInit {
  code: string;
  message: string;
  retryable: boolean;
  details: TransportDiagnostic;
  stderr?: string;
  cause?: Error;
}

export class TransportUnavailableError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly details: TransportDiagnostic;
  readonly stderr: string;
  readonly cause?: Error;

  constructor(error: TransportUnavailableErrorInit) {
    super(error.message);
    this.name = "TransportUnavailableError";
    this.code = error.code;
    this.retryable = error.retryable;
    this.details = error.details;
    this.stderr = redactDiagnosticText(error.stderr ?? "");
    this.cause = redactedCause(error.cause);
  }
}

export class RustTransportOutcomeUnknownError extends Error {
  readonly code = "AUTHORITATIVE_OUTCOME_UNKNOWN";
  readonly retryable = false;
  readonly details: TransportDiagnostic;
  readonly stderr: string;
  readonly cause?: Error;

  constructor(cause?: Error, details?: TransportDiagnostic, stderr = "") {
    super("Rust transport authoritative outcome is unknown after request timeout or worker failure");
    this.name = "RustTransportOutcomeUnknownError";
    this.stderr = redactDiagnosticText(stderr);
    this.cause = redactedCause(cause);
    this.details = details ?? mergeDiagnosticDetails({}, {
      category: "transport",
      stage: "reconciliation",
      owner: { ...OWNER, symbol: "RustTransportOutcomeUnknownError" },
      observed: { stderr: { bytes_collected: Buffer.byteLength(this.stderr), bounded: true } },
      evidence: this.stderr ? [{ type: "stderr", text: this.stderr }] : [],
      targets: ["rust-transport.ts#RustTransportOutcomeUnknownError"],
      next_checks: [{ action: "reconcile_request", target: "authoritative_outcome" }],
      execution: { request_dispatched: true, write_outcome: "unknown", retry: "reconcile_first" },
    });
  }
}

const DEFAULT_NON_DEFINITIVE_TIMEOUT_MS = 120_000;
const DEFAULT_DEFINITIVE_TIMEOUT_MS = 120_000;
type RequestContext = {
  id: string;
  method: string;
  dispatched: boolean;
  definitive: boolean;
};
type Pending = RequestContext & {
  resolve: (value: unknown) => void;
  reject: (reason: unknown) => void;
  timer?: ReturnType<typeof setTimeout>;
  signal?: AbortSignal;
  abortListener?: () => void;
  writeCompleted: boolean;
};
type QueuedWrite = { id: string; pending: Pending; body: string };
type TransportFailure = {
  code: string;
  message: string;
  retryable: boolean;
  category: DiagnosticCategory;
  stage: DiagnosticStage;
  expected?: unknown;
  observed?: unknown;
  evidence?: unknown[];
  targets?: string[];
  nextChecks?: unknown[];
  retry?: RetryAdvice;
};

const MAX_TIMEOUT_MS = 2_147_483_647;
const MAX_PENDING = 1024;
const MAX_QUEUED_BYTES = 4 * 1024 * 1024;
const MAX_STDOUT_LINE_BYTES = 1024 * 1024;

export class RustJsonlTransport {
  private child?: ChildProcessWithoutNullStreams;
  private readonly pending = new Map<string, Pending>();
  private readonly abandoned = new Set<string>();
  private readonly writeQueue: QueuedWrite[] = [];
  private queuedBytes = 0;
  private writing = false;
  private blocked = false;
  private stdoutBuffer = Buffer.alloc(0);
  private nextId = 1;
  private unusable?: Error;
  private unusableFailure?: TransportFailure;
  private outcomeUnknown = false;
  private childStarted = false;
  private cleaned = false;
  private stderrChunks: Buffer[] = [];
  private stderrBytes = 0;
  private stderrTruncated = false;
  private readonly stderrLimit: number;
  private readonly shutdownGraceMs: number;
  private readonly shutdownTimeoutMs: number;
  private childClosed = false;
  private closePromise?: Promise<void>;
  private closeResolve?: () => void;
  private closeReject?: (error: Error) => void;
  private forceKillTimer?: ReturnType<typeof setTimeout>;
  private shutdownTimer?: ReturnType<typeof setTimeout>;

  constructor(private readonly options: RustTransportOptions) {
    const limit = options.stderrLimitBytes ?? 16 * 1024;
    if (!Number.isFinite(limit) || !Number.isInteger(limit) || limit < 0) {
      throw new Error("stderrLimitBytes must be a finite non-negative integer");
    }
    this.stderrLimit = limit;
    this.shutdownGraceMs = this.shutdownDuration(options.shutdownGraceMs, 100, "shutdownGraceMs");
    this.shutdownTimeoutMs = this.shutdownDuration(options.shutdownTimeoutMs, 5_000, "shutdownTimeoutMs");
    if (this.shutdownTimeoutMs < this.shutdownGraceMs) {
      throw new Error("shutdownTimeoutMs must be greater than or equal to shutdownGraceMs");
    }
  }

  request(method: string, params: JsonObject, options: RequestOptions = {}): Promise<unknown> {
    if (!method || typeof method !== "string") {
      return Promise.reject(new Error("method must be a non-empty string"));
    }

    const id = String(this.nextId++);
    const definitive = options.settleDefinitively === true;
    const context: RequestContext = { id, method, dispatched: false, definitive };

    if (this.unusable) return Promise.reject(this.unavailableAfterFailure(context));

    const timeout = options.timeoutMs ?? (definitive ? DEFAULT_DEFINITIVE_TIMEOUT_MS : DEFAULT_NON_DEFINITIVE_TIMEOUT_MS);
    if (timeout !== undefined && (!Number.isFinite(timeout) || !Number.isInteger(timeout) || timeout < 0 || timeout > MAX_TIMEOUT_MS)) {
      return Promise.reject(this.unavailableFor(context, {
        code: "RUST_TRANSPORT_INVALID_TIMEOUT",
        message: "timeoutMs must be a non-negative integer no greater than 2147483647",
        retryable: false,
        category: "input",
        stage: "validation",
        observed: { timeout_ms: sanitizeDiagnosticValue(timeout) },
        retry: "after_change",
      }));
    }
    if (options.signal?.aborted) {
      const failure: TransportFailure = {
        code: "RUST_TRANSPORT_REQUEST_CANCELLED",
        message: "Rust transport request was cancelled",
        retryable: true,
        category: "transport",
        stage: "request_write",
        observed: { cancellation: "before_dispatch" },
      };
      const error = this.unavailableFor(context, failure);
      return Promise.reject(error);
    }
    if (this.pending.size >= MAX_PENDING) {
      return Promise.reject(this.unavailableFor(context, {
        code: "RUST_TRANSPORT_PENDING_LIMIT",
        message: "Rust transport pending request limit exceeded",
        retryable: true,
        category: "transport",
        stage: "request_write",
        expected: { pending_requests_at_most: MAX_PENDING },
        observed: { pending_requests: this.pending.size },
      }));
    }

    let body: string;
    try {
      body = `${JSON.stringify({ protocol: 1, id, method, params })}\n`;
    } catch (error) {
      return Promise.reject(this.unavailableFor(context, {
        code: "RUST_TRANSPORT_REQUEST_ENCODE_FAILED",
        message: "Rust transport could not encode request",
        retryable: false,
        category: "protocol",
        stage: "request_write",
        observed: { encode_error: redactDiagnosticText(asError(error).message) },
        retry: "after_change",
      }, asError(error)));
    }
    const bodyBytes = Buffer.byteLength(body);
    if (bodyBytes > MAX_QUEUED_BYTES) {
      return Promise.reject(this.unavailableFor(context, {
        code: "RUST_TRANSPORT_REQUEST_TOO_LARGE",
        message: "Rust transport request exceeds queue limit",
        retryable: false,
        category: "transport",
        stage: "request_write",
        expected: { request_bytes_at_most: MAX_QUEUED_BYTES },
        observed: { request_bytes: bodyBytes },
        retry: "after_change",
      }));
    }
    if (this.queuedBytes + bodyBytes > MAX_QUEUED_BYTES) {
      return Promise.reject(this.unavailableFor(context, {
        code: "RUST_TRANSPORT_WRITE_QUEUE_LIMIT",
        message: "Rust transport write queue limit exceeded",
        retryable: true,
        category: "transport",
        stage: "request_write",
        expected: { queued_bytes_at_most: MAX_QUEUED_BYTES },
        observed: { queued_bytes: this.queuedBytes, request_bytes: bodyBytes },
      }));
    }

    return new Promise((resolve, reject) => {
      const pending: Pending = {
        ...context,
        resolve,
        reject,
        signal: options.signal,
        writeCompleted: false,
      };
      this.pending.set(id, pending);
      try {
        this.ensureStarted();
      } catch (error) {
        const cause = asError(error);
        this.invalidate(cause, true, this.spawnFailure(cause));
        return;
      }
      if (timeout !== undefined) pending.timer = setTimeout(() => this.timeoutRequest(id), timeout);
      if (options.signal) {
        const onAbort = () => {
          if (!pending.dispatched) {
            this.cancelQueued(id, this.unavailableFor(pending, this.cancellationFailure("before_dispatch")));
          } else if (!pending.definitive && pending.writeCompleted) {
            this.abandonRequest(id, this.cancellationFailure("after_dispatch"));
          } else if (!pending.definitive) {
            this.abortRequest(id, this.cancellationFailure("after_dispatch"));
          }
        };
        pending.abortListener = onAbort;
        options.signal.addEventListener("abort", onAbort, { once: true });
      }
      this.writeQueue.push({ id, pending, body });
      this.queuedBytes += bodyBytes;
      this.flushWrites();
    });
  }

  // Exit, not signal delivery, is the shutdown receipt. Project lesson #338.
  close(): Promise<void> {
    if (this.closePromise) return this.closePromise;
    const child = this.child;
    if (!child || this.childClosed) {
      this.invalidate(new Error("Rust transport closed"), false, this.closedFailure());
      return (this.closePromise = Promise.resolve());
    }
    this.closePromise = new Promise<void>((resolve, reject) => {
      this.closeResolve = resolve;
      this.closeReject = reject;
    });
    this.invalidate(new Error("Rust transport closed"), false, this.closedFailure());
    this.terminateChild(child);
    this.shutdownTimer = setTimeout(() => {
      this.settleClose(new Error("Rust transport child did not exit before the shutdown deadline"));
    }, this.shutdownTimeoutMs);
    return this.closePromise;
  }

  get stderrDiagnostics(): string {
    return redactDiagnosticText(Buffer.concat(this.stderrChunks).toString("utf8"), this.stderrLimit);
  }

  get usable(): boolean {
    return !this.cleaned;
  }

  private ensureStarted(): void {
    if (this.child) return;
    this.childStarted = false;
    const child = (this.options.spawnProcess ?? spawn)(this.options.executable, this.options.args ?? [], {
      cwd: this.options.cwd,
      env: { ...process.env, ...this.options.env },
      stdio: ["pipe", "pipe", "pipe"],
      shell: false,
      windowsHide: true,
    });
    this.child = child;
    this.childClosed = false;
    child.once("spawn", () => { this.childStarted = true; });
    child.stderr.on("data", (chunk: Buffer | string) => this.collectStderr(Buffer.from(chunk)));
    child.stdin.on("error", (error) => this.invalidate(error, true, this.streamFailure(error, "request_write", "RUST_TRANSPORT_REQUEST_WRITE_FAILED", "Rust transport request write failed")));
    child.on("error", (error) => {
      if (!this.childStarted) for (const pending of this.pending.values()) pending.dispatched = false;
      this.invalidate(error, true, this.spawnFailure(error));
    });
    child.on("close", (code, signal) => {
      if (this.child !== child) return;
      this.childClosed = true;
      this.settleClose();
      if (this.cleaned) return;
      this.invalidate(new Error(`Rust transport child exited (${signal ?? code ?? "unknown"})`), false, {
        code: "RUST_TRANSPORT_CHILD_EXITED",
        message: `Rust transport child exited (${signal ?? code ?? "unknown"})`,
        retryable: true,
        category: "transport",
        stage: this.pending.size ? "request_parse" : "startup",
        observed: { exit_code: code, signal },
        evidence: [{ type: "process_exit", exit_code: code, signal }],
        nextChecks: [{ action: "inspect_process_exit", target: "child_process" }],
      });
    });
    child.stdin.on("drain", () => { this.blocked = false; this.flushWrites(); });
    child.stdout.on("data", (chunk: Buffer | string) => this.handleStdout(Buffer.from(chunk)));
    child.stdout.on("error", (error) => this.invalidate(error, true, this.streamFailure(error, "request_parse", "RUST_TRANSPORT_RESPONSE_READ_FAILED", "Rust transport response read failed")));
  }

  private flushWrites(): void {
    if (this.cleaned || this.writing || this.blocked || !this.child) return;
    const item = this.writeQueue.shift();
    if (!item) return;
    this.queuedBytes -= Buffer.byteLength(item.body);
    item.pending.dispatched = true;
    this.writing = true;
    let accepted: boolean;
    try {
      accepted = this.child.stdin.write(item.body, (error?: Error | null) => {
        if (!error) item.pending.writeCompleted = true;
        this.writing = false;
        if (error) this.invalidate(error, true, this.streamFailure(error, "request_write", "RUST_TRANSPORT_REQUEST_WRITE_FAILED", "Rust transport request write failed"));
        else this.flushWrites();
      });
    } catch (error) {
      this.writing = false;
      const cause = asError(error);
      this.invalidate(cause, true, this.streamFailure(cause, "request_write", "RUST_TRANSPORT_REQUEST_WRITE_FAILED", "Rust transport request write failed"));
      return;
    }
    if (!accepted) this.blocked = true;
  }

  private collectStderr(chunk: Buffer): void {
    if (this.stderrBytes >= this.stderrLimit) {
      this.stderrTruncated ||= chunk.length > 0;
      return;
    }
    const allowed = chunk.subarray(0, this.stderrLimit - this.stderrBytes);
    this.stderrChunks.push(allowed);
    this.stderrBytes += allowed.length;
    this.stderrTruncated ||= allowed.length < chunk.length;
  }

  private handleStdout(chunk: Buffer): void {
    if (this.stdoutBuffer.length + chunk.length > MAX_STDOUT_LINE_BYTES && !chunk.includes(10)) {
      this.invalidate(new Error("Rust transport output line exceeds maximum size"), true, this.oversizedOutputFailure(this.stdoutBuffer.length + chunk.length));
      return;
    }
    this.stdoutBuffer = Buffer.concat([this.stdoutBuffer, chunk]);
    if (this.stdoutBuffer.length > MAX_STDOUT_LINE_BYTES && !this.stdoutBuffer.includes(10)) {
      this.invalidate(new Error("Rust transport output line exceeds maximum size"), true, this.oversizedOutputFailure(this.stdoutBuffer.length));
      return;
    }
    let newline: number;
    while ((newline = this.stdoutBuffer.indexOf(10)) >= 0) {
      const lineBytes = newline;
      const line = this.stdoutBuffer.subarray(0, newline).toString("utf8").replace(/\r$/, "");
      this.stdoutBuffer = this.stdoutBuffer.subarray(newline + 1);
      if (lineBytes > MAX_STDOUT_LINE_BYTES) {
        this.invalidate(new Error("Rust transport output line exceeds maximum size"), true, this.oversizedOutputFailure(lineBytes));
        return;
      }
      this.handleLine(line, lineBytes);
      if (this.cleaned) return;
    }
    if (this.stdoutBuffer.length > MAX_STDOUT_LINE_BYTES) {
      this.invalidate(new Error("Rust transport output line exceeds maximum size"), true, this.oversizedOutputFailure(this.stdoutBuffer.length));
    }
  }

  private handleLine(line: string, lineBytes: number): void {
    let message: unknown;
    try {
      message = JSON.parse(line);
    } catch {
      this.invalidate(new Error("Rust transport received malformed JSON output"), true, this.protocolFailure(
        "RUST_TRANSPORT_MALFORMED_OUTPUT",
        "Rust transport received malformed JSON output",
        { output: "invalid_json", line_bytes: lineBytes },
      ));
      return;
    }
    if (!isObject(message)) {
      this.invalidate(new Error("Rust transport received an invalid response envelope"), true, this.protocolFailure(
        "RUST_TRANSPORT_INVALID_ENVELOPE",
        "Rust transport received an invalid response envelope",
        { output: "response_not_object", line_bytes: lineBytes },
      ));
      return;
    }
    const envelope = message;
    if (envelope.protocol !== 1 || typeof envelope.id !== "string") {
      this.invalidate(new Error("Rust transport received an invalid protocol envelope"), true, this.protocolFailure(
        "RUST_TRANSPORT_INVALID_PROTOCOL",
        "Rust transport received an invalid protocol envelope",
        { protocol: envelope.protocol, id_present: typeof envelope.id === "string" },
      ));
      return;
    }
    const id = envelope.id;
    const pending = this.pending.get(id);
    if (!pending) {
      if (this.abandoned.delete(id)) return;
      this.invalidate(new Error(`Rust transport received unknown or duplicate response id: ${id}`), true, this.protocolFailure(
        "RUST_TRANSPORT_UNKNOWN_RESPONSE_ID",
        `Rust transport received unknown or duplicate response id: ${id}`,
        { response_id: redactDiagnosticText(id, 256) },
      ));
      return;
    }
    const hasError = hasOwn(envelope, "error");
    const hasResult = hasOwn(envelope, "result");
    if (hasError === hasResult) {
      this.invalidate(new Error("Rust transport response must contain exactly one of result or error"), true, this.protocolFailure(
        "RUST_TRANSPORT_AMBIGUOUS_RESPONSE",
        "Rust transport response must contain exactly one of result or error",
        { response_id: id, has_result: hasResult, has_error: hasError },
      ));
      return;
    }
    if (hasError) {
      const error = envelope.error;
      if (!isObject(error) || typeof error.code !== "string" || typeof error.message !== "string" || typeof error.retryable !== "boolean") {
        this.invalidate(new Error("Rust transport received an invalid structured error"), true, this.protocolFailure(
          "RUST_TRANSPORT_INVALID_STRUCTURED_ERROR",
          "Rust transport received an invalid structured error",
          { response_id: id },
        ));
        return;
      }
      this.pending.delete(id);
      this.clearPending(pending);
      const structured: StructuredRustError = {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
        details: error.details,
      };
      pending.reject(new RustTransportError(structured, this.stderrDiagnostics, this.structuredErrorDetails(pending, structured)));
      return;
    }
    this.pending.delete(id);
    this.clearPending(pending);
    pending.resolve(envelope.result);
  }

  private timeoutRequest(id: string): void {
    const pending = this.pending.get(id);
    if (!pending) return;
    const failure: TransportFailure = {
      code: "RUST_TRANSPORT_REQUEST_TIMEOUT",
      message: pending.dispatched ? "Rust transport request timed out" : "Rust transport request timed out before dispatch",
      retryable: true,
      category: "transport",
      stage: pending.dispatched ? "request_parse" : "request_write",
      observed: { timeout: pending.dispatched ? "after_dispatch" : "before_dispatch" },
      nextChecks: [{ action: "inspect_request_timeout", target: `request:${id}` }],
    };
    if (!pending.dispatched) {
      this.cancelQueued(id, this.unavailableFor(pending, failure));
      return;
    }
    this.invalidate(new Error(failure.message), true, failure);
  }

  private cancelQueued(id: string, error: Error): void {
    const pending = this.pending.get(id);
    if (!pending || pending.dispatched) return;
    const index = this.writeQueue.findIndex((item) => item.id === id);
    if (index < 0) return;
    const [item] = this.writeQueue.splice(index, 1);
    this.queuedBytes -= Buffer.byteLength(item.body);
    this.pending.delete(id);
    this.clearPending(pending);
    pending.reject(error);
    this.flushWrites();
  }

  private abandonRequest(id: string, failure: TransportFailure): void {
    const pending = this.pending.get(id);
    if (!pending || !pending.dispatched || !pending.writeCompleted) return;
    const error = this.unavailableFor(pending, failure);
    this.pending.delete(id);
    this.clearPending(pending);
    this.abandoned.add(id);
    if (this.abandoned.size > MAX_PENDING) {
      const oldest = this.abandoned.values().next().value;
      if (typeof oldest === "string") this.abandoned.delete(oldest);
    }
    pending.reject(error);
  }

  private abortRequest(id: string, failure: TransportFailure): void {
    if (!this.pending.has(id)) return;
    this.invalidate(new Error(failure.message), true, failure);
  }

  private clearPending(pending: Pending): void {
    if (pending.timer) clearTimeout(pending.timer);
    if (pending.signal && pending.abortListener) pending.signal.removeEventListener("abort", pending.abortListener);
  }

  private terminateChild(child: ChildProcessWithoutNullStreams): void {
    try { child.stdin.end(); } catch {}
    if (this.forceKillTimer) return;
    this.forceKillTimer = setTimeout(() => {
      if (this.childClosed || this.child !== child) return;
      if (process.platform === "win32" && child.pid) {
        const killer = spawn("taskkill", ["/PID", String(child.pid), "/T", "/F"], { windowsHide: true, stdio: "ignore" });
        killer.unref();
      } else {
        child.kill("SIGKILL");
      }
    }, this.shutdownGraceMs);
  }

  private settleClose(error?: Error): void {
    if (this.forceKillTimer) clearTimeout(this.forceKillTimer);
    if (this.shutdownTimer) clearTimeout(this.shutdownTimer);
    this.forceKillTimer = undefined;
    this.shutdownTimer = undefined;
    const resolve = this.closeResolve;
    const reject = this.closeReject;
    this.closeResolve = undefined;
    this.closeReject = undefined;
    if (error) reject?.(error);
    else resolve?.();
  }

  private shutdownDuration(value: number | undefined, fallback: number, field: string): number {
    const duration = value ?? fallback;
    if (!Number.isFinite(duration) || !Number.isInteger(duration) || duration < 0 || duration > MAX_TIMEOUT_MS) {
      throw new Error(`${field} must be a non-negative integer no greater than ${MAX_TIMEOUT_MS}`);
    }
    return duration;
  }

  private closedFailure(): TransportFailure {
    return {
      code: "RUST_TRANSPORT_CLOSED",
      message: "Rust transport closed",
      retryable: true,
      category: "transport",
      stage: "shutdown",
      observed: { closure: "requested" },
    };
  }

  private invalidate(error: Error, kill = true, failure?: TransportFailure): void {
    if (this.cleaned) return;
    const diagnosticFailure = failure ?? this.streamFailure(error, "request_parse", "RUST_TRANSPORT_UNAVAILABLE", "Rust transport became unusable");
    const hasAuthoritativeUnknown = [...this.pending.values()].some((pending) => pending.dispatched && pending.definitive);
    this.cleaned = true;
    this.outcomeUnknown = hasAuthoritativeUnknown;
    this.unusable = error;
    this.unusableFailure = diagnosticFailure;
    const child = this.child;
    if (child) {
      if (kill) this.terminateChild(child);
    }
    this.writeQueue.length = 0;
    this.abandoned.clear();
    this.queuedBytes = 0;
    this.writing = false;
    this.blocked = false;
    for (const pending of this.pending.values()) {
      this.clearPending(pending);
      pending.reject(this.failureForPending(pending, diagnosticFailure, error));
    }
    this.pending.clear();
  }

  private failureForPending(pending: RequestContext, failure: TransportFailure, cause: Error): Error {
    if (pending.dispatched && pending.definitive) {
      const details = this.detailsFor(pending, failure, {
        request_dispatched: true,
        write_outcome: "unknown",
        retry: "reconcile_first",
      });
      const observed = isObject(details.observed) ? details.observed : {};
      observed.transport_failure_code = failure.code;
      details.observed = observed;
      return new RustTransportOutcomeUnknownError(cause, details, this.stderrDiagnostics);
    }
    return this.unavailableFor(pending, failure, cause);
  }

  private unavailableFor(context: RequestContext, failure: TransportFailure, cause?: Error): TransportUnavailableError {
    return new TransportUnavailableError({
      code: failure.code,
      message: failure.message,
      retryable: failure.retryable,
      details: this.detailsFor(context, failure),
      stderr: this.stderrDiagnostics,
      cause,
    });
  }

  private unavailableAfterFailure(context: RequestContext): TransportUnavailableError {
    const source = this.unusableFailure ?? {
      code: "RUST_TRANSPORT_UNAVAILABLE",
      message: "Rust transport is unusable",
      retryable: true,
      category: "transport" as const,
      stage: "startup" as const,
    };
    const outcomeUnknown = this.outcomeUnknown;
    const failure: TransportFailure = {
      ...source,
      code: outcomeUnknown ? "AUTHORITATIVE_OUTCOME_UNKNOWN" : source.code,
      message: outcomeUnknown
        ? "Rust transport is unusable after an authoritative outcome became unknown"
        : source.message,
      retryable: outcomeUnknown ? false : source.retryable,
      stage: outcomeUnknown ? "reconciliation" : source.stage,
      observed: {
        previous_transport_failure: source.code,
        previous_stage: source.stage,
        unavailable: true,
      },
      retry: outcomeUnknown ? "reconcile_first" : source.retry,
    };
    return this.unavailableFor(context, failure, this.unusable);
  }

  private detailsFor(context: RequestContext, failure: TransportFailure, executionOverride?: TransportDiagnostic["execution"]): TransportDiagnostic {
    const observedValue = sanitizeDiagnosticValue(failure.observed);
    const observed: JsonObject = isObject(observedValue) ? { ...observedValue } : observedValue === undefined ? {} : { failure: observedValue };
    observed.request = {
      id: context.id,
      method: redactDiagnosticText(context.method, 256),
      dispatched: context.dispatched,
    };
    observed.stderr = {
      bytes_collected: this.stderrBytes,
      limit_bytes: this.stderrLimit,
      truncated: this.stderrTruncated,
    };
    const execution = executionOverride ?? this.executionFor(context, failure);
    const evidence = [...(failure.evidence ?? []).map((item) => sanitizeDiagnosticValue(item))];
    const stderrEvidence = this.stderrEvidence();
    if (stderrEvidence) evidence.push(stderrEvidence);
    const fallback: TransportDiagnostic = {
      category: failure.category,
      stage: failure.stage,
      operation: redactDiagnosticText(context.method, 256),
      owner: { ...OWNER },
      ...(failure.expected === undefined ? {} : { expected: sanitizeDiagnosticValue(failure.expected) }),
      observed,
      evidence,
      targets: this.diagnosticTargets(failure.targets),
      next_checks: [
        ...(failure.nextChecks ?? []).map((item) => sanitizeDiagnosticValue(item)),
        ...this.defaultNextChecks(context, failure, execution),
      ],
      execution,
    };
    return mergeDiagnosticDetails({}, fallback);
  }

  private structuredErrorDetails(pending: Pending, error: StructuredRustError): TransportDiagnostic {
    const responseFailure: TransportFailure = {
      code: error.code,
      message: error.message,
      retryable: error.retryable,
      category: "operation",
      stage: "request_parse",
      observed: { response: "structured_error" },
      nextChecks: [{ action: "inspect_structured_error", target: "error.details" }],
      retry: pending.definitive ? "reconcile_first" : error.retryable ? "safe_now" : "after_change",
    };
    const fallback = this.detailsFor(pending, responseFailure);
    const producerDetails = isCanonicalDiagnosticDetails(error.details);
    const merged = mergeDiagnosticDetails(error.details, fallback);
    if (!producerDetails) return merged;

    const transportObservations: JsonObject = {
      request: {
        id: pending.id,
        method: redactDiagnosticText(pending.method, 256),
        dispatched: true,
      },
      execution: { ...fallback.execution },
    };
    const stderrEvidence = this.stderrEvidence();
    if (stderrEvidence) transportObservations.stderr = stderrEvidence;

    if (isObject(merged.observed) && !hasOwn(merged.observed, "transport")) {
      merged.observed = { ...merged.observed, transport: transportObservations };
    } else if (!hasOwn(merged, "transport")) {
      merged.transport = transportObservations;
    } else {
      merged.transport_observations = transportObservations;
    }
    return merged;
  }

  private executionFor(context: RequestContext, failure: TransportFailure): TransportDiagnostic["execution"] {
    if (!context.dispatched) {
      return {
        request_dispatched: false,
        write_outcome: "not_started",
        retry: failure.retry ?? "safe_now",
      };
    }
    if (context.definitive) {
      return {
        request_dispatched: true,
        write_outcome: "unknown",
        retry: "reconcile_first",
      };
    }
    return {
      request_dispatched: true,
      write_outcome: "unknown",
      retry: failure.retry ?? (failure.retryable ? "safe_now" : "after_change"),
    };
  }

  private diagnosticTargets(extra: string[] = []): string[] {
    return uniqueStrings([
      redactDiagnosticText(this.options.executable, 1024),
      redactDiagnosticText(this.options.cwd ?? process.cwd(), 1024),
      "rust-transport.ts#RustJsonlTransport",
      ...extra.map((target) => redactDiagnosticText(target, 1024)),
    ]);
  }

  private stderrEvidence(): JsonObject | undefined {
    if (!this.stderrBytes && !this.stderrTruncated) return undefined;
    return {
      type: "stderr",
      text: this.stderrDiagnostics,
      bytes_collected: this.stderrBytes,
      limit_bytes: this.stderrLimit,
      truncated: this.stderrTruncated,
    };
  }

  private defaultNextChecks(context: RequestContext, failure: TransportFailure, execution: TransportDiagnostic["execution"]): JsonObject[] {
    const checks: JsonObject[] = [];
    if (failure.stage === "spawn") {
      checks.push({ action: "verify_executable", target: redactDiagnosticText(this.options.executable, 1024) });
      checks.push({ action: "verify_working_directory", target: redactDiagnosticText(this.options.cwd ?? process.cwd(), 1024) });
    } else if (failure.stage === "request_parse") {
      checks.push({ action: "inspect_protocol_stream", target: "stdout" });
    } else if (failure.stage === "request_write") {
      checks.push({ action: "inspect_request_write", target: `request:${context.id}` });
    }
    if (this.stderrBytes || this.stderrTruncated) checks.push({ action: "inspect_stderr", target: "stderr" });
    if (execution.retry === "reconcile_first") checks.push({ action: "reconcile_request", target: `request:${context.id}` });
    return checks;
  }

  private spawnFailure(error: Error): TransportFailure {
    return {
      code: "RUST_TRANSPORT_SPAWN_FAILED",
      message: "Unable to start Rust transport process",
      retryable: true,
      category: "transport",
      stage: "spawn",
      observed: { spawn_error: redactDiagnosticText(error.message) },
      evidence: [{ type: "spawn_error", message: redactDiagnosticText(error.message) }],
    };
  }

  private streamFailure(error: Error, stage: DiagnosticStage, code: string, message: string): TransportFailure {
    return {
      code,
      message,
      retryable: true,
      category: "transport",
      stage,
      observed: { stream_error: redactDiagnosticText(error.message) },
      evidence: [{ type: "stream_error", message: redactDiagnosticText(error.message) }],
    };
  }

  private cancellationFailure(phase: "before_dispatch" | "after_dispatch"): TransportFailure {
    return {
      code: "RUST_TRANSPORT_REQUEST_CANCELLED",
      message: "Rust transport request was cancelled",
      retryable: true,
      category: "transport",
      stage: "request_write",
      observed: { cancellation: phase },
    };
  }

  private protocolFailure(code: string, message: string, observed: JsonObject): TransportFailure {
    return {
      code,
      message,
      retryable: false,
      category: "protocol",
      stage: "request_parse",
      expected: { protocol: 1, response_envelope: "exactly_one_of_result_or_error" },
      observed,
      evidence: [{ type: "protocol_output", ...observed }],
      retry: "after_change",
    };
  }

  private oversizedOutputFailure(observedBytes: number): TransportFailure {
    return {
      code: "RUST_TRANSPORT_OUTPUT_TOO_LARGE",
      message: "Rust transport output line exceeds maximum size",
      retryable: false,
      category: "protocol",
      stage: "request_parse",
      expected: { stdout_line_bytes_at_most: MAX_STDOUT_LINE_BYTES },
      observed: { stdout_line_bytes: observedBytes, stdout_content: "omitted" },
      evidence: [{ type: "stdout_bound", observed_bytes: observedBytes, limit_bytes: MAX_STDOUT_LINE_BYTES }],
      retry: "after_change",
    };
  }
}
