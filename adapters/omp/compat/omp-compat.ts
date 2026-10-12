// Real-CLI compatibility smoke for the lesson TTSR bridge. Launches one
// installed OMP in RPC mode inside a disposable home with only
// compat/ttsr-probe.ts loaded, reads that probe's receipt, and prints one JSON
// result. Any failed, missing, or unreached check exits nonzero.
//
//   bun compat/omp-compat.ts [--omp <executable>] [--timeout-ms <ms>]
//
// Without --omp the `omp` on PATH is used. No prompt is ever sent, so no
// provider is called; the Athanor Host extension is never loaded.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { randomUUID } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

type Check = { name: string; ok: boolean; detail: string };

const PROBE = resolve(import.meta.dir, "ttsr-probe.ts");
const DEFAULT_TIMEOUT_MS = 60_000;
const SHUTDOWN_GRACE_MS = 3_000;
// The only inherited variables: what Windows and Bun need to start a process.
// Credentials, OTEL_*, ATHANOR_*, profiles, and proxies stay behind.
const INHERITED = ["PATH", "PATHEXT", "SystemRoot", "SYSTEMROOT", "windir", "ComSpec", "SystemDrive", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE", "OS", "LANG"];
// Interactive-only today; pinned off so a future RPC startup path stays quiet too.
const CONFIG = [
  "startup:", "  checkUpdate: false", "  setupWizard: false", "  showSplash: false",
  "marketplace:", "  autoUpdate: \"off\"",
  "telemetry:", "  otlpExport: false", "",
].join("\n");

function usage(message: string): never {
  console.error(`${message}\nusage: bun compat/omp-compat.ts [--omp <executable>] [--timeout-ms <ms>]`);
  process.exit(2);
}

function parseArgs(argv: string[]): { omp: string | null; timeoutMs: number } {
  let omp: string | null = null;
  let timeoutMs = DEFAULT_TIMEOUT_MS;
  for (let i = 0; i < argv.length; i++) {
    const [flag, inline] = argv[i].includes("=") ? argv[i].split(/=(.*)/s, 2) : [argv[i], undefined];
    const value = () => inline ?? argv[++i] ?? usage(`${flag} needs a value`);
    if (flag === "--omp") omp = value();
    else if (flag === "--timeout-ms") {
      timeoutMs = Number(value());
      if (!Number.isInteger(timeoutMs) || timeoutMs <= 0) usage("--timeout-ms must be a positive integer");
    } else if (!flag.startsWith("-") && omp === null) omp = flag;
    else usage(`unknown argument ${argv[i]}`);
  }
  return { omp, timeoutMs };
}

function resolveExecutable(explicit: string | null): string {
  if (explicit !== null) {
    const path = resolve(explicit);
    if (!existsSync(path) || !statSync(path).isFile()) throw new Error(`OMP executable not found: ${path}`);
    return path;
  }
  const found = Bun.which("omp");
  if (!found) throw new Error("no `omp` on PATH; pass --omp <executable>");
  return found;
}

function isolatedEnv(root: string, receipt: string, nonce: string): Record<string, string> {
  const env: Record<string, string> = {};
  for (const key of INHERITED) {
    const value = process.env[key];
    if (value !== undefined) env[key] = value;
  }
  const dir = (name: string) => {
    const path = join(root, name);
    mkdirSync(path, { recursive: true });
    return path;
  };
  const home = dir("home");
  const agent = dir("agent");
  writeFileSync(join(agent, "config.yml"), CONFIG);
  Object.assign(env, {
    HOME: home, USERPROFILE: home,
    APPDATA: dir("home/AppData/Roaming"), LOCALAPPDATA: dir("home/AppData/Local"),
    TEMP: dir("tmp"), TMP: dir("tmp"), TMPDIR: dir("tmp"),
    XDG_CONFIG_HOME: dir("xdg/config"), XDG_DATA_HOME: dir("xdg/data"), XDG_STATE_HOME: dir("xdg/state"), XDG_CACHE_HOME: dir("xdg/cache"),
    PI_CODING_AGENT_DIR: agent,
    PI_NO_TITLE: "1", PI_NO_PTY: "1", PI_NOTIFICATIONS: "off", NO_COLOR: "1",
    ATHANOR_OMP_COMPAT_RECEIPT: receipt, ATHANOR_OMP_COMPAT_NONCE: nonce,
  });
  // OMP_PROFILE / PI_PROFILE are never copied, so the default profile resolves
  // to PI_CODING_AGENT_DIR above.
  return env;
}

function killTree(child: ChildProcess): void {
  if (child.pid === undefined || child.exitCode !== null || child.signalCode !== null) return;
  if (process.platform === "win32") {
    spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
  } else {
    try { process.kill(-child.pid, "SIGKILL"); } catch { child.kill("SIGKILL"); }
  }
}

const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

async function main(): Promise<number> {
  const { omp: requested, timeoutMs } = parseArgs(process.argv.slice(2));
  const checks: Check[] = [];
  const result: Record<string, unknown> = { ok: false, omp: { executable: requested, version: null }, checks };
  const fail = (name: string, detail: string) => checks.push({ name, ok: false, detail });
  const report = () => {
    result.ok = checks.length > 0 && checks.every((check) => check.ok);
    console.log(JSON.stringify(result, null, 2));
    return result.ok ? 0 : 1;
  };

  let executable: string;
  try {
    executable = resolveExecutable(requested);
  } catch (error) {
    fail("executable", error instanceof Error ? error.message : String(error));
    return report();
  }
  (result.omp as Record<string, unknown>).executable = executable;
  checks.push({ name: "executable", ok: true, detail: executable });

  const root = mkdtempSync(join(tmpdir(), "athanor-omp-compat-"));
  const receiptPath = join(root, "receipt.json");
  const nonce = randomUUID();
  const work = join(root, "work");
  mkdirSync(work);
  let child: ChildProcess | null = null;
  let stdout = "";
  let stderr = "";
  let ready = false;
  let exit: { code: number | null; signal: string | null } | null = null;
  let spawnError: Error | null = null;
  try {
    child = spawn(executable, [
      "--mode", "rpc", "--no-session", "--no-extensions", "--no-tools", "--no-lsp", "--no-skills", "--no-rules", "--no-title",
      "--extension", PROBE,
    ], {
      cwd: work, env: isolatedEnv(root, receiptPath, nonce), stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true, detached: process.platform !== "win32",
    });
    child.on("error", (error) => { spawnError = error; });
    child.on("exit", (code, signal) => { exit = { code, signal }; });
    child.stdout!.setEncoding("utf8").on("data", (chunk: string) => {
      stdout += chunk;
      for (const line of stdout.split("\n").slice(0, -1)) {
        try { if (JSON.parse(line)?.type === "ready") ready = true; } catch { /* Ignore non-protocol stdout; only a parsed ready frame counts. */ }
      }
      stdout = stdout.slice(stdout.lastIndexOf("\n") + 1);
    });
    child.stderr!.setEncoding("utf8").on("data", (chunk: string) => { stderr = (stderr + chunk).slice(-4_000); });

    const deadline = Date.now() + timeoutMs;
    while (!existsSync(receiptPath) && !exit && !spawnError && Date.now() < deadline) await sleep(100);
    // The probe may finish before RPC mode prints its ready frame.
    while (existsSync(receiptPath) && !ready && !exit && Date.now() < deadline) await sleep(100);

    const tail = () => (stderr.trim() ? `; stderr: ${stderr.trim()}` : "");
    if (spawnError) fail("launch", `could not start ${executable}: ${(spawnError as Error).message}`);
    else if (!existsSync(receiptPath) && exit) {
      const { code, signal } = exit as { code: number | null; signal: string | null };
      fail("launch", `OMP exited (code ${code}, signal ${signal}) before the probe receipt${tail()}`);
    } else if (!existsSync(receiptPath)) fail("launch", `no probe receipt within ${timeoutMs} ms${tail()}`);
    else checks.push({ name: "launch", ok: true, detail: "OMP started in RPC mode with only the probe extension" });

    if (!spawnError) {
      checks.push(ready
        ? { name: "rpc_ready", ok: true, detail: "RPC ready frame received" }
        : { name: "rpc_ready", ok: false, detail: `no RPC ready frame${tail()}` });
    }

    if (existsSync(receiptPath)) {
      const receipt = JSON.parse(readFileSync(receiptPath, "utf8"));
      (result.omp as Record<string, unknown>).version = receipt.version ?? null;
      if (receipt.nonce !== nonce) fail("receipt", "the receipt does not carry this run's nonce");
      else if (receipt.fatal) fail("receipt", `the probe crashed: ${receipt.fatal}`);
      else if (!Array.isArray(receipt.checks) || receipt.checks.length === 0) fail("receipt", "the receipt lists no checks");
      else checks.push({ name: "receipt", ok: true, detail: `${receipt.checks.length} probe checks` });
      if (Array.isArray(receipt.checks)) {
        for (const check of receipt.checks) {
          checks.push({ name: `probe.${check?.name}`, ok: check?.ok === true, detail: String(check?.detail ?? "") });
        }
      }
    }

    // The receipt is the whole proof; the process is ours to stop, and it must
    // still be running when we do: an exit we did not ask for is a failure.
    if (existsSync(receiptPath)) {
      const early = exit as { code: number | null; signal: string | null } | null;
      checks.push(early
        ? { name: "process_alive", ok: false, detail: `OMP exited (code ${early.code}, signal ${early.signal}) before shutdown${tail()}` }
        : { name: "process_alive", ok: true, detail: "OMP was still running at shutdown" });
    }
    child.stdin?.end();
    const graceEnd = Date.now() + SHUTDOWN_GRACE_MS;
    while (!exit && Date.now() < graceEnd) await sleep(100);
  } catch (error) {
    fail("runner", error instanceof Error ? error.message : String(error));
  } finally {
    if (child) killTree(child);
    for (let attempt = 0; attempt < 20 && child && !exit && !spawnError; attempt++) await sleep(100);
    try {
      rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
    } catch (error) {
      fail("cleanup", `could not remove ${root}: ${error instanceof Error ? error.message : String(error)}`);
    }
    if (child && !exit && !spawnError) fail("cleanup", `OMP process ${child.pid} did not exit after termination`);
  }
  return report();
}

process.exit(await main());
