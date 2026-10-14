// Guards that stand between a Host-issued close or restart and the exit.
// Only an acknowledged `applied` may end the process; a guard that holds keeps
// the command waiting, and a guard that refuses settles it `refused`.

import { roomBinding } from "./room.ts";
import { catchBoat } from "./substrate.ts";

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : value == null ? "" : String(value).trim();
}

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

// A null snapshot is OMP's documented "no job manager", so nothing runs. A
// thrown observation proves nothing about casualties, so it refuses the exit.
function asyncJobsRunning(ctx: any): { jobs: Array<{ id: string; label: string }> } | { error: string } {
  try {
    const snapshot = ctx?.getAsyncJobSnapshot?.();
    const running = Array.isArray(snapshot?.running) ? snapshot.running : [];
    return { jobs: running.map((job: any) => ({ id: text(job?.id), label: text(job?.label) || text(job?.type) })) };
  } catch (error) {
    return { error: message(error) };
  }
}

function idle(ctx: any): boolean {
  const busy = typeof ctx?.isIdle === "function" && !ctx.isIdle();
  const queued = typeof ctx?.hasPendingMessages === "function" && ctx.hasPendingMessages();
  return !busy && !queued;
}

// An exit never falls inside a turn or ahead of queued work, and session-owned
// async jobs cannot outlive the process, so they refuse rather than die.
export function exitGuard(ctx: any): { hold: string } | { refuse: string } | null {
  if (!idle(ctx)) return { hold: "a turn or queued message is still running" };
  const observed = asyncJobsRunning(ctx);
  if ("error" in observed) return { refuse: `async jobs could not be observed, so their fate is unknown: ${observed.error}` };
  if (observed.jobs.length) {
    return { refuse: `async jobs would die with the exit: ${observed.jobs.map((job) => job.label || job.id).join(", ")}` };
  }
  return null;
}

// A fresh restart abandons this conversation's context, so a paper boat must
// be waiting for the successor. paper_boat_wake is a read and never consumes.
export async function confirmPaperBoat(
  ctx: any,
  signal?: AbortSignal,
  latest: (binding: any, options?: { signal?: AbortSignal }) => Promise<Record<string, any>> = catchBoat,
): Promise<{ ok: true; boat: Record<string, unknown> } | { ok: false; code: string; error: string }> {
  let resolved: ReturnType<typeof roomBinding> = null;
  try {
    resolved = roomBinding(ctx);
  } catch (error) {
    return { ok: false, code: "fresh_boat_unconfirmed", error: message(error) };
  }
  if (!resolved) return { ok: false, code: "fresh_boat_unconfirmed", error: "this conversation holds no home to read a paper boat from" };
  let wake: Record<string, any>;
  try {
    wake = await latest(resolved.binding, { signal });
  } catch (error) {
    return { ok: false, code: "fresh_boat_unconfirmed", error: `the House could not be asked for a paper boat: ${message(error)}` };
  }
  if (wake?.ok === false) {
    return { ok: false, code: "fresh_boat_unconfirmed", error: `the House could not be asked for a paper boat: ${text(wake.error) || "refused"}` };
  }
  if (wake?.found !== true) {
    return { ok: false, code: "fresh_without_boat", error: "a fresh restart abandons this conversation's context and this room has no paper boat waiting" };
  }
  return {
    ok: true,
    boat: { confirmed: true, id: text(wake.id) || null, title: text(wake.title) || null, createdAt: text(wake.createdAt) || null },
  };
}
