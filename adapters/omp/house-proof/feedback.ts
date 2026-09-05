class HouseText {
  constructor(private readonly text: string) {}

  render(_width: number): string[] {
    return this.text.split("\n");
  }
}

class HouseBlock {
  constructor(private readonly renderLines: (width: number) => string[]) {}

  render(width: number): string[] {
    return this.renderLines(Math.max(20, width));
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
type FeedbackTheme = {
  fg?: (color: string, text: string) => string;
  status?: Partial<Record<"pending" | "running" | "success" | "error" | "warning", string>>;
};

type FeedbackUi = {
  theme?: FeedbackTheme;
  notify?: (message: string, type?: "info" | "warning" | "error") => void;
  setStatus?: (key: string, text: string | undefined) => void;
  setWidget?: (
    key: string,
    content: string[] | ((tui: unknown, theme: FeedbackTheme) => HouseText) | undefined,
    options?: { placement?: "aboveEditor" | "belowEditor" },
  ) => void;
};

type FeedbackContext = {
  hasUI?: boolean;
  ui?: FeedbackUi;
};

export type HouseContextFeedback = {
  room?: string;
  spirit?: string;
  activities: string[];
  warnings?: string[];
};

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
  "recall_policy",
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

function statusSymbol(
  theme: FeedbackTheme | undefined,
  status: "pending" | "running" | "success" | "error" | "warning",
): string {
  return theme?.status?.[status] || {
    pending: "◇",
    running: "◈",
    success: "◆",
    error: "✗",
    warning: "▲",
  }[status];
}

function styled(
  theme: FeedbackTheme | undefined,
  color: string,
  text: string,
): string {
  return theme?.fg ? theme.fg(color, text) : text;
}

type HouseFrameLine = {
  text: string;
  color?: string;
};

type HouseFrameOptions = {
  header: string;
  headerColor: string;
  borderColor?: string;
  lines: HouseFrameLine[];
  footer?: string;
};

function compactWhitespace(value: string): string {
  return value.replace(/\s+/g, " ").trim();
}

function truncatePlain(value: string, width: number): string {
  if (width <= 0) return "";
  if (Bun.stringWidth(value) <= width) return value;
  if (width === 1) return "…";
  const limit = width - 1;
  let output = "";
  let used = 0;
  for (const char of value) {
    const charWidth = Bun.stringWidth(char);
    if (used + charWidth > limit) break;
    output += char;
    used += charWidth;
  }
  return `${output}…`;
}

function richFrame(theme: FeedbackTheme | undefined, frame: HouseFrameOptions): HouseBlock {
  return new HouseBlock((availableWidth) => {
    const width = Math.max(20, Math.min(availableWidth, 112));
    const contentWidth = width - 4;
    const header = truncatePlain(frame.header, width - 5);
    const topFill = "─".repeat(Math.max(1, width - Bun.stringWidth(header) - 5));
    const borderColor = frame.borderColor || "borderMuted";
    const border = (value: string) => styled(theme, borderColor, value);
    const output = [
      `${border("╭─ ")}${styled(theme, frame.headerColor, header)}${border(` ${topFill}╮`)}`,
    ];

    for (const line of frame.lines) {
      const text = truncatePlain(line.text, contentWidth);
      const padding = " ".repeat(Math.max(0, contentWidth - Bun.stringWidth(text)));
      output.push(`${border("│ ")}${styled(theme, line.color || "toolOutput", text)}${padding}${border(" │")}`);
    }

    if (!frame.footer) {
      output.push(border(`╰${"─".repeat(width - 2)}╯`));
      return output;
    }

    const footer = truncatePlain(frame.footer, width - 5);
    const bottomFill = "─".repeat(Math.max(1, width - Bun.stringWidth(footer) - 5));
    output.push(`${border("╰─ ")}${styled(theme, "dim", footer)}${border(` ${bottomFill}╯`)}`);
    return output;
  });
}

function textValue(value: unknown): string {
  if (typeof value === "string") return value.trim();
  if (typeof value === "number") return String(value);
  return "";
}

function recordValues(value: unknown): JsonRecord[] {
  return Array.isArray(value) ? value.map(asRecord).filter((item) => Object.keys(item).length > 0) : [];
}

function stringValues(value: unknown): string[] {
  return Array.isArray(value)
    ? value.map(textValue).filter(Boolean)
    : [];
}

function displayAuthority(value: unknown): string {
  const authority = textValue(value);
  if (authority === "postgres") return "PostgreSQL authority";
  return authority ? `${authority} authority` : "authoritative receipt";
}

function displayKind(value: unknown): string {
  const kind = textValue(value) || "memory";
  return kind
    .split(/[-_]/g)
    .filter(Boolean)
    .map((part) => `${part[0]?.toUpperCase() || ""}${part.slice(1)}`)
    .join(" ");
}

function sourceRoom(value: unknown): string {
  const source = textValue(value);
  return source.match(/^db-only\/([^/]+)\//)?.[1] || "";
}

function confidenceGauge(coverage: unknown): string {
  if (typeof coverage !== "number" || !Number.isFinite(coverage)) return "";
  const bounded = Math.max(0, Math.min(1, coverage));
  const filled = Math.round(bounded * 10);
  return `${"█".repeat(filled)}${"░".repeat(10 - filled)} ${Math.round(bounded * 100)}% terms`;
}

function resultIdentity(payload: JsonRecord, args: JsonRecord): string {
  const kind = displayKind(payload.kind || args.kind);
  const id = textValue(payload.memory_id || payload.lesson_id || payload.id);
  return id ? `${kind} #${id}` : kind;
}

/** The `backup` receipt a durable write carries: `ok`, `failed`, or `skipped`. */
export type BackupReceiptView = {
  status: "ok" | "failed" | "skipped" | "unknown";
  line: HouseFrameLine | null;
  detailLines: HouseFrameLine[];
  attention: boolean;
};

function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] || path;
}

function formatBytes(value: unknown): string {
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) return "";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KiB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MiB`;
}

export function backupReceiptView(payload: JsonRecord, theme: FeedbackTheme | undefined): BackupReceiptView {
  const backup = asRecord(payload.backup);
  const status = textValue(backup.status);
  if (status === "ok") {
    const dumpPath = textValue(backup.dump_path);
    const sha = textValue(backup.sha256);
    const meta = [
      dumpPath ? baseName(dumpPath) : "",
      sha ? `sha256 ${sha.slice(0, 12)}` : "",
      formatBytes(backup.bytes),
      typeof backup.elapsed_ms === "number" ? `${backup.elapsed_ms} ms` : "",
      textValue(backup.tool),
    ].filter(Boolean);
    return {
      status: "ok",
      line: { text: `${statusSymbol(theme, "success")} backup ok · ${meta.join(" · ")}`, color: "success" },
      detailLines: [
        ...(dumpPath ? [{ text: `dump · ${dumpPath}`, color: "dim" }] : []),
        ...(sha ? [{ text: `sha256 · ${sha}`, color: "dim" }] : []),
      ],
      attention: false,
    };
  }
  if (status === "failed") {
    const code = textValue(backup.code) || "backup_failed";
    const detail = textValue(backup.detail);
    const tool = textValue(backup.tool);
    const meta = [code, tool].filter(Boolean).join(" · ");
    return {
      status: "failed",
      line: {
        text: `${statusSymbol(theme, "warning")} backup failed · ${meta}${detail ? ` · ${detail}` : ""}`,
        color: "warning",
      },
      detailLines: [],
      attention: true,
    };
  }
  if (status === "skipped") {
    return {
      status: "skipped",
      line: { text: "backup skipped", color: "dim" },
      detailLines: [],
      attention: false,
    };
  }
  return { status: "unknown", line: null, detailLines: [], attention: false };
}

function createRememberRenderers(label: string) {
  const title = feedbackTitle(label);
  const generic = createGenericToolRenderers(label);
  return {
    inline: true,
    mergeCallAndResult: true,
    renderCall(args: unknown, _options: unknown, theme: FeedbackTheme) {
      const memoryTitle = textValue(asRecord(args).title);
      const description = memoryTitle ? `: ${truncatePlain(memoryTitle, 72)}` : "";
      return new HouseText(styled(theme, "muted", `${statusSymbol(theme, "pending")} ${title}${description}`));
    },
    renderResult(
      result: ToolResponse,
      options: { expanded?: boolean; isPartial?: boolean },
      theme: FeedbackTheme,
      args?: unknown,
    ) {
      const payload = asRecord(payloadFromResponse(result));
      if (options?.isPartial || isFailure(payload, result?.isError === true)) {
        return generic.renderResult(result, options, theme);
      }

      const argumentsRecord = asRecord(args);
      const memoryTitle = textValue(argumentsRecord.title) || "Untitled memory";
      const identity = resultIdentity(payload, argumentsRecord);
      const room = textValue(payload.room || argumentsRecord.room) || "current room";
      const warnings = stringValues(payload.warnings);
      const threads = stringValues(argumentsRecord.threads || argumentsRecord.threadKeys);
      const source = textValue(payload.source_path || payload.sourcePath);
      const body = textValue(argumentsRecord.body);
      const durable = payload.durable === true ? "durable" : "stored";
      const backup = backupReceiptView(payload, theme);
      const attention = warnings.length > 0 || backup.attention;
      const lines: HouseFrameLine[] = [
        { text: memoryTitle, color: "accent" },
        { text: `${room} · ${displayAuthority(payload.authority)} · ${durable}`, color: "muted" },
        {
          text: `${statusSymbol(theme, "success")} committed${threads.length ? ` · ${threads.length} thread${threads.length === 1 ? "" : "s"}` : ""}`,
          color: "success",
        },
      ];
      if (backup.line) lines.push(backup.line);

      if (options?.expanded) {
        if (body) {
          lines.push({ text: "", color: "muted" }, { text: "Remembered", color: "muted" });
          const bodyLines = body.split(/\r?\n/);
          for (const line of bodyLines.slice(0, 12)) {
            lines.push({ text: line || " ", color: "toolOutput" });
          }
          if (bodyLines.length > 12) {
            lines.push({ text: `… ${bodyLines.length - 12} more lines`, color: "dim" });
          }
        }
        if (source) lines.push({ text: `source · ${source}`, color: "dim" });
        lines.push(...backup.detailLines);
      }

      for (const warning of warnings) {
        lines.push({ text: `${statusSymbol(theme, "warning")} ${warning}`, color: "warning" });
      }

      return richFrame(theme, {
        header: `${statusSymbol(theme, attention ? "warning" : "success")} ${title} · ${identity}`,
        headerColor: attention ? "warning" : "success",
        lines,
        footer: body || source || backup.detailLines.length
          ? (options?.expanded ? "expanded evidence" : "⟨Ctrl+O: Expand⟩")
          : undefined,
      });
    },
  };
}
function createSleepRenderers(label: string) {
  const title = feedbackTitle(label);
  const generic = createGenericToolRenderers(label);
  return {
    inline: true,
    mergeCallAndResult: true,
    renderCall(_args: unknown, _options: unknown, theme: FeedbackTheme) {
      return new HouseText(styled(
        theme,
        "muted",
        `${statusSymbol(theme, "pending")} ${title}: casting paper boat`,
      ));
    },
    renderResult(
      result: ToolResponse,
      options: { expanded?: boolean; isPartial?: boolean },
      theme: FeedbackTheme,
      args?: unknown,
    ) {
      const payload = asRecord(payloadFromResponse(result));
      if (options?.isPartial || isFailure(payload, result?.isError === true)) {
        return generic.renderResult(result, options, theme);
      }

      const argumentsRecord = asRecord(args);
      const id = textValue(payload.memory_id || payload.id);
      const identity = id ? `Paper boat #${id}` : "Paper boat";
      const room = textValue(payload.room) || "current room";
      const source = textValue(payload.source_path || payload.sourcePath);
      const outboxEvent = textValue(payload.outbox_event_id);
      const body = textValue(argumentsRecord.body);
      const warnings = stringValues(payload.warnings);
      const durable = payload.durable === true;
      const inserted = payload.inserted === true;
      const backup = backupReceiptView(payload, theme);
      const attention = warnings.length > 0 || !durable || backup.status !== "ok";
      const lines: HouseFrameLine[] = [
        {
          text: `${room} · ${displayAuthority(payload.authority)} · ${durable ? "durable" : "durability unconfirmed"}`,
          color: "muted",
        },
        {
          text: `${statusSymbol(theme, attention ? "warning" : "success")} ${inserted ? "boat cast" : "boat confirmed"} · backup ${backup.status}`,
          color: attention ? "warning" : "success",
        },
      ];
      if (backup.line) lines.push(backup.line);

      if (!attention) {
        lines.push({ text: "continuity ready for the next session", color: "accent" });
      }

      if (options?.expanded) {
        if (body) {
          lines.push({ text: "", color: "muted" }, { text: "Carried forward", color: "muted" });
          const bodyLines = body.split(/\r?\n/);
          for (const line of bodyLines.slice(0, 12)) {
            lines.push({ text: line || " ", color: "toolOutput" });
          }
          if (bodyLines.length > 12) {
            lines.push({ text: `… ${bodyLines.length - 12} more lines`, color: "dim" });
          }
        }
        if (source) lines.push({ text: `source · ${source}`, color: "dim" });
        if (outboxEvent) lines.push({ text: `outbox · ${outboxEvent}`, color: "dim" });
        lines.push(...backup.detailLines);
      }

      for (const warning of warnings) {
        lines.push({ text: `${statusSymbol(theme, "warning")} ${warning}`, color: "warning" });
      }

      return richFrame(theme, {
        header: `${statusSymbol(theme, attention ? "warning" : "success")} ${title} · ${identity}`,
        headerColor: attention ? "warning" : "success",
        lines,
        footer: body || source || outboxEvent
          ? (options?.expanded ? "expanded continuity" : "⟨Ctrl+O: Expand⟩")
          : undefined,
      });
    },
  };
}


function recallCandidateLines(
  candidate: JsonRecord,
  rank: number,
  expanded: boolean,
): HouseFrameLine[] {
  const id = textValue(candidate.memory_id || candidate.id);
  const title = textValue(candidate.title) || "Untitled memory";
  const room = sourceRoom(candidate.source_path);
  const identity = `${rank}. ${id ? `#${id} ` : ""}${title}${room ? `  ⟨${room}⟩` : ""}`;
  const meta: string[] = [];
  if (typeof candidate.score === "number") meta.push(`score ${candidate.score.toFixed(2)}`);
  const gauge = confidenceGauge(candidate.term_coverage);
  if (gauge) meta.push(gauge);
  const lines: HouseFrameLine[] = [
    { text: identity, color: "accent" },
    ...(meta.length ? [{ text: `   ${meta.join(" · ")}`, color: "muted" }] : []),
  ];

  if (!expanded) return lines;
  const excerpt = textValue(candidate.excerpt);
  if (excerpt) {
    const excerptLines = excerpt.split(/\r?\n/).map(compactWhitespace).filter(Boolean);
    for (const line of excerptLines.slice(0, 4)) {
      lines.push({ text: `   ${line}`, color: "toolOutput" });
    }
    if (excerptLines.length > 4) {
      lines.push({ text: `   … ${excerptLines.length - 4} more excerpt lines`, color: "dim" });
    }
  }
  const source = textValue(candidate.source_path);
  if (source) lines.push({ text: `   source · ${source}`, color: "dim" });
  return lines;
}

function createRecallRenderers(label: string) {
  const title = feedbackTitle(label);
  const generic = createGenericToolRenderers(label);
  return {
    inline: true,
    mergeCallAndResult: true,
    renderCall(args: unknown, _options: unknown, theme: FeedbackTheme) {
      const query = compactWhitespace(textValue(asRecord(args).query));
      const description = query ? `: ${truncatePlain(query, 80)}` : "";
      return new HouseText(styled(theme, "muted", `${statusSymbol(theme, "pending")} ${title}${description}`));
    },
    renderResult(
      result: ToolResponse,
      options: { expanded?: boolean; isPartial?: boolean },
      theme: FeedbackTheme,
      args?: unknown,
    ) {
      const payload = asRecord(payloadFromResponse(result));
      if (options?.isPartial || isFailure(payload, result?.isError === true)) {
        return generic.renderResult(result, options, theme);
      }

      const query = compactWhitespace(textValue(payload.query || asRecord(args).query));
      const found = payload.found === true;
      const source = textValue(payload.source);
      const warnings = stringValues(payload.warnings);
      const canonMatches = recordValues(payload.canonMatches);
      const candidates = recordValues(payload.retrievalCandidates);
      const total = canonMatches.length + candidates.length;
      const expanded = options?.expanded === true;
      const lines: HouseFrameLine[] = [];

      if (query) lines.push({ text: `“${query}”`, color: "toolOutput" });
      const summary = [
        source || "active substrate",
        canonMatches.length ? `${canonMatches.length} canon` : "",
        candidates.length ? `${candidates.length} memories` : "",
      ].filter(Boolean);
      lines.push({ text: summary.join(" · "), color: "muted" });

      for (const canon of canonMatches.slice(0, expanded ? 5 : 2)) {
        const term = textValue(canon.termKey || canon.name) || "canonical match";
        const type = textValue(canon.type);
        lines.push({ text: `◆ canon · ${term}${type ? ` · ${type}` : ""}`, color: "success" });
        if (expanded) {
          const canonSummary = textValue(canon.summary);
          for (const line of canonSummary.split(/\r?\n/).map(compactWhitespace).filter(Boolean).slice(0, 3)) {
            lines.push({ text: `   ${line}`, color: "toolOutput" });
          }
        }
      }

      const candidateLimit = expanded ? 10 : 3;
      candidates.slice(0, candidateLimit).forEach((candidate, index) => {
        lines.push(...recallCandidateLines(candidate, index + 1, expanded));
      });
      const hidden = candidates.length - Math.min(candidates.length, candidateLimit);
      if (hidden > 0) lines.push({ text: `… ${hidden} more memories`, color: "dim" });
      if (!found && total === 0) lines.push({ text: "No authoritative match found.", color: "warning" });
      for (const warning of warnings) {
        lines.push({ text: `${statusSymbol(theme, "warning")} ${warning}`, color: "warning" });
      }

      const expandable = canonMatches.length > 0 || candidates.length > 0;
      const status = warnings.length || !found ? "warning" : "success";
      return richFrame(theme, {
        header: `${statusSymbol(theme, status)} ${title} · ${found ? `${total || 1} matched` : "no match"}`,
        headerColor: status === "warning" ? "warning" : "success",
        lines,
        footer: expandable ? (expanded ? "expanded evidence" : "⟨Ctrl+O: Expand⟩") : undefined,
      });
    },
  };
}

function feedbackTitle(label: string): string {
  return label.startsWith("Athanor") ? label : `Athanor ${label}`;
}

function feedbackUi(ctx: FeedbackContext | undefined): FeedbackUi | undefined {
  if (!ctx?.ui || ctx.hasUI === false) return undefined;
  return ctx.ui;
}

function createGenericToolRenderers(label: string) {
  const title = feedbackTitle(label);
  return {
    renderCall(args: unknown, _options: unknown, theme: FeedbackTheme) {
      const argument = primaryArgument(args);
      const text = argument ? `${title}: ${argument}` : title;
      const line = `${statusSymbol(theme, "pending")} ${text}`;
      return new HouseText(styled(theme, "muted", line));
    },
    renderResult(result: ToolResponse, options: { expanded?: boolean; isPartial?: boolean }, theme: FeedbackTheme) {
      const payload = payloadFromResponse(result);
      const failed = isFailure(payload, result?.isError === true);
      if (options?.expanded) {
        return new HouseText(styled(theme, failed ? "error" : "toolOutput", canonicalJson(payload)));
      }
      let status: "running" | "success" | "error" = "success";
      let summary = compactResult(payload);
      let color = "success";
      if (options?.isPartial) {
        status = "running";
        summary = "working…";
        color = "accent";
      } else if (failed) {
        status = "error";
        color = "error";
      }
      return new HouseText(styled(theme, color, `${statusSymbol(theme, status)} ${title}: ${summary}`));
    },
  };
}

export function createToolRenderers(label: string, operation?: string) {
  if (operation === "remember") return createRememberRenderers(label);
  if (operation === "recall") return createRecallRenderers(label);
  if (operation === "sleep") return createSleepRenderers(label);
  return createGenericToolRenderers(label);
}

export function beginHouseToolFeedback(ctx: FeedbackContext | undefined, label: string): void {
  const ui = feedbackUi(ctx);
  if (!ui?.setStatus) return;
  const theme = ui.theme;
  ui.setStatus(
    "athanor",
    styled(theme, "accent", `${statusSymbol(theme, "running")} ${feedbackTitle(label)} · working`),
  );
}

export function completeHouseToolFeedback(
  ctx: FeedbackContext | undefined,
  label: string,
  response: ToolResponse,
): void {
  const ui = feedbackUi(ctx);
  if (!ui?.setStatus) return;
  const payload = payloadFromResponse(response);
  const failed = isFailure(payload, response?.isError === true);
  const theme = ui.theme;
  const status = failed ? "error" : "success";
  const color = failed ? "error" : "success";
  ui.setStatus(
    "athanor",
    styled(theme, color, `${statusSymbol(theme, status)} ${feedbackTitle(label)} · ${compactResult(payload)}`),
  );
}

function compactActivity(value: string): string {
  const text = value.replace(/\s+/g, " ").trim();
  return text.length > 72 ? `${text.slice(0, 71)}…` : text;
}

export function showHouseContextFeedback(
  ctx: FeedbackContext | undefined,
  feedback: HouseContextFeedback,
): void {
  const ui = feedbackUi(ctx);
  if (!ui) return;
  const activities = feedback.activities.map(compactActivity).filter(Boolean).slice(0, 5);
  const warnings = (feedback.warnings || []).map(compactActivity).filter(Boolean).slice(0, 3);
  const identity = compactActivity(feedback.spirit || feedback.room || "ready");
  const theme = ui.theme;

  if (ui.setStatus) {
    const status = warnings.length ? "warning" : "success";
    const color = warnings.length ? "warning" : "success";
    const detail = activities.length ? activities.join(" · ") : "ready";
    ui.setStatus(
      "athanor",
      styled(theme, color, `${statusSymbol(theme, status)} Athanor · ${identity} · ${detail}`),
    );
  }

  if (!ui.setWidget) return;
  if (!activities.length && !warnings.length) {
    ui.setWidget("athanor-activity", undefined);
    return;
  }
  ui.setWidget(
    "athanor-activity",
    (_tui, widgetTheme) => {
      const lines = [
        styled(widgetTheme, warnings.length ? "warning" : "accent", `◆ The Athanor · ${identity}`),
      ];
      if (activities.length) lines.push(styled(widgetTheme, "muted", `  ${activities.join(" · ")}`));
      for (const warning of warnings) {
        lines.push(styled(widgetTheme, "warning", `  ${statusSymbol(widgetTheme, "warning")} ${warning}`));
      }
      return new HouseText(lines.join("\n"));
    },
    { placement: "belowEditor" },
  );
}
