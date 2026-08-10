// Generic named-entity resolution seam for OMP.
// The House Python helper owns substrate access and lexical matching; this
// wrapper only crosses the Windows/WSL boundary and keeps failures fail-open.

import path from "node:path";
import { DIAGNOSTIC_TIMEOUT_MS } from "./constants.ts";
import { ATHANOR_ROOT } from "../athanor-root.ts";
import { runWslDiagnostic, windowsPathToWsl } from "./substrate.ts";

export type EntityMatch = {
  canonicalName: string;
  kind: string;
  matchedAlias: string;
};

export type EntityResolution = {
  ok: boolean;
  matches: EntityMatch[];
  error?: string;
};

const SCRIPT = path.join(ATHANOR_ROOT, "src", "entity-resolution.py");

export async function resolveEntities({
  room,
  roomDir,
  query,
  limit = 8,
  timeoutMs = DIAGNOSTIC_TIMEOUT_MS,
}: {
  room: string;
  roomDir: string;
  query: string;
  limit?: number;
  timeoutMs?: number;
}): Promise<EntityResolution> {
  const argv = [
    "--cd", "~", "python3", windowsPathToWsl(SCRIPT),
    "--room", String(room || ""),
    "--room-dir", windowsPathToWsl(roomDir),
    "--limit", String(limit),
    "--query-stdin",
  ];
  const probe = await runWslDiagnostic({ argv, stdin: String(query || ""), timeoutMs });
  if (probe.timedOut) return { ok: false, matches: [], error: "entity resolution timed out" };
  if (probe.spawnError) return { ok: false, matches: [], error: probe.spawnError };
  if (probe.code !== 0) return { ok: false, matches: [], error: String(probe.stderr || "").trim() || `entity resolution exited ${probe.code}` };
  try {
    const parsed = JSON.parse(String(probe.stdout || "{}"));
    const matches = Array.isArray(parsed?.matches) ? parsed.matches.filter((item) => (
      item && typeof item.canonicalName === "string" && typeof item.kind === "string" && typeof item.matchedAlias === "string"
    )) : [];
    return { ok: true, matches };
  } catch (error) {
    return { ok: false, matches: [], error: error?.message || String(error) };
  }
}
