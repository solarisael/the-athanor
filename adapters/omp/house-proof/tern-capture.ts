// Read-only capture of the Tern pane an OMP conversation runs in, and of the
// launch the Host can repeat to relaunch it. Nothing here types into a pane.

import path from "node:path";
import { existsSync } from "node:fs";

const TERN_INSPECT_TIMEOUT_MS = 5_000;

export type TernTerminal = { kind: "tern"; session: string; pane: string; socket: string; executable: string };
export type Launcher =
  | { kind: "tern"; program: string; args: string[]; environment: Record<string, string> }
  | { kind: "keeper"; keeperInstance: string };

// The OMP profile selection a relaunch must keep (pi-utils dirs.ts reads
// exactly these). A key this process lacks stays absent in the successor: the
// Host's launch helper removes it from the new tab's inherited environment.
// Nothing else from this environment is ever carried; the Host supplies the
// room scope from its own config.
const PROFILE_ENV = ["PI_CODING_AGENT_DIR", "OMP_PROFILE", "PI_PROFILE", "PI_CONFIG_DIR"] as const;

function profileEnvironment(): Record<string, string> {
  const environment: Record<string, string> = {};
  for (const key of PROFILE_ENV) {
    const value = text(process.env[key]);
    if (value) environment[key] = value;
  }
  return environment;
}

function text(value: unknown): string {
  return typeof value === "string" ? value.trim() : value == null ? "" : String(value).trim();
}

function samePath(left: string, right: string): boolean {
  const a = path.resolve(left);
  const b = path.resolve(right);
  return process.platform === "win32" ? a.toLowerCase() === b.toLowerCase() : a === b;
}

// The console shim answers the CLI with JSON on Windows; the GUI binary does not.
function ternExecutable(): string | null {
  const candidates = process.platform === "win32"
    ? [Bun.which("tern.com"), process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, "Programs", "Tern", "tern.com") : null, Bun.which("tern")]
    : [Bun.which("tern")];
  for (const candidate of candidates) {
    if (candidate && path.isAbsolute(candidate) && existsSync(candidate)) return candidate;
  }
  return null;
}

function containsBlock(node: any, pane: string): boolean {
  return Array.isArray(node?.tabs) && node.tabs.some((tab: any) =>
    Array.isArray(tab?.blocks) && tab.blocks.some((block: any) => String(block?.id) === pane));
}

// TERN_WINDOW_KEY may be empty, so the session is found by the exact pane id.
// Tern reports a full session file path per pane; it is compared to this
// conversation's file, never to its id. Ids travel as decimal strings.
export async function captureTernTerminal(sessionFile: string): Promise<{ terminal: TernTerminal | null; reason: string | null }> {
  const pane = text(process.env.TERN_PANE);
  const socket = text(process.env.TERN_PANE_SOCKET);
  if (!pane || !socket) return { terminal: null, reason: "not running in a Tern pane" };
  const executable = ternExecutable();
  if (!executable) return { terminal: null, reason: "the Tern CLI executable was not found" };
  let inspected: any;
  try {
    const child = Bun.spawn([executable, "inspect", "--json"], { stdout: "pipe", stderr: "pipe", stdin: "ignore" });
    const timer = setTimeout(() => child.kill(), TERN_INSPECT_TIMEOUT_MS);
    const [stdout, exitCode] = await Promise.all([new Response(child.stdout).text(), child.exited]);
    clearTimeout(timer);
    if (exitCode !== 0) return { terminal: null, reason: `tern inspect exited ${exitCode}` };
    inspected = JSON.parse(stdout);
  } catch (error) {
    return { terminal: null, reason: `tern inspect failed: ${error instanceof Error ? error.message : String(error)}` };
  }
  const paneRow = (Array.isArray(inspected?.panes) ? inspected.panes : []).find((row: any) => String(row?.pane) === pane);
  if (!paneRow) return { terminal: null, reason: `Tern does not list pane ${pane}` };
  const observed = text(paneRow.omp_session_file);
  if (!observed || !samePath(observed, sessionFile)) {
    return { terminal: null, reason: `Tern pane ${pane} reports session file ${observed || "<none>"}, not this conversation` };
  }
  const session = (Array.isArray(inspected?.sessions) ? inspected.sessions : []).find((row: any) => containsBlock(row, pane));
  if (!session || session.id == null) return { terminal: null, reason: `no Tern session holds pane ${pane}` };
  return { terminal: { kind: "tern", session: String(session.id), pane, socket, executable }, reason: null };
}

// OMP's own relaunch rule (cli/flag-tables restartArgv) drops one-shot session
// selectors and positionals, so a relaunch never replays the first prompt. It
// is resolved from the running OMP install so no second package copy loads.
// The Host appends `--resume <sessionFile>` for a resume restart only.
export async function ternLauncher(): Promise<{ launcher: Launcher | null; reason: string | null }> {
  const script = text(process.argv[1]);
  const scriptOnDisk = Boolean(script) && path.isAbsolute(script) && existsSync(script);
  let restartArgv: ((argv: string[], resume: string | undefined) => string[]) | null = null;
  try {
    const base = scriptOnDisk ? path.dirname(script) : path.dirname(process.execPath);
    const resolved = Bun.resolveSync("@oh-my-pi/pi-coding-agent/cli/flag-tables", base);
    restartArgv = (await import(resolved)).restartArgv ?? null;
  } catch (error) {
    return { launcher: null, reason: `OMP relaunch argv rule is unavailable: ${error instanceof Error ? error.message : String(error)}` };
  }
  if (typeof restartArgv !== "function") return { launcher: null, reason: "OMP relaunch argv rule is unavailable" };
  const userArgs = process.argv.slice(scriptOnDisk ? 2 : 1);
  const args = [...(scriptOnDisk ? [script] : []), ...restartArgv(userArgs, undefined)];
  return { launcher: { kind: "tern", program: process.execPath, args, environment: profileEnvironment() }, reason: null };
}
