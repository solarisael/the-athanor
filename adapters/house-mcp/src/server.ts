/**
 * The House organs over MCP stdio, for any MCP harness.
 *
 * Invariants:
 * - Stdout carries JSON-RPC frames only. Logs go to stderr.
 * - One tool table. Every tool comes from `registerSolarisaelTools` in
 *   adapters/omp/house-proof/tools.ts, captured through a small `pi`. Nothing
 *   here declares a schema; only the OMP-only tools below are left out.
 * - One room per process: ATHANOR_CLIENT_ROOM_DIR, or the launch directory,
 *   resolved by adapters/omp/house-proof/room.ts. No room, no server.
 * - One session per process: `mcp-<uuid>`, the room's top-level session here,
 *   as an OMP spirit session is in its own process.
 * - Host access is the OMP session's: the House-wide token, projected from
 *   ~/.omp/agent/athanor/client.json with the keys and rule installed-loader.ts uses.
 */
import { randomUUID } from "node:crypto";
import { readFileSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import type { CallToolResult } from "@modelcontextprotocol/sdk/types.js";
import { z } from "zod";
import { roomContext, type RoomContext } from "../../omp/house-proof/room.ts";
import { registerTopLevelSession } from "../../omp/house-proof/top-level-session-fence.ts";
import { registerSolarisaelTools } from "../../omp/house-proof/tools.ts";

const SERVER_NAME = "athanor-house";

// Tools that need OMP state an MCP host does not have. Every other tool that
// registerSolarisaelTools registers is exposed. Paths are under adapters/omp.
export const OMP_ONLY_TOOLS: Readonly<Record<string, string>> = Object.freeze({
  // childLogPage and the image branch read OMP transcript files through
  // ctx.sessionManager.getSessionFile() (house-proof/tools.ts); the `id+n`
  // lines zoom opens are OMP's live <chat> view.
  zoom: "reads OMP transcript files through ctx.sessionManager",
  // Resolves the selector with ctx.models.resolve and applies it with
  // pi.setModel (house-proof/tools.ts).
  house_model_default: "needs the OMP model registry and pi.setModel",
  // kittenLineageDiagnostics() counts this OMP process's task events
  // (kitten-lineage.ts).
  kitten_lineage_status: "reports in-process OMP diagnostics",
  // homeResource, homeSessionStatus, issueSessionCommand and
  // sessionCommandStatus all start with requireAttached (house-proof/session.ts).
  // Only an OMP `/spirit` attach mints that attachment: attachConversation
  // sends harness "omp" and requires a session file.
  home_resources: "needs an OMP home attachment",
  home_resource_write: "needs an OMP home attachment",
  house_sessions: "needs an OMP home attachment",
  session_control: "issues agent and session controls from an OMP attachment",
  // registerRestartDoor restarts this exact OMP instance through its
  // attachment and launcher (house-proof/restart-door.ts).
  request_restart: "restarts the OMP process",
});

type ToolContent = CallToolResult["content"];

type HouseToolResult = { content: ToolContent; isError?: boolean; details?: unknown };

export type ToolContext = {
  cwd: string;
  sessionId: string;
  hasUI: false;
  ui: { notify(): void; setStatus(): void; setWidget(): void };
};

export type HouseTool = {
  name: string;
  label?: string;
  description?: string;
  parameters: z.ZodObject;
  approval?: string;
  execute(
    toolCallId: string,
    params: Record<string, unknown>,
    signal: AbortSignal | undefined,
    onUpdate: undefined,
    ctx: ToolContext,
  ): Promise<HouseToolResult>;
};

export type HouseSession = {
  session: string;
  room: RoomContext;
  instructions: string | undefined;
};

export class StartupRefusal extends Error {
  constructor(message: string) {
    super(message);
    this.name = "StartupRefusal";
  }
}

export function log(message: string): void {
  process.stderr.write(`[${SERVER_NAME}] ${message}\n`);
}

function errorText(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const CLIENT_PROJECTION = path.join(".omp", "agent", "athanor", "client.json");
// The keys installed-loader.ts projects from client.json, with the same rule:
// a key the operator already set wins.
const PROJECTED_KEYS = [
  ["ATHANOR_STATE_DIR", "stateRoot"],
  ["ATHANOR_HOST_HOUSE_ID", "houseId"],
  ["ATHANOR_HOST_TOKEN", "hostToken"],
  ["ATHANOR_HOST_URL", "hostUrl"],
] as const;

/** Returns the keys it set. A missing client.json leaves the environment as given. */
export function projectClientEnvironment(
  env: NodeJS.ProcessEnv = process.env,
  profile: string = homedir(),
): string[] {
  const file = path.join(profile, CLIENT_PROJECTION);
  let source: Record<string, unknown>;
  try {
    source = JSON.parse(readFileSync(file, "utf8"));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      log(`no client projection at ${file}; the Host keys come from the environment`);
      return [];
    }
    throw new StartupRefusal(`the client projection ${file} is unreadable: ${errorText(error)}`);
  }
  if (source?.format !== 2) {
    throw new StartupRefusal(`the client projection ${file} has an unsupported format`);
  }
  const projected: string[] = [];
  for (const [key, field] of PROJECTED_KEYS) {
    const value = typeof source[field] === "string" ? source[field].trim() : "";
    if (!value || String(env[key] ?? "").trim()) continue;
    env[key] = value;
    projected.push(key);
  }
  return projected;
}

// The ctx each House tool reads, and nothing more:
// - cwd: with no attachment, roomContext resolves homeLaunchContext(ctx.cwd) (room.ts).
// - sessionId: hostSessionIdentity (host.ts); session.ts conversationKey falls
//   back to it when there is no session file.
// - hasUI false and a silent ui: feedbackUi (feedback.ts) and every ctx.ui?.notify?.().
// No sessionManager, agent, models or modelRegistry. The tools that need them
// are excluded above; parentContextRequest (recall.ts) sees a top-level agent,
// and orihon sends no judgment credential.
export function toolContext(roomDir: string, session: string): ToolContext {
  return {
    cwd: roomDir,
    sessionId: session,
    hasUI: false,
    ui: { notify() {}, setStatus() {}, setWidget() {} },
  };
}

// room.ts reads ATHANOR_CLIENT_ROOM_DIR first and throws when it names no
// provisioned room; without it, the launch directory must be a room.
export function resolveRoom(session: string, cwd: string = process.cwd()): RoomContext {
  const bound = process.env.ATHANOR_CLIENT_ROOM_DIR;
  let context: RoomContext | null;
  try {
    context = roomContext(toolContext(cwd, session));
  } catch (error) {
    throw new StartupRefusal(bound ? `ATHANOR_CLIENT_ROOM_DIR=${bound}: ${errorText(error)}` : errorText(error));
  }
  if (!context) {
    throw new StartupRefusal(
      `no room resolves from ${cwd}. Set ATHANOR_CLIENT_ROOM_DIR to a provisioned room directory, or launch the server inside one.`,
    );
  }
  return context;
}

export function readOrgans(houseDir: string): string | undefined {
  const file = path.join(houseDir, "ORGANS.md");
  let text: string;
  try {
    text = readFileSync(file, "utf8");
  } catch (error) {
    log(`starting without instructions: ${file} is unreadable (${errorText(error)})`);
    return undefined;
  }
  if (!text.trim()) {
    log(`starting without instructions: ${file} is empty`);
    return undefined;
  }
  return text;
}

/** Projects the Host keys, resolves the room, and mints this process's session. */
export function openHouseSession(cwd: string = process.cwd()): HouseSession {
  const projected = projectClientEnvironment();
  if (projected.length) log(`set ${projected.join(", ")} from the client projection`);
  const session = `mcp-${randomUUID()}`;
  const room = resolveRoom(session, cwd);
  // OMP adopts each spirit session as its room's top-level session on
  // session_start (adapters/omp/index.ts). Docket writes and Presence close require it.
  registerTopLevelSession(room.room, session);
  return { session, room, instructions: readOrgans(room.houseDir) };
}

/** Every tool registerSolarisaelTools registers, OMP-only ones included. */
export function captureHouseTools(version: string): HouseTool[] {
  const tools: HouseTool[] = [];
  const pi = {
    zod: z,
    registerTool(definition: HouseTool) {
      tools.push(definition);
    },
    on() {},
    registerCommand() {},
    registerHook() {},
  };
  registerSolarisaelTools(pi, { version });
  return tools;
}

export function createHouseMcpServer(house: HouseSession, version: string): McpServer {
  const server = new McpServer(
    { name: SERVER_NAME, version },
    { capabilities: { tools: {} }, instructions: house.instructions },
  );
  const ctx = toolContext(house.room.effectiveRoomDir, house.session);

  for (const tool of captureHouseTools(version)) {
    if (Object.hasOwn(OMP_ONLY_TOOLS, tool.name)) continue;
    const shape = tool.parameters?.shape;
    if (!shape) throw new Error(`${tool.name}: the schema has no object shape, so it cannot be exposed`);
    server.registerTool(
      tool.name,
      {
        title: tool.label,
        description: tool.description,
        // Loose, so an unknown key reaches registerHouseTool's refusal by name
        // instead of being stripped by the SDK's parse.
        inputSchema: z.looseObject(shape),
        ...(tool.approval ? { annotations: { readOnlyHint: tool.approval === "read" } } : {}),
      },
      async (params, extra) => {
        const result = await tool.execute(randomUUID(), params, extra.signal, undefined, ctx);
        return { content: result.content, ...(result.isError ? { isError: true } : {}) };
      },
    );
  }
  return server;
}

export function adapterVersion(): string {
  const { version } = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  if (typeof version !== "string" || !version) throw new Error("The adapter package has no version.");
  return version;
}

/** Resolves after the connection closes. */
export async function startStdioServer(): Promise<void> {
  const version = adapterVersion();
  const house = openHouseSession();
  const server = createHouseMcpServer(house, version);
  const done = Promise.withResolvers<void>();
  let stopping: Promise<void> | undefined;
  const stop = (): Promise<void> => {
    stopping ??= server.close();
    return stopping;
  };
  const requestStop = () => { void stop().then(done.resolve, done.reject); };
  server.server.onerror = (error) => log(`transport error: ${error.message}`);
  server.server.onclose = requestStop;
  process.once("SIGINT", requestStop);
  process.once("SIGTERM", requestStop);
  // The SDK does not observe stdin EOF.
  process.stdin.once("end", requestStop);
  try {
    await server.connect(new StdioServerTransport());
    log(`ready on stdio: room ${house.room.room} as ${house.room.spirit}, session ${house.session}`);
    await done.promise;
  } finally {
    process.removeListener("SIGINT", requestStop);
    process.removeListener("SIGTERM", requestStop);
    process.stdin.removeListener("end", requestStop);
    await stop();
  }
}

if (import.meta.main) {
  try {
    await startStdioServer();
  } catch (error) {
    log(error instanceof StartupRefusal ? `refused: ${error.message}` : `failed: ${errorText(error)}`);
    process.exitCode = 1;
  }
}
