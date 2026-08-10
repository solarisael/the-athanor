import { describe, expect, test } from "bun:test";
import { spawn } from "node:child_process";
import path from "node:path";
import { promisify } from "node:util";
import { discoverRustExecutable } from "../discovery.ts";
import solarisaelHouseProof from "../index.ts";
import { roomContext } from "../solarisael-house-proof/room.ts";
import { windowsPathToWsl } from "../solarisael-house-proof/substrate.ts";

const enabled = process.env.SOLARISAEL_I_UNDERSTAND_THIS_IS_AN_ISOLATED_POSTGRES_TEST === "1";
const execFile = promisify((command: string, args: string[], options: object, callback: (error: Error | null, result?: { stdout: string; stderr: string }) => void) => {
  const child = spawn(command, args, { ...options, windowsHide: true });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => { stdout += String(chunk); });
  child.stderr.on("data", (chunk) => { stderr += String(chunk); });
  child.on("error", callback);
  child.on("exit", (code) => code === 0
    ? callback(null, { stdout, stderr })
    : callback(new Error(`${command} exited ${code}: ${stderr || stdout}`)));
});

const TOTAL_TIMEOUT_MS = 270_000;
const OP_TIMEOUT_MS = 110_000;
const IDLE_MS = 75_000;

type Schema = {
  describe(description: string): Schema;
  optional(): Schema;
  default(value: unknown): Schema;
};
type Tool = { name: string; execute?: (...args: any[]) => Promise<any> };

function schema(): Schema {
  return {
    describe() { return this; },
    optional() { return this; },
    default() { return this; },
  };
}
const zod = {
  string: schema,
  boolean: schema,
  number: schema,
  enum: (_values: string[]) => schema(),
  array: (_item: Schema) => schema(),
  object: (_shape: Record<string, Schema>) => schema(),
};

function registerActualAdapter() {
  const tools: Tool[] = [];
  const pi = {
    zod,
    setLabel() {},
    on() {},
    registerTool(tool: Tool) { tools.push(tool); },
  };
  solarisaelHouseProof(pi);
  return new Map(tools.map((tool) => [tool.name, tool]));
}

async function bounded<T>(label: string, operation: (signal: AbortSignal) => Promise<T>, timeoutMs = OP_TIMEOUT_MS): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(new Error(`${label} timed out after ${timeoutMs}ms`)), timeoutMs);
  try {
    return await Promise.race([
      operation(controller.signal),
      new Promise<T>((_, reject) => controller.signal.addEventListener("abort", () => reject(controller.signal.reason), { once: true })),
    ]);
  } finally {
    clearTimeout(timer);
  }
}

function text(result: any): string {
  return String(result?.content?.find((item: any) => item?.type === "text")?.text || "");
}

async function queryAndCleanup(room: string, records: Array<{ id: number; sourcePath: string; title: string }>) {
  const substrate = process.env.ATHANOR_SUBSTRATE_ROOT;
  const testDatabase = process.env.SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL;
  if (!substrate || !testDatabase) throw new Error("Isolated PostgreSQL test requires ATHANOR_SUBSTRATE_ROOT and SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL");
  const script = [
    "import json, os, psycopg",
    "url=os.environ['SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL']",
    "conn=psycopg.connect(url)",
    "cur=conn.cursor()",
    "records=json.loads(os.environ['SOLARISAEL_OMP_RECORDS'])",
    "deleted=[]",
    "for record in records:\n cur.execute('DELETE FROM memories WHERE id=%s AND room=%s AND source_path=%s AND title=%s RETURNING id',(record['id'],os.environ['SOLARISAEL_OMP_ROOM'],record['sourcePath'],record['title']))\n deleted.extend(row[0] for row in cur.fetchall())",
    "ids=[record['id'] for record in records]",
    "cur.execute('SELECT count(*) FROM memories WHERE id = ANY(%s)',(ids,))",
    "rows=cur.fetchone()[0]",
    "cur.execute('SELECT count(*) FROM memory_chunks WHERE memory_id = ANY(%s)',(ids,))",
    "chunks=cur.fetchone()[0]",
    "conn.commit(); print(json.dumps({'deleted':deleted,'rows':rows,'chunks':chunks}))",
  ].join("\n");
  const result = await execFile("wsl.exe", ["--cd", "~", "python3", "-c", script], {
    env: {
      ...process.env,
      WSLENV: [
        process.env.WSLENV,
        "SOLARISAEL_OMP_SUBSTRATE_DIR",
        "SOLARISAEL_OMP_ROOM",
        "SOLARISAEL_OMP_RECORDS",
        "SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL",
      ].filter(Boolean).join(":"),
      SOLARISAEL_OMP_SUBSTRATE_DIR: windowsPathToWsl(substrate),
      SOLARISAEL_OMP_ROOM: room,
      SOLARISAEL_OMP_RECORDS: JSON.stringify(records),
      SOLARISAEL_SUBSTRATE_TEST_DATABASE_URL: testDatabase,
    },
  });
  return JSON.parse(result.stdout.trim());
}

describe.skipIf(!enabled)("isolated PostgreSQL seam", () => {
  test("registered remember and recall survive a 75-second WSL idle boundary", async () => {
    const configuredRust = discoverRustExecutable();
    expect(configuredRust).toBeString();
    expect(configuredRust).toBe(path.resolve(process.env.ATHANOR_SUBSTRATE_EXE!));
    const started = Date.now();
    const tools = registerActualAdapter();
    const remember = tools.get("remember");
    const recall = tools.get("recall");
    expect(remember?.execute).toBeFunction();
    expect(recall?.execute).toBeFunction();

    const cwd = process.env.SOLARISAEL_OMP_TEST_ROOM_DIR || process.cwd();
    const { room } = roomContext(cwd);
    const token = `omp-postgres-${process.pid}`;
    const records: Array<{ id: number; sourcePath: string; title: string }> = [];
    const first = {
      title: `Production PostgreSQL integration ${token}-a`,
      body: `Exact durable body marker ${token}-a; Rust WSL keepalive must preserve this lexical payload.`,
    };
    const second = {
      title: `Production PostgreSQL integration ${token}-b`,
      body: `Exact durable body marker ${token}-b; Rust WSL keepalive must preserve this lexical payload.`,
    };
    const ctx = { cwd };
    const previousKeepalive = process.env.SOLARISAEL_PG_WSL;
    process.env.SOLARISAEL_PG_WSL = "1";

    try {
      const rememberOne = async (entry: typeof first, callId: string) => {
        const written = await bounded(`remember ${callId}`, (signal) => remember!.execute!(callId, {
          title: entry.title, body: entry.body, threads: ["integration"],
        }, signal, undefined, ctx));
        expect(written?.isError).not.toBe(true);
        const receipt = JSON.parse(text(written));
        if (Number.isInteger(receipt.id) && typeof receipt.source_path === "string") {
          records.push({ id: receipt.id, sourcePath: receipt.source_path, title: entry.title });
        }
        expect(receipt.ok).toBe(true);
        expect(Number.isInteger(receipt.id)).toBe(true);
        expect(receipt.durable).toBe(true);
        expect(receipt.authority).toBe("postgres");
        expect(typeof receipt.source_path).toBe("string");
        expect(receipt.source_path.startsWith("memory/omp_")).toBe(true);
      };
      const assertRecall = async (entry: typeof first, callId: string) => {
        const result = await bounded(`recall ${callId}`, (signal) => recall!.execute!(callId, { query: entry.body }, signal, undefined, ctx));
        expect(result?.isError).not.toBe(true);
        const payload = JSON.parse(text(result));
        expect(payload.ok).toBe(true);
        expect(payload.found).toBe(true);
        const candidates = [...(payload.retrievalCandidates || []), ...(payload.contentChunks || []), ...(payload.semanticChunks || [])];
        expect(candidates.some((candidate: any) => records.some((record) => candidate.source_path === record.sourcePath && (candidate.excerpt === entry.body || candidate.body === entry.body)))).toBe(true);
      };
      await rememberOne(first, "production-postgres-remember-a");
      await assertRecall(first, "production-postgres-recall-a");
      await bounded("WSL idle boundary", () => new Promise((resolve) => setTimeout(resolve, IDLE_MS)), IDLE_MS + 2_000);
      await rememberOne(second, "production-postgres-remember-b");
      await assertRecall(second, "production-postgres-recall-b");
      expect(Date.now() - started).toBeLessThan(TOTAL_TIMEOUT_MS);
    } finally {
      try {
        const cleanup = await bounded("guarded PostgreSQL cleanup", () => queryAndCleanup(room, records), OP_TIMEOUT_MS);
        expect(cleanup.deleted.sort()).toEqual(records.map((record) => record.id).sort());
        expect(cleanup.rows).toBe(0);
        expect(cleanup.chunks).toBe(0);
      } finally {
        if (previousKeepalive === undefined) delete process.env.SOLARISAEL_PG_WSL;
        else process.env.SOLARISAEL_PG_WSL = previousKeepalive;
      }
    }
  }, TOTAL_TIMEOUT_MS);
});
