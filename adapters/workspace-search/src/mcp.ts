/**
 * Node stdio MCP host. It calls the same service as the CLI.
 *
 * Invariants:
 * - Stdout carries JSON-RPC frames only. Logs go to stderr.
 * - The stdio pipes are the trust boundary. There is no socket, no token and
 *   no network transport here. Do not add one.
 * - A failed tool returns `isError: true`. The SDK validates the input schema
 *   first, and a bad argument also comes back as an `isError` result.
 * - Search, index and status only. No memory route and no delete route.
 */
import { readFile } from "node:fs/promises";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  ADAPTER_NAME,
  closeService,
  errorPayload,
  index,
  indexShape,
  logToStderr,
  search,
  searchShape,
  status,
  statusShape,
  type IndexInput,
  type SearchInput,
  type StatusInput,
} from "./service.ts";

const INSTRUCTIONS = [
  "This server searches one local workspace with a hybrid index of files on disk.",
  "It never queries the Athanor Host, AKASHA memory, or canon.",
  "Give an absolute workspace root to every tool.",
  "Use zvec_grep_search for discovery. Once a path or symbol is known, use native grep and read for precise inspection.",
  "An INDEX_MISSING error requires explicit operator approval to create an index.",
  "Use the CLI index command for long initial builds. Never index other repositories without permission.",
  "Use zvec_grep_index for an explicitly requested refresh. Search does not refresh by default.",
].join(" ");

type ToolResult = {
  content: Array<{ type: "text"; text: string }>;
  structuredContent?: Record<string, unknown>;
  isError?: boolean;
};

export function createMcpServer(version: string): McpServer {
  const server = new McpServer(
    { name: ADAPTER_NAME, version },
    { capabilities: { tools: {} }, instructions: INSTRUCTIONS },
  );

  server.registerTool(
    "zvec_grep_search",
    {
      title: "Search the workspace index",
      description: [
        "Search the indexed workspace and return ranked file and line evidence.",
        "Use it when the wording or the location is unknown.",
        "Use it for semantic, relational or cross-file questions.",
        "Once the relevant path or symbol is known, use native grep and read for precise inspection or exhaustive references.",
        "The index must exist. This tool never makes one and never rebuilds one.",
        "Each item carries a freshness state of fresh or possibly_stale.",
        "By default, return five hits per query group and at most 2000 characters of content per hit.",
        "contentTruncated marks clipped excerpts. Use the returned path and line range to read the full code.",
      ].join(" "),
      inputSchema: searchShape,
      annotations: { readOnlyHint: false, openWorldHint: false },
    },
    (input) => runTool(() => search(input as SearchInput)),
  );

  server.registerTool(
    "zvec_grep_index",
    {
      title: "Build or update the workspace index",
      description: [
        "Warning: this tool writes to disk and embeds every scanned file.",
        "Build or update the index of one absolute root.",
        "The call blocks until the work stops. It returns no background job.",
        "The ignore-file defaults stay active unless you set the scope fields.",
      ].join(" "),
      inputSchema: indexShape,
      annotations: {
        readOnlyHint: false,
        destructiveHint: true,
        idempotentHint: false,
        openWorldHint: false,
      },
    },
    (input, extra) => runTool(() => index(input as IndexInput, { signal: extra.signal })),
  );

  server.registerTool(
    "zvec_grep_status",
    {
      title: "Show the workspace index status",
      description: [
        "Show the index identity, the model identity and the freshness of one root.",
        "The reply holds counts and small samples, never a full file table.",
        "Set includeStatus to false to skip the filesystem scan.",
      ].join(" "),
      inputSchema: statusShape,
      annotations: { readOnlyHint: true, openWorldHint: false },
    },
    (input) => runTool(() => status(input as StatusInput)),
  );

  return server;
}

async function runTool(
  operation: () => Promise<Record<string, unknown>>,
): Promise<ToolResult> {
  try {
    const data = await operation();
    return { content: [textBlock(data)], structuredContent: data };
  } catch (error) {
    return { content: [textBlock({ ok: false, error: errorPayload(error) })], isError: true };
  }
}

function textBlock(payload: unknown): { type: "text"; text: string } {
  return { type: "text", text: JSON.stringify(payload, null, 2) };
}

async function adapterVersion(): Promise<string> {
  const raw = await readFile(new URL("../package.json", import.meta.url), "utf8");
  const { version } = JSON.parse(raw);
  if (typeof version !== "string" || !version) throw new Error("The adapter package has no version.");
  return version;
}

/** Resolves after the connection closes and the service drains. */
export async function startStdioServer(): Promise<void> {
  const server = createMcpServer(await adapterVersion());
  const done = Promise.withResolvers<void>();
  let stopping: Promise<void> | undefined;
  const stop = (): Promise<void> => {
    stopping ??= Promise.resolve().then(async () => {
      try {
        await server.close();
      } finally {
        await closeService();
      }
    });
    return stopping;
  };
  const requestStop = () => { void stop().then(done.resolve, done.reject); };
  server.server.onerror = (error) => logToStderr(`transport error: ${error.message}`);
  server.server.onclose = requestStop;
  process.once("SIGINT", requestStop);
  process.once("SIGTERM", requestStop);
  // The SDK does not observe stdin EOF. Drain native handles when the client leaves.
  process.stdin.once("end", requestStop);
  try {
    await server.connect(new StdioServerTransport());
    logToStderr("ready on stdio");
    await done.promise;
  } finally {
    process.removeListener("SIGINT", requestStop);
    process.removeListener("SIGTERM", requestStop);
    process.stdin.removeListener("end", requestStop);
    await stop();
  }
}
