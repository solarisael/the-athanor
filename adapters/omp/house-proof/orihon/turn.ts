// Live turns: every new turn starts from the persisted chat view instead of
// the old conversation, and a task child starts from the bounded view plus its
// task. The Host owns the view; this module owns which harness messages a
// request carries and refuses the request when the view is not ready.
import { OrganError, requestOrgan } from "../organ.ts";
import { roomBinding } from "../room.ts";
import { hostSessionIdentity } from "../host.ts";
import { currentTurnOrigin, generatedTurnKey, turnKeysByMessage, type TurnOrigin } from "../turn-origin.ts";
import { childResolver, errorDetail, notify, preparationTimeout, type OrihonCapture } from "./index.ts";
import { isPage, lastSourceIdBefore, message as entryMessage, textContent, type Entry } from "./source.ts";
import { markViewCache } from "./cache.ts";

export const VIEW_MESSAGE = "athanor-orihon-view";
export const NOTICE_MESSAGE = "athanor-orihon-notice";
export const ORIHON_COMPACTION_NOTE = "Conversation before this point lives in the Orihon view; nothing was summarized here.";
// The Host waits for the compactor inside this budget; OMP caps one extension
// handler at 30 s, so the request ends before that cap with a visible refusal.
const TURN_TIMEOUT_MS = 25_000;
const SUBAGENT_TIMEOUT_MS = 8_000;
const LOOP_LIMIT = 128;

type ViewReceipt = {
  purpose: string;
  chat: { revision: number; units: number; throughSeq: number | null; bytes: number; lines: number; blocks: string[]; tail: string };
  complete: boolean;
  prefixDigest: string | null;
  prefixChanged: boolean;
  instructionsVersion: string;
  instructions?: string;
};

type Loop = {
  // The turn key of the message this loop answers; fixed until the loop ends.
  originKey: string;
  // The first user page of this generation, when earlier history exists that consent never captured.
  firstPage: { timestamp: number; text: string } | null;
  view: Array<Record<string, unknown>>;
  cacheAnchor: string | null;
};

// OMP's own standing notices: independently authoritative harness context the
// view does not stand for. Everything else before the turn is conversation or
// a derived block and goes.
const HARNESS_NOTICES = new Set([
  "tool-roster-notice", "xdev-mount-notice", "session-agent-notice", "eval-prelude-notice", "eval-state-context",
  "plan-mode-reference", "goal-mode-context", "vibe-mode-context", "experimental-context-notes-reminder",
  "checkpoint-active-reminder",
]);

type Child = { view: Array<Record<string, unknown>> | null; cacheAnchor: string | null };

type ToolDefinition = { name: string; description: string; inputSchema: Record<string, unknown> };
type ToolObservation = { api: string; tools: ToolDefinition[] };

// The tools the model actually received, in provider order, from the provider payload the
// gate just passed. An entry the Host would refuse (blank name, non-object schema) voids the
// whole observation so the registry fallback carries the next turn instead of a refused one.
function wireTools(payload: any, api: string): ToolObservation | null {
  if (!Array.isArray(payload?.tools)) return null;

  let tools: ToolDefinition[];
  switch (api) {
    case "anthropic-messages":
      tools = payload.tools.map((tool: any) => ({ name: tool?.name, description: tool?.description ?? "", inputSchema: tool?.input_schema }));
      break;
    case "openai-responses":
      tools = payload.tools.filter((tool: any) => tool?.type === "function")
        .map((tool: any) => ({ name: tool.name, description: tool.description ?? "", inputSchema: tool.parameters }));
      break;
    case "openai-completions":
      tools = payload.tools.map((tool: any) => ({
        name: tool?.function?.name, description: tool?.function?.description ?? "", inputSchema: tool?.function?.parameters,
      }));
      break;
    default:
      return null;
  }

  const valid = tools.every((tool) => typeof tool.name === "string" && tool.name.trim() !== ""
    && typeof tool.description === "string"
    && !!tool.inputSchema && typeof tool.inputSchema === "object" && !Array.isArray(tool.inputSchema));
  return valid ? { api, tools } : null;
}

async function within<T>(promise: Promise<T>, timeoutMs: number): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([promise, new Promise<never>((_resolve, reject) => {
      timer = setTimeout(() => reject(new Error("Orihon preparation ran out of time")), timeoutMs);
    })]);
  } finally {
    clearTimeout(timer);
  }
}

function viewReceipt(value: Record<string, unknown>, purpose: "turn" | "subagent"): ViewReceipt {
  const chat = value.chat as ViewReceipt["chat"] | undefined;
  if (value.ok !== true || value.ready !== true || value.purpose !== purpose || !chat
    || !Array.isArray(chat.blocks) || !chat.blocks.every((block) => typeof block === "string")
    || typeof chat.tail !== "string" || !Number.isInteger(chat.revision)
    || typeof value.instructionsVersion !== "string"
    || (value.instructions !== undefined && typeof value.instructions !== "string")) {
    throw new Error("Orihon Host returned an invalid chat view");
  }
  if (purpose === "turn" && value.prefixChanged === true) {
    console.warn("[athanor] Orihon prefix changed: the compactor re-keys its cache to this request's system prompt and tools");
  }
  return value as unknown as ViewReceipt;
}

// The view as harness messages: whole blocks as text parts of one message,
// then the tail alone, so a provider cache boundary can sit on a whole block.
function viewMessages(receipt: ViewReceipt, instructions: string | null): Array<Record<string, unknown>> {
  const timestamp = Date.now();
  const details = {
    revision: receipt.chat.revision, units: receipt.chat.units, throughSeq: receipt.chat.throughSeq,
    bytes: receipt.chat.bytes, lines: receipt.chat.lines, complete: receipt.complete,
    prefixDigest: receipt.prefixDigest, instructionsVersion: receipt.instructionsVersion,
  };
  // Custom messages become developer-role instructions in OMP. The book is
  // source data, so use a generated user message instead of elevating it.
  const base = { role: "user", customType: VIEW_MESSAGE, display: false, attribution: "agent", timestamp };
  const parts = [...(instructions ? [instructions] : []), ...receipt.chat.blocks].map((text) => ({ type: "text", text }));
  if (!parts.length) return [{ ...base, content: receipt.chat.tail, details }];
  return [{ ...base, content: parts, details }, { ...base, content: receipt.chat.tail }];
}

function notice(text: string): Record<string, unknown> {
  return { role: "custom", customType: NOTICE_MESSAGE, content: text, display: false, attribution: "agent", timestamp: Date.now() };
}

// A prior message stays only when it is a classified harness notice.
export function keepPrior(message: any): boolean {
  return message?.role === "custom" && message.display !== true && HARNESS_NOTICES.has(String(message.customType ?? ""));
}

// The session entry this turn answers, or null for an ephemeral side turn
// (/btw) whose question OMP never persists. A persisted page is matched by
// timestamp and text; the newest page matches by text alone.
function originEntry(entries: Entry[], origin: TurnOrigin): Entry | null {
  const wanted = origin.native
    ? { timestamp: origin.message.timestamp, text: textContent(origin.message.content) }
    : { key: generatedTurnKey(origin.message) };
  let newestPage: Entry | null = null;
  for (let index = entries.length - 1; index >= 0; index -= 1) {
    const entry = entries[index];
    const value = entryMessage(entry);
    if (!value) continue;
    if (origin.native) {
      if (!isPage(entry)) continue;
      newestPage ??= entry;
      if (value.timestamp === wanted.timestamp && textContent(value.content) === wanted.text) return entry;
    } else if (generatedTurnKey(value) === wanted.key) {
      return entry;
    }
  }
  if (origin.native && newestPage && textContent(entryMessage(newestPage).content) === wanted.text) return newestPage;
  return null;
}

function firstPageAfter(entries: Entry[], baselineId: string | null): Loop["firstPage"] {
  if (baselineId === null) return null;
  const start = entries.findIndex((entry) => entry.id === baselineId);
  if (start < 0) throw new Error("Orihon baseline source is missing");
  for (let index = start + 1; index < entries.length; index++) {
    if (!isPage(entries[index])) continue;
    const value = entryMessage(entries[index]);
    return { timestamp: value.timestamp, text: textContent(value.content) };
  }
  return null;
}

// A tool's `parameters` on the installed OMP is a live ArkType schema: a callable that
// JSON.stringify drops, so the Host would see no inputSchema and refuse the turn. The
// schema's own toJsonSchema is the supported door (the same call pi-ai's wire layer makes);
// no OMP package is imported for it. A plain object is passed through as it is.
export function toolInputSchema(parameters: unknown): Record<string, unknown> {
  const ark = parameters as { toJsonSchema?: (options: unknown) => unknown } | null;
  const schema = typeof ark === "function" && typeof ark.toJsonSchema === "function"
    ? ark.toJsonSchema({ target: "draft-2020-12", fallback: (context: { base: unknown }) => context.base })
    : parameters;

  if (!schema || typeof schema !== "object" || Array.isArray(schema)) {
    throw new Error("a tool's parameters did not resolve to a JSON Schema object");
  }
  const { $schema: _dialect, ...plain } = schema as Record<string, unknown>;
  return plain;
}

export function activeToolDefinitions(pi: any, observed?: ToolObservation): ToolDefinition[] {
  if (observed) return observed.tools;

  // enough: the first turn and a changed tool set use registry order, including xd devices; only compactor cache reuse is lost.
  if (typeof pi.getAllTools !== "function" || typeof pi.getActiveTools !== "function") {
    throw new Error("this OMP runtime exposes no tool definitions, so the stable prefix cannot be transferred");
  }
  const active = new Set<string>(pi.getActiveTools());
  return pi.getAllTools().filter((tool: any) => active.has(tool.name)).map((tool: any) => {
    try {
      return { name: tool.name, description: tool.description, inputSchema: toolInputSchema(tool.parameters) };
    } catch (error) {
      throw new Error(`tool ${tool.name}: ${errorDetail(error)}`);
    }
  });
}

// The request: the view, the kept notices, then the turn from its first message.
// History before the generation's first captured page was never captured by
// consent; it is not sent, and one notice names that boundary.
function replaced(loop: Loop, messages: any[], originIndex: number): { messages: any[] } {
  const prior = messages.slice(0, originIndex);
  const generationStart = loop.firstPage
    ? prior.findIndex((entry) => entry?.role === "user" && entry.attribution === "user" && entry.timestamp === loop.firstPage!.timestamp
      && textContent(entry.content) === loop.firstPage!.text)
    : 0;
  const uncaptured = generationStart > 0 ? prior.slice(0, generationStart).filter((entry) => !keepPrior(entry)).length : 0;
  const boundary = uncaptured
    ? [notice(`Orihon: ${uncaptured} earlier messages precede this session's first captured page. They were never recorded (capture starts at the page after /orihon log or fold), so they are not in the view and are not sent.`)]
    : [];
  return { messages: [...loop.view, ...boundary, ...prior.filter(keepPrior), ...messages.slice(originIndex)] };
}

function taskChild(ctx: any): boolean {
  return ctx?.agent?.kind === "sub" && ctx.agent.id !== "advisor" && Number(ctx.agent.depth) >= 1;
}

export function installOrihonTurns(pi: any, capture: OrihonCapture, activePrompt: (ctx: any) => string | null): void {
  const loops = new Map<string, Loop>();
  const children = new Map<string, Child>();
  const preparedRequests = new Set<string>();
  const loopActive = new Set<string>();
  const observations = new Map<string, ToolObservation>();
  const preparations = new Map<string, { token: symbol; promise: Promise<boolean> }>();
  const identityWarnings = new Set<string>();

  function currentToken(ctx: any, token: symbol): boolean {
    return preparations.get(hostSessionIdentity(ctx))?.token === token;
  }

  function trim<T>(map: Map<string, T>): void {
    while (map.size > LOOP_LIMIT) {
      const oldest = map.keys().next();
      if (oldest.done) return;
      map.delete(oldest.value);
    }
  }

  function refuse(ctx: any, error: unknown): { messages: any[] } {
    const detail = error instanceof OrganError && error.code === "orihon_view_not_ready"
      ? `${errorDetail(error)} ${JSON.stringify(error.details ?? {})}` : errorDetail(error);
    const text = `Orihon live turn refused: ${detail}. This request was not sent; the old conversation is never carried instead of the view. Send the message again once the compactor is ready, or use /orihon status.`;
    notify(ctx, text, "error");
    ctx?.abort?.();
    return { messages: [notice(text)] };
  }

  // Live mode from the cached receipt; the Host is asked once per session, before any loop opens.
  // A session once seen live that loses its identity refuses: history must not flow while the room is unreachable.
  const liveSeen = new Set<string>();
  async function liveKey(ctx: any, deadline?: number): Promise<string | null> {
    let current;
    try {
      current = capture.identity(ctx);
    } catch (error) {
      const session = String(ctx?.sessionManager?.getSessionId?.() ?? "");
      if (session && liveSeen.has(session)) throw error;
      if (!identityWarnings.has(session)) {
        identityWarnings.add(session);
        console.warn(`[athanor] Orihon live turn skipped for a session that was never live here: ${errorDetail(error)}`);
      }
      return null;
    }
    if (!current) return null;
    const mode = capture.cachedMode(ctx) ?? (await capture.prepare(ctx, deadline))?.mode;
    if (mode?.mode !== "fold") return null;
    liveSeen.add(current.session);
    return current.key;
  }

  async function openLoop(ctx: any, key: string, messages: any[], token: symbol, deadline: number): Promise<{ loop: Loop; originIndex: number }> {
    const origin = currentTurnOrigin(messages, activePrompt(ctx));
    if (!origin) throw new Error("this turn's first message could not be identified");
    const originIndex = messages.indexOf(origin.message);
    const originKey = turnKeysByMessage(messages).get(origin.message);
    if (originIndex < 0 || !originKey) throw new Error("this turn's first message has no stable key");
    const before = capture.identity(ctx);
    if (!before) throw new Error("the room detached before the turn opened");
    const boundary = originEntry(before.entries, origin);
    const prepared = await capture.captureNow(ctx, { ...(boundary ? { stopBeforeEntryId: boundary.id } : {}), deadline });
    if (!prepared?.scope || !prepared.state || prepared.mode.mode !== "fold") throw new Error("this session is no longer live");
    const instructions = prepared.mode.instructions ?? "";
    const system = ctx.getSystemPrompt?.();
    if (!Array.isArray(system) || !system.includes(instructions)) {
      throw new Error("the Orihon instructions are not in this request's system prompt");
    }
    const model = ctx?.model && typeof ctx.model.provider === "string" && typeof ctx.model.id === "string"
      ? { provider: ctx.model.provider, id: ctx.model.id } : null;
    if (!model) throw new Error("this request has no resolved model, so the Host cannot account its cache honestly");
    const observed = observations.get(hostSessionIdentity(ctx));
    const timeoutMs = preparationTimeout(TURN_TIMEOUT_MS, deadline);
    const receipt = viewReceipt(await requestOrgan(prepared.binding, "orihon_turn", {
      generation: prepared.mode.generation,
      purpose: "turn",
      throughSourceId: lastSourceIdBefore(prepared.entries, prepared.session, prepared.state.baselineId, boundary?.id ?? null, childResolver(ctx)),
      prefix: { system, tools: activeToolDefinitions(pi, observed?.api === ctx.model.api ? observed : undefined) },
      model,
      waitMs: Math.max(1000, timeoutMs - 5000),
    }, { timeoutMs, write: true }), "turn");
    const loop: Loop = {
      originKey, firstPage: firstPageAfter(prepared.entries, prepared.state.baselineId),
      view: viewMessages(receipt, null), cacheAnchor: receipt.chat.blocks.at(-1) ?? null,
    };
    if (currentToken(ctx, token)) {
      loops.set(key, loop);
      trim(loops);
    }
    return { loop, originIndex };
  }

  async function turnContext(messages: any[], ctx: any, token: symbol, deadline: number): Promise<{ messages: any[] } | undefined> {
    let key: string | null;
    try {
      key = await liveKey(ctx, deadline);
    } catch (error) {
      return currentToken(ctx, token) ? refuse(ctx, error) : undefined;
    }
    if (!key) return undefined;
    const hadCompaction = messages.some((entry) => entry?.role === "compactionSummary");
    messages = messages.filter((entry) => entry?.role !== "compactionSummary" || entry.summary !== ORIHON_COMPACTION_NOTE);
    const loop = loops.get(key);
    if (loop) {
      const keys = turnKeysByMessage(messages);
      const current = currentTurnOrigin(messages, activePrompt(ctx));
      const sameTurn = !current || keys.get(current.message) === loop.originKey;
      const originIndex = messages.findIndex((entry) => keys.get(entry) === loop.originKey);
      if (sameTurn && originIndex >= 0) return replaced(loop, messages, originIndex);
      // Only an in-loop compaction may stand for this turn's lost start.
      if (sameTurn && hadCompaction) return replaced(loop, messages, 0);
      loops.delete(key);
    }
    try {
      const opened = await openLoop(ctx, key, messages, token, deadline);
      return replaced(opened.loop, messages, opened.originIndex);
    } catch (error) {
      return currentToken(ctx, token) ? refuse(ctx, error) : undefined;
    }
  }

  // A task child's transcript is its own (task/executor.ts opens a fresh child
  // session file or an in-memory one; nothing inherits the parent's history).
  // The view goes in front of the child's own messages; a child whose first
  // message is not its task prompt is refused rather than guessed at.
  async function childContext(messages: any[], ctx: any): Promise<{ messages: any[] } | undefined> {
    const resolved = roomBinding(ctx);
    if (!resolved) return undefined;
    const key = `${resolved.binding.room}\0${resolved.binding.session}`;
    let child = children.get(key);
    if (!child) {
      try {
        const receipt = viewReceipt(await requestOrgan(resolved.binding, "orihon_turn", {
          generation: null, purpose: "subagent", throughSourceId: null, prefix: null, model: null, waitMs: null,
        }, { timeoutMs: SUBAGENT_TIMEOUT_MS, write: true }), "subagent");
        child = { view: viewMessages(receipt, receipt.instructions ?? null), cacheAnchor: receipt.chat.blocks.at(-1) ?? null };
      } catch (error) {
        if (!(error instanceof OrganError) || error.code !== "orihon_not_live") return refuse(ctx, error);
        child = { view: null, cacheAnchor: null };
      }
      children.set(key, child);
      trim(children);
    }
    if (!child.view) return undefined;
    const own = messages.filter((entry) => entry?.customType !== VIEW_MESSAGE);
    const first = own.find((entry) => entry?.role === "user" || entry?.role === "assistant" || entry?.role === "toolResult");
    if (!first || first.role !== "user") return refuse(ctx, new Error("this task child's first message is not its task prompt"));
    return { messages: [...child.view, ...own] };
  }

  // The instructions ride in the system prompt for the whole turn, so turns and compactions share one prefix.
  pi.on("before_agent_start", async (event: any, ctx: any) => {
    if (ctx?.agent?.kind !== "sub" || taskChild(ctx)) loopActive.add(hostSessionIdentity(ctx));
    if (ctx?.agent?.kind === "sub") return undefined;
    try {
      if (!(await liveKey(ctx))) return undefined;
      const instructions = capture.cachedMode(ctx)?.instructions;
      if (!instructions) return undefined;
      const parts = Array.isArray(event?.systemPrompt) ? event.systemPrompt : [];
      return parts.includes(instructions) ? undefined : { systemPrompt: [...parts, instructions] };
    } catch (error) {
      // The context gate refuses the request when the instructions are missing.
      console.warn(`[athanor] Orihon instructions were not added to the system prompt: ${errorDetail(error)}`);
      return undefined;
    }
  });

  pi.on("session_before_compact", async (event: any, ctx: any) => {
    if (ctx?.agent?.kind === "sub" || !(await liveKey(ctx))) return undefined;

    return { compaction: {
      summary: ORIHON_COMPACTION_NOTE,
      firstKeptEntryId: event.preparation.firstKeptEntryId,
      tokensBefore: event.preparation.tokensBefore,
    } };
  });

  // Registered last, so the generic context hook has anchored this turn's own blocks first.
  pi.on("context", async (event: any, ctx: any) => {
    const session = hostSessionIdentity(ctx);
    const token = Symbol(session);
    const deadline = Date.now() + TURN_TIMEOUT_MS;
    preparedRequests.delete(session);
    const messages = Array.isArray(event?.messages) ? event.messages : [];
    const work = Promise.resolve().then(async () => {
      const result = ctx?.agent?.kind === "sub"
        ? taskChild(ctx) ? await childContext(messages, ctx) : undefined
        : await turnContext(messages, ctx, token, deadline);

      if (!currentToken(ctx, token)) {
        console.warn("[athanor] Orihon ignored a late context preparation");
        return undefined;
      }

      if (result?.messages.some((message) => message?.customType === VIEW_MESSAGE)) preparedRequests.add(session);
      return result;
    });
    const promise = work.then(() => currentToken(ctx, token) && preparedRequests.has(session), () => false);
    preparations.set(session, { token, promise });

    try {
      return await within(work, Math.max(1, deadline - Date.now()));
    } catch (error) {
      if (!currentToken(ctx, token)) return undefined;

      preparations.delete(session);
      preparedRequests.delete(session);
      return refuse(ctx, error);
    }
  });

  pi.on("before_provider_request", async (event: any, ctx: any) => {
    const session = hostSessionIdentity(ctx);

    if (!loopActive.has(session)) return undefined;

    let anchor: string | null = null;
    try {
      const preparation = preparations.get(session);
      const prepared = preparation ? await within(preparation.promise, 20_000) : false;
      let required = false;
      if (taskChild(ctx)) {
        const resolved = roomBinding(ctx);
        if (!resolved) return undefined;
        const child = children.get(`${resolved.binding.room}\0${resolved.binding.session}`);
        if (!child) throw new Error("the task's recording policy has not been resolved");
        required = child.view !== null;
        anchor = child.cacheAnchor;
      } else {
        const key = await liveKey(ctx);
        if (!key) return undefined;

        required = true;
        anchor = loops.get(key)?.cacheAnchor ?? null;
      }
      if (required && (!prepared || !preparation || !currentToken(ctx, preparation.token))) {
        throw new Error("this provider request has no completed live-view preparation");
      }
    } catch (error) {
      // OMP can swallow a timed-out context handler. This independent gate
      // must still abort before any unbounded history reaches the provider.
      refuse(ctx, error);
      return undefined;
    }
    const observed = wireTools(event?.payload, ctx?.model?.api);
    if (observed) observations.set(session, observed);

    if (ctx?.model?.api !== "anthropic-messages") return undefined;
    const result = markViewCache(event?.payload, anchor);
    if (result.reason) console.warn(`[athanor] Orihon cache boundary unavailable: ${result.reason}`);
    return result.changed ? event.payload : undefined;
  });

  // The observed tool list outlives the turn: the next turn's prefix is built before its own
  // payload exists. It resets with the session, not with agent_end.
  function forget(ctx: any): void {
    const session = hostSessionIdentity(ctx);
    loopActive.delete(session);
    preparations.delete(session);
    preparedRequests.delete(session);
    try {
      const current = capture.identity(ctx);
      if (current) loops.delete(current.key);
    } catch {
      // A session without identity holds no loop.
    }
  }
  pi.on("agent_end", (event: any, ctx: any) => {
    if (event?.willContinue === true) return;
    forget(ctx);
  });
  // An in-loop compaction keeps the loop: its summary stands for the turn's lost start.
  for (const type of ["session_start", "session_switch", "session_branch", "session_tree"]) {
    pi.on(type, (_event: unknown, ctx: any) => {
      observations.delete(hostSessionIdentity(ctx));
      forget(ctx);
    });
  }
}
