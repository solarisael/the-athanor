import { createServer, type IncomingMessage, type ServerResponse } from "node:http";
import { readFile } from "node:fs/promises";
import { createReadStream, existsSync, statSync } from "node:fs";
import { join, resolve, sep, extname } from "node:path";
import { randomBytes } from "node:crypto";
import { spawn } from "node:child_process";
import { RustJsonlTransport } from "./rust-transport.ts";
import { discoverRustExecutable } from "./discovery.ts";

const MAX_BODY = 256 * 1024;
const MAX_OUTPUT = 256 * 1024;
const RPC_METHODS = new Set(["remember", "recall", "anamnesis", "anamnesis_write", "cluster_maintenance"]);
const MIME: Record<string, string> = { ".html": "text/html; charset=utf-8", ".js": "text/javascript; charset=utf-8", ".css": "text/css; charset=utf-8" };

export interface GuiServerOptions {
  executable: string;
  args?: string[];
  cwd?: string;
  port?: number;
  backupRoot?: string;
  backupKeep?: number;
  databasePath?: string;
  host?: string;
  room?: string;
  transportFactory?: () => RustJsonlTransport;
}
export interface GuiServerHandle { server: ReturnType<typeof createServer>; csrfToken: string; port: number; close(): Promise<void> }

type JsonRecord = Record<string, unknown>;
function isRecord(value: unknown): value is JsonRecord { return !!value && typeof value === "object" && !Array.isArray(value); }
function canonicalError(error: unknown, operation?: string): JsonRecord {
  const unknown = isRecord(error) && error.code === "AUTHORITATIVE_OUTCOME_UNKNOWN";
  if (isRecord(error) && typeof error.code === "string" && typeof error.message === "string" && typeof error.retryable === "boolean" && (!unknown || error.details !== undefined)) {
    return error.details === undefined
      ? { code: error.code, message: error.message, retryable: error.retryable }
      : { code: error.code, message: error.message, retryable: error.retryable, details: error.details };
  }
  const message = error instanceof Error ? error.message : String(error);
  const dispatched = !/spawn|startup|unavailable/i.test(message);
  return {
    code: unknown ? "AUTHORITATIVE_OUTCOME_UNKNOWN" : "GUI_TRANSPORT_FAILURE",
    message,
    retryable: false,
    details: {
      category: "transport",
      stage: dispatched ? "request_write" : "spawn",
      ...(operation ? { operation } : {}),
      owner: { component: "gui-server", path: "gui-server.ts", symbol: "createGuiServer" },
      observed: { error_name: error instanceof Error ? error.name : typeof error },
      evidence: [],
      targets: ["gui-server.ts#createGuiServer"],
      next_checks: [{ action: unknown ? "reconcile" : "inspect", target: "gui-server.ts#createGuiServer" }],
      execution: { request_dispatched: dispatched, write_outcome: unknown ? "unknown" : "not_started", retry: unknown ? "reconcile_first" : "never" },
    },
  };
}
function errorResponse(res: ServerResponse, status: number, error: unknown, operation?: string) {
  json(res, status, { ok: false, error: canonicalError(error, operation) });
}
function json(res: ServerResponse, status: number, value: unknown) {
  const body = JSON.stringify(value);
  res.writeHead(status, { "content-type": "application/json; charset=utf-8", "cache-control": "no-store", "x-content-type-options": "nosniff" });
  res.end(body);
}
function headers(res: ServerResponse) {
  res.setHeader("cache-control", "no-store");
  res.setHeader("content-security-policy", "default-src 'self'; script-src 'self'; style-src 'self'; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'");
  res.setHeader("x-content-type-options", "nosniff");
}
async function body(req: IncomingMessage): Promise<Buffer> {
  const len = Number(req.headers["content-length"] ?? 0);
  if (len > MAX_BODY) throw Object.assign(new Error("payload too large"), { status: 413 });
  const chunks: Buffer[] = []; let size = 0;
  for await (const chunk of req) { const b = Buffer.from(chunk as Buffer); size += b.length; if (size > MAX_BODY) throw Object.assign(new Error("payload too large"), { status: 413 }); chunks.push(b); }
  return Buffer.concat(chunks);
}
async function killTree(child: ReturnType<typeof spawn>): Promise<void> {
  if (child.pid && process.platform === "win32") {
    await new Promise<void>((resolveKill) => {
      const killer = spawn("taskkill", ["/PID", String(child.pid), "/T", "/F"], { shell: false, windowsHide: true, stdio: "ignore" });
      killer.once("close", () => resolveKill());
      killer.once("error", () => resolveKill());
    });
  } else if (child.pid) {
    try { process.kill(-child.pid, "SIGKILL"); } catch { child.kill("SIGKILL"); }
  } else child.kill();
}
type CommandDependencies = {
  spawnProcess?: typeof spawn;
  killProcessTree?: typeof killTree;
};
export function runCommand(
  executable: string,
  args: string[],
  cwd: string | undefined,
  timeout: number,
  dependencies: CommandDependencies = {},
): Promise<unknown> {
  return new Promise((resolvePromise, reject) => {
    const child = (dependencies.spawnProcess ?? spawn)(executable, args, {
      cwd, shell: false, windowsHide: true, detached: process.platform !== "win32",
      stdio: ["ignore", "pipe", "pipe"],
    });
    const out: Buffer[] = [];
    let n = 0;
    let settled = false;
    let timedOut = false;
    let timer: ReturnType<typeof setTimeout>;
    const collect = (chunk: Buffer) => {
      if (n >= MAX_OUTPUT) return;
      const accepted = chunk.subarray(0, MAX_OUTPUT - n);
      out.push(accepted);
      n += accepted.length;
    };
    const finish = (fn: (value: any) => void, value: any) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      fn(value);
    };
    child.stdout.on("data", collect);
    child.stderr.on("data", collect);
    child.on("error", error => finish(reject, error));
    child.on("close", (code, signal) => finish(
      timedOut ? reject : code === 0 ? resolvePromise : reject,
      timedOut
        ? new Error("command timed out")
        : code === 0
          ? { ok: true, output: Buffer.concat(out).toString("utf8") }
          : new Error(`command failed (${signal ?? code})`),
    ));
    timer = setTimeout(() => {
      timedOut = true;
      void (dependencies.killProcessTree ?? killTree)(child).catch(() => {});
      finish(reject, new Error("command timed out"));
    }, timeout);
  });
}

export function createGuiServer(options: GuiServerOptions): GuiServerHandle {
  const host = options.host ?? "127.0.0.1";
  if (host !== "127.0.0.1" && host !== "localhost") throw new Error("GUI server must bind to 127.0.0.1 or localhost");
  const csrfToken = randomBytes(32).toString("hex");
  const newTransport = options.transportFactory
    ?? (() => new RustJsonlTransport({ executable: options.executable, args: options.args, cwd: options.cwd }));
  let transport = newTransport();
  let replacement: Promise<void> | undefined;
  const replaceTransport = async (failed: RustJsonlTransport) => {
    if (transport !== failed) return;
    replacement ??= (async () => {
      await failed.close();
      if (transport === failed) {
        transport = newTransport();
      }
    })().finally(() => { replacement = undefined; });
    await replacement;
  };
  const requestRust = async (method: string, params: Record<string, unknown>, requestOptions: { timeoutMs?: number; settleDefinitively?: boolean } = {}) => {
    const active = transport;
    try {
      return await active.request(method, params, requestOptions);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      if (/transport|child exited|closed|unusable|output line/i.test(message)) await replaceTransport(active);
      throw canonicalError(error, method);
    }
  };
  const root = resolve(options.cwd ?? process.cwd(), "gui");
  const server = createServer(async (req, res) => {
    headers(res);
    try {
      const url = new URL(req.url ?? "/", `http://${host}`);
      const actualPort = (server.address() as { port?: number } | null)?.port;
      const authority = actualPort ? `${host}:${actualPort}` : "";
      if (req.headers.host !== authority) { json(res, 403, { ok: false, error: "invalid host" }); return; }
      if (req.method === "POST" && req.headers.origin !== `http://${authority}`) { json(res, 403, { ok: false, error: "invalid origin" }); return; }
      if (req.method === "GET" && url.pathname === "/api/csrf") { json(res, 200, { token: csrfToken }); return; }
      if (req.method === "GET" && url.pathname === "/api/backups") {
        if (!options.backupRoot) { json(res, 200, { ok: true, backups: [] }); return; }
        const { readdir } = await import("node:fs/promises");
        const base = resolve(options.backupRoot); const entries = await readdir(base, { withFileTypes: true });
        json(res, 200, { ok: true, backups: entries.filter(e => e.isFile()).map(e => e.name).slice(0, 100) }); return;
      }
      if (req.method === "GET" && url.pathname === "/api/health") {
        try { const result = await requestRust("cluster_maintenance", { operation: "check", room: options.room ?? "gui" }, { timeoutMs: 15000 }); json(res, 200, { ok: true, status: "running", cluster: result }); }
        catch (e) { errorResponse(res, 503, e, "cluster_maintenance"); }
        return;
      }
      if (req.method === "POST" && (url.pathname === "/api/rpc" || url.pathname === "/api/backup" || url.pathname === "/api/restore")) {
        if (req.headers["x-csrf-token"] !== csrfToken) { json(res, 403, { ok: false, error: "csrf" }); return; }
        const raw = await body(req); let input: any;
        try { input = JSON.parse(raw.toString("utf8")); } catch { json(res, 400, { ok: false, error: "invalid json" }); return; }
        if (!input || typeof input !== "object" || Array.isArray(input)) { json(res, 400, { ok: false, error: "object required" }); return; }
        if (url.pathname === "/api/rpc") {
          const params = input.params;
          if (typeof input.method !== "string" || !RPC_METHODS.has(input.method)) { json(res, 400, { ok: false, error: "method not allowed" }); return; }
          if (!params || typeof params !== "object" || Array.isArray(params)) { json(res, 400, { ok: false, error: "params must be object" }); return; }
          if (input.method === "cluster_maintenance" && (params as any).operation === "rebuild" && (params as any).confirm !== "REBUILD") { json(res, 400, { ok: false, error: "rebuild confirmation required" }); return; }
          const rpcParams = { ...(params as Record<string, unknown>) };
          delete rpcParams.confirm;
          const definitive = input.method === "remember" || input.method === "anamnesis_write";
          const result = await requestRust(input.method, rpcParams, definitive ? { settleDefinitively: true } : { timeoutMs: 15000 });
          json(res, 200, { ok: true, result }); return;
        }
        if (url.pathname === "/api/backup") {
          if (!options.backupRoot) { json(res, 400, { ok: false, error: "backup is not configured" }); return; }
          const keep = options.backupKeep ?? 5;
          const result = await runCommand(options.executable, [...(options.args ?? []), "backup", "--output-dir", resolve(options.backupRoot), "--keep", String(keep)], options.cwd, 60000); json(res, 200, result); return;
        }
        const target = typeof input.targetDb === "string" ? input.targetDb : "";
        const confirm = typeof input.confirm === "string" ? input.confirm : "";
        const manifest = typeof input.manifest === "string" ? input.manifest : "";
        const expected = options.databasePath ?? "";
        if (!expected || !options.backupRoot) { json(res, 400, { ok: false, error: "restore is not configured" }); return; }
        if (target !== expected || confirm !== `RESTORE ${expected}`) { json(res, 400, { ok: false, error: "typed confirmation required" }); return; }
        const backupRoot = resolve(options.backupRoot); const selected = resolve(backupRoot, manifest);
        if (!manifest || (selected !== backupRoot && !selected.startsWith(backupRoot + sep)) || !existsSync(selected) || !statSync(selected).isFile()) { json(res, 400, { ok: false, error: "invalid manifest" }); return; }
        const result = await runCommand(options.executable, [...(options.args ?? []), "restore", "--manifest", selected, "--confirm-database", target], options.cwd, 60000); json(res, 200, result); return;
      }
      if (req.method === "GET") {
        let asset = url.pathname === "/" ? "/index.html" : url.pathname;
        if (asset.includes("..") || asset.includes("\\") || !/^\/[\w./-]+$/.test(asset)) { res.writeHead(404); res.end("Not found"); return; }
        const file = resolve(root, "." + asset); if (file !== root && !file.startsWith(root + sep)) { res.writeHead(404); res.end("Not found"); return; }
        const data = await readFile(file); res.setHeader("content-type", MIME[extname(file)] ?? "application/octet-stream"); res.end(data); return;
      }
      json(res, 404, { ok: false, error: "not found" });
    } catch (e: any) { errorResponse(res, e?.status === 413 ? 413 : 500, e); }
  });
  return {
    server,
    csrfToken,
    port: options.port ?? 0,
    close: async () => {
      // Drain requests before their worker. Project lesson #338.
      await new Promise<void>(resolveClose => server.close(() => resolveClose()));
      if (replacement) await replacement;
      await transport.close();
    },
  };
}
export async function startGuiServer(options: GuiServerOptions): Promise<GuiServerHandle> {
  const handle = createGuiServer(options); await new Promise<void>((resolveListen, reject) => { handle.server.once("error", reject); handle.server.listen(handle.port, options.host ?? "127.0.0.1", resolveListen); });
  handle.port = (handle.server.address() as any).port; return handle;
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  const value = (flag: string) => {
    const index = args.indexOf(flag);
    if (index < 0 || !args[index + 1]) throw new Error(`${flag} requires a value`);
    return args[index + 1];
  };
  const executable = discoverRustExecutable({ env: process.env, moduleDir: import.meta.dir });
  if (!executable) throw new Error("Athanor substrate not selected; set ATHANOR_SUBSTRATE_EXE or ATHANOR_AUTO=1");
  const room = args.includes("--room") ? value("--room") : "gui";
  const port = args.includes("--port") ? Number(value("--port")) : 0;
  if (!Number.isInteger(port) || port < 0 || port > 65535) throw new Error("--port must be an integer from 0 to 65535");
  const handle = await startGuiServer({
    executable,
    room,
    port,
    backupRoot: args.includes("--backup-root") ? value("--backup-root") : undefined,
    databasePath: args.includes("--database") ? value("--database") : undefined,
  });
  console.log(`http://127.0.0.1:${handle.port}`);
}
