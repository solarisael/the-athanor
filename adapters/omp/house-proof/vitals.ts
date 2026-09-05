// Vitals absence is not zero; the authenticated Host stamps authority, so the
// bounded request carries no caller-supplied identity.

import { HostUnavailable, hostHttpEndpoint } from "./host.ts";
import { insulaErrorClass, isInsulaToolOperation } from "./insula.ts";
import { roomContext } from "./room.ts";

export const INSULA_VITALS_PATH = "/athanor/v1/insula/vitals";

// A fast row query is not a fast page: the window, the row count, the request
// wait, and the rendered height are each bounded here.
const READ_TIMEOUT_MS = 4_000;
const MAX_VITALS_ROWS = 1_000;
const MAX_PANEL_WIDTH = 96;
const MIN_PANEL_WIDTH = 32;

export type InsulaVitalsRange = "15m" | "1h" | "24h";

const RANGE_MS: Record<InsulaVitalsRange, number> = {
  "15m": 900_000,
  "1h": 3_600_000,
  "24h": 86_400_000,
};

export const DEFAULT_INSULA_VITALS_RANGE: InsulaVitalsRange = "1h";
export const INSULA_VITALS_RANGES = Object.keys(RANGE_MS) as InsulaVitalsRange[];

/** The three ranges the cockpit accepts. Anything else is refused, not guessed. */
export function parseInsulaVitalsRange(args: unknown): InsulaVitalsRange | null {
  const candidate = String(args ?? "").trim().toLowerCase();
  if (!candidate) return DEFAULT_INSULA_VITALS_RANGE;
  return candidate in RANGE_MS ? (candidate as InsulaVitalsRange) : null;
}

/** The Vitals dimensions and sums this cockpit reads. The Host may serve more. */
export type InsulaVitalsRow = {
  minute: string;
  room: string;
  spirit: string;
  component: string;
  layer: string;
  operation: string;
  phase: string;
  outcomeClass: string;
  eventCount: number;
  durationUsSum: number;
  durationUsMax: number | null;
  tokensInSum: number;
  tokensOutSum: number;
  dropCountSum: number;
  sourceLastObservedAt: string;
};

export type InsulaVitalsResponse = {
  schemaVersion: number;
  queryName: string;
  queryVersion: number;
  houseId: string;
  room: string;
  spirit: string;
  start: string;
  end: string;
  limit: number;
  truncated: boolean;
  rows: InsulaVitalsRow[];
};

export type InsulaVitalsSummary = {
  range: InsulaVitalsRange;
  house: string;
  room: string;
  spirit: string;
  start: string;
  end: string;
  truncated: boolean;
  requests: {
    settled: number;
    outcomes: Array<{ outcomeClass: string; count: number }>;
    meanUs: number | null;
    maxUs: number | null;
  };
  usage: { measured: number; unknown: number; tokensIn: number; tokensOut: number };
  toolCalls: number;
  errorEvents: number;
  degradedEvents: number;
  drops: number;
  latestObservedAt: string | null;
};

function fields(value: unknown, where: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new HostUnavailable(`Insula Vitals ${where} is not an object`);
  }
  return value as Record<string, unknown>;
}

function textField(source: Record<string, unknown>, key: string, where: string): string {
  const value = source[key];
  if (typeof value !== "string" || !value.trim()) {
    throw new HostUnavailable(`Insula Vitals ${where} field ${key} is not a string`);
  }
  return value;
}

function countField(source: Record<string, unknown>, key: string, where: string): number {
  const value = source[key];
  if (typeof value !== "number" || !Number.isFinite(value) || value < 0) {
    throw new HostUnavailable(`Insula Vitals ${where} field ${key} is not a count`);
  }
  return value;
}

function optionalCountField(
  source: Record<string, unknown>,
  key: string,
  where: string,
): number | null {
  const value = source[key];
  if (value === null || value === undefined) return null;
  return countField(source, key, where);
}

function flagField(source: Record<string, unknown>, key: string, where: string): boolean {
  const value = source[key];
  if (typeof value !== "boolean") {
    throw new HostUnavailable(`Insula Vitals ${where} field ${key} is not a boolean`);
  }
  return value;
}

function parseRow(value: unknown, index: number): InsulaVitalsRow {
  const where = `row ${index}`;
  const source = fields(value, where);
  return {
    minute: textField(source, "minute", where),
    room: textField(source, "room", where),
    spirit: textField(source, "spirit", where),
    component: textField(source, "component", where),
    layer: textField(source, "layer", where),
    operation: textField(source, "operation", where),
    phase: textField(source, "phase", where),
    outcomeClass: textField(source, "outcomeClass", where),
    eventCount: countField(source, "eventCount", where),
    durationUsSum: countField(source, "durationUsSum", where),
    durationUsMax: optionalCountField(source, "durationUsMax", where),
    tokensInSum: countField(source, "tokensInSum", where),
    tokensOutSum: countField(source, "tokensOutSum", where),
    dropCountSum: countField(source, "dropCountSum", where),
    sourceLastObservedAt: textField(source, "sourceLastObservedAt", where),
  };
}

/** Strict and mechanical: a response the Host did not shape is unavailable, not partial. */
export function parseInsulaVitals(payload: unknown): InsulaVitalsResponse {
  const source = fields(payload, "response");
  const rows = source.rows;
  if (!Array.isArray(rows)) throw new HostUnavailable("Insula Vitals response field rows is not an array");
  const limit = countField(source, "limit", "response");
  if (limit < 1 || limit > MAX_VITALS_ROWS || rows.length > limit) {
    throw new HostUnavailable("Insula Vitals response exceeds its row bound");
  }
  return {
    schemaVersion: countField(source, "schemaVersion", "response"),
    queryName: textField(source, "queryName", "response"),
    queryVersion: countField(source, "queryVersion", "response"),
    houseId: textField(source, "houseId", "response"),
    room: textField(source, "room", "response"),
    spirit: textField(source, "spirit", "response"),
    start: textField(source, "start", "response"),
    end: textField(source, "end", "response"),
    limit,
    truncated: flagField(source, "truncated", "response"),
    rows: rows.map(parseRow),
  };
}

export async function readInsulaVitals(
  room: string,
  range: InsulaVitalsRange,
  now: number = Date.now(),
): Promise<InsulaVitalsResponse> {
  const endpoint = hostHttpEndpoint(room, INSULA_VITALS_PATH);
  let response: Response;
  try {
    response = await fetch(endpoint.url, {
      method: "POST",
      headers: { "content-type": "application/json", authorization: `Bearer ${endpoint.token}` },
      body: JSON.stringify({
        start: new Date(now - RANGE_MS[range]).toISOString(),
        end: new Date(now).toISOString(),
        limit: MAX_VITALS_ROWS,
      }),
      signal: AbortSignal.timeout(READ_TIMEOUT_MS),
    });
  } catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    throw new HostUnavailable(`Insula Vitals is unavailable: ${detail}`);
  }
  if (!response.ok) {
    await response.arrayBuffer().catch(() => undefined);
    throw new HostUnavailable(`Insula Vitals refused with status ${response.status}`);
  }
  const payload = await response.json().catch(() => {
    throw new HostUnavailable("Insula Vitals answered with unreadable JSON");
  });
  return parseInsulaVitals(payload);
}

export function summarizeInsulaVitals(
  range: InsulaVitalsRange,
  response: InsulaVitalsResponse,
): InsulaVitalsSummary {
  const outcomes = new Map<string, number>();
  let settled = 0;
  let durationSum = 0;
  let maxUs: number | null = null;
  let measured = 0;
  let unknown = 0;
  let tokensIn = 0;
  let tokensOut = 0;
  let toolCalls = 0;
  let errorEvents = 0;
  let degradedEvents = 0;
  let drops = 0;
  let latestObservedAt: string | null = null;
  let latestAt = Number.NEGATIVE_INFINITY;

  for (const row of response.rows) {
    if (row.outcomeClass === "error") errorEvents += row.eventCount;
    if (row.outcomeClass === "degraded") degradedEvents += row.eventCount;
    drops += row.dropCountSum;
    const observedAt = Date.parse(row.sourceLastObservedAt);
    if (Number.isFinite(observedAt) && observedAt > latestAt) {
      latestAt = observedAt;
      latestObservedAt = row.sourceLastObservedAt;
    }
    if (row.operation === "provider_request" && row.phase === "end") {
      settled += row.eventCount;
      durationSum += row.durationUsSum;
      outcomes.set(row.outcomeClass, (outcomes.get(row.outcomeClass) ?? 0) + row.eventCount);
      if (row.durationUsMax !== null && (maxUs === null || row.durationUsMax > maxUs)) {
        maxUs = row.durationUsMax;
      }
      continue;
    }
    if (row.operation === "provider_usage" && row.phase === "point") {
      // The operation, not a zero token count, is what makes unmetered usage
      // visible: a degraded usage point means the provider reported nothing.
      if (row.outcomeClass === "ok") measured += row.eventCount;
      else unknown += row.eventCount;
      tokensIn += row.tokensInSum;
      tokensOut += row.tokensOutSum;
      continue;
    }
    if (isInsulaToolOperation(row.operation) && row.phase === "end") toolCalls += row.eventCount;
  }

  return {
    range,
    house: response.houseId,
    room: response.room,
    spirit: response.spirit,
    start: response.start,
    end: response.end,
    truncated: response.truncated,
    requests: {
      settled,
      outcomes: [...outcomes]
        .map(([outcomeClass, count]) => ({ outcomeClass, count }))
        .sort((left, right) => right.count - left.count || left.outcomeClass.localeCompare(right.outcomeClass)),
      meanUs: settled > 0 ? durationSum / settled : null,
      maxUs,
    },
    usage: { measured, unknown, tokensIn, tokensOut },
    toolCalls,
    errorEvents,
    degradedEvents,
    drops,
    latestObservedAt,
  };
}

function tally(value: number): string {
  return value.toLocaleString("en-US");
}

function span(microseconds: number | null): string {
  if (microseconds === null) return "unmeasured";
  const milliseconds = microseconds / 1_000;
  return milliseconds < 1_000 ? `${Math.round(milliseconds)}ms` : `${(milliseconds / 1_000).toFixed(1)}s`;
}

function stamp(value: string, length: number): string {
  const parsed = Date.parse(value);
  return Number.isFinite(parsed) ? `${new Date(parsed).toISOString().slice(0, length)}Z` : value;
}

export function insulaCockpitLines(summary: InsulaVitalsSummary): string[] {
  const outcomes = summary.requests.outcomes
    .map((outcome) => `${outcome.outcomeClass} ${tally(outcome.count)}`)
    .join(" · ");
  const lines = [
    `window ${summary.range} · ${stamp(summary.start, 16)} → ${stamp(summary.end, 16)}`,
    // Whose numbers these are, as the Host stamped them — the request could
    // not have asked for another House even if it wanted to.
    `stamped ${summary.house} · ${summary.room}/${summary.spirit}`,
    `provider requests ${tally(summary.requests.settled)} settled${outcomes ? ` · ${outcomes}` : ""}`,
    `latency ${span(summary.requests.meanUs)} mean · ${span(summary.requests.maxUs)} max`,
    `usage ${tally(summary.usage.measured)} measured · ${tally(summary.usage.unknown)} unknown`,
    `tokens in ${tally(summary.usage.tokensIn)} · out ${tally(summary.usage.tokensOut)}`,
    `tool calls ${tally(summary.toolCalls)} · errors ${tally(summary.errorEvents)} · degraded ${tally(summary.degradedEvents)} · writer drops ${tally(summary.drops)}`,
    summary.latestObservedAt
      ? `latest observation ${stamp(summary.latestObservedAt, 19)}`
      : "no observation in this window",
  ];
  if (summary.truncated) {
    lines.push(`row limit ${tally(MAX_VITALS_ROWS)} reached · these counts are partial`);
  }
  return lines;
}

export function insulaUnavailableLines(range: InsulaVitalsRange, error: unknown): string[] {
  return [
    `window ${range} · Host Vitals unavailable (${insulaErrorClass(error)})`,
    "no counts shown: this is absence, not zero",
  ];
}

type CockpitTheme = { fg?: (color: string, text: string) => string };

function styled(theme: CockpitTheme | undefined, color: string, text: string): string {
  return theme?.fg ? theme.fg(color, text) : text;
}

function panel(
  theme: CockpitTheme | undefined,
  header: string,
  lines: readonly string[],
  availableWidth: number,
): string[] {
  const width = Math.max(MIN_PANEL_WIDTH, Math.min(availableWidth, MAX_PANEL_WIDTH));
  const contentWidth = width - 4;
  const border = (value: string) => styled(theme, "borderMuted", value);
  const title = header.slice(0, contentWidth);
  const topFill = "─".repeat(Math.max(1, width - Bun.stringWidth(title) - 5));
  const rows = [`${border("╭─ ")}${styled(theme, "accent", title)}${border(` ${topFill}╮`)}`];
  for (const line of lines) {
    const text = line.slice(0, contentWidth);
    const padding = " ".repeat(Math.max(0, contentWidth - Bun.stringWidth(text)));
    rows.push(`${border("│ ")}${styled(theme, "toolOutput", text)}${padding}${border(" │")}`);
  }
  const footer = "esc · enter · q to close";
  const bottomFill = "─".repeat(Math.max(1, width - Bun.stringWidth(footer) - 5));
  rows.push(`${border("╰─ ")}${styled(theme, "dim", footer)}${border(` ${bottomFill}╯`)}`);
  return rows;
}

class InsulaCockpitPanel {
  #rendered: { width: number; rows: string[] } | null = null;

  constructor(
    private readonly header: string,
    private readonly lines: readonly string[],
    private readonly theme: CockpitTheme | undefined,
    private readonly close: () => void,
  ) {}

  render(width: number): readonly string[] {
    // The content never changes, so one array per width is the stable
    // reference the render engine treats as proof that nothing moved.
    if (this.#rendered?.width === width) return this.#rendered.rows;
    const rows = panel(this.theme, this.header, this.lines, width);
    this.#rendered = { width, rows };
    return rows;
  }

  handleInput(data: string): void {
    if (data === "\x1b" || data === "\r" || data === "\n" || data === "q" || data === "Q") this.close();
  }
}

type CockpitContext = {
  cwd?: string;
  mode?: string;
  hasUI?: boolean;
  ui?: {
    theme?: CockpitTheme;
    notify?: (message: string, type?: "info" | "warning" | "error") => void;
    custom?: (
      factory: (
        tui: unknown,
        theme: CockpitTheme,
        keybindings: unknown,
        done: () => void,
      ) => unknown,
      options?: { overlay?: boolean },
    ) => Promise<void>;
  };
};

export async function showInsulaCockpit(args: unknown, ctx: CockpitContext): Promise<void> {
  const range = parseInsulaVitalsRange(args);
  if (!range) {
    ctx?.ui?.notify?.(
      `Athanor /insula takes one optional range: ${INSULA_VITALS_RANGES.join(", ")}.`,
      "warning",
    );
    return;
  }
  const { room } = roomContext(ctx?.cwd);
  let lines: string[];
  let level: "info" | "warning" = "info";
  try {
    lines = insulaCockpitLines(summarizeInsulaVitals(range, await readInsulaVitals(room, range)));
  } catch (error) {
    lines = insulaUnavailableLines(range, error);
    level = "warning";
  }
  const header = `Insula · ${room}`;
  if (ctx?.mode === "tui" && ctx?.hasUI && typeof ctx?.ui?.custom === "function") {
    await ctx.ui.custom(
      (_tui, theme, _keybindings, done) => new InsulaCockpitPanel(header, lines, theme, () => done()),
      { overlay: true },
    );
    return;
  }
  ctx?.ui?.notify?.(`${header} · ${lines.join(" · ")}`, level);
}
