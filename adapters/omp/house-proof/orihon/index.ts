import { requestOrgan, OrganError } from "../organ.ts";
import { roomBinding } from "../room.ts";
import { onAttachmentChanged, onBeforeShutdownDetach } from "../session.ts";
import { persistBaseline, persistCheckpoint, restoreCheckpoint, type Checkpoint, type Scope } from "./checkpoint.ts";
import { foldOptions } from "./runtime.ts";
import {
  ATTEMPT_ENTRY, BATCH_TARGET_BYTES, MAX_RECORDS, MAX_TEXT_BYTES,
  replayRecords, sourceEntries, splitParts, type Entry, type PendingRecord, type ReplayOptions,
} from "./source.ts";
import { childSessionOf } from "./transcript.ts";

type ModeSettings = { mode: "off" | "log" | "fold"; folder?: unknown };
export type ModeReceipt = ModeSettings & {
  generation: string;
  defaults: ModeSettings;
  // The Orihon section of the stable system prompt, for turns and compactions alike.
  instructions: string | null;
  instructionsVersion: string | null;
  liveStatus: unknown;
};

export type OrihonIdentity = {
  binding: { room: string; spirit: string; session: string };
  context: Record<string, any>;
  session: string;
  entries: Entry[];
  key: string;
};

export type PreparedSession = OrihonIdentity & { mode: ModeReceipt; scope: Scope | null; state: Checkpoint | null };

type CaptureOptions = ReplayOptions & { deadline?: number };

export function preparationTimeout(cap: number, deadline?: number): number {
  const remaining = deadline === undefined ? cap : Math.min(cap, deadline - Date.now());

  if (remaining <= 250) throw new Error("Orihon preparation ran out of time");

  return remaining;
}

// The handles the turn gate uses: the same identity, mode and capture path as the lifecycle hooks.
export type OrihonCapture = {
  identity(ctx: any): OrihonIdentity | null;
  cachedMode(ctx: any): ModeReceipt | null;
  prepare(ctx: any, deadline?: number): Promise<PreparedSession | null>;
  // Capture now, strictly before an entry when asked. Rejects on any failure; nothing is swallowed.
  captureNow(ctx: any, options?: CaptureOptions): Promise<PreparedSession | null>;
};

function modeReceipt(value: Record<string, unknown>, spirit: string, sourceSession: string): ModeReceipt {
  const defaults = value.defaults as ModeSettings | undefined;

  if (value.ok !== true || (value.mode !== "off" && value.mode !== "log" && value.mode !== "fold")
    || typeof value.generation !== "string" || !value.generation.trim()
    || value.spirit !== spirit || value.sourceSession !== sourceSession
    || !defaults || !["off", "log", "fold"].includes(defaults.mode)
    || !Array.isArray(value.supportedModes) || !value.supportedModes.includes("off") || !value.supportedModes.includes("log")
    || (value.instructions !== undefined && typeof value.instructions !== "string")) {
    throw new Error("Orihon Host returned invalid session settings or spirit defaults");
  }
  if (value.mode === "fold" && (typeof value.instructions !== "string" || !value.instructions.trim())) {
    throw new Error("Orihon Host reported live mode without its prompt instructions");
  }
  return {
    mode: value.mode, generation: value.generation, folder: value.folder, defaults,
    instructions: typeof value.instructions === "string" ? value.instructions : null,
    instructionsVersion: typeof value.instructionsVersion === "string" ? value.instructionsVersion : null,
    liveStatus: value.liveStatus ?? null,
  };
}

function acceptedSourceIds(value: unknown, pending: PendingRecord[]): boolean {
  return Array.isArray(value) && value.length === pending.length
    && value.every((id, index) => id === pending[index].record.sourceId);
}

function acceptedCounts(value: Record<string, unknown>, length: number): boolean {
  return Number.isInteger(value.appended) && Number(value.appended) >= 0
    && Number.isInteger(value.replayed) && Number(value.replayed) >= 0
    && Number(value.appended) + Number(value.replayed) === length;
}

function captureAccepted(value: Record<string, unknown>, generation: string, pending: PendingRecord[]): boolean {
  return value.ok === true && (value.mode === "log" || value.mode === "fold") && value.generation === generation
    && acceptedSourceIds(value.acceptedSourceIds, pending) && acceptedCounts(value, pending.length);
}

function captureOff(value: Record<string, unknown>): value is Record<string, unknown> & { generation: string } {
  return value.ok === true && value.mode === "off" && typeof value.generation === "string"
    && value.generation.length > 0 && acceptedSourceIds(value.acceptedSourceIds, []) && acceptedCounts(value, 0);
}

export function notify(ctx: any, text: string, level: "info" | "warning" | "error"): void {
  if (level !== "info") console.warn(`[athanor] ${text}`);
  try {
    ctx.ui?.notify?.(text, level);
  } catch (error) {
    console.warn(`[athanor] Orihon notice could not reach the UI: ${error instanceof Error ? error.message : String(error)}`);
  }
}

export function errorDetail(error: unknown): string {
  return error instanceof OrganError ? `${error.code}: ${error.message}` : error instanceof Error ? error.message : "unknown failure";
}

// Child sessions are proven by their transcript headers under this session's artifacts directory.
export function childResolver(ctx: any): ReplayOptions["resolveChild"] {
  let file = "";
  try {
    file = String(ctx?.sessionManager?.getSessionFile?.() ?? "");
  } catch {
    file = "";
  }
  return file ? (jobId) => childSessionOf(file, jobId) : () => null;
}

export function installOrihon(pi: any): OrihonCapture {
  const inFlight = new Map<string, Promise<unknown>>();
  const modes = new Map<string, ModeReceipt>();

  function identity(ctx: any): OrihonIdentity | null {
    if (ctx?.agent?.kind === "sub" || process.env.ATHANOR_REPLAY_MODE === "1") return null;
    const resolved = roomBinding(ctx);
    if (!resolved) return null;
    const source = sourceEntries(ctx);
    if (resolved.binding.session !== source.session) throw new Error("Orihon Host and replay session identities disagree");
    return { ...resolved, ...source, key: `${resolved.binding.room}\0${resolved.binding.spirit}\0${source.session}` };
  }

  function stillOwned(ctx: any, scope: Scope): boolean {
    const current = identity(ctx);
    return current?.binding.room === scope.room && current.binding.spirit === scope.spirit
      && current.session === scope.sourceSession;
  }

  function failure(ctx: any, error: unknown): void {
    notify(ctx, `Orihon capture is incomplete (${errorDetail(error)}). The last accepted checkpoint stays in place; a later lifecycle boundary retries it.`, "warning");
  }

  async function prepare(ctx: any, requestedMode?: "off" | "log" | "fold", folder: Record<string, unknown> = {}, deadline?: number): Promise<PreparedSession | null> {
    const initial = identity(ctx);
    if (!initial) return null;
    const receipt = await requestOrgan(initial.binding, "orihon_mode",
      requestedMode ? { mode: requestedMode, ...folder } : {}, { timeoutMs: preparationTimeout(3_000, deadline), write: requestedMode !== undefined });
    const mode = modeReceipt(receipt, initial.binding.spirit, initial.session);
    const current = identity(ctx);
    if (!current || current.key !== initial.key) throw new Error("Orihon session changed before its mode response");
    modes.set(current.key, mode);
    if (mode.mode === "off") return { ...current, mode, scope: null, state: null };
    const scope: Scope = { room: current.binding.room, spirit: current.binding.spirit,
      sourceSession: current.session, generation: mode.generation };
    let state = restoreCheckpoint(current.entries, scope);
    if (!state) {
      state = persistBaseline(pi, scope, current.entries);
      notify(ctx, "Orihon log starts at your next user page. Earlier pages and records from an off interval stay outside this generation.", "info");
    }
    return { ...current, mode, scope, state };
  }

  async function capture(ctx: any, options: CaptureOptions = {}): Promise<PreparedSession | null> {
    const prepared = await prepare(ctx, undefined, {}, options.deadline);
    if (!prepared?.scope || !prepared.state) return prepared;
    const { binding, session, entries, mode, scope, state } = prepared;
    const records = replayRecords(entries, session, state.baselineId, state.cursor, { ...options, resolveChild: childResolver(ctx) });
    let chunk: PendingRecord[] = [];
    const params = (pending: PendingRecord[]) => ({
      harness: "omp", sourceSession: session, generation: mode.generation, records: pending.map((item) => item.record),
    });
    const packetOverhead = Buffer.byteLength(JSON.stringify(params([])), "utf8");
    let packetBytes = packetOverhead;
    const deliver = async (pending: PendingRecord[]): Promise<void> => {
      if (!stillOwned(ctx, scope)) throw new Error("Orihon replay source changed before dispatch");
      let result: Record<string, unknown>;
      try {
        result = await requestOrgan(binding, "orihon_capture", params(pending), { timeoutMs: preparationTimeout(10_000, options.deadline), write: true });
      } catch (error) {
        if (!(error instanceof OrganError) || error.code !== "prepared_batch_too_large" || pending.length < 2) throw error;
        // Rust knows the redacted size. This refusal occurs before any record is accepted.
        const middle = Math.floor(pending.length / 2);
        await deliver(pending.slice(0, middle));
        await deliver(pending.slice(middle));
        return;
      }
      if (captureOff(result)) {
        modes.set(prepared.key, { ...mode, mode: "off", generation: result.generation });
        throw new Error("Orihon turned off before acceptance; no checkpoint advanced");
      }
      if (!captureAccepted(result, mode.generation, pending)) {
        throw new Error("Orihon acceptance does not match its generation and source IDs");
      }
      if (!stillOwned(ctx, scope)) throw new Error("Orihon accepted the records, but the source session changed before checkpoint persistence");
      persistCheckpoint(pi, scope, pending[pending.length - 1].cursor);
    };
    const send = async () => {
      if (!chunk.length) return;
      await deliver(chunk);
      chunk = [];
      packetBytes = packetOverhead;
    };
    for (const item of records) {
      const recordBytes = Buffer.byteLength(JSON.stringify(item.record), "utf8");
      const separatorBytes = chunk.length ? 1 : 0;
      if (chunk.length === MAX_RECORDS || packetBytes + recordBytes + separatorBytes > BATCH_TARGET_BYTES) {
        await send();
      }
      packetBytes += recordBytes + (chunk.length ? 1 : 0);
      chunk.push(item);
      // Rust must redact an oversized echo before it measures the prepared batch.
      if (packetBytes > BATCH_TARGET_BYTES) await send();
    }
    await send();
    return prepared;
  }

  // One capture per session at a time; a caller that needs the outcome awaits the shared promise.
  function run(ctx: any, key: string, options: CaptureOptions): Promise<PreparedSession | null> {
    const existing = inFlight.get(key);
    const work = (existing ?? Promise.resolve()).then(() => undefined, () => undefined)
      .then(() => capture(ctx, options))
      .finally(() => { if (inFlight.get(key) === work) inFlight.delete(key); });
    inFlight.set(key, work);
    return work;
  }

  function schedule(ctx: any): Promise<void> {
    let key: string;
    try {
      const current = identity(ctx);
      if (!current) return Promise.resolve();
      key = current.key;
    } catch (error) {
      failure(ctx, error);
      return Promise.resolve();
    }
    return run(ctx, key, {}).then(() => undefined, (error) => failure(ctx, error));
  }

  async function captureNow(ctx: any, options: CaptureOptions = {}): Promise<PreparedSession | null> {
    const current = identity(ctx);
    if (!current) return null;
    return run(ctx, current.key, options);
  }

  async function attach(_event: unknown, ctx: any): Promise<void> {
    try {
      await prepare(ctx);
      return schedule(ctx);
    } catch (error) {
      failure(ctx, error);
    }
  }

  // Install before the adapter's input-revising hooks. OMP can bypass this hook before extension dispatch.
  pi.on("tool_call", (event: any, ctx: any) => {
    try {
      const current = identity(ctx);
      if (!current || modes.get(current.key)?.mode === "off") return;
      if (typeof event?.toolCallId !== "string" || !event.toolCallId || typeof event.toolName !== "string" || !event.toolName) {
        throw new Error("OMP attempted tool has no source identity");
      }
      const input = typeof event.input === "string" ? event.input : JSON.stringify(event.input);
      if (typeof input !== "string") throw new Error("OMP attempted tool input is not serializable");
      const source = { version: 1, sourceSession: current.session, toolCallId: event.toolCallId, toolName: event.toolName };
      // An oversized input is split into several persistent entries, each within the record bound; nothing is cut.
      const parts = splitParts(input);
      if (parts.length === 1) {
        pi.appendEntry(ATTEMPT_ENTRY, { ...source, input });
        return;
      }
      parts.forEach((part, index) => pi.appendEntry(ATTEMPT_ENTRY, { ...source, input: part, part: index + 1, parts: parts.length }));
    } catch (error) {
      failure(ctx, error);
    }
  });
  pi.on("session_start", attach);
  pi.on("session_switch", attach);
  pi.on("session_branch", attach);
  pi.on("session_tree", attach);
  onAttachmentChanged(pi, (ctx) => attach(undefined, ctx));
  pi.on("turn_end", (_event: unknown, ctx: any) => schedule(ctx));
  pi.on("agent_end", (_event: unknown, ctx: any) => schedule(ctx));
  pi.on("session_compact", (_event: unknown, ctx: any) => schedule(ctx));
  onBeforeShutdownDetach(pi, (ctx) => schedule(ctx));

  pi.registerCommand("orihon", {
    description: "Show Orihon status, set off/log, or go live with fold local MODEL:TAG or fold hosted [provider/model]",
    handler: async (args: string, ctx: any) => {
      const [word, ...rest] = args.trim().split(/\s+/);
      const command = word || "status";
      if (!["status", "off", "log", "fold"].includes(command) || rest.length > 2 || (rest.length && command !== "fold")) {
        notify(ctx, "Use /orihon status, off, log, fold local MODEL:TAG, or fold hosted [provider/model]. Fold is live mode: every new turn sees the chat view instead of the old conversation.", "warning");
        return;
      }
      try {
        const current = identity(ctx);
        if (!current) throw new Error("Orihon requires an attached House room");
        await inFlight.get(current.key)?.catch(() => undefined);
        const folder = command === "fold" ? await foldOptions(rest, ctx.cwd, pi) : {};
        if (identity(ctx)?.key !== current.key) throw new Error("Orihon session changed before folder configuration");
        const prepared = await prepare(ctx, command === "status" ? undefined : command as "off" | "log" | "fold", folder);
        if (!prepared) throw new Error("Orihon room detached before its mode response");
        notify(ctx, JSON.stringify({
          spirit: prepared.binding.spirit, sourceSession: prepared.session,
          thisSession: { mode: prepared.mode.mode, folder: prepared.mode.folder, generation: prepared.mode.generation },
          defaultForNewSessions: prepared.mode.defaults,
          baseline: prepared.state?.baselineId ?? null, checkpoint: prepared.state?.cursor ?? null,
          waitingForUserPage: prepared.mode.mode !== "off" && !prepared.state?.cursor,
          instructionsVersion: prepared.mode.instructionsVersion, liveStatus: prepared.mode.liveStatus,
          authority: "provenance",
        }, null, 2), "info");
        if (command !== "status") notify(ctx, `Orihon ${prepared.mode.mode} applies to this session and is saved for new ${prepared.binding.spirit} sessions. Other open sessions keep their settings.`, "info");
        if (command === "fold") notify(ctx, "Orihon is live: the compactor folds in the background and your next turn starts from the chat view. Use /orihon status to inspect liveStatus.", "info");
        if (prepared.mode.mode !== "off") schedule(ctx);
      } catch (error) {
        notify(ctx, `Orihon command failed (${errorDetail(error)}). Use /orihon status to inspect the current configuration.`, "warning");
      }
    },
  });

  return {
    identity,
    cachedMode: (ctx) => {
      const current = identity(ctx);
      return current ? modes.get(current.key) ?? null : null;
    },
    prepare: (ctx, deadline) => prepare(ctx, undefined, {}, deadline),
    captureNow,
  };
}
