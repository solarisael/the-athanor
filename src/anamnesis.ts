import { queryAnamnesis } from "../adapters/omp/solarisael-house-proof/anamnesis.ts";
import { validateAmbientRoom } from "./spirit.ts";

export async function runAnamnesisQuery(roomDir, roomName, options = {}) {
  const ambient = validateAmbientRoom(roomDir, roomName);
  const requestedMode = options?.mode;
  const mode = requestedMode === "consult" ? "consult" : requestedMode === "wake" || requestedMode == null ? "wake" : String(requestedMode);
  if (mode !== "wake" && mode !== "consult") return { ok: false, mode, entries: [], warnings: [`invalid anamnesis mode: ${mode}`] };
  if (!ambient.ok) return { ok: false, mode, entries: [], warnings: [ambient.error] };
  const limitValue = Number(options?.limit);
  const limit = Number.isFinite(limitValue) ? Math.max(1, Math.min(50, Math.floor(limitValue))) : undefined;
  const query = typeof options?.query === "string" ? options.query : "";
  if (mode === "consult" && !query.trim()) return { ok: false, mode, entries: [], warnings: ["consult requires a non-empty query"] };
  const result = await queryAnamnesis(ambient.effectiveRoomDir, ambient.resolvedRoom, {
    mode,
    ...(mode === "consult" ? { query } : {}),
    ...(limit === undefined ? {} : { limit }),
  });
  return {
    ok: result?.ok === true,
    mode,
    entries: Array.isArray(result?.entries) ? result.entries : [],
    warnings: result?.ok === true
      ? (Array.isArray(result.warnings) ? result.warnings.map(String) : [])
      : [String(result?.error || "anamnesis source failed")],
  };
}
