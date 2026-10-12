// OMP extension loaded only by compat/omp-compat.ts. It drives the DEV lesson
// bridge against the live AgentSession OMP hands every extension through
// `pi.pi`, then writes one receipt. It never calls a provider or the Host.
import { renameSync, writeFileSync } from "node:fs";
import type { NativeLessonPlan } from "../house-proof/context.ts";
import { capturedAgentSession, contextLessonManagerAvailable, installLessonTtsrBridge, syncLessonTtsr } from "../house-proof/lesson-ttsr.ts";

const MARKER = "OMP_AUDIT_MARKER";
const NATIVE = "omp-compat-native";
const ACTIVE = "omp-compat-active";
const INACTIVE = "omp-compat-inactive";
const FRESH = "omp-compat-fresh";

// Every check is named up front; one not reached stays failed, never absent.
const CHECKS = [
  "api_surface", "bridge_install", "baseline_capture", "rule_setup",
  "raw_control_delta", "raw_control_snapshot", "raw_control_ast",
  "delta_filter", "snapshot_filter", "ast_filter",
  "refresh_rebuild", "refresh_resync_delta", "refresh_resync_snapshot", "refresh_resync_ast",
  "new_session_id", "new_session_sync", "new_session_delta", "new_session_snapshot", "new_session_ast",
] as const;
type CheckName = typeof CHECKS[number];
type Check = { name: CheckName; ok: boolean; detail: string };

function rule(name: string, provider: string): Record<string, unknown> {
  return {
    name,
    path: `audit://${name}`,
    content: `OMP compatibility probe rule ${name}.`,
    description: name,
    condition: [MARKER],
    astCondition: ["console.log($A)"],
    scope: ["text", "tool:write"],
    _source: { provider, providerName: provider, path: `audit://${name}`, level: "native" },
  };
}

function plan(names: string[]): NativeLessonPlan {
  return {
    token: "omp-compat", requiresCredential: false, turnId: "omp-compat", warnings: [],
    rules: names.map((name) => ({ rule: rule(name, "athanor-lessons"), block: false })),
  };
}

let stream = 0;
const textContext = () => ({ source: "text", streamKey: `omp-compat-${++stream}` });
const toolContext = () => ({ source: "tool", toolName: "write", filePaths: ["probe.ts"], streamKey: `omp-compat-${++stream}` });
const TEXT = `before ${MARKER} after`;
const SOURCE = `console.log("${MARKER}");\n`;

type Surface = "delta" | "snapshot" | "ast";

async function fired(manager: any, surface: Surface, raw = false): Promise<string[]> {
  const target = raw ? manager.constructor.prototype : manager;
  let rules: unknown;
  if (surface === "delta") rules = target.checkDelta.call(manager, TEXT, textContext(), { final: true });
  else if (surface === "snapshot") rules = target.checkSnapshot.call(manager, TEXT, toolContext(), { final: true });
  else rules = await target.checkAstSnapshot.call(manager, SOURCE, toolContext());
  if (!Array.isArray(rules)) throw new Error(`${surface} returned ${typeof rules}, not a rule list`);
  return rules.map((entry: any) => String(entry?.name)).sort();
}

function expectNames(names: string[], include: string[], exclude: string[]): string {
  const missing = include.filter((name) => !names.includes(name));
  const leaked = exclude.filter((name) => names.includes(name));
  const seen = `fired [${names.join(", ")}]`;
  if (missing.length || leaked.length) {
    throw new Error(`${seen}; missing [${missing.join(", ")}]; must not fire [${leaked.join(", ")}]`);
  }
  return seen;
}

async function probe(pi: any, ctx: any, installWarning: string | null): Promise<{ version: string | null; checks: Check[] }> {
  const results = new Map<CheckName, Check>(CHECKS.map((name) => [name, { name, ok: false, detail: "not reached: an earlier check failed" }]));
  const run = async (name: CheckName, body: () => string | Promise<string>) => {
    try {
      results.set(name, { name, ok: true, detail: await body() });
      return true;
    } catch (error) {
      results.set(name, { name, ok: false, detail: error instanceof Error ? error.message : String(error) });
      return false;
    }
  };
  const version = typeof pi?.pi?.VERSION === "string" ? pi.pi.VERSION : null;
  const finish = () => ({ version, checks: CHECKS.map((name) => results.get(name)!) });

  const AgentSession = pi?.pi?.AgentSession;
  if (!await run("api_surface", () => {
    if (!version) throw new Error("pi.pi.VERSION is unavailable");
    if (typeof AgentSession !== "function") throw new Error("pi.pi.AgentSession is unavailable");
    for (const method of ["getContextUsage", "refreshBaseSystemPrompt", "newSession"]) {
      if (typeof AgentSession.prototype[method] !== "function") throw new Error(`AgentSession.${method} is unavailable`);
    }
    if (typeof ctx?.sessionManager?.getSessionId !== "function") throw new Error("ctx.sessionManager.getSessionId is unavailable");
    return `OMP ${version}`;
  })) return finish();

  if (!await run("bridge_install", () => {
    if (installWarning) throw new Error(installWarning);
    return "installLessonTtsrBridge accepted pi.pi.AgentSession";
  })) return finish();

  let session: any;
  let manager: any;
  if (!await run("baseline_capture", () => {
    if (!contextLessonManagerAvailable(ctx)) throw new Error("the bridge did not capture this session through getContextUsage");
    const id = String(ctx.sessionManager.getSessionId());
    session = capturedAgentSession(id);
    if (!(session instanceof AgentSession)) throw new Error("the captured session is not an instance of pi.pi.AgentSession");
    manager = session.ttsrManager;
    for (const method of ["addRule", "getRules", "checkDelta", "checkSnapshot", "checkAstSnapshot"]) {
      if (typeof manager?.[method] !== "function") throw new Error(`session.ttsrManager.${method} is unavailable`);
    }
    return `captured live AgentSession ${id}`;
  })) return finish();

  if (!await run("rule_setup", () => {
    if (!manager.addRule(rule(NATIVE, "native"))) throw new Error("OMP rejected the native rule; TTSR may be disabled");
    const both = syncLessonTtsr({ ctx, plan: plan([ACTIVE, INACTIVE]) });
    if (both.warnings.length || both.active !== 2) throw new Error(`sync of two rules returned ${JSON.stringify(both)}`);
    const one = syncLessonTtsr({ ctx, plan: plan([ACTIVE]) });
    if (one.warnings.length || one.active !== 1) throw new Error(`sync of one rule returned ${JSON.stringify(one)}`);
    const registered = manager.getRules().map((entry: any) => entry.name).sort();
    return `registered [${registered.join(", ")}]`;
  })) return finish();

  // The prototype call skips the bridge filter: it proves the inactive rule is
  // registered and would fire, so its absence below is filtering, not a miss.
  const all = [NATIVE, ACTIVE, INACTIVE];
  await run("raw_control_delta", async () => expectNames(await fired(manager, "delta", true), all, []));
  await run("raw_control_snapshot", async () => expectNames(await fired(manager, "snapshot", true), all, []));
  await run("raw_control_ast", async () => expectNames(await fired(manager, "ast", true), all, []));
  await run("delta_filter", async () => expectNames(await fired(manager, "delta"), [NATIVE, ACTIVE], [INACTIVE]));
  await run("snapshot_filter", async () => expectNames(await fired(manager, "snapshot"), [NATIVE, ACTIVE], [INACTIVE]));
  await run("ast_filter", async () => expectNames(await fired(manager, "ast"), [NATIVE, ACTIVE], [INACTIVE]));

  // OMP rebuilds its rule set here; the next sync must put the active lesson back.
  let resync = "";
  if (!await run("refresh_rebuild", async () => {
    await session.refreshBaseSystemPrompt();
    const synced = syncLessonTtsr({ ctx, plan: plan([ACTIVE]) });
    resync = JSON.stringify(synced);
    if (synced.warnings.length || synced.active !== 1) throw new Error(`refresh sync returned ${resync}`);
    return resync;
  })) return finish();
  const afterRefresh = (surface: Surface) => async () => `${expectNames(await fired(manager, surface), [ACTIVE], [INACTIVE])}; sync ${resync}`;
  await run("refresh_resync_delta", afterRefresh("delta"));
  await run("refresh_resync_snapshot", afterRefresh("snapshot"));
  await run("refresh_resync_ast", afterRefresh("ast"));

  const previousId = String(ctx.sessionManager.getSessionId());
  if (!await run("new_session_id", async () => {
    if (!(await session.newSession())) throw new Error("session.newSession() was cancelled");
    const nextId = String(ctx.sessionManager.getSessionId());
    if (nextId === previousId) throw new Error(`ctx.sessionManager still reports ${previousId}`);
    const same = session.ttsrManager === manager;
    manager = session.ttsrManager;
    return `${previousId} -> ${nextId}; ${same ? "same" : "replaced"} TTSR manager`;
  })) return finish();

  let freshSync = "";
  if (!await run("new_session_sync", () => {
    const synced = syncLessonTtsr({ ctx, plan: plan([ACTIVE, FRESH]) });
    freshSync = JSON.stringify(synced);
    if (synced.warnings.length || synced.active !== 2) throw new Error(`new session sync returned ${freshSync}`);
    return freshSync;
  })) return finish();
  const afterNew = (surface: Surface) => async () => `${expectNames(await fired(manager, surface), [ACTIVE, FRESH], [INACTIVE])}; sync ${freshSync}`;
  await run("new_session_delta", afterNew("delta"));
  await run("new_session_snapshot", afterNew("snapshot"));
  await run("new_session_ast", afterNew("ast"));
  return finish();
}

function writeReceipt(body: Record<string, unknown>): void {
  const target = process.env.ATHANOR_OMP_COMPAT_RECEIPT;
  if (!target) return;
  const pending = `${target}.partial`;
  writeFileSync(pending, JSON.stringify(body));
  renameSync(pending, target);
}

export default function ompCompatProbe(pi: any) {
  const nonce = process.env.ATHANOR_OMP_COMPAT_NONCE ?? null;
  const installWarning = installLessonTtsrBridge(pi);
  let started = false;
  pi.on("session_start", (_event: unknown, ctx: any) => {
    if (started) return;
    started = true;
    // Run outside the session_start callback: newSession re-enters the extension runner.
    setTimeout(() => {
      probe(pi, ctx, installWarning)
        .then((result) => writeReceipt({ nonce, ...result }))
        .catch((error) => writeReceipt({ nonce, version: null, checks: [], fatal: error instanceof Error ? error.stack ?? error.message : String(error) }));
    }, 0);
  });
}
