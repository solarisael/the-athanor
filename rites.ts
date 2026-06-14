// Session rites — the wake / write / sleep lifecycle (see docs/session-rites.md).
//
// Each rite is a thin seam onto the canonical substrate, never a reimplementation
// of it. The akashic write below routes through record_memory.py (the single
// writer) the same way recall routes through postgres-memory-source.py: python
// in WSL, payload on stdin, raw outcome mapped onto a result, fail-open.

import path from "node:path";
import { runWsl, windowsPathToWsl } from "./wsl.ts";

// record_memory.py lives in the shared house, not in the plugin:
// <sharedRoot>/house/substrate. SOLARISAEL_SUBSTRATE overrides the dir.
// enough: convention path + env override; a real config field the day rooms
// stop living one level under a shared root.
function resolveSubstrate(sharedRoot) {
  const dir = process.env.SOLARISAEL_SUBSTRATE
    || path.join(sharedRoot, "house", "substrate");
  return { dir, recordMemory: path.join(dir, "record_memory.py") };
}

// Write one memory through record_memory.py — the akashic rite.
//
// Cost, made visible: one DB write plus an inline Ollama embed pass (the writer
// wakes Ollama if it's asleep and puts it back after). That embed is why a
// write is retrievable on the very next turn — and why the timeout is generous.
//
// Backup is OFF by default: a mid-session save shouldn't trigger a 54MB pg_dump
// every time. The sleep rite passes backup:true to snapshot once per session.
//
// Refuses an empty body and a missing source-path (the UPSERT key). Never
// throws — resolves to { ok, id?, summary?, error? } the caller maps.
export async function recordSessionMemory({
  sharedRoot, room, title, sourcePath, body,
  type = "session", threads = [], canonTouches = [],
  backup = false, timeoutMs = 90000,
}) {
  const trimmedBody = String(body || "").trim();
  if (!trimmedBody) return { ok: false, error: "refusing to write an empty memory" };
  if (!sourcePath) return { ok: false, error: "source-path is required (the upsert key)" };

  const { dir, recordMemory } = resolveSubstrate(sharedRoot);

  const argv = [
    "python3", windowsPathToWsl(recordMemory),
    "--room", String(room || "").toLowerCase(),
    "--type", String(type),
    "--title", String(title || "untitled"),
    "--source-path", String(sourcePath),
    "--body-stdin",
  ];
  for (const thread of threads) argv.push("--thread", String(thread));
  for (const entity of canonTouches) argv.push("--canon-touches", String(entity));
  if (!backup) argv.push("--no-backup");

  const outcome = await runWsl({ argv, cwd: dir, stdin: trimmedBody, timeoutMs });

  if (outcome.timedOut) return { ok: false, error: "record_memory timed out (cold embed pass?)" };
  if (outcome.spawnError) return { ok: false, error: outcome.spawnError };
  if (outcome.code !== 0) {
    return { ok: false, error: outcome.stderr.trim() || `record_memory exited ${outcome.code}` };
  }

  const summary = outcome.stdout.trim();
  const idMatch = /id=(\d+)/.exec(summary);
  return { ok: true, id: idMatch ? Number(idMatch[1]) : null, summary };
}

// Catch the latest paper boat for a room — the wake (anamnesis) read. Spawns
// catch_boat.py (a single-job latest-row lookup, not the ranked retrieval
// reader). Fail-open: resolves to { ok, found?, title?, body?, ..., error? }.
export async function catchLatestBoat({ sharedRoot, room, timeoutMs = 8000 }) {
  const { dir } = resolveSubstrate(sharedRoot);
  const script = path.join(dir, "catch_boat.py");

  const outcome = await runWsl({
    argv: ["python3", windowsPathToWsl(script), "--room", String(room || "").toLowerCase()],
    cwd: dir,
    timeoutMs,
  });

  if (outcome.timedOut) return { ok: false, error: "catch_boat timed out" };
  if (outcome.spawnError) return { ok: false, error: outcome.spawnError };
  if (outcome.code !== 0) {
    return { ok: false, error: outcome.stderr.trim() || `catch_boat exited ${outcome.code}` };
  }

  try {
    return { ok: true, ...JSON.parse(outcome.stdout) };
  } catch (err) {
    return { ok: false, error: err?.message || String(err) };
  }
}
