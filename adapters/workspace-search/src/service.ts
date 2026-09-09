/**
 * Shared core for the CLI host and the MCP host. It uses the public
 * `@zvec/zvec-grep` entry point only.
 *
 * Invariants:
 * - One handle for each request, closed in `finally`. Requests run one at a
 *   time, so one native index handle is open at most. The library keeps its
 *   own cross-process locks; their errors surface as they are.
 * - The model is made once here and lent to the library as `borrowed`, so
 *   only `closeService` disposes it.
 * - Never pass `endpoint`, `apiKey`, `device` or `embedding` to
 *   `createZvecGrep`. The model owns its endpoint, and a different endpoint
 *   makes the library demand a rebuild.
 * - Search never makes, rebuilds or drops an index.
 * - Filesystem index only: no database, no canon, no memory route.
 */
import { isAbsolute, resolve } from "node:path";
import { z } from "zod";
import { createZvecGrep } from "@zvec/zvec-grep";
import type {
  EmbeddingModel,
  EmbeddingModelInfo,
  IndexProgress,
  IndexResult,
  WorkspaceIndexInfo,
  WorkspaceIndexStatus,
  ZvecGrep,
  ZvecGrepContextResult,
  ZvecGrepContextRoute,
  ZvecGrepInfoResult,
} from "@zvec/zvec-grep";
import { createNemotronModel } from "./nemotron.ts";

export const ADAPTER_NAME = "athanor-workspace-search";
export const DEFAULT_LIMIT = 5;
export const MAX_LIMIT = 50;
const MAX_CONTENT_CHARS = 2000;
const MAX_TIMINGS = 24;
const SAMPLES = 5;

const text = z.string().min(1).max(1024);
const queryText = z.string().min(1).max(4000);
const queryList = z.array(queryText).min(1).max(32);
const pathList = z.array(text).min(1).max(128);
const rootText = text.describe("Absolute path of the workspace root.");
const timeValue = z
  .union([z.number(), z.string().min(1)])
  .describe("Epoch milliseconds or a date string.");
const flag = (note: string) => z.boolean().describe(note).optional();
const whole = (max: number) => z.number().int().min(1).max(max);

const scopeShape = {
  globs: pathList.describe("Path rules, such as ['src/**', '!src/gen/**'].").optional(),
  insensitiveGlobs: pathList.optional(),
  fileTypes: pathList.describe("File-type names of ripgrep, such as ['ts'].").optional(),
  excludedFileTypes: pathList.optional(),
  hidden: flag("Include dotfiles. Omit this field to exclude them."),
  noIgnore: flag("Skip the ignore-file rules. Omit this field to honour .gitignore."),
  ignoreFiles: pathList.optional(),
  maxDepth: z.number().int().min(1).optional(),
  maxFileSizeBytes: z.number().int().min(1).optional(),
  follow: flag("Follow symbolic links."),
  embeddingConcurrency: whole(64)
    .describe(
      "Warning: one local endpoint serves every request. Omit this field to keep the provider default of 1.",
    )
    .optional(),
};

export const searchShape = {
  root: rootText,
  query: queryText.describe("One hybrid query group, lexical plus semantic.").optional(),
  queries: queryList.describe("Several hybrid query groups.").optional(),
  fts: queryList.describe("Lexical-only groups. Ranked, never exhaustive.").optional(),
  vector: queryList.describe("Semantic-only groups.").optional(),
  fuse: flag("Fuse every group into one ranked plan."),
  limit: whole(MAX_LIMIT)
    .describe(`Items for each group. Default ${DEFAULT_LIMIT}.`)
    .optional(),
  preferSymbol: flag("Prefer a whole indexed symbol."),
  symbolTypes: z
    .array(z.enum(["module", "class", "interface", "function", "value", "alias"]))
    .min(1)
    .optional(),
  modifiedAfter: timeValue.optional(),
  modifiedBefore: timeValue.optional(),
  trace: flag("Add the recall trace and the ranking trace of each hit."),
  autoUpdate: flag(
    "Refresh the index inline before the search. The call then blocks and takes a write lock. No background refresh exists.",
  ),
  ...scopeShape,
};

export const indexShape = {
  root: rootText,
  rebuild: flag("Warning: this discards the existing index and builds it again."),
  resetPaths: flag("Replace the recorded root paths."),
  ...scopeShape,
};

export const statusShape = {
  root: rootText,
  includeStatus: flag("Run the filesystem freshness scan. Default true."),
};

export const searchInput = z.object(searchShape);
export const indexInput = z.object(indexShape);
export const statusInput = z.object(statusShape);

export type SearchInput = z.infer<typeof searchInput>;
export type IndexInput = z.infer<typeof indexInput>;
export type StatusInput = z.infer<typeof statusInput>;
export type IndexHooks = {
  onProgress?: (progress: IndexProgress) => void;
  signal?: AbortSignal;
};

export class WorkspaceSearchError extends Error {
  code: string;
  details: Record<string, unknown>;

  constructor(code: string, message: string, details: Record<string, unknown> = {}) {
    super(message);
    this.name = "WorkspaceSearchError";
    this.code = code;
    this.details = details;
  }
}

export function errorPayload(error: unknown): Record<string, unknown> {
  const converted = asError(error);
  return { code: converted.code, message: converted.message, ...converted.details };
}

/** Stdout can carry JSON-RPC frames, so every log line goes to stderr. */
export function logToStderr(message: string): void {
  process.stderr.write(`[${ADAPTER_NAME}] ${message}\n`);
}

let model: EmbeddingModel | undefined;
let tail: Promise<unknown> = Promise.resolve();

/**
 * Every schema key uses the name of the library option, so a validated input
 * spreads straight into the call. The lines after the spread hold the values
 * this adapter decides: the query routes, the bounded limit and the times.
 */
export function search(input: SearchInput): Promise<Record<string, unknown>> {
  const limit = input.limit ?? DEFAULT_LIMIT;
  const routes = [
    ...(input.fts ?? []).map((query) => ({ mode: "fts" as const, query })),
    ...(input.vector ?? []).map((query) => ({ mode: "vector" as const, query })),
  ];
  requireQuery(input, routes);
  return serial(() =>
    withGrep(input.root, async (grep, root, modelInfo) => {
      const info = await grep.info({ root, includeStatus: false });
      requireIndexed(info);
      const result = await grep.context({
        ...input,
        root,
        routes: routes.length > 0 ? routes : undefined,
        limit,
        autoUpdate: input.autoUpdate === true,
        modifiedAfter: epochMs(input.modifiedAfter),
        modifiedBefore: epochMs(input.modifiedBefore),
      });
      return searchPayload(result, info, modelInfo, limit, input.autoUpdate === true);
    }),
  );
}

export function index(
  input: IndexInput,
  hooks: IndexHooks = {},
): Promise<Record<string, unknown>> {
  return serial(() =>
    withGrep(input.root, async (grep, root, modelInfo) => {
      const before = await grep.info({ root, includeStatus: false });
      const result = await grep.index({
        ...input,
        ...hooks,
        root,
        rootPaths: [root],
        rebuild: input.rebuild === true,
        resetPaths: input.resetPaths === true,
      });
      const after = await grep.info({ root, includeStatus: false });
      if (result.filesFailed > 0) {
        throw new WorkspaceSearchError("INDEX_INCOMPLETE", "Some files remain unindexed. Inspect status and retry the index command.", {
          result: indexPayload(result, before, after, modelInfo, input.rebuild === true),
        });
      }
      return indexPayload(result, before, after, modelInfo, input.rebuild === true);
    }),
  );
}

export function status(input: StatusInput): Promise<Record<string, unknown>> {
  return serial(() =>
    withGrep(input.root, async (grep, root, modelInfo) => {
      const info = await grep.info({
        root,
        includeStatus: input.includeStatus !== false,
      });
      return statusPayload(info, modelInfo);
    }),
  );
}

/** Queued, so work in flight finishes before the model is disposed. */
export function closeService(): Promise<void> {
  return serial(async () => {
    await model?.dispose();
    model = undefined;
  });
}

function serial<T>(operation: () => Promise<T>): Promise<T> {
  const result = tail.then(operation, operation);
  tail = result.then(noop, noop);
  return result;
}

function noop(): void {}

async function withGrep<T>(
  requestedRoot: string,
  operation: (grep: ZvecGrep, root: string, model: EmbeddingModelInfo) => Promise<T>,
): Promise<T> {
  if (!isAbsolute(requestedRoot)) {
    throw new WorkspaceSearchError(
      "ROOT_NOT_ABSOLUTE",
      `The workspace root must be an absolute path. Received: ${requestedRoot}`,
    );
  }
  const root = resolve(requestedRoot);
  model ??= createNemotronModel();
  const grep = await createZvecGrep({
    root,
    embeddingModel: model,
    embeddingModelOwnership: "borrowed",
  });
  try {
    return await operation(grep, root, model.info);
  } catch (error) {
    throw asError(error);
  } finally {
    await grep.close();
  }
}

function requireQuery(input: SearchInput, routes: ZvecGrepContextRoute[]): void {
  if (!input.query && (input.queries?.length ?? 0) === 0 && routes.length === 0) {
    throw new WorkspaceSearchError(
      "EMPTY_QUERY",
      "Give at least one of query, queries, fts or vector.",
    );
  }
}

function requireIndexed(info: ZvecGrepInfoResult): void {
  if (info.indexed) {
    return;
  }
  throw new WorkspaceSearchError(
    "INDEX_MISSING",
    `No usable index for ${info.root}. A search never makes one. Build it first: ` +
      `run the CLI command "index --root ${info.root}", or call zvec_grep_index with that root.`,
    {
      root: info.root,
      indexPolicy: info.indexPolicy,
      indexPath: info.indexPath,
      libraryHint: info.suggestion,
    },
  );
}

function epochMs(value: number | string | undefined): number | undefined {
  if (value === undefined) {
    return undefined;
  }
  const parsed = typeof value === "number" ? value : Date.parse(value);
  if (!Number.isFinite(parsed)) {
    throw new WorkspaceSearchError("INVALID_TIME", `Cannot read the time ${value}.`);
  }
  return parsed;
}

function searchPayload(
  result: ZvecGrepContextResult,
  info: ZvecGrepInfoResult,
  modelInfo: EmbeddingModelInfo,
  limit: number,
  inlineRefresh: boolean,
): Record<string, unknown> {
  const items = result.items.map(({ content, file, excerptRange, ...rest }) => ({
    ...rest,
    path: file.absolutePath,
    relativePath: file.relativePath,
    lines: lineLabel(excerptRange ?? rest.range),
    content: content.slice(0, MAX_CONTENT_CHARS),
    contentTruncated: content.length > MAX_CONTENT_CHARS,
  }));
  return {
    root: result.root,
    query: result.query,
    source: result.source,
    coverage: result.coverage,
    freshness: {
      returnedItems: result.items.some((item) => item.status === "possibly_stale") ? "possibly_stale" : "fresh",
      workspace: inlineRefresh ? "refreshed_inline" : "not_scanned",
    },
    inlineRefresh,
    backgroundRefresh: false,
    limitPerGroup: limit,
    itemCount: items.length,
    items,
    index: indexIdentity(info.workspaceIndex),
    model: modelInfo,
    diagnostics: {
      ...result.diagnostics,
      timings: result.diagnostics.timings?.slice(0, MAX_TIMINGS),
    },
  };
}

function lineLabel(range: ZvecGrepContextResult["items"][number]["range"]): string | undefined {
  if (range.kind !== "text") {
    return undefined;
  }
  return `${range.startLine}-${range.endLine}`;
}

function indexPayload(
  result: IndexResult,
  before: ZvecGrepInfoResult,
  after: ZvecGrepInfoResult,
  modelInfo: EmbeddingModelInfo,
  rebuild: boolean,
): Record<string, unknown> {
  const { timings, scanDiagnostics, filesPending, ...counts } = result;
  return {
    root: after.root,
    action: rebuild ? "rebuild" : before.indexed ? "update" : "create",
    indexed: after.indexed,
    indexPolicy: after.indexPolicy,
    indexPath: after.indexPath,
    ...counts,
    pendingFilesProcessed: filesPending,
    skipped: scanDiagnostics && {
      ...scanDiagnostics,
      skippedSamples: scanDiagnostics.skippedSamples.slice(0, SAMPLES),
    },
    index: indexIdentity(after.workspaceIndex),
    model: modelInfo,
    timings: timings?.slice(0, MAX_TIMINGS),
  };
}

function statusPayload(
  info: ZvecGrepInfoResult,
  modelInfo: EmbeddingModelInfo,
): Record<string, unknown> {
  return {
    root: info.root,
    indexed: info.indexed,
    indexPolicy: info.indexPolicy,
    source: info.source,
    home: info.home,
    indexPath: info.indexPath,
    index: indexIdentity(info.workspaceIndex),
    model: modelInfo,
    modelMatchesIndex: modelMatchesIndex(info.workspaceIndex, modelInfo),
    scan: info.status ? scanSummary(info.status) : { performed: false },
    backgroundRefresh: false,
    watcher: false,
    libraryHint: info.suggestion,
  };
}

/** Counts plus small samples. A full file table never goes into the payload. */
function scanSummary(status: WorkspaceIndexStatus): Record<string, unknown> {
  const { pendingFiles, failedFiles, addedFiles, modifiedFiles, deletedFiles, ...counts } =
    status;
  const changes =
    counts.filesPending + counts.filesAdded + counts.filesModified + counts.filesDeleted + counts.filesFailed;
  return {
    performed: true,
    freshness: changes === 0 ? "up_to_date" : "changes_pending",
    counts,
    samples: {
      pending: fileSample(pendingFiles),
      failed: fileSample(failedFiles),
      added: fileSample(addedFiles),
      modified: fileSample(modifiedFiles),
      deleted: fileSample(deletedFiles),
    },
  };
}

function fileSample(
  files: WorkspaceIndexStatus["pendingFiles"],
): Record<string, unknown> {
  return {
    total: files.length,
    paths: files.slice(0, SAMPLES).map((file) => file.relativePath),
    errors: files
      .slice(0, SAMPLES)
      .map((file) => file.indexStatus?.error)
      .filter((message) => message !== undefined),
  };
}

function indexIdentity(
  info: WorkspaceIndexInfo | undefined,
): Record<string, unknown> | null {
  if (!info) {
    return null;
  }
  const { rootPaths, ...rest } = info;
  return {
    ...rest,
    rootPaths: rootPaths.slice(0, SAMPLES).map((rootPath) => rootPath.absolutePath),
    rootPathCount: rootPaths.length,
  };
}

function modelMatchesIndex(
  info: WorkspaceIndexInfo | undefined,
  modelInfo: EmbeddingModelInfo,
): boolean | null {
  const embedding = info?.embedding;
  if (!embedding) {
    return null;
  }
  return (
    embedding.provider === modelInfo.provider &&
    embedding.model === modelInfo.name &&
    embedding.dimension === modelInfo.dimension &&
    embedding.metric === modelInfo.metric
  );
}

const INDEX_MISSING_CODES = [
  "ZVEC_GREP.ENGINE.SERVICE.WORKSPACE_INDEX_NOT_FOUND",
  "ZVEC_GREP.ENGINE.SERVICE.INDEX_MISSING",
];

/** Keeps the real code and detail of a library error or a provider error. */
function asError(error: unknown): WorkspaceSearchError {
  if (error instanceof WorkspaceSearchError) {
    return error;
  }
  const source = (error ?? {}) as Record<string, unknown>;
  const details: Record<string, unknown> = {};
  for (const key of ["code", "context", "detail"]) {
    if (typeof source[key] === "string") {
      details[key === "code" ? "libraryCode" : key] = source[key];
    }
  }
  const message = error instanceof Error ? error.message : String(error);
  const libraryCode = details.libraryCode;
  if (typeof libraryCode === "string" && INDEX_MISSING_CODES.includes(libraryCode)) {
    return new WorkspaceSearchError("INDEX_MISSING", message, details);
  }
  return new WorkspaceSearchError(
    libraryCode === undefined ? "UNEXPECTED_ERROR" : "LIBRARY_ERROR",
    message,
    details,
  );
}
