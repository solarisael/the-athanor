// Generic named-entity resolution seam for OMP. Rust owns substrate access,
// active-authority selection, lexical matching, ordering, and bounds.
import { DIAGNOSTIC_TIMEOUT_MS } from "./constants.ts";
import { discoverRustExecutable } from "../discovery.ts";
import { RustJsonlTransport } from "../rust-transport.ts";

export type EntityMatch = { canonicalName: string; kind: string; matchedAlias: string };
export type EntityResolution = { ok: boolean; matches: EntityMatch[]; error?: string };

const transports = new Map<string, RustJsonlTransport>();
function transport(roomDir: string): RustJsonlTransport | null {
  const executable = discoverRustExecutable();
  if (!executable) return null;
  const key = `${executable}\0${roomDir}`;
  let current = transports.get(key);
  if (current && !current.usable) { transports.delete(key); void current.close().catch(() => {}); current = undefined; }
  if (!current) { current = new RustJsonlTransport({ executable, cwd: roomDir }); transports.set(key, current); }
  return current;
}

export async function resolveEntities({ room, roomDir, query, limit = 8, timeoutMs = DIAGNOSTIC_TIMEOUT_MS }: {
  room: string; roomDir: string; query: string; limit?: number; timeoutMs?: number;
}): Promise<EntityResolution> {
  const client = transport(roomDir);
  if (!client) return { ok: false, matches: [], error: "Rust substrate executable is unavailable" };
  try {
    const result = await client.request("entity_resolve", { room: String(room || ""), query: String(query || ""), limit }, { timeoutMs });
    if (!result || typeof result !== "object" || Array.isArray(result)) return { ok: false, matches: [], error: "Rust entity resolver returned an invalid result" };
    const value = result as Record<string, unknown>;
    const matches = Array.isArray(value.matches) ? value.matches.filter((item) => {
      const row = item as Record<string, unknown>;
      return row && typeof row.canonicalName === "string" && typeof row.kind === "string" && typeof row.matchedAlias === "string";
    }) as EntityMatch[] : [];
    return { ok: value.ok === true, matches, ...(value.ok === true ? {} : { error: String(value.error || "entity resolution failed") }) };
  } catch (error) {
    return { ok: false, matches: [], error: error instanceof Error ? error.message : String(error) };
  }
}
