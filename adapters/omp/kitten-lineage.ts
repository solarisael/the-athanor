import { readFile } from "node:fs/promises";

type TaskInput = {
  name?: unknown;
  agent?: unknown;
  task?: unknown;
};

type SettledTaskResult = {
  index?: unknown;
  id?: unknown;
  agent?: unknown;
  task?: unknown;
  output?: unknown;
  stderr?: unknown;
  exitCode?: unknown;
  aborted?: unknown;
  abortReason?: unknown;
  error?: unknown;
};

export type KittenQuestProgress = {
  id?: unknown;
  index?: unknown;
  agent?: unknown;
  task?: unknown;
  assignment?: unknown;
  sessionFile?: unknown;
  parentToolCallId?: unknown;
};

export type KittenQuestLifecycle = {
  id?: unknown;
  index?: unknown;
  agent?: unknown;
  status?: unknown;
  sessionFile?: unknown;
  parentToolCallId?: unknown;
};

export type KittenQuestMemory = {
  resultId: string;
  title: string;
  body: string;
  threads: [string, string];
};

const compactLine = (value: unknown): string => String(value ?? "").replace(/\s+/g, " ").trim();

export function kittenLifecycleJoinKey(payload: unknown): string {
  const event = payload && typeof payload === "object" && !Array.isArray(payload)
    ? payload as Record<string, unknown>
    : {};
  const parentToolCallId = compactLine(event.parentToolCallId);
  const index = Number(event.index);
  if (parentToolCallId && Number.isInteger(index) && index >= 0) return `${parentToolCallId}:${index}`;
  return compactLine(event.id);
}

export function lineageSlug(value: unknown, fallback = "unknown"): string {
  const slug = compactLine(value)
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 63)
    .replace(/-+$/g, "");
  return slug || fallback;
}

function targetSection(task: string): string {
  const match = task.match(/(?:^|\n)#{1,6}\s+Target\s*\n([\s\S]*?)(?=\n#{1,6}\s+|$)/i);
  const section = match?.[1]
    ?.split(/\r?\n/)
    .map((line) => line.replace(/^\s*[-*]\s*/, "").trim())
    .find(Boolean);
  return section || task.split(/\r?\n/).map((line) => line.trim()).find(Boolean) || "general";
}

export function questDomain(task: string): string {
  const target = targetSection(task)
    .replace(/[`*_]/g, "")
    .replace(/[A-Za-z]:[\\/][^\s,;]+/g, (path) => path.split(/[\\/]/).filter(Boolean).slice(-2).join(" "))
    .replace(/(?:\.\.?[\\/])+/g, " ");
  return lineageSlug(target, "general");
}

function reportText(result: SettledTaskResult): string {
  const output = String(result.output ?? "").trim();
  const error = compactLine(result.error || result.abortReason);
  const stderr = String(result.stderr ?? "").trim();
  if (output) return output;
  if (error) return error;
  if (stderr) return stderr;
  return Number(result.exitCode) === 0 ? "Quest completed without a textual report." : "Quest ended without a textual report.";
}

export function extractKittenQuestMemories(input: unknown, details: unknown): KittenQuestMemory[] {
  const taskInput = input && typeof input === "object" ? input as Record<string, unknown> : {};
  const tasks = Array.isArray(taskInput.tasks) ? taskInput.tasks as TaskInput[] : [];
  const taskDetails = details && typeof details === "object" ? details as Record<string, unknown> : {};
  const results = Array.isArray(taskDetails.results) ? taskDetails.results as SettledTaskResult[] : [];

  return results.flatMap((result, position) => {
    if (!Number.isInteger(result.exitCode)) return [];
    const index = Number.isInteger(result.index) ? Number(result.index) : position;
    const requested = tasks[index] || {};
    const kitten = compactLine(requested.name || result.id) || `kitten-${index + 1}`;
    const quest = String(result.task ?? requested.task ?? "").trim();
    if (!quest) return [];
    const domain = questDomain(quest);
    const status = result.aborted === true ? "aborted" : Number(result.exitCode) === 0 ? "completed" : "failed";
    const agent = compactLine(result.agent || requested.agent);
    const report = reportText(result);
    const body = [
      "Quest",
      quest,
      "",
      "Report",
      report,
      "",
      `Outcome: ${status}${agent ? `; role: ${agent}` : ""}.`,
    ].join("\n");
    return [{
      resultId: compactLine(result.id) || `${index}`,
      title: `${kitten} quest: ${compactLine(targetSection(quest)).slice(0, 120)}`,
      body,
      threads: [`kitten:${lineageSlug(kitten)}`, `domain:${domain}`],
    } satisfies KittenQuestMemory];
  });
}

export function extractKittenLifecycleMemory(
  progress: KittenQuestProgress,
  lifecycle: KittenQuestLifecycle,
  report: string,
): KittenQuestMemory | null {
  const quest = String(progress.assignment ?? progress.task ?? "").trim();
  const id = compactLine(lifecycle.id || progress.id);
  if (!quest || !id) return null;
  const status = compactLine(lifecycle.status);
  const [memory] = extractKittenQuestMemories({
    tasks: [{
      name: id,
      agent: progress.agent || lifecycle.agent,
      task: quest,
    }],
  }, {
    results: [{
      index: 0,
      id,
      agent: lifecycle.agent || progress.agent,
      task: quest,
      output: report,
      exitCode: status === "completed" ? 0 : 1,
      aborted: status === "aborted",
      abortReason: status === "aborted" ? "Quest aborted." : undefined,
      error: status === "failed" ? "Quest failed." : undefined,
    }],
  });
  return memory || null;
}

export function kittenReportPath(sessionFile: unknown): string | null {
  const source = String(sessionFile ?? "").trim();
  if (!source) return null;
  return /\.jsonl$/i.test(source) ? source.replace(/\.jsonl$/i, ".md") : `${source}.md`;
}

export async function readKittenReport(sessionFile: unknown): Promise<string> {
  const reportPath = kittenReportPath(sessionFile);
  if (!reportPath) return "";
  try {
    return (await readFile(reportPath, "utf8")).trim();
  } catch {
    return "";
  }
}

export function kittenQuestIdempotencyKey(toolCallId: unknown, resultId: unknown): string {
  return `${compactLine(toolCallId)}:${compactLine(resultId)}`;
}

type KittenLineageDiagnostics = {
  progressEvents: number;
  lifecycleEvents: number;
  lifecycleWithoutProgress: number;
  writeCommitted: number;
  writeFailed: number;
  lastProgressId: string | null;
  lastLifecycleId: string | null;
  lastLifecycleStatus: string | null;
  lastProgressKeys: string[];
  lastLifecycleKeys: string[];
};

const lineageDiagnostics: KittenLineageDiagnostics = {
  progressEvents: 0,
  lifecycleEvents: 0,
  lifecycleWithoutProgress: 0,
  writeCommitted: 0,
  writeFailed: 0,
  lastProgressId: null,
  lastLifecycleId: null,
  lastLifecycleStatus: null,
  lastProgressKeys: [],
  lastLifecycleKeys: [],
};

function safeKeys(payload: unknown): string[] {
  return payload && typeof payload === "object" && !Array.isArray(payload)
    ? Object.keys(payload).sort().slice(0, 24)
    : [];
}

export function noteKittenProgress(payload: unknown, id: string): void {
  lineageDiagnostics.progressEvents += 1;
  lineageDiagnostics.lastProgressId = id || null;
  lineageDiagnostics.lastProgressKeys = safeKeys(payload);
}

export function noteKittenLifecycle(payload: unknown, id: string, progressFound: boolean): void {
  lineageDiagnostics.lifecycleEvents += 1;
  if (!progressFound) lineageDiagnostics.lifecycleWithoutProgress += 1;
  lineageDiagnostics.lastLifecycleId = id || null;
  lineageDiagnostics.lastLifecycleStatus = compactLine((payload as Record<string, unknown>)?.status) || null;
  lineageDiagnostics.lastLifecycleKeys = safeKeys(payload);
}

export function noteKittenLineageWrite(committed: boolean): void {
  if (committed) lineageDiagnostics.writeCommitted += 1;
  else lineageDiagnostics.writeFailed += 1;
}

export function kittenLineageDiagnostics(): KittenLineageDiagnostics {
  return {
    ...lineageDiagnostics,
    lastProgressKeys: [...lineageDiagnostics.lastProgressKeys],
    lastLifecycleKeys: [...lineageDiagnostics.lastLifecycleKeys],
  };
}

export function kittenLineageDisabled(environ: Record<string, string | undefined> = process.env): boolean {
  return environ.SOLARISAEL_REPLAY_MODE === "1" || environ.ATHANOR_DISABLE_KITTEN_LINEAGE === "1";
}
