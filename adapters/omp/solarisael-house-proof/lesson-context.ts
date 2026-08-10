import path from "node:path";
import { DIAGNOSTIC_TIMEOUT_MS } from "./constants.ts";
import { ATHANOR_ROOT } from "../athanor-root.ts";
import { runWslDiagnostic, windowsPathToWsl } from "./substrate.ts";

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
  register?: string;
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

/** Invoke the canonical structured query over the existing WSL substrate boundary. */
export async function runLessonContext(input: LessonContextInput): Promise<LessonContext> {
  const script = path.join(ATHANOR_ROOT, "src", "lesson-context.py");
  const argv = ["--cd", "~", "python3", windowsPathToWsl(script),
    "--room-dir", windowsPathToWsl(input.effectiveRoomDir), "--room", String(input.room || "shared")];
  for (const project of input.projects || []) argv.push("--project", String(project));
  for (const shape of input.shapes || []) argv.push("--shape", String(shape));
  for (const term of input.terms || []) argv.push("--term", String(term));
  for (const stage of input.stages || []) argv.push("--stage", String(stage));
  for (const register of input.registers || []) argv.push("--register", String(register));
  for (const language of input.languages || []) argv.push("--language", String(language));
  for (const technology of input.technologies || []) argv.push("--technology", String(technology));
  argv.push("--limit", String(input.limit ?? 8));
  try {
    const probe = await runWslDiagnostic({ argv, stdin: "", timeoutMs: DIAGNOSTIC_TIMEOUT_MS });
    if (probe.timedOut || probe.spawnError || probe.code !== 0) return EMPTY;
    const parsed = JSON.parse(String(probe.stdout || "{}"));
    return {
      codingLessons: Array.isArray(parsed.codingLessons) ? parsed.codingLessons as LessonRecord[] : [],
      projectLessons: Array.isArray(parsed.projectLessons) ? parsed.projectLessons as LessonRecord[] : [],
      match: parsed.match && typeof parsed.match === "object" ? parsed.match : {},
    };
  } catch {
    return EMPTY;
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
