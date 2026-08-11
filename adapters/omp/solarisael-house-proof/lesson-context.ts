import { DIAGNOSTIC_TIMEOUT_MS } from "./constants.ts";
import { discoverRustExecutable } from "../discovery.ts";
import { RustJsonlTransport } from "../rust-transport.ts";

export type LessonContextInput = {
  effectiveRoomDir: string;
  room: string;
  projects?: string[];
  shapes?: string[];
  terms?: string[];
  stages?: string[];
  registers?: string[];
  languages?: string[];
  technologies?: string[];
  limit?: number;
};

export type LessonRecord = {
  id: number;
  type?: "coding" | "project";
  title?: string;
  lesson?: string;
  proof_pattern?: string;
  trigger_context?: string;
  project?: string;
  register?: string | string[];
  stage?: string[];
  language_keys?: string[];
  technology_keys?: string[];
  match?: { score?: number; matched?: string[] };
  semantic?: { similarity: number };
};

export type LessonContext = {
  codingLessons: LessonRecord[];
  projectLessons: LessonRecord[];
  match: Record<string, unknown>;
};

export type LessonRanker = (query: string, passages: string[]) => Promise<number[] | null>;

const EMPTY: LessonContext = {
  codingLessons: [], projectLessons: [], match: { scopes: [], projects: [], limit: 0 },
};
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


/** Invoke Rust's typed authority and eligibility policy. */
export async function runLessonContext(input: LessonContextInput): Promise<LessonContext> {
  const client = lessonTransport(input.effectiveRoomDir);
  if (!client) return EMPTY;
  try {
    const parsed = await client.request("lesson_context", {
      room: String(input.room || "shared"),
      projects: input.projects || [],
      shapes: input.shapes || [],
      terms: input.terms || [],
      stages: input.stages || [],
      registers: input.registers || [],
      languages: input.languages || [],
      technologies: input.technologies || [],
      limit: input.limit ?? 8,
    }, { timeoutMs: DIAGNOSTIC_TIMEOUT_MS }) as Record<string, unknown>;
    return {
      codingLessons: Array.isArray(parsed?.codingLessons) ? parsed.codingLessons as LessonRecord[] : [],
      projectLessons: Array.isArray(parsed?.projectLessons) ? parsed.projectLessons as LessonRecord[] : [],
      match: parsed?.match && typeof parsed.match === "object" ? parsed.match as Record<string, unknown> : {},
    };
  } catch {
    return EMPTY;
  }
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

function cosine(left: number[], right: number[]): number {
  let product = 0; let leftNorm = 0; let rightNorm = 0;
  for (let index = 0; index < left.length; index += 1) {
    const a = left[index]; const b = right[index];
    product += a * b; leftNorm += a * a; rightNorm += b * b;
  }
  return leftNorm && rightNorm ? product / Math.sqrt(leftNorm * rightNorm) : 0;
}

function passageFor(lesson: LessonRecord): string {
  return [lesson.title, lesson.trigger_context, lesson.proof_pattern, lesson.register,
    ...(lesson.stage || []), lesson.lesson].filter(Boolean).join("\n").slice(0, 4_000);
}

// Measured with the resident Nemotron Q4 model: relevant lesson passages land
// around 0.18-0.23 while unrelated eligible lessons top out below 0.10.
const LESSON_SIMILARITY_FLOOR = 0.15;

/** One request creates one query vector and vectors only for pre-filtered lessons. */
export async function rankEligibleLessons(query: string, lessons: LessonRecord[],
                                          ranker: LessonRanker = embedAndRank): Promise<LessonRecord[] | null> {
  if (!query.trim() || !lessons.length) return lessons;
  const similarities = await ranker(query, lessons.map(passageFor));
  if (!similarities || similarities.length !== lessons.length) return null;
  return lessons.map((lesson, index) => ({
    ...lesson,
    semantic: { similarity: Number.isFinite(similarities[index]) ? similarities[index] : 0 },
  }))
    .filter((lesson) => (lesson.semantic?.similarity || 0) >= LESSON_SIMILARITY_FLOOR)
    .sort((left, right) => (right.semantic?.similarity || 0) - (left.semantic?.similarity || 0) || left.id - right.id);
}

export async function embedAndRank(query: string, passages: string[]): Promise<number[] | null> {
  const endpoint = process.env.SOLARISAEL_EMBED_URL || process.env.SOLARISAEL_LMSTUDIO_URL || "http://127.0.0.1:11434/api/embed";
  const model = process.env.SOLARISAEL_EMBED_MODEL || "hf.co/zenmagnets/Nemotron-3-Embed-1B-Q4_K_M-GGUF:latest";
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 30_000);
  try {
    const response = await fetch(endpoint, {
      method: "POST", headers: { "Content-Type": "application/json" }, signal: controller.signal,
      body: JSON.stringify({ model, input: [`query: ${query}`, ...passages.map((passage) => `passage: ${passage}`)] }),
    });
    const payload = await response.json() as { embeddings?: unknown; data?: Array<{ embedding?: unknown }> };
    const raw = Array.isArray(payload.embeddings) ? payload.embeddings : payload.data?.map((item) => item.embedding);
    if (!raw || raw.length !== passages.length + 1 || !raw.every((vector) => Array.isArray(vector) && vector.length)) return null;
    const [queryVector, ...documentVectors] = raw as number[][];
    if (!queryVector.every(Number.isFinite) || !documentVectors.every((vector) => vector.length === queryVector.length && vector.every(Number.isFinite))) return null;
    return documentVectors.map((vector) => cosine(queryVector, vector));
  } catch {
    return null;
  } finally {
    clearTimeout(timeout);
  }
}
