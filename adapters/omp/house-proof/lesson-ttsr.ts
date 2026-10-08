import { randomUUID } from "node:crypto";
import { isAbsolute, resolve } from "node:path";
import type { NativeLessonPlan } from "./context.ts";

const BRIDGE_STATE = Symbol.for("solarisael.athanor.lesson-ttsr.v1");
const PROVIDER = "athanor-lessons";

type BlockGuard = { manager: any; rules: Map<string, Record<string, unknown>>; signature: string };
type ManagerRecord = {
  manager: any; session: any; active: Set<string>; known: Set<string>; patched: boolean; guard: BlockGuard | null;
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
    record = { manager, session, active: new Set(), known: new Set(), patched: false, guard: null };
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

/** A `tool_call` refusal when an edit or write would put a block lesson's match on disk. */
export async function blockLessonRefusal(event: any, ctx: any): Promise<{ block: true; reason: string } | undefined> {
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
  try {
    for (const [index, snapshot] of snapshots.entries()) {
      const context = {
        source: "tool",
        toolName: event.toolName,
        // A fresh key per call: the AST throttle skips a snapshot it has already seen under the same key.
        streamKey: `athanor-block:${event?.toolCallId ?? randomUUID()}#${index}`,
        filePaths: snapshot.paths.flatMap((path) => pathCandidates(path, cwd)),
      };
      const matched = [
        ...guard.manager.checkSnapshot(snapshot.digest, context),
        ...(await guard.manager.checkAstSnapshot(snapshot.digest, context)),
      ];
      for (const rule of matched) hits.set(String(rule.name), guard.rules.get(String(rule.name)) ?? rule);
    }
  } finally {
    guard.manager.resetBuffer?.();
  }
  if (hits.size === 0) return undefined;

  const reasons = [...hits.values()].map((rule) => `${rule.path}\n${rule.content}`);
  return { block: true, reason: `Refused by The Athanor block lesson:\n\n${reasons.join("\n\n")}` };
}
