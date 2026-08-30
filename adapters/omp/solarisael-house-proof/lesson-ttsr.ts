import { createHash } from "node:crypto";
import { runLessonQuery } from "./lesson-context.ts";

const BRIDGE_STATE = Symbol.for("solarisael.athanor.lesson-ttsr.v1");
const STREAMING_TEXT_STATE = Symbol.for("solarisael.athanor.streaming-text-scrollback.v1");
const PROVIDER = "athanor-lessons";
const FAMILIES = ["coding", "writing", "design", "audio"] as const;

const LANGUAGE_EXTENSIONS: Record<string, string[]> = {
  rust: ["rs"], typescript: ["ts", "tsx", "mts", "cts"], javascript: ["js", "jsx", "mjs", "cjs"],
  python: ["py"], powershell: ["ps1", "psm1", "psd1"], shell: ["sh", "bash"], sql: ["sql"],
  css: ["css", "scss", "sass", "less"], html: ["html", "htm"], markdown: ["md", "mdx"],
  go: ["go"], ruby: ["rb"], java: ["java"], c: ["c", "h"], cpp: ["cc", "cpp", "hpp"],
  csharp: ["cs"], lua: ["lua"], zig: ["zig"], gdscript: ["gd"], glsl: ["glsl", "vert", "frag"],
};

type LessonRow = {
  id: number; type: string; title: string; lesson: string; proofPattern?: string | null; project?: string | null;
  languageKeys?: string[]; tags?: string[]; condition?: string[]; astCondition?: string[]; triggerScope?: string[];
  interruptMode?: "block" | "remind" | null; repeatCooldownSecs?: number | null;
};

type ManagerRecord = { manager: any; known: Set<string> };
type BridgeState = {
  sessions: Map<string, ManagerRecord[]>;
  desired: Map<string, Record<string, unknown>[]>;
  patchedPrototype?: object;
};
type StreamingTextSnapshot = { key: string; text: string; gapRows: number };
type StreamingTextState = {
  activeText: string | null;
  markdown: any | null;
  nativeKeys: string[] | null;
  snapshots: StreamingTextSnapshot[];
  stableRows: Array<{ key: string }>;
  colorTransform?: (text: string) => string;
};

function state(): BridgeState {
  const root = globalThis as typeof globalThis & { [BRIDGE_STATE]?: BridgeState };
  return root[BRIDGE_STATE] ??= { sessions: new Map(), desired: new Map() };
}


function captureSession(session: any): void {
  const sessionId = String(session?.sessionManager?.getSessionId?.() ?? "").trim();
  const manager = session?.ttsrManager;
  if (!sessionId || !manager || typeof manager.addRule !== "function") return;
  const bridge = state();
  const records = bridge.sessions.get(sessionId) ?? [];
  let record = records.find((candidate) => candidate.manager === manager);
  if (!record) {
    record = { manager, known: new Set() };
    records.push(record);
    bridge.sessions.set(sessionId, records);
  }
  const desired = bridge.desired.get(sessionId) ?? [];
  for (const rule of desired) {
    const name = String(rule.name);
    if (!record.known.has(name) && record.manager.addRule(rule)) record.known.add(name);
  }
}

export function installLessonTtsrBridge(pi: any): string | null {
  const AgentSession = pi?.pi?.AgentSession;
  const prototype = AgentSession?.prototype;
  if (!prototype || typeof prototype.getContextUsage !== "function") return "OMP does not expose AgentSession.getContextUsage";
  const bridge = state();
  if (bridge.patchedPrototype === prototype) return null;
  const original = prototype.getContextUsage;
  Object.defineProperty(prototype, "getContextUsage", {
    configurable: true,
    value: function (...args: unknown[]) {
      captureSession(this);
      return original.apply(this, args);
    },
  });
  bridge.patchedPrototype = prototype;
  return null;
}

function trimBlankRows(rows: readonly string[]): readonly string[] {
  let start = 0;
  let end = rows.length;
  while (start < end && !/\S/.test(rows[start] ?? "")) start += 1;
  while (end > start && !/\S/.test(rows[end - 1] ?? "")) end -= 1;
  return rows.slice(start, end);
}

function isRenderedPrefix(prefix: readonly string[], rows: readonly string[]): boolean {
  return prefix.length <= rows.length && prefix.every((row, index) => row === rows[index]);
}

function singleStreamingText(message: any, transient: boolean): string | null {
  if (!transient || !Array.isArray(message?.content)) return null;
  const textIndex = message.content.findIndex((block: any) => block?.type === "text");
  if (textIndex < 0 || textIndex !== message.content.length - 1) return null;
  if (!message.content.slice(0, textIndex).every((block: any) =>
    block?.type === "thinking" || block?.type === "redactedThinking"
  )) return null;
  const text = String(message.content[textIndex]?.text ?? "").trim();
  return text.length > 0 ? text : null;
}

function stableKeys(rows: readonly any[]): string[] {
  return rows.map((row) => String(row?.key ?? ""));
}

function sameKeys(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((key, index) => key === right[index]);
}

export function installStreamingTranscriptBridge(pi: any): string | null {
  const AssistantMessageComponent = pi?.pi?.AssistantMessageComponent;
  const Markdown = pi?.pi?.Markdown;
  const getMarkdownTheme = pi?.pi?.getMarkdownTheme;
  const prototype = AssistantMessageComponent?.prototype;
  if (!prototype || typeof Markdown !== "function" || typeof getMarkdownTheme !== "function") {
    return "OMP does not expose the streaming transcript bridge";
  }
  if (prototype[STREAMING_TEXT_STATE]) return null;

  const originalUpdateContent = prototype.updateContent;
  const originalRender = prototype.render;
  const originalGetStableRows = prototype.getTranscriptStableRows;
  const originalRenderStableRows = prototype.renderTranscriptStableRows;
  const originalSetTextColorTransform = prototype.setTextColorTransform;
  if (
    typeof originalUpdateContent !== "function"
    || typeof originalRender !== "function"
    || typeof originalGetStableRows !== "function"
    || typeof originalRenderStableRows !== "function"
  ) {
    return "OMP streaming transcript API is incomplete";
  }

  const states = new WeakMap<object, StreamingTextState>();
  const stateFor = (component: object): StreamingTextState => {
    let record = states.get(component);
    if (!record) {
      record = { activeText: null, markdown: null, nativeKeys: null, snapshots: [], stableRows: [] };
      states.set(component, record);
    }
    return record;
  };
  const renderTextSnapshot = (
    record: StreamingTextState,
    snapshot: StreamingTextSnapshot,
    width: number,
  ): readonly string[] => {
    const options = record.colorTransform ? { color: record.colorTransform } : undefined;
    return new Markdown(snapshot.text, 1, 0, getMarkdownTheme(), options, 0).render(width);
  };
  const renderCombinedSnapshot = (
    component: object,
    record: StreamingTextState,
    snapshot: StreamingTextSnapshot,
    width: number,
  ): readonly string[] => {
    const nativeCount = record.nativeKeys?.length ?? 0;
    const nativeRender = nativeCount > 0
      ? originalRenderStableRows.call(component, nativeCount, width)
      : [];
    return [
      ...nativeRender,
      ...new Array(snapshot.gapRows).fill(""),
      ...renderTextSnapshot(record, snapshot, width),
    ];
  };

  Object.defineProperty(prototype, "updateContent", {
    configurable: true,
    value: function (message: any, opts?: { transient?: boolean }) {
      const result = originalUpdateContent.apply(this, arguments);
      const record = stateFor(this);
      const text = singleStreamingText(message, opts?.transient === true);
      if (text === null) {
        record.activeText = null;
        record.markdown = null;
        return result;
      }
      if (record.activeText !== null && !text.startsWith(record.activeText)) {
        if (record.snapshots.length > 0) {
          record.activeText = null;
          record.markdown = null;
          return result;
        }
        record.markdown = null;
        record.nativeKeys = null;
      }
      const options = record.colorTransform ? { color: record.colorTransform } : undefined;
      if (!record.markdown) {
        record.markdown = new Markdown(text, 1, 0, getMarkdownTheme(), options, 0);
      } else {
        record.markdown.setText(text);
      }
      record.markdown.transientRenderCache = true;
      record.activeText = text;
      return result;
    },
  });

  Object.defineProperty(prototype, "render", {
    configurable: true,
    value: function (width: number) {
      const rendered = originalRender.apply(this, arguments);
      const record = states.get(this);
      if (!record?.markdown || record.activeText === null) return rendered;

      const nativeRows = originalGetStableRows.call(this);
      const currentNativeKeys = stableKeys(nativeRows);
      if (record.nativeKeys === null) {
        record.nativeKeys = currentNativeKeys;
      } else if (!sameKeys(record.nativeKeys, currentNativeKeys)) {
        if (record.snapshots.length > 0) {
          record.activeText = null;
          record.markdown = null;
          return rendered;
        }
        record.nativeKeys = currentNativeKeys;
      }

      record.markdown.render(width);
      const stableText = String(record.markdown.getLastRenderStableText?.() ?? "");
      if (stableText.length === 0 || !/\S/.test(record.activeText.slice(stableText.length))) return rendered;
      const previous = record.snapshots.at(-1);
      if (previous?.text === stableText || (previous && !stableText.startsWith(previous.text))) return rendered;

      const textRender = renderTextSnapshot(record, { key: "", text: stableText, gapRows: 0 }, width);
      const liveRender = trimBlankRows(rendered);
      const nativeRender = currentNativeKeys.length > 0
        ? originalRenderStableRows.call(this, currentNativeKeys.length, width)
        : [];
      if (!isRenderedPrefix(nativeRender, liveRender)) return rendered;
      let textStart = -1;
      for (let start = nativeRender.length; start + textRender.length <= liveRender.length; start += 1) {
        if (!liveRender.slice(nativeRender.length, start).every((row) => !/\S/.test(row))) break;
        if (isRenderedPrefix(textRender, liveRender.slice(start))) {
          textStart = start;
          break;
        }
      }
      if (textStart < 0) return rendered;

      const snapshot = {
        key: createHash("sha256").update(stableText).digest("hex"),
        text: stableText,
        gapRows: textStart - nativeRender.length,
      };
      const stableRender = renderCombinedSnapshot(this, record, snapshot, width);
      if (!isRenderedPrefix(stableRender, liveRender)) return rendered;
      const previousRender = previous ? renderCombinedSnapshot(this, record, previous, width) : nativeRender;
      if (!isRenderedPrefix(previousRender, stableRender) || previousRender.length === stableRender.length) return rendered;
      record.snapshots.push(snapshot);
      record.stableRows.push({ key: snapshot.key });
      return rendered;
    },
  });

  Object.defineProperty(prototype, "getTranscriptStableRows", {
    configurable: true,
    value: function () {
      const nativeRows = originalGetStableRows.apply(this, arguments);
      const record = states.get(this);
      if (!record?.stableRows.length || !sameKeys(record.nativeKeys ?? [], stableKeys(nativeRows))) return nativeRows;
      return [...nativeRows, ...record.stableRows];
    },
  });

  Object.defineProperty(prototype, "renderTranscriptStableRows", {
    configurable: true,
    value: function (count: number, width: number) {
      const nativeRows = originalGetStableRows.call(this);
      const record = states.get(this);
      if (!record?.snapshots.length || !sameKeys(record.nativeKeys ?? [], stableKeys(nativeRows))) {
        return originalRenderStableRows.apply(this, arguments);
      }
      const nativeCount = nativeRows.length;
      if (count <= nativeCount) return originalRenderStableRows.call(this, count, width);
      const index = Math.min(Math.trunc(count) - nativeCount, record.snapshots.length);
      return index <= 0 ? [] : renderCombinedSnapshot(this, record, record.snapshots[index - 1]!, width);
    },
  });

  if (typeof originalSetTextColorTransform === "function") {
    Object.defineProperty(prototype, "setTextColorTransform", {
      configurable: true,
      value: function (transform?: (text: string) => string) {
        const record = stateFor(this);
        if (record.snapshots.length > 0 && record.colorTransform !== transform) {
          record.activeText = null;
          record.markdown = null;
        }
        record.colorTransform = transform;
        return originalSetTextColorTransform.apply(this, arguments);
      },
    });
  }

  Object.defineProperty(prototype, STREAMING_TEXT_STATE, { value: true });
  return null;
}

function normalizeProject(value: unknown): string {
  return String(value ?? "").trim().toLowerCase().replace(/\s+/g, "-");
}

function globsFor(row: LessonRow): string[] | undefined {
  const extensions = [...new Set((row.languageKeys ?? []).flatMap((key) => LANGUAGE_EXTENSIONS[String(key).toLowerCase()] ?? []))];
  const project = normalizeProject(row.project);
  if (project && extensions.length) return extensions.map((ext) => `**/${project}/**/*.${ext}`);
  if (project) return [`**/${project}/**`];
  if (extensions.length) return extensions.map((ext) => `**/*.${ext}`);
  return undefined;
}

function nativeRule(row: LessonRow, activeProject: string | null): Record<string, unknown> | null {
  const condition = (row.condition ?? []).filter(Boolean);
  const astCondition = (row.astCondition ?? []).filter(Boolean);
  if (!condition.length && !astCondition.length) return null;
  const languageBound = (row.languageKeys ?? []).length > 0;
  const projectBound = row.type === "project" && Boolean(row.project);
  let scope = (row.triggerScope ?? []).filter(Boolean);
  if (languageBound) scope = scope.filter((token) => token !== "text");
  if (projectBound && normalizeProject(row.project) !== normalizeProject(activeProject)) scope = scope.filter((token) => token !== "text");
  if ((languageBound || projectBound) && scope.length === 0) scope = ["tool"];
  if (scope.length > 0 && !scope.some((token) => token === "text" || token === "tool" || token.startsWith("tool:"))) return null;

  const identity = JSON.stringify({ condition, astCondition, scope, project: row.project, languageKeys: row.languageKeys, mode: row.interruptMode, body: row.lesson });
  const digest = createHash("sha256").update(identity).digest("hex").slice(0, 12);
  const name = `athanor-${row.type}-${row.id}-${digest}`;
  return {
    name,
    path: `athanor://lessons/${row.type}/${row.id}`,
    content: [row.title, row.lesson, row.proofPattern ? `Proof pattern: ${row.proofPattern}` : ""].filter(Boolean).join("\n\n"),
    description: row.title,
    condition: condition.length ? condition : undefined,
    astCondition: astCondition.length ? astCondition : undefined,
    scope: scope.length ? scope : undefined,
    globs: globsFor(row),
    interruptMode: row.interruptMode === "remind" ? "never" : "always",
    _source: { provider: PROVIDER, providerName: "The Athanor", path: `athanor://lessons/${row.type}/${row.id}`, level: "native" },
  };
}

function rowsFrom(result: unknown): LessonRow[] {
  if (!result || typeof result !== "object" || (result as any).ok !== true || !Array.isArray((result as any).lessons)) return [];
  return (result as any).lessons.filter((row: LessonRow) =>
    row.tags?.includes("ttsr-approved") && ((row.condition?.length ?? 0) > 0 || (row.astCondition?.length ?? 0) > 0));
}

export async function syncLessonTtsr(args: {
  ctx: any; roomDir: string; room: string; activeProject: string | null;
}): Promise<{ active: number; added: number; warnings: string[] }> {
  args.ctx.getContextUsage?.();
  const sessionId = String(args.ctx.sessionManager?.getSessionId?.() ?? args.ctx.sessionID ?? "").trim();
  const bridge = state();
  const records = bridge.sessions.get(sessionId) ?? [];
  if (records.length === 0) {
    return { active: 0, added: 0, warnings: ["native OMP TTSR manager unavailable"] };
  }

  const queries = FAMILIES.map((family) =>
    runLessonQuery(args.roomDir, args.room, { type: family, limit: 50 }));
  if (args.activeProject) {
    queries.push(runLessonQuery(args.roomDir, args.room, {
      type: "project",
      project: args.activeProject,
      limit: 50,
    }));
  }
  const results = await Promise.all(queries);
  const queryFailures = results.filter((result: any) => result?.ok !== true);
  if (queryFailures.length > 0) {
    const warnings = queryFailures.map((result: any) =>
      `native lesson query failed: ${String(result?.error ?? "unknown refusal")}`);
    return { active: bridge.desired.get(sessionId)?.length ?? 0, added: 0, warnings };
  }

  const rows = results.flatMap(rowsFrom);
  const queriedRules = rows.map((row) => nativeRule(row, args.activeProject))
    .filter((rule): rule is Record<string, unknown> => rule !== null);
  const desired = bridge.desired.get(sessionId);
  const changed = desired !== undefined
    && JSON.stringify(desired.map((rule) => rule.name)) !== JSON.stringify(queriedRules.map((rule) => rule.name));
  const rules = desired ?? queriedRules;
  if (!desired) bridge.desired.set(sessionId, rules);
  const next = new Set(rules.map((rule) => String(rule.name)));
  let added = 0;
  const warnings: string[] = changed
    ? ["native lesson rules changed; restart OMP to apply the new set"]
    : [];
  for (const rule of rules) {
    const name = String(rule.name);
    let ruleAdded = false;
    for (const record of records) {
      if (record.known.has(name)) continue;
      if (record.manager.addRule(rule)) {
        record.known.add(name);
        ruleAdded = true;
      } else {
        warnings.push(`native OMP rejected ${name}`);
      }
    }
    if (ruleAdded) added += 1;
  }
  return { active: next.size, added, warnings };
}
