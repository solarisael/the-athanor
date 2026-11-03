// Real-OMP cutover smoke. One installed OMP in RPC mode, inside a disposable
// home, loads compat/cutover-probe.ts and attaches to a loopback fake Host.
// The runner then plays a native cutover: it writes the cutover marker and
// stops the Host, starts a fresh Host on the same port while the marker
// stands, and removes the marker. It prints one JSON result and exits nonzero
// on any failed check.
//
//   bun compat/cutover-smoke.ts [--omp <executable>]
//
// No prompt is sent, so no provider is called. No live Host, database, or
// user session is touched; the runner stops only the OMP process it started.
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

type Check = { name: string; ok: boolean; detail: string };
type Receipt = { pid: number; sessionFile: string | null; attachError: string | null; status: string | null; connected: boolean | null; error: string | null; hostToken: string | null };

const PROBE = resolve(import.meta.dir, "cutover-probe.ts");
const STEP_TIMEOUT_MS = 30_000;
// Long enough for several 1 s polls and one 10 s heartbeat window to reach
// the fresh Host while the marker stands.
const HELD_MS = 12_000;
const INHERITED = ["PATH", "PATHEXT", "SystemRoot", "SYSTEMROOT", "windir", "ComSpec", "SystemDrive", "NUMBER_OF_PROCESSORS", "PROCESSOR_ARCHITECTURE", "OS", "LANG"];
const CONFIG = [
  "startup:", "  checkUpdate: false", "  setupWizard: false", "  showSplash: false",
  "marketplace:", "  autoUpdate: \"off\"",
  "telemetry:", "  otlpExport: false", "",
].join("\n");

const sleep = (ms: number) => new Promise((done) => setTimeout(done, ms));

// The Host's portable-session route, reduced to the lease path. A fresh
// instance remembers no registration, exactly like a restarted Host.
function fakeHost(port: number, homeDir: string, token: string) {
  const actions: string[] = [];
  const known = new Set<string>();
  const server = Bun.serve({
    port,
    hostname: "127.0.0.1",
    async fetch(request) {
      if (request.headers.get("authorization") !== `Bearer ${token}`) {
        actions.push("unauthorized");
        return Response.json({ ok: false, error: { code: "unauthorized", message: "wrong Host token" } }, { status: 401 });
      }
      const body = await request.json() as Record<string, any>;
      actions.push(String(body.action));
      if (body.action === "attach") {
        known.add(body.instanceId);
        return Response.json({
          ok: true, room: "kintsu", instanceId: body.instanceId, conversationId: body.conversationId,
          workspace: body.workspace, spirit: "Kintsu", operator: "Sol", homeDir,
          context: { revision: "smoke", text: "You are Kintsu." }, controls: ["deliver"], chatDelivery: false,
          restart: { supported: false, launcher: null }, heartbeatSecs: 10,
        });
      }
      if (!known.has(body.instanceId)) {
        return Response.json({ ok: false, error: { code: "instance_unknown", message: "attach first" } }, { status: 404 });
      }
      return Response.json({ ok: true, commands: [], contextRevision: "smoke" });
    },
  });
  return { server, actions, attaches: () => actions.filter((action) => action === "attach").length };
}

const sha256 = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");

function writeJson(path: string, value: unknown) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(value));
}

// A disposable installed tree that configureInstalledAthanor verifies. The
// adapter entry points are placeholders: the probe never imports them, and the
// gate checks only their recorded size and digest.
function installedTree(root: string, profile: string, stateRoot: string, port: number): string {
  const program = join(root, "program");
  const compatibility = { hostApi: 1, substrateApi: 1, deliveryApi: 1, schemaVersion: 18 };
  const launcher = Buffer.from("smoke launcher\n");
  mkdirSync(join(program, "bin"), { recursive: true });
  writeFileSync(join(program, "bin", "athanor.exe"), launcher);
  writeJson(join(program, "current.json"), { version: "0.10.1", previousVersion: "0.10.0" });
  writeJson(join(program, "versions", "0.10.1", "release-manifest.json"), {
    format: 1, product: "the-athanor", version: "0.10.1", platform: "windows-x64", schemaVersion: 18,
    compatibility: { hostApi: 1, substrateApi: 1, deliveryApi: 1 },
    artifacts: [{ component: "app", path: "bin/athanor.exe", sha256: sha256(launcher), size: launcher.length, executable: true }],
  });

  const sources = ["hygiene.ts", "index.ts"].map((path) => ({ path, bytes: Buffer.from(`// smoke placeholder ${path}\n`) }));
  const artifacts = sources.map(({ path, bytes }) => ({ path, sha256: sha256(bytes), size: bytes.length }));
  const identity = [
    "format=1", "component=omp-adapter", "version=0.9.3",
    ...Object.entries(compatibility).map(([field, value]) => `${field}=${value}`),
    ...artifacts.map((artifact) => `artifact=${artifact.path}\t${artifact.sha256}\t${artifact.size}`),
  ].join("\n");
  const releaseId = `0.9.3-${createHash("sha256").update(`${identity}\n`, "utf8").digest("hex")}`;
  const component = join(program, "components", "omp-adapter");
  for (const { path, bytes } of sources) {
    mkdirSync(join(component, "versions", releaseId), { recursive: true });
    writeFileSync(join(component, "versions", releaseId, path), bytes);
  }
  writeJson(join(component, "versions", releaseId, "component-manifest.json"), {
    format: 1, component: "omp-adapter", version: "0.9.3", releaseId, compatibility, artifacts,
  });
  writeJson(join(component, "current.json"), { format: 1, releaseId, previousReleaseId: null });
  writeClient(profile, stateRoot, port, "smoke-token");
  return program;
}

function writeClient(profile: string, stateRoot: string, port: number, hostToken: string) {
  writeJson(join(profile, ".omp", "agent", "athanor", "client.json"), {
    format: 2, houseId: "smoke", hostToken, stateRoot, hostUrl: `ws://127.0.0.1:${port}`,
    defaultRoom: "kintsu", rooms: { kintsu: { spirit: "Kintsu" } },
  });
}

function killTree(child: ChildProcess): void {
  if (child.pid === undefined || child.exitCode !== null || child.signalCode !== null) return;
  if (process.platform === "win32") spawnSync("taskkill", ["/PID", String(child.pid), "/T", "/F"], { stdio: "ignore", windowsHide: true });
  else child.kill("SIGKILL");
}

async function main(): Promise<number> {
  const flag = process.argv.indexOf("--omp");
  const executable = flag >= 0 ? resolve(process.argv[flag + 1] ?? "") : Bun.which("omp");
  const checks: Check[] = [];
  const record = (name: string, ok: boolean, detail: string) => checks.push({ name, ok, detail });
  const report = () => {
    const ok = checks.length > 0 && checks.every((check) => check.ok);
    console.log(JSON.stringify({ ok, omp: executable, checks }, null, 2));
    return ok ? 0 : 1;
  };
  if (!executable || !existsSync(executable)) {
    record("executable", false, "no OMP executable; pass --omp <executable>");
    return report();
  }

  const root = mkdtempSync(join(tmpdir(), "athanor-cutover-smoke-"));
  const dir = (name: string) => {
    const path = join(root, name);
    mkdirSync(path, { recursive: true });
    return path;
  };
  const receiptPath = join(root, "receipt.json");
  const stateRoot = dir("state");
  const marker = join(dir("state/cutover"), "cutover.json");
  const work = dir("work");
  const agent = dir("agent");
  const profile = dir("home");
  writeFileSync(join(agent, "config.yml"), CONFIG);

  let host = fakeHost(0, dir("homes/kintsu"), "smoke-token");
  const port = host.server.port;
  const program = installedTree(root, profile, stateRoot, port);
  const env: Record<string, string> = {};
  for (const key of INHERITED) if (process.env[key] !== undefined) env[key] = process.env[key]!;
  Object.assign(env, {
    HOME: profile, USERPROFILE: profile, APPDATA: dir("home/AppData/Roaming"), LOCALAPPDATA: dir("home/AppData/Local"),
    TEMP: dir("tmp"), TMP: dir("tmp"), TMPDIR: dir("tmp"), PI_CODING_AGENT_DIR: agent,
    PI_NO_TITLE: "1", PI_NO_PTY: "1", PI_NOTIFICATIONS: "off", NO_COLOR: "1",
    ATHANOR_CUTOVER_SMOKE_RECEIPT: receiptPath,
    ATHANOR_CUTOVER_SMOKE_PROGRAM_ROOT: program,
    ATHANOR_CUTOVER_SMOKE_PROFILE: profile,
  });

  let child: ChildProcess | null = null;
  let exit: string | null = null;
  let stderr = "";
  const receipt = (): Receipt | null => {
    try {
      return JSON.parse(readFileSync(receiptPath, "utf8"));
    } catch {
      // Absent before the probe starts, or caught mid-rewrite: the next read decides.
      return null;
    }
  };
  const until = async (name: string, condition: () => boolean, detail: () => string) => {
    const deadline = Date.now() + STEP_TIMEOUT_MS;
    while (!condition() && !exit && Date.now() < deadline) await sleep(100);
    const ok = condition() && !exit;
    record(name, ok, ok ? detail() : `${detail()}; receipt ${JSON.stringify(receipt())}; exit ${exit}; stderr ${stderr.trim().slice(-1_000)}`);
    return ok;
  };

  try {
    child = spawn(executable, [
      "--mode", "rpc", "--no-extensions", "--no-tools", "--no-lsp", "--no-skills", "--no-rules", "--no-title",
      "--extension", PROBE,
    ], { cwd: work, env, stdio: ["pipe", "pipe", "pipe"], windowsHide: true });
    child.on("exit", (code, signal) => { exit = `code ${code}, signal ${signal}`; });
    child.on("error", (error) => { exit = `spawn error ${error.message}`; });
    child.stdout!.resume();
    child.stderr!.setEncoding("utf8").on("data", (chunk: string) => { stderr = (stderr + chunk).slice(-4_000); });

    if (!await until("attached", () => receipt()?.status === "attached" && receipt()?.connected === true,
      () => `OMP PID ${receipt()?.pid} (launcher ${child!.pid}) attached to the loopback Host`)) return report();
    const originalPid = receipt()!.pid;

    // Cutover begins: the marker first, then the Host goes away and the
    // installation rotates the Host credential.
    writeFileSync(marker, JSON.stringify({ format: 1, cutoverId: "smoke" }));
    host.server.stop(true);
    writeClient(profile, stateRoot, port, "rotated-token");
    if (!await until("disconnected", () => receipt()?.connected === false,
      () => "the session observed the Host absence and stayed attached-but-disconnected")) return report();

    // A fresh Host answers while the marker stands: it forgot the instance
    // and accepts only the rotated credential.
    host = fakeHost(port, join(root, "homes/kintsu"), "rotated-token");
    await sleep(HELD_MS);
    const asked = host.actions.filter((action) => action !== "attach").length;
    record("held_during_cutover", asked > 0 && host.attaches() === 0 && receipt()?.status === "attached" && !exit,
      `fresh Host saw ${asked} non-attach request(s) and ${host.attaches()} attach(es) in ${HELD_MS} ms; receipt ${JSON.stringify(receipt())}`);

    unlinkSync(marker);
    await until("reconnected", () => receipt()?.connected === true && receipt()?.hostToken === "rotated-token",
      () => "the same OMP process passed the real reconnect gate, reloaded the rotated token, and attached again");
    record("omp_alive", !exit && receipt()?.pid === originalPid,
      exit ? `OMP exited: ${exit}` : `OMP PID ${originalPid} stayed ${receipt()?.pid} through the cutover`);
  } catch (error) {
    record("runner", false, error instanceof Error ? error.message : String(error));
  } finally {
    host.server.stop(true);
    child?.stdin?.end();
    for (let wait = 0; wait < 30 && child && !exit; wait++) await sleep(100);
    if (child) killTree(child);
    for (let wait = 0; wait < 20 && child && !exit; wait++) await sleep(100);
    if (child && !exit) record("cleanup", false, `OMP PID ${child.pid} did not exit after termination`);
    try {
      rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
    } catch (error) {
      record("cleanup", false, `could not remove ${root}: ${error instanceof Error ? error.message : String(error)}`);
    }
  }
  return report();
}

process.exit(await main());
