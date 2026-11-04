import { createHash } from "node:crypto";
import path from "node:path";
import { runRecallQuery } from "../src/memory.ts";
import { runWsl, windowsPathToWsl } from "../src/wsl.ts";
import { directMemoryId, scoreExpected, sliceMetrics, summarizeMetrics } from "./metrics.ts";

type Golden = {
  id: string;
  room: string;
  query: string;
  expected_memory_ids: number[];
  expected_any: boolean;
  style: "sharp" | "natural";
  notes: string;
};

type MemoryRow = {
  id: number;
  room: string;
  source_path: string;
  retrievable: boolean;
};

type Candidate = Record<string, unknown> & {
  id?: string;
  source?: string;
  source_table?: string;
  source_id?: string | number;
  room?: string;
  source_path?: string;
  lanes?: string[];
  score?: number;
  title?: string;
};

const EVAL_DIR = path.dirname(new URL(import.meta.url).pathname.replace(/^\/(?=[A-Za-z]:)/, ""));
const REPO_ROOT = path.resolve(EVAL_DIR, "..");
const DEFAULT_RUNTIME_ROOT = "C:/Solarisael/Obsidian/obsidian";
const RUNTIME_ROOT = path.resolve(
  process.env.SOLARISAEL_OBSIDIAN_ROOT
    || process.env.SOLARISAEL_VAULT_ROOT
    || DEFAULT_RUNTIME_ROOT,
);
const SUBSTRATE_DIR = path.resolve(
  process.env.SOLARISAEL_SUBSTRATE || path.join(RUNTIME_ROOT, "house", "substrate"),
);
const RUN_STARTED_AT = new Date();
const RUN_DATE = RUN_STARTED_AT.toISOString().slice(0, 10);
const RUN_ID = RUN_STARTED_AT.toISOString().replace(/\.\d{3}Z$/, "Z").replaceAll(":", "-");
const OUTPUT_STEM = process.env.SOLARISAEL_EVAL_OUTPUT || `legacy-id-ranking-eval-${RUN_ID}`;
const GOLDEN_PATH = path.join(EVAL_DIR, "legacy-id-ranking-queries.json");
const EMBED_URL = process.env.SOLARISAEL_EMBED_URL
  || process.env.SOLARISAEL_LMSTUDIO_URL
  || "http://127.0.0.1:11434/api/embed";
const TOP_K = [1, 5, 10] as const;

function shellQuote(value: string): string {
  return `'${String(value).replaceAll("'", "'\\''")}'`;
}

function normalizePath(value: unknown): string {
  return String(value || "")
    .replaceAll("\\", "/")
    .replace(/^house\//, "")
    .replace(/^\.\//, "")
    .trim();
}

function mapKey(room: unknown, sourcePath: unknown): string {
  return `${String(room || "").toLowerCase()}|${normalizePath(sourcePath)}`;
}

async function runPsql(sql: string, timeoutMs = 30_000): Promise<{ stdout: string; error?: string }> {
  const command = [
    `cd ${shellQuote(windowsPathToWsl(SUBSTRATE_DIR))}`,
    "set -a && . ./.env && set +a",
    `psql -X -A -t -F '|' -c ${shellQuote(sql)}`,
  ].join(" && ");
  const outcome = await runWsl({
    argv: ["-d", "Ubuntu-24.04", "--", "bash", "-lc", command],
    cwd: REPO_ROOT,
    timeoutMs,
  });
  if (outcome.timedOut) return { stdout: "", error: "psql timed out" };
  if (outcome.spawnError) return { stdout: "", error: outcome.spawnError };
  if (outcome.code !== 0) {
    return { stdout: "", error: outcome.stderr.trim() || `psql exited ${outcome.code}` };
  }
  return { stdout: outcome.stdout };
}

async function verifyMemoryIds(ids: number[]): Promise<{ rows: MemoryRow[]; error?: string }> {
  const uniqueIds = [...new Set(ids.map((value) => Number(value)).filter(Number.isInteger))];
  if (!uniqueIds.length) return { rows: [] };

  const sql = `SELECT id, room, source_path, (archived_at IS NULL AND (source_path NOT LIKE 'db-only/%' OR type = 'paper-boat'))::int AS retrievable FROM memories WHERE id = ANY(ARRAY[${uniqueIds.join(",")}]::bigint[]) ORDER BY id;`;
  const outcome = await runPsql(sql);
  if (outcome.error) return { rows: [], error: outcome.error };

  const rows: MemoryRow[] = [];
  for (const line of outcome.stdout.trim().split(/\r?\n/).filter(Boolean)) {
    const [id, room, source_path, retrievable] = line.split("|");
    const numericId = Number(id);
    if (Number.isInteger(numericId) && room && source_path) {
      rows.push({ id: numericId, room, source_path, retrievable: retrievable === "1" });
    }
  }
  return { rows };
}

async function loadCorpusSnapshot(): Promise<{ values: Record<string, number | string>; error?: string }> {
  const sql = [
    "SELECT 'memories_total', count(*)::text FROM memories",
    "UNION ALL SELECT 'memories_active', count(*)::text FROM memories WHERE archived_at IS NULL",
    "UNION ALL SELECT 'memories_visible', count(*)::text FROM memories WHERE archived_at IS NULL AND (source_path NOT LIKE 'db-only/%' OR type = 'paper-boat')",
    "UNION ALL SELECT 'memory_chunks', count(*)::text FROM memory_chunks",
    "UNION ALL SELECT 'embedded_chunks', count(*)::text FROM memory_chunks WHERE body_embedding IS NOT NULL",
    "UNION ALL SELECT 'memory_threads', count(*)::text FROM memory_threads",
    "UNION ALL SELECT 'named_entities', count(*)::text FROM named_entities",
    "UNION ALL SELECT 'newest_memory_id', COALESCE(max(id), 0)::text FROM memories",
    "UNION ALL SELECT 'newest_memory_created_at', COALESCE(max(created_at)::text, '') FROM memories",
  ].join(" ");
  const outcome = await runPsql(sql);
  if (outcome.error) return { values: {}, error: outcome.error };

  const values: Record<string, number | string> = {};
  for (const line of outcome.stdout.trim().split(/\r?\n/).filter(Boolean)) {
    const separator = line.indexOf("|");
    if (separator < 0) continue;
    const key = line.slice(0, separator);
    const value = line.slice(separator + 1);
    values[key] = /^\d+$/.test(value) ? Number(value) : value;
  }
  return { values };
}

async function probeEmbedEndpoint(): Promise<{ status: "available" | "degraded"; detail?: string }> {
  let url: URL;
  try {
    url = new URL(EMBED_URL);
  } catch (error) {
    return { status: "degraded", detail: `invalid embed URL: ${error instanceof Error ? error.message : String(error)}` };
  }

  // A GET to the service root is deliberately only a reachability probe. The
  // retrieval source owns the POST/embed contract and remains fail-open.
  const root = new URL(url.origin);
  try {
    const response = await fetch(root, { signal: AbortSignal.timeout(2_000) });
    return { status: "available", detail: `reachable (${response.status})` };
  } catch (error) {
    return { status: "degraded", detail: error instanceof Error ? error.message : String(error) };
  }
}

function laneNames(candidate: Candidate): string[] {
  const lanes = new Set<string>();
  const source = String(candidate.source || "").toLowerCase();
  const sourceTable = String(candidate.source_table || "").toLowerCase();
  const rawLanes = Array.isArray(candidate.lanes) ? candidate.lanes : [];

  if (source === "entity" || sourceTable === "named_entities") lanes.add("entity");
  if (source === "semantic" || rawLanes.includes("semantic")) lanes.add("semantic");
  if (source === "content" || rawLanes.includes("content")) lanes.add("content");
  if (source === "date" || rawLanes.includes("date")) lanes.add("date");
  if (rawLanes.includes("search") || (!lanes.size && !["semantic", "content", "date"].includes(source))) {
    lanes.add("lexical");
  }
  return [...lanes];
}


function memoryIdForCandidate(candidate: Candidate, rowsByKey: Map<string, MemoryRow>): number | null {
  const direct = directMemoryId(candidate);
  if (direct !== null) return direct;
  const room = String(candidate.room || "").toLowerCase();
  const sourcePath = normalizePath(candidate.source_path);
  if (!sourcePath) return null;
  const exact = rowsByKey.get(mapKey(room, sourcePath));
  if (exact) return exact.id;

  // Some old source lanes omit room metadata. Only accept an unambiguous path.
  const matches = [...rowsByKey.values()].filter((row) => normalizePath(row.source_path) === sourcePath);
  return matches.length === 1 ? matches[0].id : null;
}

function summarizeCandidate(candidate: Candidate, memoryId: number | null): Record<string, unknown> {
  return {
    memory_id: memoryId,
    id: candidate.id || "",
    source: candidate.source || "",
    source_table: candidate.source_table || "",
    source_id: candidate.source_id || "",
    room: candidate.room || "",
    title: candidate.title || "",
    source_path: candidate.source_path || "",
    score: Number(candidate.score || 0),
    lanes: laneNames(candidate),
  };
}

function percent(value: number): number {
  return Math.round(value * 10000) / 100;
}


function markdownReport(report: Record<string, any>): string {
  const metrics = report.metrics;
  const formatPercent = (value: number) => `${(value * 100).toFixed(1)}%`;
  const formatInterval = (interval: { lower: number; upper: number }) =>
    `${formatPercent(interval.lower)}–${formatPercent(interval.upper)}`;
  const lines = [
    `# Legacy historical-row ranking evaluation — ${report.measurement_date}`,
    "",
    `Generated at **${report.generated_at}** by [run-id-ranking-evals.ts](./run-id-ranking-evals.ts) against the live substrate.`,
    "",
    "## Measurement identity",
    "",
    `- output stem: \`${report.output_stem}\``,
    `- golden SHA-256: \`${report.golden_suite.sha256}\``,
    `- goldens: **${report.golden_count}**`,
    `- verified expected IDs: **${report.verification.verified_count}/${report.verification.expected_count}**`,
    `- evaluable goldens: **${report.golden_suite.evaluable_count}/${report.golden_count}**`,
    `- excluded invalid contracts: **${report.golden_suite.excluded.length}**`,
    `- runtime root: \`${report.runtime_root}\``,
    `- substrate: \`${report.substrate_dir}\``,
    "",
    "## Corpus snapshot",
    "",
    "| Field | Value |",
    "| --- | ---: |",
  ];
  for (const [key, value] of Object.entries(report.corpus_snapshot.values)) {
    lines.push(`| ${key} | ${value} |`);
  }
  if (report.corpus_snapshot.error) lines.push("", `Corpus snapshot error: \`${report.corpus_snapshot.error}\``);

  lines.push(
    "",
    "## Scoreboard",
    "",
    "Degraded queries count as misses. Recall intervals are Wilson 95% intervals.",
    "",
    "| Metric | Hits | Score | 95% interval |",
    "| --- | ---: | ---: | ---: |",
    `| recall@1 | ${metrics.recall_hits.at_1}/${metrics.evaluated} | ${formatPercent(metrics.recall_at_1)} | ${formatInterval(metrics.confidence_95.recall_at_1)} |`,
    `| recall@5 | ${metrics.recall_hits.at_5}/${metrics.evaluated} | ${formatPercent(metrics.recall_at_5)} | ${formatInterval(metrics.confidence_95.recall_at_5)} |`,
    `| recall@10 | ${metrics.recall_hits.at_10}/${metrics.evaluated} | ${formatPercent(metrics.recall_at_10)} | ${formatInterval(metrics.confidence_95.recall_at_10)} |`,
    `| MRR | — | ${metrics.mrr.toFixed(4)} | — |`,
    "",
    "## Query health and latency",
    "",
    `- healthy queries: **${metrics.query_health.healthy}/${metrics.evaluated}**`,
    `- degraded queries: **${metrics.query_health.degraded}/${metrics.evaluated}**`,
    `- latency mean: **${metrics.latency_ms.mean.toFixed(1)} ms**`,
    `- latency p50: **${metrics.latency_ms.p50.toFixed(1)} ms**`,
    `- latency p95: **${metrics.latency_ms.p95.toFixed(1)} ms**`,
    `- latency range: **${metrics.latency_ms.min.toFixed(1)}–${metrics.latency_ms.max.toFixed(1)} ms**`,
    "",
    "## Slices",
  );
  for (const [dimension, slices] of Object.entries(report.slices)) {
    lines.push("", `### By ${dimension}`, "", "| Slice | N | R@1 | R@5 | R@10 | MRR | Healthy |", "| --- | ---: | ---: | ---: | ---: | ---: | ---: |");
    for (const [name, slice] of Object.entries(slices as Record<string, any>)) {
      lines.push(`| ${name} | ${slice.evaluated} | ${formatPercent(slice.recall_at_1)} | ${formatPercent(slice.recall_at_5)} | ${formatPercent(slice.recall_at_10)} | ${slice.mrr.toFixed(4)} | ${slice.query_health.healthy}/${slice.evaluated} |`);
    }
  }

  lines.push("", "## Golden coverage", "");
  for (const [key, value] of Object.entries(report.golden_suite.coverage)) {
    lines.push(`- ${key}: **${typeof value === "object" ? JSON.stringify(value) : value}**`);
  }
  if (report.golden_suite.warnings.length) {
    lines.push("", "### Coverage warnings", "");
    for (const warning of report.golden_suite.warnings) lines.push(`- ${warning}`);
    if (report.golden_suite.excluded.length) {
      lines.push("", "### Excluded invalid contracts", "", "| Golden | Expected IDs | Reason |", "| --- | --- | --- |");
      for (const item of report.golden_suite.excluded) {
        lines.push(`| ${item.id} | ${item.expected_memory_ids.join(", ")} | ${item.reason} |`);
      }
    }
  }

  lines.push(
    "",
    "## Lane attribution",
    "",
    "Attribution counts a hit in every lane represented by its fused candidate. One hit can count in multiple lanes.",
    "",
    "| Lane | Hit evidence | Share of hit evidence |",
    "| --- | ---: | ---: |",
  );
  const laneCounts = report.lane_attribution.counts as Record<string, number>;
  const laneTotal = Object.values(laneCounts).reduce((sum, value) => sum + value, 0);
  for (const lane of ["lexical", "semantic", "content", "date", "entity"]) {
    const count = laneCounts[lane] || 0;
    lines.push(`| ${lane} | ${count} | ${laneTotal ? `${((count / laneTotal) * 100).toFixed(1)}%` : "0.0%"} |`);
  }
  lines.push("", "## Lane status", "", `- semantic embed endpoint: **${report.lane_status.semantic.status}** (${report.lane_status.semantic.detail})`);
  if (report.lane_status.semantic.status === "degraded") {
    lines.push("  - Semantic evidence is degraded. Other lanes still score, and the run remains fail-open.");
  }
  lines.push(`- lexical/entity/content/date lanes: **${report.lane_status.lexical}**`);

  lines.push("", "## Worst-performing goldens", "", "| Golden | Room | Satisfaction rank | Query |", "| --- | --- | ---: | --- |");
  for (const item of report.worst_performing) {
    lines.push(`| ${item.id} | ${item.room} | ${item.satisfaction_rank ?? "miss"} | ${String(item.query).replaceAll("|", "\\|")} |`);
  }
  lines.push("", "## Observations", "");
  for (const observation of report.observations) lines.push(`- ${observation}`);

  lines.push("", "## Per-golden detail", "", "| Golden | Status | @1 | @5 | @10 | MRR | Rank | Latency | Hit lanes |", "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |");
  for (const item of report.queries) {
    const hitLanes = item.hit_lanes.length ? item.hit_lanes.join(", ") : "—";
    lines.push(`| ${item.id} | ${item.status} | ${item.recall_at_1 ? "yes" : "no"} | ${item.recall_at_5 ? "yes" : "no"} | ${item.recall_at_10 ? "yes" : "no"} | ${item.mrr.toFixed(4)} | ${item.satisfaction_rank ?? "miss"} | ${item.latency_ms.toFixed(1)} ms | ${hitLanes} |`);
  }
  lines.push("");
  return lines.join("\n");
}

async function main(): Promise<void> {
  const goldenSource = await Bun.file(GOLDEN_PATH).text();
  const goldens = JSON.parse(goldenSource) as Golden[];
  if (!Array.isArray(goldens) || !goldens.length) throw new Error("legacy-id-ranking-queries.json is empty");

  const expectedIds = goldens.flatMap((golden) => golden.expected_memory_ids);
  const [verification, corpusSnapshot, embedProbe] = await Promise.all([
    verifyMemoryIds(expectedIds),
    loadCorpusSnapshot(),
    probeEmbedEndpoint(),
  ]);
  const expectedSet = new Set(expectedIds);
  const returnedSet = new Set(verification.rows.map((row) => row.id));
  const missingIds = [...expectedSet].filter((id) => !returnedSet.has(id));
  const rowsByKey = new Map(verification.rows.map((row) => [mapKey(row.room, row.source_path), row]));
  const rowsById = new Map(verification.rows.map((row) => [row.id, row]));
  const expectedIdsForGolden = new Map<string, number[]>();
  const excludedGoldens: Array<{ id: string; expected_memory_ids: number[]; reason: string }> = [];
  const evaluableGoldens = goldens.filter((golden) => {
    const uniqueExpectedIds = [...new Set(golden.expected_memory_ids)];
    const retrievableExpectedIds = uniqueExpectedIds.filter((id) => rowsById.get(id)?.retrievable);
    expectedIdsForGolden.set(golden.id, retrievableExpectedIds);
    const valid = golden.expected_any
      ? retrievableExpectedIds.length > 0
      : retrievableExpectedIds.length > 0 && retrievableExpectedIds.length === uniqueExpectedIds.length;
    if (!valid) {
      const invisibleIds = uniqueExpectedIds.filter((id) => !rowsById.get(id)?.retrievable);
      excludedGoldens.push({
        id: golden.id,
        expected_memory_ids: uniqueExpectedIds,
        reason: invisibleIds.length
          ? `Expected IDs are outside the active retrieval policy: ${invisibleIds.join(", ")}`
          : "No retrievable expected target is configured.",
      });
    }
    return valid;
  });

  const queries: Record<string, any>[] = [];
  const laneCounts: Record<string, number> = { lexical: 0, semantic: 0, content: 0, date: 0, entity: 0 };

  for (const golden of evaluableGoldens) {
    const roomDir = path.join(RUNTIME_ROOT, golden.room);
    const startedAt = performance.now();
    let result: any;
    let error = "";
    try {
      result = await runRecallQuery(roomDir, golden.room, golden.query);
      if (!result?.ok) error = String(result?.error || "recall returned not ok");
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    }
    const latencyMs = Math.round((performance.now() - startedAt) * 100) / 100;

    const candidates = Array.isArray(result?.retrievalCandidates) ? result.retrievalCandidates as Candidate[] : [];
    const summarized = candidates.map((candidate) => {
      const memoryId = memoryIdForCandidate(candidate, rowsByKey);
      return { candidate, summary: summarizeCandidate(candidate, memoryId), memoryId };
    });
    const scoredExpectedIds = expectedIdsForGolden.get(golden.id) || [];
    const expected = new Set(scoredExpectedIds);
    const hitRanks = summarized
      .map((entry, index) => ({ index: index + 1, memoryId: entry.memoryId, lanes: laneNames(entry.candidate) }))
      .filter((entry) => entry.memoryId !== null && expected.has(entry.memoryId));
    const scored = scoreExpected(
      summarized.map((entry) => entry.memoryId),
      scoredExpectedIds,
      golden.expected_any,
    );
    const rankForScore = scored.satisfaction_rank || 0;
    const at1 = rankForScore > 0 && rankForScore <= 1;
    const at5 = rankForScore > 0 && rankForScore <= 5;
    const at10 = rankForScore > 0 && rankForScore <= 10;
    const hitLanes = [...new Set(hitRanks.flatMap((hit) => hit.lanes))];
    for (const lane of hitLanes) laneCounts[lane] = (laneCounts[lane] || 0) + 1;

    queries.push({
      id: golden.id,
      room: golden.room,
      query: golden.query,
      style: golden.style,
      notes: golden.notes,
      expected_memory_ids: golden.expected_memory_ids,
      scored_expected_memory_ids: scoredExpectedIds,
      expected_any: golden.expected_any,
      expected_semantics: golden.expected_any ? "any" : "all",
      status: error ? "degraded" : "ok",
      error: error || null,
      latency_ms: latencyMs,
      recall_at_1: at1,
      recall_at_5: at5,
      recall_at_10: at10,
      satisfaction_rank: scored.satisfaction_rank,
      first_expected_rank: scored.first_expected_rank,
      mrr: rankForScore ? 1 / rankForScore : 0,
      hit_memory_ids: scored.hit_memory_ids,
      hit_lanes: hitLanes,
      retrieval_counts: {
        candidates: candidates.length,
        search: Array.isArray(result?.searchCandidates) ? result.searchCandidates.length : 0,
        semantic: Array.isArray(result?.semanticChunks) ? result.semanticChunks.length : 0,
        content: Array.isArray(result?.contentChunks) ? result.contentChunks.length : 0,
        date: Array.isArray(result?.dateMatches) ? result.dateMatches.length : 0,
      },
      top_candidates: summarized.slice(0, 10).map((entry) => entry.summary),
    });
  }

  const metrics = summarizeMetrics(queries as any);
  const slices = {
    room: sliceMetrics(queries as any, (query: any) => query.room),
    style: sliceMetrics(queries as any, (query: any) => query.style),
  };
  const worst = [...queries]
    .sort((left, right) => {
      const leftMiss = left.satisfaction_rank === null;
      const rightMiss = right.satisfaction_rank === null;
      if (leftMiss !== rightMiss) return leftMiss ? -1 : 1;
      return Number(right.satisfaction_rank || 0) - Number(left.satisfaction_rank || 0)
        || String(left.id).localeCompare(String(right.id));
    })
    .slice(0, 10)
    .map((item) => ({
      id: item.id,
      room: item.room,
      query: item.query,
      satisfaction_rank: item.satisfaction_rank,
      mrr: item.mrr,
    }));

  const roomCoverage = Object.fromEntries(
    [...new Set(goldens.map((golden) => golden.room))]
      .sort()
      .map((room) => [room, goldens.filter((golden) => golden.room === room).length]),
  );
  const styleCoverage = Object.fromEntries(
    [...new Set(goldens.map((golden) => golden.style))]
      .sort()
      .map((style) => [style, goldens.filter((golden) => golden.style === style).length]),
  );
  const expectedAnyCount = goldens.filter((golden) => golden.expected_any).length;
  const multiTargetCount = goldens.filter((golden) => golden.expected_memory_ids.length > 1).length;
  const coverageWarnings: string[] = [];
  for (const room of ["kodo", "kintsu", "tuner"]) {
    if (!roomCoverage[room]) coverageWarnings.push(`No ${room} goldens are present.`);
  }
  if (expectedAnyCount === goldens.length) {
    coverageWarnings.push("Every golden uses expected-any semantics; expected-all behavior is not exercised.");
  }
  if (multiTargetCount < 3) {
    coverageWarnings.push(`Only ${multiTargetCount} multi-target golden${multiTargetCount === 1 ? " is" : "s are"} present.`);
  }
  if (excludedGoldens.length) {
    coverageWarnings.push(`${excludedGoldens.length} goldens were excluded because their expected targets are outside the active retrieval policy.`);
  }
  const roomCounts = Object.values(roomCoverage) as number[];
  if (roomCounts.length > 1 && Math.max(...roomCounts) > Math.min(...roomCounts) * 2) {
    coverageWarnings.push(`Room coverage is imbalanced: ${JSON.stringify(roomCoverage)}.`);
  }

  const observations: string[] = [];
  const topTenMisses = queries.filter((query) => query.satisfaction_rank === null);
  if (missingIds.length) observations.push(`Verification failed for memory IDs: ${missingIds.join(", ")}.`);
  if (metrics.query_health.degraded) {
    observations.push(`${metrics.query_health.degraded} query failures counted as misses: ${queries.filter((query) => query.status === "degraded").map((query) => query.id).join(", ")}.`);
  }
  if (metrics.recall_at_10 < 1) {
    observations.push(`${Math.round((1 - metrics.recall_at_10) * 100)}% of goldens missed their expected target contract in the top 10.`);
  }
  if (topTenMisses.length) {
    observations.push(`Top-10 misses (${topTenMisses.length}): ${topTenMisses.map((query) => query.id).join(", ")}.`);
  }
  if (metrics.recall_at_10 > metrics.recall_at_1) {
    observations.push(`The gap from recall@1 (${percent(metrics.recall_at_1)}%) to recall@10 (${percent(metrics.recall_at_10)}%) shows ranking depth matters.`);
  }
  if (laneCounts.entity === 0) observations.push("No expected hit was attributed to the entity lane.");
  if (laneCounts.date === 0) observations.push("No expected hit was attributed to the date lane.");
  if (embedProbe.status === "degraded") observations.push("Semantic retrieval was endpoint-degraded during this run.");
  if (corpusSnapshot.error) observations.push(`Corpus snapshot failed: ${corpusSnapshot.error}.`);
  if (!observations.length) observations.push("No additional weakness surfaced beyond the measured scoreboard.");

  const generatedAt = new Date().toISOString();
  const report = {
    measurement_date: RUN_DATE,
    baseline_date: RUN_DATE,
    run_started_at: RUN_STARTED_AT.toISOString(),
    generated_at: generatedAt,
    output_stem: OUTPUT_STEM,
    runtime_root: RUNTIME_ROOT,
    substrate_dir: SUBSTRATE_DIR,
    golden_count: goldens.length,
    golden_suite: {
      path: GOLDEN_PATH,
      sha256: createHash("sha256").update(goldenSource).digest("hex"),
      bytes: Buffer.byteLength(goldenSource),
      coverage: {
        rooms: roomCoverage,
        styles: styleCoverage,
        expected_any: expectedAnyCount,
        expected_all: goldens.length - expectedAnyCount,
        multi_target: multiTargetCount,
        evaluable: evaluableGoldens.length,
        excluded: excludedGoldens.length,
      },
      evaluable_count: evaluableGoldens.length,
      excluded: excludedGoldens,
      warnings: coverageWarnings,
    },
    corpus_snapshot: corpusSnapshot,
    verification: {
      expected_count: expectedSet.size,
      verified_count: returnedSet.size,
      missing_ids: missingIds,
      error: verification.error || null,
      retrievable_expected_count: verification.rows.filter((row) => row.retrievable).length,
    },
    measurement_contract: {
      scoring: "end-to-end retrievalCandidates ranking over goldens whose expected targets are allowed by the active retrieval policy; invalid contracts are reported and excluded; degraded queries count as misses",
      expected_any: "first expected target rank",
      expected_all: "rank where every expected target has appeared",
      latency: "sequential per-query wall-clock duration around runRecallQuery",
      recall_interval: "Wilson 95%",
    },
    lane_status: {
      lexical: verification.error ? "degraded" : "available",
      semantic: { status: embedProbe.status, detail: embedProbe.detail || "" },
      content: verification.error ? "degraded" : "available",
      date: verification.error ? "degraded" : "available",
      entity: verification.error ? "degraded" : "available",
    },
    metrics,
    slices,
    lane_attribution: {
      counts: laneCounts,
      hit_count: queries.filter((query) => Number(query.satisfaction_rank) > 0).length,
    },
    worst_performing: worst,
    observations,
    queries,
  };

  const jsonPath = path.join(EVAL_DIR, `${OUTPUT_STEM}.json`);
  const markdownPath = path.join(EVAL_DIR, `${OUTPUT_STEM}.md`);
  await Bun.write(jsonPath, `${JSON.stringify(report, null, 2)}\n`);
  await Bun.write(markdownPath, markdownReport(report));

  console.log(`Legacy ID ranking eval: ${metrics.evaluated}/${goldens.length} evaluable queries; verified ${returnedSet.size}/${expectedSet.size} expected IDs; excluded ${excludedGoldens.length} invalid contracts`);
  console.log(`recall@1 ${(metrics.recall_at_1 * 100).toFixed(1)}% | recall@5 ${(metrics.recall_at_5 * 100).toFixed(1)}% | recall@10 ${(metrics.recall_at_10 * 100).toFixed(1)}% | MRR ${metrics.mrr.toFixed(4)}`);
  console.log(`query health ${metrics.query_health.healthy}/${metrics.evaluated} | latency p50 ${metrics.latency_ms.p50.toFixed(1)} ms | p95 ${metrics.latency_ms.p95.toFixed(1)} ms`);
  console.log(`lane attribution ${JSON.stringify(laneCounts)} | semantic ${embedProbe.status}`);
  console.log(`worst-performing ${worst.slice(0, 5).map((item) => `${item.id}:${item.satisfaction_rank ?? "miss"}`).join(", ")}`);
  console.log(`wrote ${jsonPath}`);
  console.log(`wrote ${markdownPath}`);

  if (
    verification.error
    || missingIds.length
    || corpusSnapshot.error
    || metrics.query_health.degraded
  ) process.exitCode = 1;
}

await main();
