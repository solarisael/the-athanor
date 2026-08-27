import { createHash } from "node:crypto";
import { runLessonQuery } from "./lesson-context.ts";

const BRIDGE_STATE = Symbol.for("solarisael.athanor.lesson-ttsr.v1");
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

type ManagerRecord = { manager: any; active: Set<string>; known: Set<string>; patched: boolean };
type BridgeState = { sessions: Map<string, ManagerRecord>; patchedPrototype?: object };

function state(): BridgeState {
  const root = globalThis as typeof globalThis & { [BRIDGE_STATE]?: BridgeState };
  return root[BRIDGE_STATE] ??= { sessions: new Map() };
}

function filterAthanor(record: ManagerRecord, rules: unknown): unknown {
  if (!Array.isArray(rules)) return rules;
  return rules.filter((rule) => rule?._source?.provider !== PROVIDER || record.active.has(String(rule?.name ?? "")));
}

function patchManager(record: ManagerRecord): void {
  if (record.patched) return;
  for (const method of ["checkDelta", "checkSnapshot", "checkAstSnapshot"]) {
    const original = record.manager?.[method];
    if (typeof original !== "function") throw new Error(`OMP TTSR manager has no ${method} method`);
    Object.defineProperty(record.manager, method, {
      configurable: true,
      value: (...args: unknown[]) => {
        const result = original.apply(record.manager, args);
        return result && typeof result.then === "function"
          ? result.then((rules: unknown) => filterAthanor(record, rules))
          : filterAthanor(record, result);
      },
    });
  }
  record.patched = true;
}

function captureSession(session: any): void {
  const sessionId = String(session?.sessionManager?.getSessionId?.() ?? "").trim();
  const manager = session?.ttsrManager;
  if (!sessionId || !manager || typeof manager.addRule !== "function") return;
  const bridge = state();
  let record = bridge.sessions.get(sessionId);
  if (!record || record.manager !== manager) {
    record = { manager, active: new Set(), known: new Set(), patched: false };
    bridge.sessions.set(sessionId, record);
  }
  patchManager(record);
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
  const record = state().sessions.get(sessionId);
  if (!record) return { active: 0, added: 0, warnings: ["native OMP TTSR manager unavailable"] };

  const queries = FAMILIES.map((family) => runLessonQuery(args.roomDir, args.room, { type: family, limit: 50 }));
  if (args.activeProject) queries.push(runLessonQuery(args.roomDir, args.room, { type: "project", project: args.activeProject, limit: 50 }));
  const results = await Promise.all(queries);
  const rows = results.flatMap(rowsFrom);
  const rules = rows.map((row) => nativeRule(row, args.activeProject)).filter((rule): rule is Record<string, unknown> => rule !== null);
  const next = new Set(rules.map((rule) => String(rule.name)));
  let added = 0;
  const warnings: string[] = [];
  for (const rule of rules) {
    const name = String(rule.name);
    if (record.known.has(name)) continue;
    if (record.manager.addRule(rule)) {
      record.known.add(name);
      added += 1;
    } else {
      warnings.push(`native OMP rejected ${name}`);
    }
  }
  record.active = next;
  return { active: next.size, added, warnings };
}
