import { randomUUID } from "node:crypto";
import { isAbsolute, resolve } from "node:path";
import type { NativeLessonPlan } from "./context.ts";
import type { HostBinding } from "./host.ts";
import { requestOrgan } from "./organ.ts";

const BRIDGE_STATE = Symbol.for("solarisael.athanor.lesson-ttsr.v1");
const PROVIDER = "athanor-lessons";

type BlockGuard = { manager: any; rules: Map<string, Record<string, unknown>>; signature: string };
// Where OMP's matcher last saw a rule: `ttsr_triggered` names the rule, never the surface.
type LastMatch = { patternKind: "regex" | "ast"; surface: "tool" | "prose"; tool?: string; path?: string };
type ManagerRecord = {
  manager: any; session: any; active: Set<string>; known: Set<string>; patched: boolean; guard: BlockGuard | null;
  lastMatch: Map<string, LastMatch>;
};
type BridgeState = { sessions: Map<string, ManagerRecord>; patchedPrototype?: object };

function state(): BridgeState {
  const root = globalThis as typeof globalThis & { [BRIDGE_STATE]?: BridgeState };
  return root[BRIDGE_STATE] ??= { sessions: new Map() };
}

function filterAthanor(record: ManagerRecord, rules: unknown): unknown {
  if (!Array.isArray(rules)) return rules;
  return rules.filter((rule) => rule?._source?.provider !== PROVIDER || record.active.has(String(rule?.name ?? "")));
}

function matchContext(method: string, context: any): LastMatch {
  const tool = context?.source === "tool";
  const path = Array.isArray(context?.filePaths) ? context.filePaths.find((item: unknown) => typeof item === "string") : undefined;
  return {
    patternKind: method === "checkAstSnapshot" ? "ast" : "regex",
    surface: tool ? "tool" : "prose",
    tool: tool && typeof context?.toolName === "string" ? context.toolName : undefined,
    path: tool ? path : undefined,
  };
}

function rememberMatches(record: ManagerRecord, method: string, context: unknown, rules: unknown): unknown {
  const filtered = filterAthanor(record, rules);
  if (!Array.isArray(filtered)) return filtered;
  for (const rule of filtered) {
    if (rule?._source?.provider === PROVIDER) record.lastMatch.set(String(rule.name), matchContext(method, context));
  }
  return filtered;
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
          ? result.then((rules: unknown) => rememberMatches(record, method, args[1], rules))
          : rememberMatches(record, method, args[1], result);
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
    record = {
      manager, session, active: new Set(), known: new Set(), patched: false, guard: null,
      lastMatch: new Map(),
    };
    bridge.sessions.set(sessionId, record);
  }
  record.session = session;
  patchManager(record);
}

/** The live OMP AgentSession this bridge captured for a session id, if any. */
export function capturedAgentSession(sessionId: string): any {
  return state().sessions.get(sessionId.trim())?.session ?? null;
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

export function contextLessonManagerAvailable(ctx: any): boolean {
  ctx.getContextUsage?.();
  const sessionId = String(ctx.sessionManager?.getSessionId?.() ?? ctx.sessionID ?? "").trim();
  return state().sessions.has(sessionId);
}

export function syncLessonTtsr(args: { ctx: any; plan: NativeLessonPlan }) {
  args.ctx.getContextUsage?.();
  const sessionId = String(args.ctx.sessionManager?.getSessionId?.() ?? args.ctx.sessionID ?? "").trim();
  const record = state().sessions.get(sessionId);
  const warnings = [...args.plan.warnings];
  if (!record) return { active: 0, added: 0, warnings };

  const rules = args.plan.rules.map((entry) => entry.rule);
  const next = new Set(rules.map((rule) => String(rule.name)));
  let added = 0;
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
  const guardWarning = armBlockGuard(record, args.plan.rules.filter((entry) => entry.block).map((entry) => entry.rule));
  if (guardWarning) warnings.push(guardWarning);
  return { active: next.size, added, warnings };
}

export type LessonFireBinding = HostBinding;

// A native rule names its lesson in `path` (athanor://lessons/<family>/<id>) and
// carries its own patterns, so the ledger row needs nothing the plan did not send.
function lessonIdentity(rule: Record<string, any>): { family: string; id: number } | null {
  const match = /^athanor:\/\/lessons\/([a-z]+)\/(\d+)$/.exec(String(rule?.path ?? ""));
  return match ? { family: match[1], id: Number(match[2]) } : null;
}

function patternFor(rule: Record<string, any>, kind: "regex" | "ast"): string | undefined {
  const patterns = rule?.[kind === "ast" ? "astCondition" : "condition"];
  return Array.isArray(patterns) && patterns.length === 1 ? String(patterns[0]) : undefined;
}

async function recordFire(binding: LessonFireBinding, rule: Record<string, any>, match: LastMatch, urgency: "block" | "remind"): Promise<boolean> {
  const lesson = lessonIdentity(rule);
  if (!lesson) {
    console.warn(`[athanor] native fire ${rule?.name} has no lesson path; not recorded`);
    return false;
  }
  // The Host binds `room` and `session` from the sender and refuses callers that send them.
  const params = {
    family: lesson.family, id: lesson.id,
    surface: match.surface, tool: match.tool, path: match.path,
    patternKind: match.patternKind, matchedPattern: patternFor(rule, match.patternKind), urgency,
  };
  try {
    await requestOrgan(binding, "lesson_trigger_record", params, { write: true, timeoutMs: 10_000 });
    return true;
  } catch (error) {
    // The fire already happened; a lost ledger row must stay visible, never fatal.
    const reason = error instanceof Error ? error.message : String(error);
    console.warn(`[athanor] lesson ${lesson.family}#${lesson.id} fired but the ledger refused it: ${reason}`);
    return false;
  }
}

/** Ledger rows for the Athanor rules OMP just announced through `ttsr_triggered`. */
export async function recordNativeFires(binding: LessonFireBinding, rules: Array<Record<string, any>>): Promise<number> {
  const record = state().sessions.get(binding.session);
  if (!record) return 0;
  let recorded = 0;
  for (const rule of rules) {
    if (rule?._source?.provider !== PROVIDER) continue;
    const match = record.lastMatch.get(String(rule.name));
    if (!match) {
      console.warn(`[athanor] native fire ${rule.name} has no match context; not recorded`);
      continue;
    }
    const urgency = rule.interruptMode === "never" ? "remind" : "block";
    if (await recordFire(binding, rule, match, urgency)) recorded += 1;
  }
  return recorded;
}

// The native manager interrupts a stream once per session (repeatMode "once"),
// so the model's identical retry used to land on disk. A block lesson refuses at
// tool execution instead, through a second OMP manager that holds only block
// rules. Only OMP's coordinator marks rules injected, so this one never goes
// quiet. OMP's matcher stays the only matcher.
const GUARD_SETTINGS = { enabled: true };

function armBlockGuard(record: ManagerRecord, rules: Array<Record<string, unknown>>): string | null {
  const signature = rules.map((rule) => String(rule.name)).sort().join("\n");
  if (record.guard?.signature === signature) return null;
  record.guard = null;
  if (rules.length === 0) return null;

  const Manager = record.manager?.constructor;
  const manager = typeof Manager === "function" ? new Manager(GUARD_SETTINGS) : null;
  if (typeof manager?.replaceRules !== "function" || typeof manager?.checkAstSnapshot !== "function") {
    return "block lessons unguarded at execution: OMP TTSR manager has no replaceRules/checkAstSnapshot";
  }
  const accepted: Set<string> = manager.replaceRules(rules);
  const kept = rules.filter((rule) => accepted.has(String(rule.name)));
  record.guard = { manager, rules: new Map(kept.map((rule) => [String(rule.name), rule])), signature };
  const rejected = rules.length - kept.length;
  return rejected > 0 ? `block guard rejected ${rejected} rule${rejected === 1 ? "" : "s"}` : null;
}

type GuardSnapshot = { digest: string; paths: string[] };

// # enough: edit digests carry the patch's inserted lines, as native TTSR sees them, so an
// edit that only empties an existing catch body escapes; the way up is an OMP post-edit snapshot.
function guardSnapshots(tool: any, input: unknown): GuardSnapshot[] {
  const entries = tool?.matcherEntries?.(input);
  if (Array.isArray(entries) && entries.length > 0) {
    return entries.map((entry: { path: string; digest: string }) => ({ digest: entry.digest, paths: [entry.path] }));
  }
  const digest = tool?.matcherDigest?.(input);
  if (typeof digest !== "string") return [];
  const paths = tool?.matcherPaths?.(input);
  return [{ digest, paths: Array.isArray(paths) && paths.length > 0 ? [...paths] : pathsFromArgs(input) }];
}

// OMP's write tool has no matcherPaths; the inspector then reads `path`-named
// arguments, and the AST language comes from their extension.
function pathsFromArgs(input: unknown): string[] {
  if (!input || typeof input !== "object") return [];
  return Object.entries(input as Record<string, unknown>).flatMap(([key, value]) => {
    const name = key.toLowerCase();
    if (typeof value === "string" && name.endsWith("path")) return [value];
    if (Array.isArray(value) && name.endsWith("paths")) return value.filter((item) => typeof item === "string");
    return [];
  });
}

// Mirrors OMP's TtsrToolInspector candidates closely enough for extension and
// basename globs; the inspector itself is private to the session coordinator.
function pathCandidates(rawPath: string, cwd: string): string[] {
  const raw = rawPath.trim().replaceAll("\\", "/");
  if (!raw) return [];
  const absolute = (isAbsolute(rawPath) ? rawPath : resolve(cwd, rawPath)).replaceAll("\\", "/");
  return [...new Set([raw, absolute])];
}

/**
 * A `tool_call` refusal when an edit or write would put a block lesson's match on disk.
 * With a binding, every refusal also lands in the ledger before the refusal returns.
 */
export async function blockLessonRefusal(
  event: any,
  ctx: any,
  binding?: LessonFireBinding,
): Promise<{ block: true; reason: string } | undefined> {
  const sessionId = String(ctx?.sessionManager?.getSessionId?.() ?? "").trim();
  const record = state().sessions.get(sessionId);
  const guard = record?.guard;
  if (!guard) return undefined;

  const tools: any[] = record.session?.agent?.state?.tools ?? [];
  const tool = tools.find((candidate) => candidate?.name === event?.toolName);
  const snapshots = guardSnapshots(tool, event?.input ?? {});
  if (snapshots.length === 0) return undefined;

  const cwd = String(ctx?.cwd ?? process.cwd());
  const hits = new Map<string, Record<string, unknown>>();
  const matches = new Map<string, LastMatch>();
  try {
    for (const [index, snapshot] of snapshots.entries()) {
      const context = {
        source: "tool",
        toolName: event.toolName,
        // A fresh key per call: the AST throttle skips a snapshot it has already seen under the same key.
        streamKey: `athanor-block:${event?.toolCallId ?? randomUUID()}#${index}`,
        filePaths: snapshot.paths.flatMap((path) => pathCandidates(path, cwd)),
      };
      const regexHits: any[] = guard.manager.checkSnapshot(snapshot.digest, context);
      const astHits: any[] = await guard.manager.checkAstSnapshot(snapshot.digest, context);
      const kinds: Array<[any[], "regex" | "ast"]> = [[regexHits, "regex"], [astHits, "ast"]];
      for (const [matched, patternKind] of kinds) {
        for (const rule of matched) {
          const name = String(rule.name);
          hits.set(name, guard.rules.get(name) ?? rule);
          if (!matches.has(name)) matches.set(name, { patternKind, surface: "tool", tool: event.toolName, path: snapshot.paths[0] });
        }
      }
    }
  } finally {
    guard.manager.resetBuffer?.();
  }
  if (hits.size === 0) return undefined;

  if (binding) {
    for (const [name, match] of matches) {
      const rule = hits.get(name);
      if (rule) await recordFire(binding, rule, match, "block");
    }
  }

  const reasons = [...hits.values()].map((rule) => `${rule.path}\n${rule.content}`);
  return { block: true, reason: `Refused by The Athanor block lesson:\n\n${reasons.join("\n\n")}` };
}
