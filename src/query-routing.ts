import { MEMORY_STOPWORDS, MEMORY_TOKEN_RE } from "./paths.ts";

// Pure query parsing/routing leaf. No memory/retrieval imports here: adapters and
// ranking code depend on this module, so it must stay cycle-free.

const ROUTING_STOPWORDS = new Set([
  "about",
  "again",
  "also",
  "before",
  "could",
  "gonna",
  "gotta",
  "have",
  "into",
  "just",
  "know",
  "left",
  "like",
  "little",
  "make",
  "much",
  "need",
  "nice",
  "now",
  "please",
  "really",
  "should",
  "that",
  "then",
  "there",
  "this",
  "turn",
  "use",
  "using",
  "wanna",
  "want",
  "what",
  "when",
  "where",
  "which",
  "with",
  "worth",
]);

const CASUAL_CONTACT_TERMS = new Set([
  "awoo",
  "cuddle",
  "dummy",
  "eep",
  "hehe",
  "hello",
  "hiii",
  "love",
  "morning",
  "scratch",
  "scratches",
  "sleep",
  "slept",
  "uwu",
  "wuv",
]);

const TECHNICAL_PROJECT_TERMS = new Set([
  "adapter",
  "advisor",
  "api",
  "bm25",
  "bun",
  "candidate",
  "candidates",
  "core",
  "coverage",
  "debug",
  "embedding",
  "embeddings",
  "fallback",
  "fusion",
  "hook",
  "hooks",
  "hygiene",
  "index",
  "integration",
  "json",
  "memory",
  "omp",
  "opencode",
  "package",
  "plugin",
  "postgres",
  "query",
  "recall",
  "retrieval",
  "routing",
  "runtime",
  "smoke",
  "source",
  "substrate",
  "test",
  "tests",
  "tool",
  "tools",
  "vector",
  "verification",
  "v1",
  "wsl",
]);

const MEMORY_LOOKUP_TERMS = new Set([
  "canon",
  "happened",
  "memory",
  "recall",
  "remember",
  "remind",
  "thread",
  "timeline",
]);

const QUERY_DATE_RE = /\b\d{4}-\d{2}-\d{2}\b/g;
const QUOTED_PHRASE_RE = /"([^"]+)"|'([^']+)'|`([^`]+)`/g;
const CODE_TOKEN_RE = /[A-Za-zÀ-ÿ0-9]+(?:[._:/+#-][A-Za-zÀ-ÿ0-9]+)+/g;

function text(value) {
  if (value === null || value === undefined) return "";
  return String(value).trim();
}

function unique(values) {
  const out = [];
  const seen = new Set();
  for (const value of values) {
    const normalized = text(value).toLowerCase();
    if (!normalized || seen.has(normalized)) continue;
    seen.add(normalized);
    out.push(normalized);
  }
  return out;
}

function uniqueOriginal(values) {
  const out = [];
  const seen = new Set();
  for (const value of values) {
    const original = text(value);
    const key = original.toLowerCase();
    if (!original || seen.has(key)) continue;
    seen.add(key);
    out.push(original);
  }
  return out;
}

function cleanTerm(value) {
  const term = text(value)
    .toLowerCase()
    .replace(/^[._:/+#-]+|[._:/+#-]+$/g, "");
  if (term.length <= 1 || MEMORY_STOPWORDS.has(term) || ROUTING_STOPWORDS.has(term)) return "";
  return term;
}

function extractQuotedPhrases(query) {
  const phrases = [];
  for (const match of text(query).matchAll(QUOTED_PHRASE_RE)) {
    const phrase = text(match[1] || match[2] || match[3]);
    if (phrase) phrases.push(phrase);
  }
  return unique(phrases);
}

function splitCodeTokenParts(token) {
  return text(token).split(/[._:/+#-]+/).filter(Boolean);
}

export function parseRetrievalQuery(query) {
  const source = text(query);
  const lower = source.toLowerCase();
  const quotedPhrases = extractQuotedPhrases(source);
  const dateTokens = unique(lower.match(QUERY_DATE_RE) || []);
  const originalTokens = source.match(MEMORY_TOKEN_RE) || [];
  const rawTokens = lower.match(MEMORY_TOKEN_RE) || [];
  const codeishTokens = source.match(CODE_TOKEN_RE) || [];
  const terms = [];
  const codeTokens = [];

  for (const token of rawTokens) {
    const cleaned = cleanTerm(token);
    if (cleaned) terms.push(cleaned);
  }

  for (const token of codeishTokens) {
    codeTokens.push(token);
    for (const part of splitCodeTokenParts(token)) {
      const partTerm = cleanTerm(part);
      if (partTerm) terms.push(partTerm);
    }
  }

  for (const phrase of quotedPhrases) {
    for (const token of phrase.match(MEMORY_TOKEN_RE) || []) {
      const cleaned = cleanTerm(token);
      if (cleaned) terms.push(cleaned);
    }
  }

  const uniqueTerms = unique(terms).slice(0, 24);
  const entityHints = uniqueOriginal(originalTokens.filter((token) => (
    /^[A-ZÀ-ÖØ-Þ]/.test(token)
    || /[a-z][A-Z]/.test(token)
    || (token.length > 8 && !MEMORY_STOPWORDS.has(token.toLowerCase()))
  ))).slice(0, 12);

  return {
    query: source,
    terms: uniqueTerms,
    requiredTerms: [...dateTokens, ...quotedPhrases].slice(0, 12),
    optionalTerms: uniqueTerms,
    quotedPhrases,
    codeTokens: uniqueOriginal(codeTokens).slice(0, 12),
    dateTokens,
    entityHints,
    stopwordStrippedQuery: uniqueTerms.join(" "),
  };
}

export function classifyRetrievalQuery(query) {
  const parsed = parseRetrievalQuery(query);
  const termSet = new Set(parsed.terms);
  const source = parsed.query.toLowerCase();
  const technicalHits = parsed.terms.filter((term) => TECHNICAL_PROJECT_TERMS.has(term)).length;
  const casualHits = parsed.terms.filter((term) => CASUAL_CONTACT_TERMS.has(term)).length;
  const memoryHits = parsed.terms.filter((term) => MEMORY_LOOKUP_TERMS.has(term)).length;
  const reasons = [];

  let intent = "general";
  if (parsed.dateTokens.length) {
    intent = "date_lookup";
    reasons.push("date-token");
  } else if (technicalHits >= 2 || /\b(?:solarisael[- ]house|plugin|omp|opencode|runtime|query routing|retrieval|postgres|pgvector|tests?)\b/.test(source)) {
    intent = "technical_project";
    reasons.push("technical-project-terms");
  } else if (memoryHits || /\b(?:what happened|do you remember|remember when|recall)\b/.test(source)) {
    intent = "memory_lookup";
    reasons.push("memory-lookup-language");
  } else if (parsed.terms.length <= 3 && casualHits > 0) {
    intent = "casual_contact";
    reasons.push("casual-contact-low-information");
  } else if (!parsed.terms.length) {
    intent = "casual_contact";
    reasons.push("no-meaningful-terms");
  }

  const lanes = {
    lexical: intent !== "casual_contact",
    candidates: intent !== "casual_contact",
    semantic: intent === "general" || intent === "memory_lookup",
    content: intent === "technical_project" || intent === "memory_lookup" || intent === "date_lookup" || intent === "general",
    date: parsed.dateTokens.length > 0,
    canon: intent !== "casual_contact" || termSet.has("canon"),
    codingLessons: intent === "technical_project" && (termSet.has("coding") || termSet.has("lessons")),
    projectLessons: intent === "technical_project",
  };

  return {
    ...parsed,
    intent,
    shouldAutoRecall: intent !== "casual_contact",
    lanes,
    reasons,
  };
}

export function shouldAutoRecall(query) {
  return classifyRetrievalQuery(query).shouldAutoRecall;
}
