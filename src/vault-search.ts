import { lstat, readFile, readdir } from "node:fs/promises";
import path from "node:path";

const ROOM_MARKER = ".solarisael-room.json";
const DEFAULT_MAX_FILE_BYTES = 512 * 1024;
const DEFAULT_MAX_FILES = 5_000;
const MAX_RESULTS = 8;
const CACHE_TTL_MS = 30_000;
const CHUNK_CHARS = 6_000;
const CHUNK_OVERLAP = 400;
const EXCERPT_CHARS = 900;

const ELIGIBLE_EXTENSIONS = new Set([".md", ".markdown", ".json", ".jsonl", ".txt"]);
const IGNORED_DIRECTORIES = new Set([
  ".git", ".hg", ".svn", ".cache", ".idea", ".next", ".nuxt", ".turbo",
  "node_modules", "vendor", "dist", "build", "coverage", "target", "out",
]);
const IGNORED_FILES = [
  /^\.env(?:\.|$)/i,
  /(?:^|[-_.])(secret|secrets|credential|credentials)(?:[-_.]|$)/i,
  /\.(?:pem|key|p12|pfx|keystore)$/i,
  /^(?:package-lock\.json|bun\.lockb?|pnpm-lock\.yaml|yarn\.lock)$/i,
];
const STOPWORDS = new Set([
  "a", "an", "and", "are", "as", "at", "be", "but", "by", "do", "for", "from", "how", "i", "in", "is", "it", "of", "on", "or", "that", "the", "this", "to", "was", "we", "what", "when", "where", "which", "with", "you",
  "a", "ao", "aos", "as", "com", "como", "da", "das", "de", "do", "dos", "e", "em", "eu", "na", "nas", "no", "nos", "o", "os", "ou", "para", "por", "que", "se", "um", "uma",
]);

const FIELD_WEIGHTS = {
  path: 4.2,
  title: 3.8,
  heading: 3.4,
  keys: 2.6,
  tags: 2.8,
  body: 1,
  metadata: 1.4,
} as const;
const FIELD_B = {
  path: 0.2,
  title: 0.25,
  heading: 0.3,
  keys: 0.45,
  tags: 0.3,
  body: 0.75,
  metadata: 0.5,
} as const;

type FieldName = keyof typeof FIELD_WEIGHTS;
type FieldText = Record<FieldName, string>;
type FieldTerms = Record<FieldName, Map<string, number>>;

type VaultDocument = {
  sourcePath: string;
  title: string;
  headingPath: string;
  body: string;
  fields: FieldText;
  terms: FieldTerms;
  lengths: Record<FieldName, number>;
};

type IgnoreRule = { negated: boolean; directoryOnly: boolean; regex: RegExp };
type VaultConfig = {
  roots: string[];
  ignore: string[];
  maxFileBytes: number;
  maxFiles: number;
};
type VaultIndex = {
  builtAt: number;
  roots: string[];
  documents: VaultDocument[];
  scannedFiles: number;
  warnings: string[];
};

const cache = new Map<string, VaultIndex>();

function normalizedPath(value: string) {
  return value.replaceAll("\\", "/");
}

function boundedInteger(value: unknown, fallback: number, minimum: number, maximum: number) {
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed >= minimum && parsed <= maximum ? parsed : fallback;
}

async function loadConfig(roomDir: string): Promise<VaultConfig> {
  let marker: Record<string, unknown> = {};
  try {
    const parsed = JSON.parse(await readFile(path.join(roomDir, ROOM_MARKER), "utf8"));
    if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) marker = parsed;
  } catch {
    // The room contract validates the marker elsewhere. Search remains useful for tests and recovery.
  }

  const configuredRoots = Array.isArray(marker.vaultRoots)
    ? marker.vaultRoots.filter((root): root is string => typeof root === "string" && root.trim().length > 0)
    : [];
  const roots = (configuredRoots.length > 0 ? configuredRoots : [roomDir])
    .map((root) => path.resolve(roomDir, root))
    .filter((root, index, all) => all.indexOf(root) === index);
  const ignore = Array.isArray(marker.vaultIgnore)
    ? marker.vaultIgnore.filter((rule): rule is string => typeof rule === "string" && rule.trim().length > 0)
    : [];

  return {
    roots,
    ignore,
    maxFileBytes: boundedInteger(marker.vaultMaxFileBytes, DEFAULT_MAX_FILE_BYTES, 16 * 1024, 4 * 1024 * 1024),
    maxFiles: boundedInteger(marker.vaultMaxFiles, DEFAULT_MAX_FILES, 1, 50_000),
  };
}

function escapeRegex(value: string) {
  return value.replace(/[|\\{}()[\]^$+?.]/g, "\\$&");
}

function globRegex(pattern: string) {
  const normalized = normalizedPath(pattern.trim()).replace(/^\.\//, "");
  const anchored = normalized.startsWith("/");
  const body = anchored ? normalized.slice(1) : normalized;
  let source = "";
  for (let index = 0; index < body.length; index += 1) {
    const char = body[index];
    if (char === "*") {
      if (body[index + 1] === "*") {
        source += ".*";
        index += 1;
      } else {
        source += "[^/]*";
      }
    } else if (char === "?") {
      source += "[^/]";
    } else {
      source += escapeRegex(char);
    }
  }
  return new RegExp(anchored ? `^${source}(?:/|$)` : `(?:^|/)${source}(?:/|$)`, "i");
}

function parseIgnoreRules(lines: string[]): IgnoreRule[] {
  const rules: IgnoreRule[] = [];
  for (const raw of lines) {
    let value = raw.trim();
    if (!value || value.startsWith("#")) continue;
    const negated = value.startsWith("!");
    if (negated) value = value.slice(1);
    const directoryOnly = value.endsWith("/");
    if (directoryOnly) value = value.slice(0, -1);
    if (!value) continue;
    rules.push({ negated, directoryOnly, regex: globRegex(value) });
  }
  return rules;
}

async function rootIgnoreRules(root: string, configured: string[]) {
  let gitignore: string[] = [];
  try {
    gitignore = (await readFile(path.join(root, ".gitignore"), "utf8")).split(/\r?\n/);
  } catch {
    // A Vault root does not need to be a Git repository.
  }
  return parseIgnoreRules([...gitignore, ...configured]);
}

function ignoredByRules(relativePath: string, isDirectory: boolean, rules: IgnoreRule[]) {
  let ignored = false;
  for (const rule of rules) {
    if (rule.directoryOnly && !isDirectory) continue;
    if (rule.regex.test(relativePath)) ignored = !rule.negated;
  }
  return ignored;
}

function ignoredFile(name: string) {
  return IGNORED_FILES.some((pattern) => pattern.test(name));
}

async function collectFiles(config: VaultConfig) {
  const files: Array<{ root: string; absolute: string }> = [];
  const warnings: string[] = [];
  let limitReached = false;

  for (const root of config.roots) {
    let rootStat;
    try {
      rootStat = await lstat(root);
    } catch {
      warnings.push(`Vault root unavailable: ${normalizedPath(root)}`);
      continue;
    }
    if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) {
      warnings.push(`Vault root is not a readable directory: ${normalizedPath(root)}`);
      continue;
    }
    const rules = await rootIgnoreRules(root, config.ignore);

    const walk = async (directory: string): Promise<void> => {
      if (limitReached) return;
      let entries;
      try {
        entries = await readdir(directory, { withFileTypes: true });
      } catch {
        warnings.push(`Vault directory unreadable: ${normalizedPath(directory)}`);
        return;
      }
      entries.sort((left, right) => left.name.localeCompare(right.name));
      for (const entry of entries) {
        if (limitReached) return;
        const absolute = path.join(directory, entry.name);
        const relative = normalizedPath(path.relative(root, absolute));
        if (entry.isSymbolicLink()) continue;
        if (entry.isDirectory()) {
          if (IGNORED_DIRECTORIES.has(entry.name.toLowerCase())) continue;
          if (ignoredByRules(relative, true, rules)) continue;
          await walk(absolute);
          continue;
        }
        if (!entry.isFile() || ignoredFile(entry.name) || ignoredByRules(relative, false, rules)) continue;
        if (!ELIGIBLE_EXTENSIONS.has(path.extname(entry.name).toLowerCase())) continue;
        files.push({ root, absolute });
        if (files.length >= config.maxFiles) {
          limitReached = true;
          warnings.push(`Vault file limit reached (${config.maxFiles}); results cover only the scanned prefix.`);
        }
      }
    };
    await walk(root);
    if (limitReached) break;
  }
  return { files, warnings };
}

function normalizedText(value: unknown) {
  return String(value ?? "").normalize("NFKD").replace(/\p{M}/gu, "").toLowerCase();
}

function tokens(value: unknown) {
  const found = normalizedText(value).match(/[\p{L}\p{N}_:+#./-]+/gu) || [];
  const result: string[] = [];
  for (const raw of found) {
    const token = raw.replace(/^[_:+#./-]+|[_:+#./-]+$/g, "");
    if (!token) continue;
    result.push(token);
    for (const part of token.split(/[_:+#./-]+/)) {
      if (part && part !== token) result.push(part);
    }
  }
  return result;
}

function termFrequency(value: string) {
  const frequencies = new Map<string, number>();
  for (const term of tokens(value)) frequencies.set(term, (frequencies.get(term) || 0) + 1);
  return frequencies;
}

function makeDocument(input: {
  sourcePath: string;
  pathText?: string;
  title: string;
  headingPath?: string;
  body: string;
  keys?: string;
  tags?: string;
  metadata?: string;
}) {
  const fields: FieldText = {
    path: input.pathText || input.sourcePath,
    title: input.title,
    heading: input.headingPath || "",
    keys: input.keys || "",
    tags: input.tags || "",
    body: input.body,
    metadata: input.metadata || "",
  };
  const terms = Object.fromEntries(
    Object.entries(fields).map(([field, value]) => [field, termFrequency(value)]),
  ) as FieldTerms;
  const lengths = Object.fromEntries(
    Object.entries(terms).map(([field, frequencies]) => [field, [...frequencies.values()].reduce((sum, count) => sum + count, 0)]),
  ) as Record<FieldName, number>;
  return {
    sourcePath: input.sourcePath,
    title: input.title,
    headingPath: input.headingPath || "",
    body: input.body,
    fields,
    terms,
    lengths,
  } satisfies VaultDocument;
}

function splitBody(body: string) {
  const trimmed = body.trim();
  if (trimmed.length <= CHUNK_CHARS) return trimmed ? [trimmed] : [];
  const chunks: string[] = [];
  for (let start = 0; start < trimmed.length; start += CHUNK_CHARS - CHUNK_OVERLAP) {
    chunks.push(trimmed.slice(start, start + CHUNK_CHARS).trim());
    if (start + CHUNK_CHARS >= trimmed.length) break;
  }
  return chunks.filter(Boolean);
}

function markdownDocuments(sourcePath: string, pathText: string, content: string) {
  const title = path.basename(sourcePath, path.extname(sourcePath));
  const documents: VaultDocument[] = [];
  let body = content;
  if (body.startsWith("---\n") || body.startsWith("---\r\n")) {
    const match = /^---\r?\n([\s\S]*?)\r?\n---\r?\n?/.exec(body);
    if (match) {
      documents.push(makeDocument({ sourcePath, pathText, title, headingPath: "__frontmatter__", body: match[1], keys: match[1].split(/\r?\n/).map((line) => line.split(":", 1)[0]).join(" ") }));
      body = body.slice(match[0].length);
    }
  }

  const headings: string[] = [];
  let currentHeading = "__preamble__";
  let current: string[] = [];
  const flush = () => {
    const headingPath = currentHeading === "__preamble__" ? currentHeading : headings.filter(Boolean).join(" > ");
    for (const [index, chunk] of splitBody(current.join("\n")).entries()) {
      documents.push(makeDocument({
        sourcePath,
        pathText,
        title,
        headingPath: index === 0 ? headingPath : `${headingPath} [${index + 1}]`,
        body: chunk,
      }));
    }
    current = [];
  };

  for (const line of body.split(/\r?\n/)) {
    const heading = /^(#{1,6})\s+(.+?)\s*$/.exec(line);
    if (!heading) {
      current.push(line);
      continue;
    }
    flush();
    const level = heading[1].length;
    headings.length = level;
    headings[level - 1] = heading[2].trim();
    currentHeading = heading[2].trim();
  }
  flush();
  return documents;
}

function flattenJson(value: unknown, pointer = "$") {
  const keys: string[] = [];
  const values: string[] = [];
  const visit = (entry: unknown, current: string) => {
    if (entry === null || typeof entry === "boolean" || typeof entry === "number" || typeof entry === "string") {
      values.push(`${current}: ${String(entry)}`);
      return;
    }
    if (Array.isArray(entry)) {
      entry.forEach((item, index) => visit(item, `${current}/${index}`));
      return;
    }
    if (entry && typeof entry === "object") {
      for (const [key, child] of Object.entries(entry)) {
        keys.push(key);
        visit(child, `${current}/${key.replaceAll("~", "~0").replaceAll("/", "~1")}`);
      }
    }
  };
  visit(value, pointer);
  return { keys: keys.join(" "), body: values.join("\n") };
}

function jsonDocuments(sourcePath: string, pathText: string, value: unknown, headingPrefix = "") {
  const title = path.basename(sourcePath, path.extname(sourcePath));
  const records: Array<{ heading: string; value: unknown }> = [];
  if (Array.isArray(value)) {
    value.forEach((entry, index) => records.push({ heading: `${headingPrefix}/${index}` || `/${index}`, value: entry }));
  } else if (value && typeof value === "object") {
    for (const [key, entry] of Object.entries(value)) {
      const escaped = key.replaceAll("~", "~0").replaceAll("/", "~1");
      records.push({ heading: `${headingPrefix}/${escaped}` || `/${escaped}`, value: entry });
    }
  } else {
    records.push({ heading: headingPrefix || "$", value });
  }
  return records.flatMap(({ heading, value: record }) => {
    const flattened = flattenJson(record, heading || "$");
    return splitBody(flattened.body).map((body, index) => makeDocument({
      sourcePath,
      pathText,
      title,
      headingPath: index === 0 ? heading : `${heading} [${index + 1}]`,
      body,
      keys: flattened.keys,
      metadata: heading,
    }));
  });
}

function jsonlDocuments(sourcePath: string, pathText: string, content: string, warnings: string[]) {
  const documents: VaultDocument[] = [];
  let malformed = 0;
  for (const [index, line] of content.split(/\r?\n/).entries()) {
    if (!line.trim()) continue;
    try {
      documents.push(...jsonDocuments(sourcePath, pathText, JSON.parse(line), `line:${index + 1}`));
    } catch {
      malformed += 1;
    }
  }
  if (malformed > 0) warnings.push(`${sourcePath}: skipped ${malformed} malformed JSONL record${malformed === 1 ? "" : "s"}.`);
  return documents;
}

async function parseFile(file: { root: string; absolute: string }, maxFileBytes: number, warnings: string[]) {
  const stat = await lstat(file.absolute);
  const sourcePath = normalizedPath(file.absolute);
  const pathText = normalizedPath(path.relative(file.root, file.absolute));
  if (stat.size > maxFileBytes) {
    warnings.push(`${sourcePath}: skipped file larger than ${maxFileBytes} bytes.`);
    return [];
  }
  const content = await readFile(file.absolute, "utf8");
  const extension = path.extname(file.absolute).toLowerCase();
  if (extension === ".md" || extension === ".markdown") return markdownDocuments(sourcePath, pathText, content);
  if (extension === ".jsonl") return jsonlDocuments(sourcePath, pathText, content, warnings);
  if (extension === ".json") {
    try {
      return jsonDocuments(sourcePath, pathText, JSON.parse(content));
    } catch {
      warnings.push(`${sourcePath}: skipped malformed JSON.`);
      return [];
    }
  }
  return splitBody(content).map((body, index) => makeDocument({
    sourcePath,
    pathText,
    title: path.basename(sourcePath, extension),
    headingPath: index === 0 ? "__document__" : `__document__ [${index + 1}]`,
    body,
  }));
}

async function buildIndex(config: VaultConfig): Promise<VaultIndex> {
  const collected = await collectFiles(config);
  const documents: VaultDocument[] = [];
  for (const file of collected.files) {
    try {
      documents.push(...await parseFile(file, config.maxFileBytes, collected.warnings));
    } catch {
      collected.warnings.push(`${normalizedPath(file.absolute)}: skipped unreadable text file.`);
    }
  }
  return {
    builtAt: Date.now(),
    roots: config.roots.map(normalizedPath),
    documents,
    scannedFiles: collected.files.length,
    warnings: collected.warnings,
  };
}

function configKey(config: VaultConfig) {
  return JSON.stringify(config);
}

async function loadIndex(config: VaultConfig) {
  const key = configKey(config);
  const existing = cache.get(key);
  if (existing && Date.now() - existing.builtAt < CACHE_TTL_MS) return existing;
  const built = await buildIndex(config);
  cache.set(key, built);
  return built;
}

function queryTerms(query: string) {
  const all = [...new Set(tokens(query))];
  const meaningful = all.filter((term) => term.length > 1 && !STOPWORDS.has(term));
  return meaningful.length > 0 ? meaningful : all;
}

function quotedTerms(query: string) {
  return [...query.matchAll(/["“”]([^"“”]+)["“”]/g)].map((match) => normalizedText(match[1])).filter(Boolean);
}

function averages(documents: VaultDocument[]) {
  const result = {} as Record<FieldName, number>;
  for (const field of Object.keys(FIELD_WEIGHTS) as FieldName[]) {
    result[field] = Math.max(1, documents.reduce((sum, document) => sum + document.lengths[field], 0) / Math.max(1, documents.length));
  }
  return result;
}

function documentFrequencies(documents: VaultDocument[], terms: string[]) {
  return new Map(terms.map((term) => [
    term,
    documents.reduce((count, document) => count + (Object.values(document.terms).some((field) => field.has(term)) ? 1 : 0), 0),
  ]));
}

function excerpt(document: VaultDocument, terms: string[]) {
  const body = document.body.replace(/\s+/g, " ").trim();
  if (body.length <= EXCERPT_CHARS) return body;
  const lower = normalizedText(body);
  const positions = terms.map((term) => lower.indexOf(term)).filter((position) => position >= 0);
  const position = positions.length > 0 ? Math.min(...positions) : 0;
  const start = Math.max(0, position - Math.floor(EXCERPT_CHARS / 3));
  const clipped = body.slice(start, start + EXCERPT_CHARS).trim();
  return `${start > 0 ? "…" : ""}${clipped}${start + EXCERPT_CHARS < body.length ? "…" : ""}`;
}

function rank(index: VaultIndex, query: string) {
  const terms = queryTerms(query);
  if (terms.length === 0 || index.documents.length === 0) return [];
  const compoundTerms = terms.filter((term) => term.length >= 4 && /[_:+#./-]/.test(term));
  const avg = averages(index.documents);
  const df = documentFrequencies(index.documents, terms);
  const normalizedQuery = normalizedText(query).trim();
  const exactPhrases = [...new Set([normalizedQuery, ...quotedTerms(query)].filter((term) => term.length >= 3))];
  const totalDocuments = index.documents.length;
  const k1 = 1.2;

  return index.documents.map((document) => {
    let score = 0;
    const matchedTerms: string[] = [];
    const matchedFields = new Set<FieldName>();
    for (const term of terms) {
      let combinedTf = 0;
      for (const field of Object.keys(FIELD_WEIGHTS) as FieldName[]) {
        const tf = document.terms[field].get(term) || 0;
        if (tf <= 0) continue;
        matchedFields.add(field);
        const normalizedTf = tf / (1 - FIELD_B[field] + FIELD_B[field] * document.lengths[field] / avg[field]);
        combinedTf += FIELD_WEIGHTS[field] * normalizedTf;
      }
      if (combinedTf <= 0) continue;
      matchedTerms.push(term);
      const frequency = df.get(term) || 0;
      const idf = Math.log(1 + (totalDocuments - frequency + 0.5) / (frequency + 0.5));
      score += idf * ((k1 + 1) * combinedTf) / (k1 + combinedTf);
    }

    const exactFields = new Set<FieldName>();
    for (const phrase of exactPhrases) {
      for (const field of Object.keys(FIELD_WEIGHTS) as FieldName[]) {
        if (!phrase || !normalizedText(document.fields[field]).includes(phrase)) continue;
        exactFields.add(field);
        score += FIELD_WEIGHTS[field] * (field === "body" ? 1.5 : 2.25);
      }
    }
    const uniqueMatched = [...new Set(matchedTerms)];
    const reasons = [
      matchedFields.size > 0 ? `BM25F fields: ${[...matchedFields].join(", ")}` : "",
      exactFields.size > 0 ? `exact content fields: ${[...exactFields].join(", ")}` : "",
    ].filter(Boolean);
    return {
      document,
      score,
      matchedTerms: uniqueMatched,
      missingTerms: terms.filter((term) => !uniqueMatched.includes(term)),
      reasons,
    };
  }).filter((entry) => entry.score > 0 && (compoundTerms.length === 0 || compoundTerms.some((term) => entry.matchedTerms.includes(term))))
    .sort((left, right) => right.score - left.score || left.document.sourcePath.localeCompare(right.document.sourcePath) || left.document.headingPath.localeCompare(right.document.headingPath))
    .slice(0, MAX_RESULTS)
    .map((entry) => ({
      source_path: entry.document.sourcePath,
      title: entry.document.title,
      heading_path: entry.document.headingPath,
      sources: [entry.document.sourcePath],
      score: entry.score,
      term_coverage: entry.matchedTerms.length / Math.max(1, terms.length),
      matched_terms: entry.matchedTerms,
      missing_terms: entry.missingTerms,
      reasons: entry.reasons,
      excerpt: excerpt(entry.document, entry.matchedTerms),
    }));
}

export function clearVaultSearchCache() {
  cache.clear();
}

export async function searchVault(roomDir: string, query: string) {
  const config = await loadConfig(path.resolve(roomDir));
  const index = await loadIndex(config);
  const retrievalCandidates = rank(index, query);
  return {
    ok: true,
    query,
    found: retrievalCandidates.length > 0,
    source: "vault-files",
    authority: "vault-files",
    roots: index.roots,
    scannedFiles: index.scannedFiles,
    indexedDocuments: index.documents.length,
    retrievalCandidates,
    canonMatches: [],
    semanticChunks: [],
    contentChunks: [],
    dateMatches: [],
    queryDates: [],
    taxonomy: {
      memoryTypes: ["vault-file"],
      threadKeys: [],
      namedEntities: [],
      fileTypes: ["markdown", "json", "jsonl", "text"],
    },
    warnings: index.warnings,
  };
}
