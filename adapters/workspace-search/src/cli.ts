#!/usr/bin/env node
/**
 * CLI host. Same service and same schemas as the MCP host.
 *
 * Every command prints one JSON object on stdout. Exit codes: 0 for success,
 * 1 for a failed operation, 2 for bad arguments. The `server` command speaks
 * JSON-RPC on stdout, so its own failures go to stderr.
 */
import { resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { z } from "zod";
import type { IndexProgress } from "@zvec/zvec-grep";
import {
  closeService,
  errorPayload,
  index,
  indexInput,
  logToStderr,
  search,
  searchInput,
  status,
  statusInput,
  DEFAULT_LIMIT,
  MAX_LIMIT,
  WorkspaceSearchError,
} from "./service.ts";

const PROGRESS_INTERVAL_MS = 500;
const SCOPE_SPEC =
  "globs:list insensitiveGlobs:list fileTypes:list excludedFileTypes:list" +
  " hidden:flag noIgnore:flag ignoreFiles:list maxDepth:num maxFileSizeBytes:num" +
  " follow:flag embeddingConcurrency:num";
const SPECS: Record<string, string> = {
  search:
    "root:text query:text queries:list fts:list vector:list fuse:flag limit:num" +
    " preferSymbol:flag symbolTypes:list modifiedAfter:time modifiedBefore:time" +
    ` trace:flag autoUpdate:flag ${SCOPE_SPEC}`,
  index: `root:text rebuild:flag resetPaths:flag ${SCOPE_SPEC}`,
  status: "root:text includeStatus:flag",
};

export async function main(argv: readonly string[]): Promise<number> {
  const [command = "help", ...rest] = argv;
  if (command === "help" || command.startsWith("--help") || rest.includes("--help")) {
    process.stdout.write(`${helpText(command)}\n`);
    return 0;
  }
  if (command === "server") {
    return await runServer(rest);
  }
  try {
    const data = await runCommand(command, rest);
    emit({ ok: true, command, data });
    return 0;
  } catch (error) {
    const payload = errorPayload(error);
    emit({ ok: false, command, error: payload });
    return payload.code === "INVALID_ARGUMENTS" ? 2 : 1;
  } finally {
    await closeService();
  }
}

function runCommand(
  command: string,
  argv: readonly string[],
): Promise<Record<string, unknown>> {
  if (command === "search") {
    return search(validate(searchInput, command, argv));
  }
  if (command === "index") {
    return index(validate(indexInput, command, argv), indexHooks());
  }
  if (command === "status") {
    return status(validate(statusInput, command, argv));
  }
  throw usageError(`Unknown command: ${command}. Use search, index, status or server.`);
}

async function runServer(argv: readonly string[]): Promise<number> {
  try {
    if (argv.length > 0) {
      throw usageError(`The server command takes no arguments. Received ${argv[0]}.`);
    }
    // Imported here, so a search never loads the MCP SDK.
    const { startStdioServer } = await import("./mcp.ts");
    await startStdioServer();
    return 0;
  } catch (error) {
    const payload = errorPayload(error);
    logToStderr(JSON.stringify({ ok: false, command: "server", error: payload }));
    return payload.code === "INVALID_ARGUMENTS" ? 2 : 1;
  }
}

function validate<Schema extends z.ZodType>(
  schema: Schema,
  command: string,
  argv: readonly string[],
): z.output<Schema> {
  const result = schema.safeParse(parseFlags(SPECS[command], argv));
  if (result.success) {
    return result.data;
  }
  const details = result.error.issues
    .map((issue) => `${issue.path.join(".") || "input"}: ${issue.message}`)
    .join("; ");
  throw usageError(`Invalid arguments. ${details}`);
}

/** Accepts `--file-types x`, `--fileTypes=x`, and repeated list flags. */
function parseFlags(spec: string, argv: readonly string[]): Record<string, unknown> {
  const kinds = new Map(
    spec.split(" ").map((entry) => entry.split(":") as [string, string]),
  );
  const values: Record<string, unknown> = { root: process.cwd() };
  let position = 0;
  while (position < argv.length) {
    const token = argv[position];
    position += 1;
    const separator = token.indexOf("=");
    const name = separator === -1 ? token : token.slice(0, separator);
    const inline = separator === -1 ? undefined : token.slice(separator + 1);
    const key = camelCase(name);
    const kind = token.startsWith("--") ? kinds.get(key) : undefined;
    if (kind === undefined) {
      throw usageError(`Unknown argument: ${token}. Add --help for the flag list.`);
    }
    if (kind === "flag") {
      if (inline !== undefined && inline !== "true" && inline !== "false") {
        throw usageError(`The flag ${name} accepts only true or false.`);
      }
      values[key] = inline !== "false";
      continue;
    }
    const raw = inline ?? argv[position];
    if (inline === undefined) {
      position += 1;
    }
    if (raw === undefined) {
      throw usageError(`The flag ${name} needs a value.`);
    }
    values[key] = convert(kind, raw, values[key]);
  }
  values.root = resolve(String(values.root));
  return values;
}

function convert(kind: string, raw: string, current: unknown): unknown {
  if (kind === "list") {
    return [...(Array.isArray(current) ? (current as string[]) : []), raw];
  }
  if (kind === "num") {
    return Number(raw);
  }
  if (kind === "time") {
    return Number.isFinite(Number(raw)) ? Number(raw) : raw;
  }
  return raw;
}

function camelCase(name: string): string {
  return name
    .replace(/^--/, "")
    .replace(/-([a-z])/g, (_match, letter: string) => letter.toUpperCase());
}

function usageError(message: string): WorkspaceSearchError {
  return new WorkspaceSearchError("INVALID_ARGUMENTS", message);
}

/**
 * Progress lines go to stderr. One interrupt aborts the index run through the
 * library, which then closes its own handles.
 */
function indexHooks(): { onProgress: (progress: IndexProgress) => void; signal: AbortSignal } {
  const controller = new AbortController();
  process.once("SIGINT", () => {
    logToStderr("interrupt received: stopping the index run");
    controller.abort(new Error("interrupted by SIGINT"));
  });
  let last = 0;
  return {
    signal: controller.signal,
    onProgress: (progress) => {
      const now = Date.now();
      if (progress.phase !== "done" && now - last < PROGRESS_INTERVAL_MS) {
        return;
      }
      last = now;
      logToStderr(
        `${progress.phase} ${progress.filesIndexed ?? 0}/${progress.filesTotal ?? 0}`,
      );
    },
  };
}

function emit(payload: Record<string, unknown>): void {
  process.stdout.write(`${JSON.stringify(payload, null, 2)}\n`);
}

function helpText(command: string): string {
  const spec = SPECS[command];
  if (spec) {
    const flags = spec
      .split(" ")
      .map((entry) => {
        const [name, kind] = entry.split(":");
        return kind === "flag" ? `  --${name}` : `  --${name} <${kind}>`;
      })
      .join("\n");
    return `Usage: workspace-search ${command} [flags]\n\n${flags}`;
  }
  return [
    "Workspace search for one local repository.",
    "It uses the public zvec-grep library and a local embedding model.",
    "",
    "Usage: workspace-search <command> [flags]",
    "",
    "  search   Search an existing index. It returns file and line evidence.",
    "  index    Build or update the index of one root. This writes to disk.",
    "  status   Show the index identity, the model identity and the freshness.",
    "  server   Start the MCP server on stdio.",
    "  help     Show this text.",
    "",
    "Add --help after a command to see the flags of that command.",
    "Flag names accept both --file-types and --fileTypes. Repeat a list flag.",
    `A search returns at most --limit items for each group, ${DEFAULT_LIMIT} by default, ${MAX_LIMIT} at most.`,
    "",
    "What this tool does:",
    "  A search never makes an index. It fails with the code INDEX_MISSING.",
    "  A rebuild happens only with --rebuild. No delete command exists.",
    "  No daemon and no file watcher run here.",
    "  The index changes only during an index run.",
    "  A search with --auto-update refreshes the index inline first.",
    "  That refresh blocks the search and takes a write lock.",
    "  Results carry a freshness state of fresh or possibly_stale.",
    "  This tool indexes repository files. It never queries the Athanor Host or AKASHA.",
    "",
    "Exit codes: 0 success, 1 failed operation, 2 bad arguments.",
  ].join("\n");
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = await main(process.argv.slice(2));
}
