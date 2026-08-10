// Conservative presentation boundary for automatic recall.
// This module never changes the underlying recall query or explicit/manual recall.

const DEFAULT_GLUE_TERMS = [
  "a", "an", "and", "are", "as", "assignment", "at", "be", "below", "by", "can",
  "change", "create", "do", "for", "from", "how", "i", "in", "is", "it", "me",
  "my", "of", "on", "only", "or", "please", "read", "restart", "restarted",
  "review", "show", "target", "that", "the", "this", "to", "we", "what", "when",
  "where", "which", "with", "wonder", "work", "works", "you", "your",
];

const text = (value: unknown): string => value == null ? "" : String(value).trim();
const list = (value: unknown): string[] => Array.isArray(value) ? value.map(text).filter(Boolean) : [];
const key = (value: unknown): string => text(value).toLocaleLowerCase();
const NON_DISTINCTIVE_EXACT_TERMS = new Set(["before", "change", "restarted", "review", "same", "tool", "tools", "wonder", "work", "works"]);

function isDistinctiveExactTerm(value: unknown): boolean {
  const term = key(value);
  return term.length >= 7 && !NON_DISTINCTIVE_EXACT_TERMS.has(term);
}
const pathKey = (value: unknown): string => key(value).replace(/\\/g, "/").replace(/^house\//, "");

function termIn(value: unknown, term: string): boolean {
  const haystack = key(value);
  const needle = key(term);
  if (!needle || needle.length < 2) return false;
  return haystack === needle || haystack.includes(needle);
}

export interface RecallViewportOptions {
  /** Terms which are conversational glue, rather than memory intent. */
  glueTerms?: string[];
  /** Number of distinct evidence signals required without an exact signal. */
  minIndependentSignals?: number;
  /** Number of automatic exposures allowed per candidate identity per session. */
  saturationLimit?: number;
  /** Optional session state; omit to use a fresh, one-call session. */
  session?: RecallViewportSession;
}

export interface RecallViewportSession {
  exposures: Map<string, number>;
}

export interface RecallSuppression {
  identity: string;
  reason: "zero-terms" | "glue-only" | "insufficient-evidence" | "saturated";
}

export interface RecallViewportResult<T = any> {
  candidates: T[];
  keptCandidates: T[];
  suppressions: RecallSuppression[];
  diagnostics: { kept: number; suppressed: number; reasons: Record<string, number> };
}

export function createRecallViewportSession(): RecallViewportSession {
  return { exposures: new Map() };
}

function identityOf(candidate: any, index: number): string {
  const explicit = text(candidate?.id || candidate?.identity || candidate?.candidate_id);
  if (explicit) return explicit;
  const path = pathKey(candidate?.source_path);
  const title = key(candidate?.title);
  return path || title ? `${path}|${title}` : `candidate:${index}`;
}

function exactSignals(candidate: any, result: any, query: string): Set<string> {
  const signals = new Set<string>();
  const sources = list(candidate?.sources).map(key);
  const reasons = list(candidate?.reasons).map(key);
  const path = pathKey(candidate?.source_path);
  const title = text(candidate?.title);
  const terms = list(candidate?.matched_terms);
  const canon = Array.isArray(result?.canonMatches) ? result.canonMatches : [];
  if (sources.some((s) => s.includes("canon")) || reasons.some((s) => s.includes("canon")) || canon.some((m: any) => (Array.isArray(m?.files) && m.files.some((f: any) => pathKey(typeof f === "object" ? f?.file : f) === path)) || termIn(query, m?.termKey))) signals.add("canon");
  if (sources.some((s) => s.includes("entity")) || reasons.some((s) => s.includes("named entity"))) signals.add("entity");
  if (sources.some((s) => s.includes("date")) || (Array.isArray(result?.dateMatches) && result.dateMatches.some((d: any) => pathKey(d?.source_path) === path))) signals.add("date");
  if (sources.some((s) => s.includes("project")) || reasons.some((s) => s.includes("project"))) signals.add("project");
  if ((key(title).length >= 7 && termIn(query, title)) || terms.some((term) => isDistinctiveExactTerm(term) && termIn(title, term))) signals.add("title");
  if (terms.some((term) => isDistinctiveExactTerm(term) && termIn(path, term)) || (path.length >= 7 && termIn(query, path))) signals.add("path");
  return signals;
}

/** Filter compact results for automatic injection. Explicit recall remains untouched. */
export function automaticRecallViewport<T = any>(result: any, options: RecallViewportOptions = {}): RecallViewportResult<T> {
  const candidates = Array.isArray(result?.retrievalCandidates) ? result.retrievalCandidates : [];
  const glue = new Set((options.glueTerms || DEFAULT_GLUE_TERMS).map(key));
  const minimum = Math.max(1, options.minIndependentSignals ?? 2);
  const limit = Math.max(1, options.saturationLimit ?? 2);
  const session = options.session || createRecallViewportSession();
  const query = text(result?.query);
  const suppressions: RecallSuppression[] = [];
  const keptCandidates: T[] = [];
  candidates.forEach((candidate: any, index: number) => {
    const identity = identityOf(candidate, index);
    const matched = list(candidate?.matched_terms);
    const meaningful = matched.filter((term) => !glue.has(key(term)));
    let reason: RecallSuppression["reason"] | null = null;
    if (!matched.length) reason = "zero-terms";
    else if (!meaningful.length) reason = "glue-only";
    const exact = exactSignals(candidate, result, query);
    const independent = new Set(meaningful.map(key));
    exact.forEach((signal) => independent.add(`exact:${signal}`));
    if (!reason && exact.size === 0 && independent.size < minimum) reason = "insufficient-evidence";
    const exposed = session.exposures.get(identity) || 0;
    if (!reason && exposed >= limit) reason = "saturated";
    if (reason) suppressions.push({ identity, reason });
    else { keptCandidates.push(candidate); session.exposures.set(identity, exposed + 1); }
  });
  const reasons: Record<string, number> = {};
  suppressions.forEach(({ reason }) => { reasons[reason] = (reasons[reason] || 0) + 1; });
  return { candidates: keptCandidates, keptCandidates, suppressions, diagnostics: { kept: keptCandidates.length, suppressed: suppressions.length, reasons } };
}

export default automaticRecallViewport;
