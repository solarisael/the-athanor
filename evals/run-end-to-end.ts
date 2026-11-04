import { createHash, randomUUID } from "node:crypto";
import path from "node:path";
import { tmpdir } from "node:os";
import { unlink } from "node:fs/promises";
import { runRecallQuery } from "../src/memory.ts";
import { classifyRetrievalQuery } from "../src/query-routing.ts";
import { runWsl, windowsPathToWsl } from "../src/wsl.ts";
import { compactRecall } from "../../solarisael-house-omp/solarisael-house-proof/recall.ts";
import { automaticRecallViewport } from "../../solarisael-house-omp/solarisael-house-proof/recall-viewport.ts";
import { resolveEntities } from "../../solarisael-house-omp/solarisael-house-proof/entity-resolution.ts";
import { scoreTrial, summarizeTrials, type JudgeResult } from "./end-to-end-metrics.ts";

const EVAL_DIR = path.dirname(new URL(import.meta.url).pathname.replace(/^\/(?:[A-Za-z]:)/, (value) => value.slice(1)));
const REPO_ROOT = path.resolve(EVAL_DIR, "..");
const RUNTIME_ROOT = path.resolve(
  process.env.SOLARISAEL_OBSIDIAN_ROOT
  || process.env.SOLARISAEL_VAULT_ROOT
  || "C:/Solarisael/Obsidian/obsidian",
);
const SUBSTRATE_DIR = process.env.SOLARISAEL_SUBSTRATE
  ? path.resolve(process.env.SOLARISAEL_SUBSTRATE)
  : path.join(RUNTIME_ROOT, "house", "substrate");
const SCENARIO_PATH = path.join(EVAL_DIR, "living-scenarios.json");
const STARTED_AT = new Date();
const RUN_ID = STARTED_AT.toISOString().replace(/\.\d{3}Z$/, "Z").replaceAll(":", "-");
const OUTPUT_STEM = process.env.SOLARISAEL_E2E_OUTPUT || `end-to-end-memory-eval-${RUN_ID}`;
const ANSWER_MODEL = process.env.SOLARISAEL_E2E_ANSWER_MODEL || "";
const JUDGE_MODEL = process.env.SOLARISAEL_E2E_JUDGE_MODEL || ANSWER_MODEL;
const CONCURRENCY = Math.max(1, Number(process.env.SOLARISAEL_E2E_CONCURRENCY || 3));
const MODEL_TIMEOUT_MS = Math.max(30_000, Number(process.env.SOLARISAEL_E2E_MODEL_TIMEOUT_MS || 180_000));

const ANSWER_SYSTEM_PROMPT = `You are the answer model in a Solarisael memory evaluation.
Answer the user turn as you normally would, using the supplied automatic recall viewport when it is relevant.
Retrieved evidence can be incomplete, irrelevant, historical, or stale. Decide what deserves authority.
Do not invent missing facts. State uncertainty when the evidence is insufficient.
Return only strict JSON with this shape:
{"answer":"natural prose answer","citations":["E1","D1"]}
Citations must contain only evidence_id values from the supplied viewport that actually support the answer.`;

const JUDGE_SYSTEM_PROMPT = `You are an independent evaluator of a memory-assisted answer.
The REFERENCE PACKET is the authority oracle. It contains live current records selected by active unsuperseded thread, or explicit historical records for a historical scenario.
The AUTOMATIC RECALL VIEWPORT is evidence available to the answer model. It is not automatically authoritative.
Judge semantics, not exact wording. Do not reward a criterion merely because its wording appears in the evaluation prompt.
For each criterion, report whether the answer met it and whether the automatic viewport contained enough evidence to meet it.
For each risk, report whether the answer violated it.
Mark answer claims unsupported when neither the reference packet nor ordinary reasoning supports them.
Assess citations against the cited viewport item.
Return only strict JSON with this shape:
{"criteria":[{"id":"criterion-id","status":"met|partial|miss","evidence_available":true,"explanation":"brief"}],"risks":[{"id":"risk-id","violated":false,"explanation":"brief"}],"unsupported_claims":[],"temporal_correct":true,"uncertainty_correct":true,"citation_support":[{"citation_id":"E1","supported":true,"explanation":"brief"}],"overall_pass":true,"summary":"brief"}`;

type ScenarioCriterion = { id: string; text: string };
type ScenarioRisk = { id: string; text: string };
type Scenario = {
  id: string;
  room: string;
  user_turn: string;
  reference: {
    mode: "current" | "historical";
    rooms?: string[];
    threads?: string[];
    memory_ids?: number[];
  };
  distractor_memory_ids?: number[];
  criteria: ScenarioCriterion[];
  risks: ScenarioRisk[];
};
type ScenarioSuite = { version: number; default_trials: number; scenarios: Scenario[] };
type ReferenceRecord = {
  id: number;
  room: string;
  title: string;
  source_path: string;
  created_at: string;
  archived_at: string | null;
  superseded_by: number | null;
  threads: string[];
  body: string;
};
type ModelRun = {
  parsed: any;
  raw: string;
  model: string;
  provider: string;
  usage: Record<string, unknown>;
  latency_ms: number;
};
type PreparedScenario = {
  scenario: Scenario;
  reference: ReferenceRecord[];
  distractors: ReferenceRecord[];
  fullViewport: Record<string, any>;
  retrieval: Record<string, any>;
};

type TrialRecord = {
  scenario_id: string;
  variant: "full" | "no-recall" | "stale-distractor";
  trial: number;
  answer: { answer: string; citations: string[] };
  judge: JudgeResult;
  score: ReturnType<typeof scoreTrial>;
  answer_model: Omit<ModelRun, "parsed" | "raw">;
  judge_model: Omit<ModelRun, "parsed" | "raw">;
};

function shellQuote(value: unknown): string {
  return `'${String(value ?? "").replaceAll("'", `'"'"'`)}'`;
}

function sqlLiteral(value: unknown): string {
  return `'${String(value ?? "").replaceAll("'", "''")}'`;
}

async function runPsql(sql: string, timeoutMs = 30_000): Promise<string> {
  const command = [
    `cd ${shellQuote(windowsPathToWsl(SUBSTRATE_DIR))}`,
    "set -a && . ./.env && set +a",
    `psql -X -A -t -c ${shellQuote(sql)}`,
  ].join(" && ");
  const outcome = await runWsl({
    argv: ["-d", "Ubuntu-24.04", "--", "bash", "-lc", command],
    cwd: REPO_ROOT,
    timeoutMs,
  });
  if (outcome.timedOut) throw new Error("reference query timed out");
  if (outcome.spawnError) throw new Error(outcome.spawnError);
  if (outcome.code !== 0) throw new Error(outcome.stderr.trim() || `psql exited ${outcome.code}`);
  return outcome.stdout;
}

async function loadReferenceRecords(reference: Scenario["reference"]): Promise<ReferenceRecord[]> {
  const conditions: string[] = [];
  if (reference.mode === "current") {
    const rooms = (reference.rooms || []).map(sqlLiteral).join(",");
    const threads = (reference.threads || []).map(sqlLiteral).join(",");
    if (!rooms || !threads) throw new Error("current reference requires rooms and threads");
    conditions.push(`m.room IN (${rooms})`);
    conditions.push("m.archived_at IS NULL");
    conditions.push("m.superseded_by IS NULL");
    conditions.push(`EXISTS (SELECT 1 FROM memory_threads wanted WHERE wanted.memory_id = m.id AND wanted.thread_key IN (${threads}))`);
  } else {
    const ids = (reference.memory_ids || []).map(Number).filter(Number.isInteger);
    if (!ids.length) throw new Error("historical reference requires memory_ids");
    conditions.push(`m.id IN (${ids.join(",")})`);
  }
  const sql = `SELECT json_build_object(
    'id', m.id,
    'room', m.room,
    'title', m.title,
    'source_path', m.source_path,
    'created_at', m.created_at,
    'archived_at', m.archived_at,
    'superseded_by', m.superseded_by,
    'threads', COALESCE((SELECT json_agg(mt.thread_key ORDER BY mt.thread_key) FROM memory_threads mt WHERE mt.memory_id = m.id), '[]'::json),
    'body', left(m.body, 6000)
  )::text FROM memories m WHERE ${conditions.join(" AND ")} ORDER BY m.created_at DESC LIMIT 8;`;
  const lines = (await runPsql(sql)).trim().split(/\r?\n/).filter(Boolean);
  return lines.map((line) => JSON.parse(line) as ReferenceRecord);
}

async function loadRecordsById(ids: number[]): Promise<ReferenceRecord[]> {
  const normalized = [...new Set(ids.map(Number).filter(Number.isInteger))];
  if (!normalized.length) return [];
  return loadReferenceRecords({ mode: "historical", memory_ids: normalized });
}

function evidenceIds(viewport: Record<string, any>): string[] {
  return [
    ...(viewport.retrievalCandidates || []),
    ...(viewport.canonMatches || []),
    ...(viewport.dateMatches || []),
    ...(viewport.semanticChunks || []),
    ...(viewport.contentChunks || []),
  ].map((item) => String(item?.evidence_id || "")).filter(Boolean);
}

function assignEvidenceIds(viewport: Record<string, any>): Record<string, any> {
  return {
    ...viewport,
    retrievalCandidates: (viewport.retrievalCandidates || []).map((item: any, index: number) => ({ ...item, evidence_id: `E${index + 1}` })),
    canonMatches: (viewport.canonMatches || []).map((item: any, index: number) => ({ ...item, evidence_id: `C${index + 1}` })),
    dateMatches: (viewport.dateMatches || []).map((item: any, index: number) => ({ ...item, evidence_id: `D${index + 1}` })),
    semanticChunks: (viewport.semanticChunks || []).map((item: any, index: number) => ({ ...item, evidence_id: `S${index + 1}` })),
    contentChunks: (viewport.contentChunks || []).map((item: any, index: number) => ({ ...item, evidence_id: `T${index + 1}` })),
  };
}

async function buildAutomaticViewport(scenario: Scenario): Promise<{ viewport: Record<string, any>; retrieval: Record<string, any> }> {
  const roomDir = path.join(RUNTIME_ROOT, scenario.room);
  const preliminary = classifyRetrievalQuery(scenario.user_turn);
  let resolution: any = { ok: true, matches: [] };
  if (preliminary.entityResolutionSuggested) {
    try {
      resolution = await resolveEntities({ room: scenario.room, roomDir, query: scenario.user_turn });
    } catch {
      resolution = { ok: false, matches: [] };
    }
  }
  const route = classifyRetrievalQuery(scenario.user_turn, {
    recognizedEntities: (resolution.matches || []).map((match: any) => match.canonicalName),
  });
  if (!route.shouldAutoRecall) {
    return {
      viewport: assignEvidenceIds({ ok: true, query: route.recallQuery || scenario.user_turn, found: false, retrievalCandidates: [], canonMatches: [], semanticChunks: [], contentChunks: [], dateMatches: [] }),
      retrieval: { route, viewport: { kept: 0, suppressed: 0, reasons: {} } },
    };
  }
  const recalled = await runRecallQuery(roomDir, scenario.room, route.recallQuery || scenario.user_turn);
  const compact = compactRecall(recalled);
  const filtered = automaticRecallViewport(compact);
  const filteredCanon = filtered.keptCandidates.length ? compact.canonMatches : [];
  const automatic = assignEvidenceIds({
    ...compact,
    retrievalCandidates: filtered.keptCandidates,
    canonMatches: filteredCanon,
    semanticChunks: [],
    contentChunks: [],
    found: Boolean(filtered.keptCandidates.length || filteredCanon.length || compact.dateMatches?.length),
  });
  return {
    viewport: automatic,
    retrieval: {
      route,
      source: recalled.source,
      raw_candidate_count: recalled.retrievalCandidates?.length || 0,
      viewport: filtered.diagnostics,
    },
  };
}

function withDistractors(viewport: Record<string, any>, records: ReferenceRecord[]): Record<string, any> {
  if (!records.length) return viewport;
  const start = (viewport.retrievalCandidates || []).length;
  return {
    ...viewport,
    found: true,
    retrievalCandidates: [
      ...(viewport.retrievalCandidates || []),
      ...records.map((record, index) => ({
        evidence_id: `E${start + index + 1}`,
        source_path: record.room === "house" ? `house/${record.source_path}` : record.source_path,
        title: record.title,
        heading_path: "",
        sources: ["memory"],
        score: null,
        term_coverage: null,
        matched_terms: [],
        missing_terms: [],
        reasons: [],
        excerpt: record.body.slice(0, 900),
        created_at: record.created_at,
      })),
    ],
  };
}

function parseJsonText(raw: string): any {
  const stripped = raw.trim().replace(/^```(?:json)?\s*/i, "").replace(/\s*```$/, "");
  try {
    return JSON.parse(stripped);
  } catch {
    const start = stripped.indexOf("{");
    const end = stripped.lastIndexOf("}");
    if (start >= 0 && end > start) return JSON.parse(stripped.slice(start, end + 1));
    throw new Error(`model did not return JSON: ${raw.slice(0, 300)}`);
  }
}

async function runOmpJson({ prompt, systemPrompt, cwd, model }: { prompt: string; systemPrompt: string; cwd: string; model?: string }): Promise<ModelRun> {
  const promptPath = path.join(tmpdir(), `solarisael-eval-${randomUUID()}.txt`);
  await Bun.write(promptPath, prompt);
  const args = [
    "-p",
    "--mode=json",
    "--no-session",
    "--no-tools",
    "--no-extensions",
    "--no-skills",
    "--no-rules",
    "--thinking=low",
    `--system-prompt=${systemPrompt}`,
    ...(model ? [`--model=${model}`] : []),
    `@${promptPath}`,
  ];
  const started = performance.now();
  const child = Bun.spawn(["omp", ...args], {
    cwd,
    env: { ...process.env, NO_COLOR: "1" },
    stdout: "pipe",
    stderr: "pipe",
  });
  const timer = setTimeout(() => child.kill(), MODEL_TIMEOUT_MS);
  try {
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(child.stdout).text(),
      new Response(child.stderr).text(),
      child.exited,
    ]);
    if (exitCode !== 0) throw new Error(stderr.trim() || `omp exited ${exitCode}`);
    let assistant: any = null;
    for (const line of stdout.split(/\r?\n/).filter(Boolean)) {
      try {
        const event = JSON.parse(line);
        if (event.type === "message_end" && event.message?.role === "assistant") assistant = event.message;
      } catch {
        // OMP diagnostics can appear beside JSONL events. Only message_end is authoritative.
      }
    }
    const raw = (assistant?.content || []).filter((item: any) => item?.type === "text").map((item: any) => item.text).join("");
    if (!raw) throw new Error("OMP produced no assistant message");
    return {
      parsed: parseJsonText(raw),
      raw,
      model: assistant.model || "unknown",
      provider: assistant.provider || "unknown",
      usage: assistant.usage || {},
      latency_ms: performance.now() - started,
    };
  } finally {
    clearTimeout(timer);
    await unlink(promptPath).catch(() => undefined);
  }
}

function normalizeAnswer(parsed: any): { answer: string; citations: string[] } {
  const answer = typeof parsed?.answer === "string" ? parsed.answer.trim() : "";
  const citations = Array.isArray(parsed?.citations)
    ? [...new Set(parsed.citations.map((item: unknown) => String(item).trim()).filter(Boolean))]
    : [];
  if (!answer) throw new Error("answer model returned no answer text");
  return { answer, citations };
}

function normalizeJudge(parsed: any): JudgeResult {
  if (!Array.isArray(parsed?.criteria) || !Array.isArray(parsed?.risks)) throw new Error("judge returned an invalid contract");
  return {
    criteria: parsed.criteria,
    risks: parsed.risks,
    unsupported_claims: Array.isArray(parsed.unsupported_claims) ? parsed.unsupported_claims.map(String) : ["judge omitted unsupported_claims"],
    temporal_correct: parsed.temporal_correct === true,
    uncertainty_correct: parsed.uncertainty_correct === true,
    citation_support: Array.isArray(parsed.citation_support) ? parsed.citation_support : [],
    overall_pass: parsed.overall_pass === true,
    summary: typeof parsed.summary === "string" ? parsed.summary : "",
  };
}

async function prepareScenario(scenario: Scenario): Promise<PreparedScenario> {
  const [reference, distractors, recall] = await Promise.all([
    loadReferenceRecords(scenario.reference),
    loadRecordsById(scenario.distractor_memory_ids || []),
    buildAutomaticViewport(scenario),
  ]);
  if (!reference.length) throw new Error(`${scenario.id}: reference oracle resolved no records`);
  return { scenario, reference, distractors, fullViewport: recall.viewport, retrieval: recall.retrieval };
}

function variantViewport(prepared: PreparedScenario, variant: TrialRecord["variant"]): Record<string, any> {
  if (variant === "no-recall") {
    return assignEvidenceIds({ ok: true, query: prepared.scenario.user_turn, found: false, retrievalCandidates: [], canonMatches: [], semanticChunks: [], contentChunks: [], dateMatches: [] });
  }
  if (variant === "stale-distractor") return withDistractors(prepared.fullViewport, prepared.distractors);
  return prepared.fullViewport;
}

async function runTrial(prepared: PreparedScenario, variant: TrialRecord["variant"], trial: number): Promise<TrialRecord> {
  const viewport = variantViewport(prepared, variant);
  const answerRun = await runOmpJson({
    cwd: path.join(RUNTIME_ROOT, prepared.scenario.room),
    model: ANSWER_MODEL,
    systemPrompt: ANSWER_SYSTEM_PROMPT,
    prompt: [
      `ROOM: ${prepared.scenario.room}`,
      `CURRENT DATE: ${STARTED_AT.toISOString().slice(0, 10)}`,
      "AUTOMATIC RECALL VIEWPORT:",
      JSON.stringify(viewport, null, 2),
      "USER TURN:",
      prepared.scenario.user_turn,
    ].join("\n\n"),
  });
  const answer = normalizeAnswer(answerRun.parsed);
  const judgeRun = await runOmpJson({
    cwd: REPO_ROOT,
    model: JUDGE_MODEL,
    systemPrompt: JUDGE_SYSTEM_PROMPT,
    prompt: [
      `SCENARIO ID: ${prepared.scenario.id}`,
      `AUTHORITY MODE: ${prepared.scenario.reference.mode}`,
      "CRITERIA:",
      JSON.stringify(prepared.scenario.criteria, null, 2),
      "RISKS:",
      JSON.stringify(prepared.scenario.risks, null, 2),
      "REFERENCE PACKET:",
      JSON.stringify(prepared.reference, null, 2),
      "AUTOMATIC RECALL VIEWPORT:",
      JSON.stringify(viewport, null, 2),
      "ANSWER UNDER TEST:",
      JSON.stringify(answer, null, 2),
    ].join("\n\n"),
  });
  const judge = normalizeJudge(judgeRun.parsed);
  const ids = evidenceIds(viewport);
  const score = scoreTrial({
    judge,
    expectedCriterionIds: prepared.scenario.criteria.map((item) => item.id),
    expectedRiskIds: prepared.scenario.risks.map((item) => item.id),
    answerCitations: answer.citations,
    availableEvidenceIds: ids,
    requireCitation: variant !== "no-recall" && ids.length > 0,
  });
  const modelSummary = ({ model, provider, usage, latency_ms }: ModelRun) => ({ model, provider, usage, latency_ms });
  return {
    scenario_id: prepared.scenario.id,
    variant,
    trial,
    answer,
    judge,
    score,
    answer_model: modelSummary(answerRun),
    judge_model: modelSummary(judgeRun),
  };
}

async function mapConcurrent<T, R>(items: T[], limit: number, worker: (item: T) => Promise<R>): Promise<R[]> {
  const results = new Array<R>(items.length);
  let next = 0;
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, async () => {
    while (true) {
      const index = next++;
      if (index >= items.length) return;
      results[index] = await worker(items[index]);
    }
  }));
  return results;
}

function markdownReport(report: any): string {
  const lines = [
    `# End-to-end memory evaluation — ${report.measurement_date}`,
    "",
    `Generated at **${report.generated_at}** by [run-end-to-end.ts](./run-end-to-end.ts).`,
    "",
    "## Contract",
    "",
    "This evaluates the complete user-turn → automatic multi-lane recall viewport → model selection → answer path. Reference evidence resolves from current unsuperseded threads at run time, except scenarios explicitly marked historical. Exact memory IDs are not retrieval targets.",
    "",
    `- scenarios: **${report.scenario_count}**`,
    `- trials per variant: **${report.trials_per_variant}**`,
    `- completed trials: **${report.trials.length}**`,
    `- scenario SHA-256: \`${report.scenario_suite.sha256}\``,
    `- answer model request: \`${report.models.answer_requested || "active OMP default"}\``,
    `- judge model request: \`${report.models.judge_requested || "active OMP default"}\``,
    "",
    "## Scenario results",
    "",
    "| Scenario | Variant | Passes | Success | Criteria | Evidence available | Risk violations | Unsupported claims |",
    "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |",
  ];
  for (const item of report.summary.summaries) {
    lines.push(`| ${item.scenario_id} | ${item.variant} | ${item.passes}/${item.trials} | ${(item.success_rate * 100).toFixed(1)}% | ${(item.mean_criterion_coverage * 100).toFixed(1)}% | ${(item.mean_evidence_coverage * 100).toFixed(1)}% | ${item.mean_risk_violations.toFixed(2)} | ${item.mean_unsupported_claims.toFixed(2)} |`);
  }
  lines.push("", "## Memory lift", "", "| Scenario | Full | No recall | Lift |", "| --- | ---: | ---: | ---: |");
  for (const item of report.summary.memory_lift) {
    const value = (number: number | null) => number === null ? "n/a" : `${(number * 100).toFixed(1)}%`;
    lines.push(`| ${item.scenario_id} | ${value(item.full_success_rate)} | ${value(item.no_recall_success_rate)} | ${item.memory_lift === null ? "n/a" : `${item.memory_lift >= 0 ? "+" : ""}${(item.memory_lift * 100).toFixed(1)} pp`} |`);
  }
  lines.push("", "## Retrieval viewport", "", "| Scenario | Raw candidates | Kept | Suppressed | Reference records |", "| --- | ---: | ---: | ---: | ---: |");
  for (const prepared of report.prepared) {
    lines.push(`| ${prepared.scenario_id} | ${prepared.retrieval.raw_candidate_count ?? 0} | ${prepared.retrieval.viewport?.kept ?? 0} | ${prepared.retrieval.viewport?.suppressed ?? 0} | ${prepared.reference_ids.length} |`);
  }
  lines.push("", "## Trial detail", "", "| Scenario | Variant | Trial | Pass | Failure class | Answer model | Judge model |", "| --- | --- | ---: | --- | --- | --- | --- |");
  for (const trial of report.trials) {
    lines.push(`| ${trial.scenario_id} | ${trial.variant} | ${trial.trial} | ${trial.score.passed ? "yes" : "no"} | ${trial.score.failure_class} | ${trial.answer_model.provider}/${trial.answer_model.model} | ${trial.judge_model.provider}/${trial.judge_model.model} |`);
  }
  lines.push("", "The JSON report contains the exact viewports, dynamically resolved reference records, answers, citations, judge decisions, usage, and latency for audit.", "");
  return lines.join("\n");
}

async function main() {
  const scenarioSource = await Bun.file(SCENARIO_PATH).text();
  const suite = JSON.parse(scenarioSource) as ScenarioSuite;
  if (suite.version !== 1 || !Array.isArray(suite.scenarios) || !suite.scenarios.length) throw new Error("living-scenarios.json has an invalid contract");
  const trialsPerVariant = Math.max(1, Number(process.env.SOLARISAEL_E2E_TRIALS || suite.default_trials || 3));
  const prepared = await Promise.all(suite.scenarios.map(prepareScenario));
  const jobs: Array<{ prepared: PreparedScenario; variant: TrialRecord["variant"]; trial: number }> = [];
  for (const item of prepared) {
    const variants: TrialRecord["variant"][] = ["full", "no-recall"];
    if (item.distractors.length) variants.push("stale-distractor");
    for (const variant of variants) {
      for (let trial = 1; trial <= trialsPerVariant; trial++) jobs.push({ prepared: item, variant, trial });
    }
  }
  console.log(`End-to-end memory eval: ${suite.scenarios.length} scenarios, ${jobs.length} model+judge trials, concurrency ${CONCURRENCY}`);
  const trials = await mapConcurrent(jobs, CONCURRENCY, async (job) => {
    const result = await runTrial(job.prepared, job.variant, job.trial);
    console.log(`${result.scenario_id}:${result.variant}:${result.trial} ${result.score.passed ? "pass" : result.score.failure_class}`);
    return result;
  });
  const generatedAt = new Date().toISOString();
  const summary = summarizeTrials(trials);
  const report = {
    measurement_date: generatedAt.slice(0, 10),
    run_started_at: STARTED_AT.toISOString(),
    generated_at: generatedAt,
    output_stem: OUTPUT_STEM,
    runtime_root: RUNTIME_ROOT,
    substrate_dir: SUBSTRATE_DIR,
    scenario_count: suite.scenarios.length,
    trials_per_variant: trialsPerVariant,
    scenario_suite: {
      path: SCENARIO_PATH,
      sha256: createHash("sha256").update(scenarioSource).digest("hex"),
      bytes: Buffer.byteLength(scenarioSource),
    },
    models: { answer_requested: ANSWER_MODEL || null, judge_requested: JUDGE_MODEL || null },
    measurement_contract: {
      unit: "complete user turn through automatic multi-lane recall, viewport filtering, model evidence selection, and answer judgment",
      reference: "current unsuperseded thread leaves resolved at run time, or explicit historical records",
      memory_lift: "full-recall first-response success rate minus no-recall first-response success rate",
      provenance: "evaluation-only evidence IDs; no chain-of-thought collection",
      stochasticity: "independent first responses; no pass@k rescue",
    },
    prepared: prepared.map((item) => ({
      scenario_id: item.scenario.id,
      retrieval: item.retrieval,
      viewport: item.fullViewport,
      reference_ids: item.reference.map((record) => record.id),
      reference_records: item.reference,
      distractor_ids: item.distractors.map((record) => record.id),
    })),
    summary,
    trials,
  };
  const jsonPath = path.join(EVAL_DIR, `${OUTPUT_STEM}.json`);
  const markdownPath = path.join(EVAL_DIR, `${OUTPUT_STEM}.md`);
  await Bun.write(jsonPath, `${JSON.stringify(report, null, 2)}\n`);
  await Bun.write(markdownPath, markdownReport(report));
  console.log(`wrote ${jsonPath}`);
  console.log(`wrote ${markdownPath}`);
}

await main();
