import type { ProjectContext } from "./project-context.ts";

export type LessonLike = {
  id: number;
  type?: string;
  title?: string;
  lesson?: string;
  proof_pattern?: string;
  trigger_context?: string;
  project?: string;
  register?: string;
  stage?: string[];
  match?: { score?: number };
  semantic?: { similarity?: number };
};

export type LessonWorkSignals = {
  room: string;
  project: ProjectContext;
  prompt?: string;
  toolName?: string;
  target?: string;
  stages?: string[];
  registers?: string[];
};

export type LessonWorkingState = {
  room: string;
  project: ProjectContext;
  terms: string[];
  topicTerms: string[];
  stages: string[];
  registers: string[];
  workKind: string;
  targetKind: string;
  signature: string;
};

export type LessonWorkingSet = {
  signature: string;
  lessons: LessonLike[];
};

const STOP_WORDS = new Set(["about", "after", "again", "before", "between", "could", "from", "have", "into", "just", "more", "only", "please", "should", "that", "their", "there", "these", "this", "with", "would"]);
const workingSets = new Map<string, LessonWorkingSet>();
const activeStates = new Map<string, LessonWorkingState>();

function normal(value: unknown): string {
  return String(value ?? "").trim().toLowerCase();
}

function unique(values: Iterable<string>): string[] {
  return [...new Set([...values].map(normal).filter(Boolean))].sort();
}

function overlapRatio(left: string[], right: string[]): number {
  if (!left.length || !right.length) return 0;
  const rightSet = new Set(right);
  const overlap = left.filter((term) => rightSet.has(term)).length;
  return overlap / Math.min(left.length, right.length);
}

function promptTerms(prompt: string): string[] {
  return unique((prompt.match(/[a-z][a-z0-9_-]{2,}/gi) || [])
    .filter((term) => !STOP_WORDS.has(term.toLowerCase()))).slice(0, 12);
}

function declaredStages(prompt: string, fallback: string[] = []): string[] {
  const matched = [...prompt.matchAll(/\b(?:phase|stage)\s*[:#-]?\s*([a-z0-9][a-z0-9_-]*)\b/gi)]
    .flatMap((match) => [match[1], `${match[0].split(/\s|:|#|-/)[0]}-${match[1]}`]);
  return matched.length ? unique(matched) : unique(fallback);
}

function targetKind(target: string): string {
  const leaf = target.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || "";
  const extension = /\.([a-z0-9]+)$/i.exec(leaf)?.[1]?.toLowerCase();
  return extension || (leaf ? "path" : "");
}

function toolKind(toolName: string): string {
  const name = normal(toolName);
  if (/^(edit|write|ast[_-]?edit)/.test(name)) return "mutate";
  if (/^(test|verify|lint|build)/.test(name)) return "verify";
  return name || "prompt";
}

function stateKey(state: LessonWorkingState): string {
  return [state.room, normal(state.project.root), normal(state.project.project)].join("\0");
}

function score(lesson: LessonLike): number {
  return Number(lesson.semantic?.similarity ?? lesson.match?.score ?? 0);
}

export function deriveLessonWorkingState(signals: LessonWorkSignals): LessonWorkingState {
  const prompt = String(signals.prompt || "");
  const topicTerms = promptTerms(prompt);
  const stages = declaredStages(prompt, signals.stages);
  const registers = unique(signals.registers || []);
  const terms = unique([...topicTerms, ...promptTerms(String(signals.target || "")), ...promptTerms(String(signals.toolName || ""))]).slice(0, 16);
  const workKind = toolKind(String(signals.toolName || ""));
  const target = targetKind(String(signals.target || ""));
  const signature = [normal(signals.room), normal(signals.project.root), normal(signals.project.project),
    workKind, target, stages.join(","), registers.join(","), ""].join("\0");
  return {
    room: normal(signals.room) || "house", project: signals.project, terms, topicTerms,
    stages, registers, workKind, targetKind: target, signature,
  };
}

/** Small prose changes retain state; explicit phase or abrupt topic changes refresh it. */
export function stateForLessonPrompt(state: LessonWorkingState, prompt: string): LessonWorkingState {
  const incomingTerms = promptTerms(prompt);
  const stages = declaredStages(prompt, state.stages);
  const abruptTopicChange = incomingTerms.length >= 3
    && state.topicTerms.length >= 3
    && overlapRatio(incomingTerms, state.topicTerms) < 0.25;
  const topic = abruptTopicChange ? incomingTerms.slice(0, 6).join(",") : "";
  const signature = [state.room, normal(state.project.root), normal(state.project.project),
    state.workKind, state.targetKind, stages.join(","), state.registers.join(","), topic].join("\0");
  const topicTerms = abruptTopicChange
    ? incomingTerms
    : unique([...state.topicTerms, ...incomingTerms]).slice(0, 12);
  return { ...state, terms: unique([...state.terms, ...incomingTerms]).slice(0, 16), topicTerms, stages, signature };
}

export function rememberActiveLessonState(state: LessonWorkingState): void {
  activeStates.set(state.room, state);
}

export function activeLessonState(room: string): LessonWorkingState | null {
  return activeStates.get(normal(room) || "house") || null;
}
export function currentLessonWorkingSet(state: LessonWorkingState): LessonLike[] | null {
  const set = workingSets.get(stateKey(state));
  return set?.signature === state.signature ? set.lessons : null;
}


/**
 * Keep a small stable working set. A material state change re-ranks, but an
 * existing lesson stays when it remains competitive; prose-only signatures
 * return the exact persisted set without another embedding request.
 */
export function updateLessonWorkingSet(state: LessonWorkingState, ranked: LessonLike[]): { lessons: LessonLike[]; refreshed: boolean } {
  const key = stateKey(state);
  const prior = workingSets.get(key);
  if (prior?.signature === state.signature) return { lessons: prior.lessons, refreshed: false };

  const eligible = ranked.slice().sort((a, b) => score(b) - score(a) || a.id - b.id);
  const maximum = Math.min(6, eligible.length);
  const cutoff = maximum ? score(eligible[maximum - 1]) : 0;
  const available = new Map(eligible.map((lesson) => [lesson.id, lesson]));
  const retained = (prior?.lessons || [])
    .map((lesson) => available.get(lesson.id))
    .filter((lesson): lesson is LessonLike => Boolean(lesson) && score(lesson) >= cutoff - 0.03);
  const chosen = [...retained];
  for (const lesson of eligible) {
    if (chosen.length >= maximum) break;
    if (!chosen.some((current) => current.id === lesson.id)) chosen.push(lesson);
  }
  const lessons = chosen.slice(0, maximum);
  workingSets.set(key, { signature: state.signature, lessons });
  return { lessons, refreshed: true };
}

export function clearLessonWorkingSets(): void {
  workingSets.clear();
  activeStates.clear();
}
