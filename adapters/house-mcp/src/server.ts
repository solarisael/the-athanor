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
 * - Host access is the OMP session's: the House-wide token. resolveHostEnvironment
 *   takes each Host key from the environment, else from client.json (Windows),
 *   else from the Linux data root, so the server needs no OMP environment.
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
// The keys installed-loader.ts projects from client.json.
const CLIENT_KEYS = [
  ["ATHANOR_STATE_DIR", "stateRoot"],
  ["ATHANOR_HOST_HOUSE_ID", "houseId"],
  ["ATHANOR_HOST_TOKEN", "hostToken"],
  ["ATHANOR_HOST_URL", "hostUrl"],
] as const;
// house-proof/host.ts refuses every Host call without these keys. Without
// ATHANOR_HOST_URL, it uses ws://127.0.0.1:8787.
const REQUIRED_HOST_KEYS = ["ATHANOR_HOST_TOKEN", "ATHANOR_HOST_HOUSE_ID"] as const;

// Environment key to value. A blank value means that the place does not hold the key.
type HostKeys = Record<string, string>;

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : "";
}

/**
 * Sets each Host key that `env` lacks, and returns the keys it set with the
 * place they came from. It returns no value, so the token stays out of the logs.
 *
 * The order:
 * 1. A key that is already set in `env` wins.
 * 2. Else ~/.omp/agent/athanor/client.json, when it exists. The Windows
 *    installer writes it.
 * 3. Else the Linux data root: $ATHANOR_DATA_ROOT, else $XDG_STATE_HOME/athanor,
 *    else ~/.local/state/athanor. host.token holds the token, and
 *    config/runtime.json holds hostPort and houseId (athanor-nix modules/athanor.nix).
 *
 * Refuses when no place holds the token or the House id.
 */
export function resolveHostEnvironment(
  env: NodeJS.ProcessEnv = process.env,
  profile: string = homedir(),
): { source: string; set: string[] } {
  const clientFile = path.join(profile, CLIENT_PROJECTION);
  const stateHome = text(env.XDG_STATE_HOME) || path.join(profile, ".local", "state");
  const dataRoot = text(env.ATHANOR_DATA_ROOT) || path.join(stateHome, "athanor");
  const tokenFile = path.join(dataRoot, "host.token");
  const configFile = path.join(dataRoot, "config", "runtime.json");

  const client = readClientProjection(clientFile);
  const source = client ? clientFile : dataRoot;
  const found = client ?? readDataRoot(tokenFile, configFile);

  const set: string[] = [];
  for (const [key, value] of Object.entries(found)) {
    if (text(env[key]) || !value) continue;

    env[key] = value;
    set.push(key);
  }

  const missing = REQUIRED_HOST_KEYS.filter((key) => !text(env[key]));

  if (missing.length) {
    throw new StartupRefusal(
      `no ${missing.join(" and no ")}. The server reads the environment, then ${clientFile} when that file exists, else ${tokenFile} and ${configFile}.`,
    );
  }

  return { source, set };
}

// client.json as Host keys, or undefined when the file does not exist.
function readClientProjection(file: string): HostKeys | undefined {
  const source = readJsonIfPresent(file);

  if (source === undefined) return undefined;
  if (source?.format !== 2) {
    throw new StartupRefusal(`the client projection ${file} has an unsupported format`);
  }

  return Object.fromEntries(CLIENT_KEYS.map(([key, field]) => [key, text(source[field])]));
}

// The data root as Host keys. A file that does not exist holds no key.
function readDataRoot(tokenFile: string, configFile: string): HostKeys {
  const config = readJsonIfPresent(configFile);
  const port = config?.hostPort;
  const portIsValid = typeof port === "number" && Number.isInteger(port) && port >= 1 && port <= 65_535;

  if (port !== undefined && !portIsValid) {
    throw new StartupRefusal(`${configFile} has an invalid hostPort`);
  }

  return {
    ATHANOR_HOST_TOKEN: text(readIfPresent(tokenFile)),
    ATHANOR_HOST_HOUSE_ID: text(config?.houseId),
    // runtime.json names only the port: the Host binds 127.0.0.1 (crates/athanor-install/src/app.rs).
    ATHANOR_HOST_URL: port === undefined ? "" : `ws://127.0.0.1:${port}`,
  };
}

// The text of the file, or undefined when the file does not exist.
function readIfPresent(file: string): string | undefined {
  try {
    return readFileSync(file, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") return undefined;
    throw new StartupRefusal(`${file} is unreadable: ${errorText(error)}`);
  }
}

function readJsonIfPresent(file: string): Record<string, unknown> | undefined {
  const source = readIfPresent(file);

  if (source === undefined) return undefined;

  try {
    return JSON.parse(source);
  } catch (error) {
    throw new StartupRefusal(`${file} is not valid JSON: ${errorText(error)}`);
  }
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

/** Resolves the Host keys and the room, and mints this process's session. */
export function openHouseSession(cwd: string = process.cwd()): HouseSession {
  const host = resolveHostEnvironment();
  if (host.set.length) log(`set ${host.set.join(", ")} from ${host.source}`);
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
