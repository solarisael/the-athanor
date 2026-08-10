class HouseText {
  constructor(private readonly text: string) {}

  render(_width: number): string[] {
    return this.text.split("\n");
  }
}

type JsonRecord = Record<string, unknown>;
type ToolContent = { type: "text"; text: string };
type ToolResponse = {
  isError?: boolean;
  content?: ToolContent[];
  details?: unknown;
};

type ToolUpdate = (result: { isError?: boolean; content: ToolContent[]; details: unknown }) => void;

const SENSITIVE_KEY = /(?:api[_-]?key|authorization|cookie|credential|pass(?:word)?|secret|session|token|private[_-]?key|database[_-]?(?:url|dsn)|connection[_-]?string|(?:request_?)?(?:body|payload))/i;
const SENSITIVE_ASSIGNMENT = /\b((?:api[_-]?key|authorization|cookie|credential|pass(?:word)?|secret|session|token|private[_-]?key|database[_-]?(?:url|dsn)|connection[_-]?string|(?:request_?)?(?:body|payload))\s*[=:]\s*)([^\s,;]+)/gi;
const SENSITIVE_JSON_FIELD = /((?:["'])(?:api[_-]?key|authorization|cookie|credential|pass(?:word)?|secret|session|token|private[_-]?key|database[_-]?(?:url|dsn)|connection[_-]?string|(?:request_?)?(?:body|payload))(?:["'])\s*:\s*)(?:"(?:\\.|[^"])*"|[^,\s}]+)/gi;
const AUTH_HEADER = /\b(Bearer|Basic|Token)\s+[A-Za-z0-9._~+/=-]+/gi;
const AUTHENTICATED_URL = /([a-z][a-z0-9+.-]*:\/\/)([^\s/@]+)@/gi;
const WRITE_OPERATIONS = new Set([
  "remember",
  "delete_lesson",
  "update_lesson",
  "set_room_state",
  "sleep",
  "house_routing_mode",
  "house_model_default",
  "anamnesis_write",
]);
const VALID_WRITE_OUTCOMES = new Set(["not_started", "rolled_back", "committed", "unknown"]);
const VALID_RETRIES = new Set(["safe_now", "after_change", "reconcile_first", "never"]);
const VALID_CATEGORIES = new Set(["input", "transport", "protocol", "configuration", "database", "embedding", "filesystem", "backup", "authorization", "operation", "reconciliation", "internal"]);
const VALID_STAGES = new Set(["validation", "spawn", "startup", "request_write", "request_parse", "configuration_load", "database_connect", "database_query", "embedding_request", "transaction", "backup", "response_encode", "reconciliation", "shutdown"]);

function asRecord(value: unknown): JsonRecord {
  return value && typeof value === "object" && !Array.isArray(value) ? value as JsonRecord : {};
}

function redactString(value: string): string {
  return value
    .replace(AUTHENTICATED_URL, "$1[redacted]@")
    .replace(AUTH_HEADER, "$1 [redacted]")
    .replace(SENSITIVE_ASSIGNMENT, "$1[redacted]")
    .replace(SENSITIVE_JSON_FIELD, "$1\"[redacted]\"");
}

function redact(value: unknown, key?: string, seen = new WeakSet<object>()): unknown {
  if (key && SENSITIVE_KEY.test(key)) {
    return { redacted: true, present: value !== undefined && value !== null && value !== "" };
  }
  if (typeof value === "string") return redactString(value);
  if (value === null || typeof value === "boolean" || typeof value === "number") return value;
  if (typeof value === "bigint") return value.toString();
  if (value === undefined) return null;
  if (value instanceof Error) {
    return { name: value.name, message: redactString(value.message) };
  }
  if (Array.isArray(value)) return value.map((item) => redact(item, undefined, seen));
  if (typeof value === "object") {
    if (seen.has(value)) return "[circular]";
    seen.add(value);
    const output: JsonRecord = {};
    for (const [entryKey, entryValue] of Object.entries(value)) {
      output[entryKey] = redact(entryValue, entryKey, seen);
    }
    return output;
  }
  return String(value);
}

function canonicalJson(value: unknown): string {
  return JSON.stringify(value, null, 2) ?? "null";
}

function payloadFromResponse(response: ToolResponse): unknown {
  const text = response.content?.find((entry) => entry?.type === "text")?.text;
  if (typeof text === "string") {
    try {
      return JSON.parse(text);
    } catch {
      if (text.trim()) {
        return { ...asRecord(response.details), error: text };
      }
    }
  }
  return response.details ?? {};
}

function isFailure(payload: unknown, declaredError: boolean): boolean {
  if (declaredError) return true;
  const result = asRecord(payload);
  return result.ok === false || result.status === "error";
}

function diagnosticCategory(code: string, message: string, operation: string): string {
  const subject = `${code} ${message} ${operation}`.toLowerCase();
  if (subject.includes("valid") || subject.includes("require") || subject.includes("must ") || subject.includes("invalid") || subject.includes("does not accept") || subject.includes("mutually exclusive") || subject.includes("non-empty")) return "input";
  if (subject.includes("outcome_unknown") || subject.includes("reconcil")) return "reconciliation";
  if (subject.includes("transport") || subject.includes("spawn") || subject.includes("timeout") || subject.includes("connection")) return "transport";
  if (subject.includes("protocol") || subject.includes("malformed")) return "protocol";
  if (subject.includes("config") || subject.includes("environment")) return "configuration";
  if (subject.includes("postgres") || subject.includes("database")) return "database";
  if (subject.includes("authorization") || subject.includes("unauthoriz") || subject.includes("permission")) return "authorization";
  if (subject.includes("file") || subject.includes("path")) return "filesystem";
  return "operation";
}

function defaultStage(operation: string, category: string): string {
  if (category === "input") return "validation";
  if (category === "reconciliation") return "reconciliation";
  return WRITE_OPERATIONS.has(operation) ? "request_write" : "request_parse";
}

function defaultNextCheck(operation: string, retry: string): JsonRecord {
  if (retry === "reconcile_first") {
    return { action: "reconcile", operation, retry: "reconcile_first" };
  }
  if (retry === "after_change") return { action: "correct_input", operation, retry: "after_change" };
  if (retry === "safe_now") return { action: "retry", operation, retry: "safe_now" };
  return { action: "inspect", operation, retry: "never" };
}

function evidenceRecords(values: unknown[]): JsonRecord[] {
  return values.map((value) => {
    const record = asRecord(value);
    return Object.keys(record).length > 0 ? record : { kind: "upstream", value };
  });
}

function nextCheckRecords(values: unknown[], operation: string): JsonRecord[] {
  return values.map((value) => {
    const record = asRecord(value);
    return {
      ...record,
      action: typeof record.action === "string" ? record.action : "inspect",
      operation: typeof record.operation === "string" ? record.operation : operation,
      ...(Object.keys(record).length > 0 ? {} : { value }),
    };
  });
}

function canonicalError(payload: unknown, operation: string): JsonRecord {
  const source = asRecord(payload);
  const redactedDetails = redact(source.details);
  const sourceDetails = redactedDetails && typeof redactedDetails === "object" && !Array.isArray(redactedDetails)
    ? redactedDetails as JsonRecord
    : source.details === undefined ? {} : { upstream_details: redactedDetails };
  const code = typeof source.code === "string" && source.code.trim() ? source.code : "tool_failure";
  const sourceMessage = typeof source.message === "string"
    ? source.message
    : typeof source.error === "string"
      ? source.error
      : "Athanor tool failed";
  const message = redactString(sourceMessage);
  const retryable = typeof source.retryable === "boolean" ? source.retryable : false;
  const outcomeUnknown = source.outcome === "unknown" || code.toLowerCase() === "outcome_unknown";
  const inferredCategory = diagnosticCategory(code, message, operation);
  const declaredCategory = typeof sourceDetails.category === "string" ? sourceDetails.category : "";
  const category = VALID_CATEGORIES.has(declaredCategory) ? declaredCategory : inferredCategory;
  const sourceExecution = asRecord(sourceDetails.execution);
  const sourceRetry = typeof sourceExecution.retry === "string" ? sourceExecution.retry : "";
  const retry = outcomeUnknown
    ? "reconcile_first"
    : VALID_RETRIES.has(sourceRetry)
      ? sourceRetry
      : retryable
        ? "safe_now"
        : category === "input"
          ? "after_change"
          : "never";
  const sourceWriteOutcome = typeof sourceExecution.write_outcome === "string" ? sourceExecution.write_outcome : "";
  const writeOutcome = outcomeUnknown
    ? "unknown"
    : VALID_WRITE_OUTCOMES.has(sourceWriteOutcome)
      ? sourceWriteOutcome
      : category === "input"
        ? "not_started"
        : "not_started";
  const sourceDispatched = sourceExecution.request_dispatched;
  const requestDispatched = typeof sourceDispatched === "boolean"
    ? sourceDispatched
    : outcomeUnknown
      ? true
      : category === "input"
        ? false
        : null;
  const observed = {
    ...asRecord(sourceDetails.observed),
    ...Object.fromEntries(Object.entries(source)
      .filter(([key]) => !["ok", "status", "error", "message", "code", "retryable", "details"].includes(key))
      .map(([key, value]) => [key, redact(value, key)])),
  };
  const sourceEvidence = evidenceRecords(Array.isArray(sourceDetails.evidence)
    ? sourceDetails.evidence
    : Array.isArray(source.evidence)
      ? redact(source.evidence) as unknown[]
      : []);
  const sourceTargets = Array.isArray(sourceDetails.targets)
    ? sourceDetails.targets
    : Array.isArray(source.targets)
      ? redact(source.targets) as unknown[]
      : [];
  const existingNextChecks = nextCheckRecords(Array.isArray(sourceDetails.next_checks) ? sourceDetails.next_checks : [], operation);
  const hasReconciliationCheck = existingNextChecks.some((check) => check.action === "reconcile");
  const nextChecks = outcomeUnknown && !hasReconciliationCheck
    ? [defaultNextCheck(operation, retry), ...existingNextChecks]
    : existingNextChecks.length > 0
      ? existingNextChecks
      : [defaultNextCheck(operation, retry)];
  const details: JsonRecord = {
    ...sourceDetails,
    category,
    stage: VALID_STAGES.has(typeof sourceDetails.stage === "string" ? sourceDetails.stage : "")
      ? sourceDetails.stage
      : defaultStage(operation, category),
    operation,
    expected: sourceDetails.expected ?? null,
    observed,
    evidence: sourceEvidence,
    targets: sourceTargets,
    next_checks: nextChecks,
    execution: {
      ...sourceExecution,
      request_dispatched: requestDispatched,
      write_outcome: writeOutcome,
      retry,
    },
  };
  const preservedSource = Object.fromEntries(
    Object.entries(source)
      .filter(([key]) => ![
        "ok",
        "status",
        "error",
        "message",
        "code",
        "retryable",
        "details",
        "evidence",
        "targets",
      ].includes(key))
      .map(([key, value]) => [key, redact(value, key)]),
  );
  return {
    ...preservedSource,
    ok: false,
    status: "error",
    code,
    error: message,
    message,
    retryable,
    details,
  };
}

function finalFeedback(payload: unknown, operation: string, declaredError = false) {
  if (isFailure(payload, declaredError)) {
    const error = canonicalError(payload, operation);
    return { isError: true, content: [{ type: "text" as const, text: canonicalJson(error) }], details: error };
  }
  try {
    return { content: [{ type: "text" as const, text: canonicalJson(payload) }], details: payload };
  } catch (error) {
    return finalFeedback({ error }, operation, true);
  }
}

export function normalizeToolResponse(response: ToolResponse, operation: string) {
  return finalFeedback(payloadFromResponse(response), operation, response.isError === true);
}

export function toolThrown(error: unknown, operation: string) {
  const sourceError = error && typeof error === "object" ? error as {
    message?: unknown;
    code?: unknown;
    retryable?: unknown;
    details?: unknown;
    stderr?: unknown;
    cause?: unknown;
  } : {};
  const redactedDetails = redact(sourceError.details);
  const details = redactedDetails && typeof redactedDetails === "object" && !Array.isArray(redactedDetails)
    ? redactedDetails as JsonRecord
    : sourceError.details === undefined ? {} : { upstream_details: redactedDetails };
  const stderr = typeof sourceError.stderr === "string" ? sourceError.stderr.slice(0, 4096) : "";
  const evidence = Array.isArray(details.evidence) ? [...details.evidence] : [];
  if (stderr) evidence.push({ source: "rust_stderr", text: stderr, truncated: String(sourceError.stderr).length > stderr.length });
  return finalFeedback({
    error: typeof sourceError.message === "string" ? sourceError.message : String(error),
    ...(typeof sourceError.code === "string" ? { code: sourceError.code } : {}),
    ...(typeof sourceError.retryable === "boolean" ? { retryable: sourceError.retryable } : {}),
    details: { ...details, evidence },
    ...(sourceError.cause !== undefined ? { cause: sourceError.cause } : {}),
  }, operation, true);
}

export function emitToolUpdate(onUpdate: unknown, operation: string): void {
  if (typeof onUpdate !== "function") return;
  const update = {
    status: "running",
    operation,
    details: {
      category: "operation",
      stage: WRITE_OPERATIONS.has(operation) ? "request_write" : "request_parse",
      operation,
      execution: { request_dispatched: false, write_outcome: "not_started", retry: "never" },
    },
  };
  (onUpdate as ToolUpdate)({ content: [{ type: "text", text: canonicalJson(update) }], details: update });
}

const SUMMARY_SKIP = /^(?:ok|status|family|version|path|query)$/i;
const SUMMARY_MAX_FIELDS = 6;
const SUMMARY_MAX_LENGTH = 160;

function summaryFragment(key: string, value: unknown): string | null {
  const label = key.replace(/([a-z0-9])([A-Z])/g, "$1 $2").replace(/[_-]+/g, " ").toLowerCase();
  if (Array.isArray(value)) return `${value.length} ${label}`;
  if (typeof value === "boolean") return value ? label : `not ${label}`;
  if (typeof value === "number") return `${label} ${value}`;
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed && trimmed.length <= 48 ? `${label} ${trimmed}` : null;
}

function summaryFields(payload: JsonRecord): string[] {
  const fragments: string[] = [];
  for (const [key, value] of Object.entries(payload)) {
    if (SUMMARY_SKIP.test(key) || SENSITIVE_KEY.test(key)) continue;
    const fragment = summaryFragment(key, value);
    if (fragment) fragments.push(fragment);
  }
  return fragments;
}

function compactResult(result: unknown): string {
  const payload = asRecord(result);
  if (payload.status === "error" || payload.ok === false) {
    const code = typeof payload.code === "string" ? payload.code : "tool_failure";
    const message = typeof payload.message === "string" ? payload.message : "failed";
    return `${code}: ${message}`;
  }
  let fragments = summaryFields(payload);
  // Some payloads keep everything one level down — room_state is {path, state} — so a thin
  // top level means the content is nested, not missing.
  if (fragments.length < 2) {
    for (const [key, value] of Object.entries(payload)) {
      if (SUMMARY_SKIP.test(key) || SENSITIVE_KEY.test(key)) continue;
      if (!value || typeof value !== "object" || Array.isArray(value)) continue;
      fragments = fragments.concat(summaryFields(value as JsonRecord));
    }
  }
  const line = fragments.slice(0, SUMMARY_MAX_FIELDS).join(" · ");
  if (!line) return "completed";
  return line.length > SUMMARY_MAX_LENGTH ? `${line.slice(0, SUMMARY_MAX_LENGTH - 1)}…` : line;
}

function primaryArgument(args: unknown): string {
  for (const [key, value] of Object.entries(asRecord(args))) {
    if (SENSITIVE_KEY.test(key) || typeof value !== "string") continue;
    const trimmed = value.trim();
    if (trimmed) return trimmed.length > 72 ? `${trimmed.slice(0, 71)}…` : trimmed;
  }
  return "";
}

export function createToolRenderers(label: string) {
  const title = label.startsWith("Athanor") ? label : `Athanor ${label}`;
  return {
    renderCall(args: unknown, _options: unknown, theme: { fg?: (color: string, text: string) => string }) {
      const argument = primaryArgument(args);
      const text = argument ? `${title}: ${argument}` : title;
      return new HouseText(theme?.fg ? theme.fg("muted", text) : text);
    },
    renderResult(result: ToolResponse, options: { expanded?: boolean; isPartial?: boolean }, theme: { fg?: (color: string, text: string) => string }) {
      const payload = payloadFromResponse(result);
      const text = options?.expanded
        ? canonicalJson(payload)
        : options?.isPartial
          ? `${title}: working…`
          : `${title}: ${compactResult(payload)}`;
      const color = isFailure(payload, result?.isError === true) ? "error" : "muted";
      return new HouseText(theme?.fg ? theme.fg(color, text) : text);
    },
  };
}
