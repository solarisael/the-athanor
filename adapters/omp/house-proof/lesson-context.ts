import { DIAGNOSTIC_TIMEOUT_MS } from "./constants.ts";
import { discoverRustExecutable } from "../discovery.ts";
import { RustJsonlTransport } from "../rust-transport.ts";

const transports = new Map<string, RustJsonlTransport>();

function lessonTransport(roomDir: string): RustJsonlTransport | null {
  const executable = discoverRustExecutable();
  if (!executable) return null;
  const key = `${executable}\0${roomDir}`;
  let current = transports.get(key);
  if (current && !current.usable) {
    transports.delete(key);
    void current.close().catch(() => {});
    current = undefined;
  }
  if (!current) {
    current = new RustJsonlTransport({ executable, cwd: roomDir });
    transports.set(key, current);
  }
  return current;
}

export async function runLessonQuery(roomDir: string, room: string, filters: Record<string, unknown>) {
  const client = lessonTransport(roomDir);
  if (!client) return { ok: false, lessons: [], taxonomy: [], error: "Rust substrate executable is unavailable" };
  try {
    return await client.request("lesson_query", { room, ...filters }, {
      timeoutMs: DIAGNOSTIC_TIMEOUT_MS,
    }) as Record<string, unknown>;
  } catch (error) {
    return {
      ok: false,
      lessons: [],
      taxonomy: [],
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

