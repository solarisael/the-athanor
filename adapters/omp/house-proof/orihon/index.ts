import { requestOrgan, OrganError } from "../organ.ts";
import { roomBinding } from "../room.ts";
import { onAttachmentChanged, onBeforeShutdownDetach } from "../session.ts";
import { persistBaseline, persistCheckpoint, restoreCheckpoint, type Scope } from "./checkpoint.ts";
import {
  ATTEMPT_ENTRY, BATCH_TARGET_BYTES, MAX_RECORDS, MAX_TEXT_BYTES,
  replayRecords, sourceEntries, type PendingRecord,
} from "./source.ts";

type ModeReceipt = { mode: "off" | "log"; generation: string };

function modeReceipt(value: Record<string, unknown>): ModeReceipt {
  if (value.ok !== true || (value.mode !== "off" && value.mode !== "log")
    || typeof value.generation !== "string" || !value.generation.trim()
    || !Array.isArray(value.supportedModes) || !value.supportedModes.includes("off") || !value.supportedModes.includes("log")) {
    throw new Error("Orihon Host returned an invalid mode or generation");
  }
  return { mode: value.mode, generation: value.generation };
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
  return value.ok === true && value.mode === "log" && value.generation === generation
    && acceptedSourceIds(value.acceptedSourceIds, pending) && acceptedCounts(value, pending.length);
}

function captureOff(value: Record<string, unknown>): value is Record<string, unknown> & { generation: string } {
  return value.ok === true && value.mode === "off" && typeof value.generation === "string"
    && value.generation.length > 0 && acceptedSourceIds(value.acceptedSourceIds, []) && acceptedCounts(value, 0);
}

function notify(ctx: any, text: string, level: "info" | "warning"): void {
  if (level === "warning") console.warn(`[athanor] ${text}`);
  try {
    ctx.ui?.notify?.(text, level);
  } catch (error) {
    console.warn(`[athanor] Orihon notice could not reach the UI: ${error instanceof Error ? error.message : String(error)}`);
  }
}

export function installOrihon(pi: any): void {
  const inFlight = new Map<string, Promise<void>>();
  const modes = new Map<string, ModeReceipt>();

  function identity(ctx: any) {
    if (ctx?.agent?.kind === "sub" || process.env.ATHANOR_REPLAY_MODE === "1") return null;
    const resolved = roomBinding(ctx);
    if (!resolved) return null;
    const source = sourceEntries(ctx);
    if (resolved.binding.session !== source.session) throw new Error("Orihon Host and replay session identities disagree");
    return { ...resolved, ...source, key: `${resolved.binding.room}\0${source.session}` };
  }

  function stillOwned(ctx: any, scope: Scope): boolean {
    const current = identity(ctx);
    return current?.binding.room === scope.room && current.session === scope.sourceSession;
  }

  function failure(ctx: any, error: unknown): void {
    const detail = error instanceof OrganError ? `${error.code}: ${error.message}` : error instanceof Error ? error.message : "unknown failure";
    notify(ctx, `Orihon capture is incomplete (${detail}). The last accepted checkpoint stays in place; a later lifecycle boundary retries it.`, "warning");
  }

  async function prepare(ctx: any, requestedMode?: "off" | "log") {
    const initial = identity(ctx);
    if (!initial) return null;
    const mode = modeReceipt(await requestOrgan(initial.binding, "orihon_mode",
      requestedMode ? { mode: requestedMode } : {}, { timeoutMs: 3_000, write: requestedMode !== undefined }));
    const current = identity(ctx);
    if (!current || current.key !== initial.key) throw new Error("Orihon session changed before its mode response");
    modes.set(current.key, mode);
    if (mode.mode === "off") return { ...current, mode, scope: null, state: null };
    const scope: Scope = { room: current.binding.room, sourceSession: current.session, generation: mode.generation };
    let state = restoreCheckpoint(current.entries, scope);
    if (!state) {
      state = persistBaseline(pi, scope, current.entries);
      notify(ctx, "Orihon log starts at your next user page. Earlier pages and records from an off interval stay outside this generation.", "info");
    }
    return { ...current, mode, scope, state };
  }

  async function capture(ctx: any): Promise<void> {
    const prepared = await prepare(ctx);
    if (!prepared?.scope || !prepared.state) return;
    const { binding, session, entries, mode, scope, state } = prepared;
    const records = replayRecords(entries, session, state.baselineId, state.cursor);
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
        result = await requestOrgan(binding, "orihon_capture", params(pending), { timeoutMs: 10_000, write: true });
      } catch (error) {
        if (!(error instanceof OrganError) || error.code !== "prepared_batch_too_large" || pending.length < 2) throw error;
        // Rust knows the redacted size. This refusal occurs before any record is accepted.
        const middle = Math.floor(pending.length / 2);
        await deliver(pending.slice(0, middle));
        await deliver(pending.slice(middle));
        return;
      }
      if (captureOff(result)) {
        modes.set(prepared.key, { mode: "off", generation: result.generation });
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
  }

  function schedule(ctx: any): Promise<void> {
    let key: string;
    try {
      const current = identity(ctx);
      if (!current) return Promise.resolve();
      const existing = inFlight.get(current.key);
      if (existing) return existing;
      key = current.key;
    } catch (error) {
      failure(ctx, error);
      return Promise.resolve();
    }
    const work = capture(ctx).catch((error) => failure(ctx, error)).finally(() => inFlight.delete(key));
    inFlight.set(key, work);
    return work;
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
      const source = { version: 1, sourceSession: current.session, toolCallId: event.toolCallId, toolName: event.toolName };
      if (typeof input !== "string" || Buffer.byteLength(input, "utf8") > MAX_TEXT_BYTES) {
        pi.appendEntry(ATTEMPT_ENTRY, { ...source, unavailableReason: "input exceeds the replay-entry limit" });
        throw new Error("OMP original attempted input exceeds its replay-entry limit; the source gap stays explicit");
      }
      pi.appendEntry(ATTEMPT_ENTRY, { ...source, input });
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
    description: "Show Orihon status, or set off/log capture mode",
    handler: async (args: string, ctx: any) => {
      const command = args.trim() || "status";
      if (command !== "status" && command !== "off" && command !== "log") {
        notify(ctx, "Use /orihon status, /orihon off, or /orihon log. Fold mode is not implemented.", "warning");
        return;
      }
      try {
        const current = identity(ctx);
        if (!current) throw new Error("Orihon requires an attached House room");
        await inFlight.get(current.key);
        const prepared = await prepare(ctx, command === "status" ? undefined : command);
        if (!prepared) throw new Error("Orihon room detached before its mode response");
        notify(ctx, JSON.stringify({
          mode: prepared.mode.mode, generation: prepared.mode.generation, sourceSession: prepared.session,
          baseline: prepared.state?.baselineId ?? null, checkpoint: prepared.state?.cursor ?? null,
          waitingForUserPage: prepared.mode.mode === "log" && !prepared.state?.cursor,
          authority: "provenance",
        }, null, 2), "info");
        if (prepared.mode.mode === "log") schedule(ctx);
      } catch (error) {
        failure(ctx, error);
      }
    },
  });
}
