import { createHash, randomUUID } from "node:crypto";
import path from "node:path";
import { tmpdir } from "node:os";
import { mkdir, readFile, unlink, writeFile } from "node:fs/promises";

const EVAL_DIR = path.dirname(new URL(import.meta.url).pathname.replace(/^\/(?:[A-Za-z]:)/, (value) => value.slice(1)));
const REPO_ROOT = path.resolve(EVAL_DIR, "..");
const OMP_ROOT = path.resolve(EVAL_DIR, "../../solarisael-house-omp");
const ROOM_DIR = path.resolve(process.env.SOLARISAEL_DAILY_ROOM_DIR || "C:/Solarisael/Obsidian/obsidian/tuner");
const SESSION_ROOT = path.resolve(process.env.SOLARISAEL_OMP_SESSION_ROOT || "C:/Users/Administrador/.omp/agent/sessions");
const TELEMETRY_PATH = path.join(ROOM_DIR, ".omp", "runtime", "recall-turns.jsonl");
const OUTPUT_DIR = path.join(ROOM_DIR, ".omp", "runtime", "daily-memory-eval");
const SAMPLE_SIZE = Math.max(1, Number(process.env.SOLARISAEL_DAILY_SAMPLE || 4));
const REPLAY_EXTENSION = path.join(EVAL_DIR, "replay-recall-extension.ts");
const HYGIENE_EXTENSION = path.join(OMP_ROOT, "hygiene.ts");
const REPLAY_TIMEOUT_MS = Math.max(30_000, Number(process.env.SOLARISAEL_DAILY_REPLAY_TIMEOUT_MS || 300_000));

const text = (value: unknown): string => value == null ? "" : String(value);
const hash = (value: string): string => createHash("sha256").update(value).digest("hex");
const messageText = (message: any): string => Array.isArray(message?.content)
  ? message.content.filter((item: any) => item?.type === "text").map((item: any) => text(item.text)).join("\n")
  : text(message?.content);

function parseJsonLines(source: string): any[] {
  return source.split(/\r?\n/).filter(Boolean).flatMap((line) => {
    try { return [JSON.parse(line)]; } catch { return []; }
  });
}

function toolCalls(message: any): any[] {
  return Array.isArray(message?.content) ? message.content.filter((item: any) => item?.type === "toolCall") : [];
}

function safeOriginalTool(call: any): boolean {
  const name = text(call?.name);
  if (["read", "grep", "glob", "lsp", "recall", "anamnesis", "coding_lessons", "room_state", "house_lane_status"].includes(name)) return true;
  if (name === "write" && /^xd:\/(?:\/)?(?:recall|anamnesis|coding_lessons|room_state|house_lane_status)$/i.test(text(call?.arguments?.path))) return true;
  return false;
}

type Telemetry = {
  captured_at: string;
  session_id: string;
  room: string;
  prompt_sha256: string;
  prompt_chars: number;
  status: "injected" | "empty" | "skipped" | "error";
  route: any;
  viewport_diagnostics: any;
  viewport: any;
  error: string | null;
};

type JoinedTurn = {
  turn_id: string;
  telemetry: Telemetry;
  session_path: string;
  session_lines_before_turn: string[];
  user_turn: string;
  original_answer: string;
  original_tool_calls: any[];
  safe_to_replay: boolean;
};

async function sessionIndex(): Promise<{ byId: Map<string, string>; roomPaths: string[] }> {
  const byId = new Map<string, string>();
  const glob = new Bun.Glob("**/*.jsonl");
  for await (const relative of glob.scan({ cwd: SESSION_ROOT, absolute: false, onlyFiles: true })) {
    const match = /_(019[0-9a-f-]+)\.jsonl$/i.exec(relative);
    if (match && !relative.includes("/" + path.basename(relative, ".jsonl") + "/")) {
      byId.set(match[1], path.join(SESSION_ROOT, relative));
    }
  }
  const roomKey = `--${ROOM_DIR.replace(/[:\\/]/g, "-")}--`;
  const roomPaths = [...byId.values()].filter((sessionPath) => path.basename(path.dirname(sessionPath)) === roomKey);
  return { byId, roomPaths };
}

function joinTurn(telemetry: Telemetry, sessionPath: string, source: string): JoinedTurn | null {
  const lines = source.split(/\r?\n/);
  const events = parseJsonLines(source);
  const capturedAt = Date.parse(telemetry.captured_at);
  const targetMatches = events
    .map((event, index) => ({ event, index }))
    .filter(({ event }) => event?.type === "message"
      && event?.message?.role === "user"
      && hash(messageText(event.message)) === telemetry.prompt_sha256)
    .sort((left, right) => {
      const leftDelta = Math.abs(capturedAt - Date.parse(left.event.timestamp || ""));
      const rightDelta = Math.abs(capturedAt - Date.parse(right.event.timestamp || ""));
      return leftDelta - rightDelta;
    });
  if (!targetMatches.length) return null;
  const { event: target, index: targetIndex } = targetMatches[0];
  const session = events.find((event) => event?.type === "session");
  const rawLineIndex = lines.findIndex((line) => {
    try { return JSON.parse(line)?.id === target.id; } catch { return false; }
  });
  const nextUserIndex = events.findIndex((event, index) => index > targetIndex && event?.type === "message" && event?.message?.role === "user");
  const between = events.slice(targetIndex + 1, nextUserIndex >= 0 ? nextUserIndex : events.length);
  const assistantMessages = between.filter((event) => event?.type === "message" && event?.message?.role === "assistant").map((event) => event.message);
  const calls = assistantMessages.flatMap(toolCalls);
  const originalAnswer = assistantMessages.map(messageText).filter(Boolean).at(-1) || "";
  return {
    turn_id: `${session?.id || telemetry.session_id || path.basename(sessionPath)}:${target.id}`,
    telemetry,
    session_path: sessionPath,
    session_lines_before_turn: lines.slice(0, rawLineIndex).filter(Boolean),
    user_turn: messageText(target.message),
    original_answer: originalAnswer,
    original_tool_calls: calls.map((call) => ({ name: call.name, arguments: call.arguments })),
    safe_to_replay: calls.every(safeOriginalTool),
  };
}

async function loadJoinedTurns(): Promise<{ telemetry: Telemetry[]; joined: JoinedTurn[] }> {
  let source = "";
  try { source = await readFile(TELEMETRY_PATH, "utf8"); } catch { return { telemetry: [], joined: [] }; }
  const telemetry = parseJsonLines(source) as Telemetry[];
  const sessions = await sessionIndex();
  const sessionSources = new Map<string, string>();
  const joinedByTurn = new Map<string, JoinedTurn>();
  for (const entry of telemetry) {
    const directPath = sessions.byId.get(entry.session_id);
    const candidates = directPath ? [directPath] : sessions.roomPaths;
    for (const sessionPath of candidates) {
      let sessionSource = sessionSources.get(sessionPath);
      if (sessionSource === undefined) {
        sessionSource = await readFile(sessionPath, "utf8");
        sessionSources.set(sessionPath, sessionSource);
      }
      const turn = joinTurn(entry, sessionPath, sessionSource);
      if (!turn) continue;
      if (!joinedByTurn.has(turn.turn_id)) joinedByTurn.set(turn.turn_id, turn);
      break;
    }
  }
  return { telemetry, joined: [...joinedByTurn.values()] };
}

function parseAssistant(stdout: string): { answer: string; model: string; provider: string; tool_calls: string[] } {
  let finalMessage: any = null;
  const calls: string[] = [];
  for (const event of parseJsonLines(stdout)) {
    if (event?.type === "message_end" && event?.message?.role === "assistant") {
      finalMessage = event.message;
      calls.push(...toolCalls(event.message).map((call) => text(call.name)));
    }
  }
  if (!finalMessage) throw new Error("replay produced no assistant response");
  return {
    answer: messageText(finalMessage),
    model: finalMessage.model || "unknown",
    provider: finalMessage.provider || "unknown",
    tool_calls: calls,
  };
}

async function replayTurn(turn: JoinedTurn, condition: "memory" | "no-memory") {
  const id = randomUUID();
  const prefixPath = path.join(tmpdir(), `solarisael-prefix-${id}.jsonl`);
  const viewportPath = path.join(tmpdir(), `solarisael-viewport-${id}.json`);
  await writeFile(prefixPath, `${turn.session_lines_before_turn.join("\n")}\n`, "utf8");
  await writeFile(viewportPath, JSON.stringify(turn.telemetry.viewport || { found: false }), "utf8");
  const extensionArgs = condition === "memory"
    ? ["--no-extensions", "-e", HYGIENE_EXTENSION, "-e", REPLAY_EXTENSION]
    : ["--no-extensions", "-e", HYGIENE_EXTENSION];
  const args = [
    "-p",
    "--mode=json",
    "--thinking=auto",
    "--no-tools",
    "-r", prefixPath,
    ...extensionArgs,
    turn.user_turn,
  ];
  const started = performance.now();
  const child = Bun.spawn(["omp", ...args], {
    cwd: ROOM_DIR,
    env: {
      ...process.env,
      NO_COLOR: "1",
      SOLARISAEL_RECALL_TELEMETRY: "0",
      SOLARISAEL_DISABLE_AUTO_RECALL: "1",
      SOLARISAEL_REPLAY_VIEWPORT: viewportPath,
      SOLARISAEL_REPLAY_MODE: "1",
    },
    stdout: "pipe",
    stderr: "pipe",
  });
  const timer = setTimeout(() => child.kill(), REPLAY_TIMEOUT_MS);
  try {
    const [stdout, stderr, code] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    if (code !== 0) throw new Error(stderr.trim() || `omp exited ${code}`);
    return { condition, ...parseAssistant(stdout), latency_ms: performance.now() - started };
  } finally {
    clearTimeout(timer);
    await Promise.all([unlink(prefixPath).catch(() => undefined), unlink(viewportPath).catch(() => undefined)]);
  }
}

function blindPair(turn: JoinedTurn, memory: any, noMemory: any) {
  const memoryIsA = Number.parseInt(hash(turn.turn_id).slice(0, 2), 16) % 2 === 0;
  return {
    turn_id: turn.turn_id,
    user_turn: turn.user_turn,
    conversation_source: turn.session_path,
    captured_at: turn.telemetry.captured_at,
    A: memoryIsA ? memory.answer : noMemory.answer,
    B: memoryIsA ? noMemory.answer : memory.answer,
    original_answer: turn.original_answer,
    sealed_mapping: { A: memoryIsA ? "memory" : "no-memory", B: memoryIsA ? "no-memory" : "memory" },
    replay: { memory, no_memory: noMemory },
  };
}

async function writeReport(data: { telemetry: Telemetry[]; joined: JoinedTurn[] }, pairs: any[] = []) {
  await mkdir(OUTPUT_DIR, { recursive: true });
  const statusCounts: Record<string, number> = {};
  for (const entry of data.telemetry) statusCounts[entry.status] = (statusCounts[entry.status] || 0) + 1;
  const report = {
    generated_at: new Date().toISOString(),
    room_dir: ROOM_DIR,
    telemetry_path: TELEMETRY_PATH,
    contract: {
      source: "real production user turns joined by session ID and SHA-256 of the exact user message",
      prompt_storage: "telemetry does not duplicate raw user text; OMP session logs remain the text authority",
      replay: "exact saved conversation prefix plus the captured automatic recall viewport; tools disabled to prevent future filesystem leakage",
      ground_truth: "Sol or his explicitly delegated blinded reviewer; mapping stays sealed until judgment",
      scope: "conversational turns only; engineering task outcomes require a separate isolated worktree replay",
    },
    observed: {
      telemetry_turns: data.telemetry.length,
      joined_turns: data.joined.length,
      safe_replay_turns: data.joined.filter((turn) => turn.safe_to_replay).length,
      status_counts: statusCounts,
      original_explicit_memory_tool_turns: data.joined.filter((turn) => turn.original_tool_calls.some((call) => /recall|anamnesis|coding_lessons/.test(call.name))).length,
    },
    blind_pairs: pairs.map(({ sealed_mapping, replay, original_answer: _original, ...pair }) => {
      const publicRun = ({ condition: _condition, answer: _answer, ...metadata }: any) => metadata;
      return {
        ...pair,
        replay_metadata: {
          A: publicRun(sealed_mapping.A === "memory" ? replay.memory : replay.no_memory),
          B: publicRun(sealed_mapping.B === "memory" ? replay.memory : replay.no_memory),
        },
      };
    }),
  };
  const runId = report.generated_at.replace(/\.\d{3}Z$/, "Z").replaceAll(":", "-");
  const reportPath = path.join(OUTPUT_DIR, `daily-memory-${runId}.json`);
  const sealedPath = path.join(OUTPUT_DIR, `.daily-memory-${runId}-mapping.json`);
  await writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`, "utf8");
  if (pairs.length) await writeFile(sealedPath, `${JSON.stringify(Object.fromEntries(pairs.map((pair) => [pair.turn_id, pair.sealed_mapping])), null, 2)}\n`, "utf8");
  console.log(`Daily memory audit: ${data.telemetry.length} telemetry turns, ${data.joined.length} joined, ${data.joined.filter((turn) => turn.safe_to_replay).length} safe to replay`);
  console.log(`Blind pairs: ${pairs.length}`);
  console.log(`wrote ${reportPath}`);
  if (pairs.length) console.log(`sealed mapping ${sealedPath}`);
}

async function main() {
  const data = await loadJoinedTurns();
  if (!process.argv.includes("--replay")) {
    await writeReport(data);
    if (!data.telemetry.length) console.log("Collection is enabled, but no post-installation real turns exist yet. No daily-use score is available.");
    return;
  }
  const eligible = data.joined
    .filter((turn) => turn.safe_to_replay && turn.telemetry.status === "injected" && turn.telemetry.viewport?.found)
    .sort((left, right) => right.telemetry.captured_at.localeCompare(left.telemetry.captured_at))
    .slice(0, SAMPLE_SIZE);
  const pairs = [];
  for (const turn of eligible) {
    const [memory, noMemory] = await Promise.all([replayTurn(turn, "memory"), replayTurn(turn, "no-memory")]);
    pairs.push(blindPair(turn, memory, noMemory));
    console.log(`replayed ${turn.turn_id}`);
  }
  await writeReport(data, pairs);
}

await main();
