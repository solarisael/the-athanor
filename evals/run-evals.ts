import path from "node:path";
import { runRecallQuery } from "../src/memory.ts";
import { runWsl } from "../src/wsl.ts";

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
const RUNTIME_ROOT = process.env.SOLARISAEL_OBSIDIAN_ROOT || DEFAULT_RUNTIME_ROOT;
const OUTPUT_STEM = process.env.SOLARISAEL_EVAL_OUTPUT || "baseline-2026-07-18";
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

async function verifyMemoryIds(ids: number[]): Promise<{ rows: MemoryRow[]; error?: string }> {
  const uniqueIds = [...new Set(ids.map((value) => Number(value)).filter(Number.isInteger))];
  if (!uniqueIds.length) return { rows: [] };

  const sql = `SELECT id, room, source_path FROM memories WHERE id = ANY(ARRAY[${uniqueIds.join(",")}]::bigint[]) ORDER BY id;`;
  const command = [
    "cd /mnt/c/Solarisael/Obsidian/obsidian/house/substrate",
    "set -a && . ./.env && set +a",
    `psql -X -A -t -F '|' -c ${shellQuote(sql)}`,
  ].join(" && ");
  const outcome = await runWsl({
    argv: ["-d", "Ubuntu-24.04", "--", "bash", "-lc", command],
    cwd: REPO_ROOT,
    timeoutMs: 30_000,
  });
  if (outcome.timedOut) return { rows: [], error: "verification SELECT timed out" };
  if (outcome.spawnError) return { rows: [], error: outcome.spawnError };
  if (outcome.code !== 0) return { rows: [], error: outcome.stderr.trim() || `psql exited ${outcome.code}` };

  const rows: MemoryRow[] = [];
  for (const line of outcome.stdout.trim().split(/\r?\n/).filter(Boolean)) {
    const [id, room, source_path] = line.split("|");
    const numericId = Number(id);
    if (Number.isInteger(numericId) && room && source_path) rows.push({ id: numericId, room, source_path });
  }
  return { rows };
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

function directMemoryId(candidate: Candidate): number | null {
  const idMatch = /^memories:(\d+)$/.exec(String(candidate.id || ""));
  if (idMatch) return Number(idMatch[1]);
  if (String(candidate.source_table || "").toLowerCase() === "memories") {
    const numeric = Number(candidate.source_id);
    if (Number.isInteger(numeric)) return numeric;
  }
  return null;
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

function average(values: number[]): number {
  return values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0;
}

function markdownReport(report: Record<string, any>): string {
  const metrics = report.metrics;
  const lines = [
    `# Retrieval baseline — ${report.baseline_date}`,
    "",
    `Generated by [run-evals.ts](./run-evals.ts) against the live substrate. Goldens: **${report.golden_count}**; verified expected IDs: **${report.verification.verified_count}/${report.verification.expected_count}**.`,
    "",
    "## Scoreboard",
    "",
    "| Metric | Score |",
    "| --- | ---: |",
    `| recall@1 | ${(metrics.recall_at_1 * 100).toFixed(1)}% |`,
    `| recall@5 | ${(metrics.recall_at_5 * 100).toFixed(1)}% |`,
    `| recall@10 | ${(metrics.recall_at_10 * 100).toFixed(1)}% |`,
    `| MRR | ${metrics.mrr.toFixed(4)} |`,
    "",
    "## Lane attribution",
    "",
    "Attribution counts a hit in every lane represented by its fused candidate (one hit may therefore count in multiple lanes).",
    "",
    "| Lane | Hit evidence | Share of hit evidence |",
    "| --- | ---: | ---: |",
  ];
  const laneCounts = report.lane_attribution.counts as Record<string, number>;
  const laneTotal = Object.values(laneCounts).reduce((sum, value) => sum + value, 0);
  for (const lane of ["lexical", "semantic", "content", "date", "entity"]) {
    const count = laneCounts[lane] || 0;
    lines.push(`| ${lane} | ${count} | ${laneTotal ? `${((count / laneTotal) * 100).toFixed(1)}%` : "0.0%"} |`);
  }
  lines.push("", "## Lane status", "", `- semantic embed endpoint: **${report.lane_status.semantic.status}** (${report.lane_status.semantic.detail})`);
  if (report.lane_status.semantic.status === "degraded") {
    lines.push("  - Semantic evidence is degraded; lexical/content/date/entity evidence still scores and the run remains fail-open.");
  }
  lines.push(`- lexical/entity/content/date lanes: **${report.lane_status.lexical}**`, "", "## Worst-performing goldens", "", "| Golden | Room | First expected rank | Query |", "| --- | --- | ---: | --- |");
  for (const item of report.worst_performing) {
    lines.push(`| ${item.id} | ${item.room} | ${item.first_expected_rank ?? "miss"} | ${String(item.query).replaceAll("|", "\\|")} |`);
  }
  lines.push("", "## Observations", "");
  for (const observation of report.observations) lines.push(`- ${observation}`);
  lines.push("", "## Per-golden detail", "", "| Golden | @1 | @5 | @10 | MRR | Hit lanes |", "| --- | ---: | ---: | ---: | ---: | --- |");
  for (const item of report.queries) {
    const hitLanes = item.hit_lanes.length ? item.hit_lanes.join(", ") : "—";
    lines.push(`| ${item.id} | ${item.recall_at_1 ? "yes" : "no"} | ${item.recall_at_5 ? "yes" : "no"} | ${item.recall_at_10 ? "yes" : "no"} | ${item.mrr.toFixed(4)} | ${hitLanes} |`);
  }
  lines.push("");
  return lines.join("\n");
}

async function main(): Promise<void> {
  const goldens = await Bun.file(path.join(EVAL_DIR, "golden-queries.json")).json() as Golden[];
  if (!Array.isArray(goldens) || !goldens.length) throw new Error("golden-queries.json is empty");

  const expectedIds = goldens.flatMap((golden) => golden.expected_memory_ids);
  const verification = await verifyMemoryIds(expectedIds);
  const expectedSet = new Set(expectedIds);
  const returnedSet = new Set(verification.rows.map((row) => row.id));
  const missingIds = [...expectedSet].filter((id) => !returnedSet.has(id));
  const rowsByKey = new Map(verification.rows.map((row) => [mapKey(row.room, row.source_path), row]));
  const embedProbe = await probeEmbedEndpoint();

  const queries: Record<string, unknown>[] = [];
  const laneCounts: Record<string, number> = { lexical: 0, semantic: 0, content: 0, date: 0, entity: 0 };
  let recallAt1 = 0;
  let recallAt5 = 0;
  let recallAt10 = 0;
  let reciprocalRank = 0;
  let evaluated = 0;

  for (const golden of goldens) {
    const roomDir = path.join(RUNTIME_ROOT, golden.room);
    let result: any;
    let error = "";
    try {
      result = await runRecallQuery(roomDir, golden.room, golden.query);
      if (!result?.ok) error = String(result?.error || "recall returned not ok");
    } catch (caught) {
      error = caught instanceof Error ? caught.message : String(caught);
    }

    const candidates = Array.isArray(result?.retrievalCandidates) ? result.retrievalCandidates as Candidate[] : [];
    const summarized = candidates.map((candidate) => {
      const memoryId = memoryIdForCandidate(candidate, rowsByKey);
      return { candidate, summary: summarizeCandidate(candidate, memoryId), memoryId };
    });
    const expected = new Set(golden.expected_memory_ids);
    const hitRanks = summarized
      .map((entry, index) => ({ index: index + 1, memoryId: entry.memoryId, lanes: laneNames(entry.candidate) }))
      .filter((entry) => entry.memoryId !== null && expected.has(entry.memoryId));
    const firstExpectedRank = hitRanks[0]?.index ?? null;
    const rankForScore = firstExpectedRank === null ? 0 : firstExpectedRank;
    const at1 = rankForScore > 0 && rankForScore <= 1;
    const at5 = rankForScore > 0 && rankForScore <= 5;
    const at10 = rankForScore > 0 && rankForScore <= 10;
    const hitLanes = [...new Set(hitRanks.flatMap((hit) => hit.lanes))];
    for (const lane of hitLanes) laneCounts[lane] = (laneCounts[lane] || 0) + 1;

    evaluated += 1;
    recallAt1 += at1 ? 1 : 0;
    recallAt5 += at5 ? 1 : 0;
    recallAt10 += at10 ? 1 : 0;
    reciprocalRank += rankForScore ? 1 / rankForScore : 0;

    queries.push({
      id: golden.id,
      room: golden.room,
      query: golden.query,
      style: golden.style,
      expected_memory_ids: golden.expected_memory_ids,
      expected_any: golden.expected_any,
      status: error ? "degraded" : "ok",
      error: error || null,
      recall_at_1: at1,
      recall_at_5: at5,
      recall_at_10: at10,
      first_expected_rank: firstExpectedRank,
      mrr: rankForScore ? 1 / rankForScore : 0,
      hit_memory_ids: [...new Set(hitRanks.map((hit) => hit.memoryId))],
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

  const metrics = {
    evaluated,
    recall_at_1: evaluated ? recallAt1 / evaluated : 0,
    recall_at_5: evaluated ? recallAt5 / evaluated : 0,
    recall_at_10: evaluated ? recallAt10 / evaluated : 0,
    mrr: evaluated ? reciprocalRank / evaluated : 0,
  };
  const worst = [...queries]
    .sort((a, b) => (Number(a.first_expected_rank ?? 9999) - Number(b.first_expected_rank ?? 9999)) || String(a.id).localeCompare(String(b.id)))
    .reverse()
    .slice(0, 10)
    .map((item) => ({ id: item.id, room: item.room, query: item.query, first_expected_rank: item.first_expected_rank, mrr: item.mrr }));

  const observations: string[] = [];
  const topTenMisses = queries.filter((query) => query.first_expected_rank === null);
  if (missingIds.length) observations.push(`Verification failed for memory IDs: ${missingIds.join(", ")}.`);
  if (metrics.recall_at_10 < 1) observations.push(`${Math.round((1 - metrics.recall_at_10) * 100)}% of goldens missed every expected memory in the top 10; inspect the worst-performing queries before changing ranking.`);
  if (topTenMisses.length) {
    observations.push(`Top-10 misses (${topTenMisses.length}): ${topTenMisses.map((query) => query.id).join(", ")}.`);
  }
  const rowsById = new Map(verification.rows.map((row) => [row.id, row]));
  const missedDbOnlyIds = [...new Set(topTenMisses.flatMap((query) => query.expected_memory_ids as number[]))]
    .filter((id) => rowsById.get(id)?.source_path.startsWith("db-only/"));
  if (missedDbOnlyIds.length) {
    observations.push(`DB-only expected targets missed in every top 10: ${missedDbOnlyIds.join(", ")}; this baseline exposes a visibility gap for non-paper-boat DB-only memories.`);
  }
  if (metrics.recall_at_1 > metrics.recall_at_10) observations.push("[INFERENCE] recall@1 exceeded recall@10 unexpectedly; check report generation.");
  if (metrics.recall_at_10 > metrics.recall_at_1) observations.push(`The gap from recall@1 (${percent(metrics.recall_at_1)}%) to recall@10 (${percent(metrics.recall_at_10)}%) shows ranking depth matters for this golden set.`);
  if (laneCounts.entity === 0) observations.push("No expected hit was attributed to the entity lane; named-entity pointer coverage is not exercised by these goldens.");
  if (laneCounts.date === 0) observations.push("No expected hit was attributed to the date lane; date-tagged retrieval may need more date-shaped goldens.");
  if (embedProbe.status === "degraded") observations.push("Semantic retrieval was endpoint-degraded during this run; the baseline is lexical/content/date/entity evidence only.");
  if (!observations.length) observations.push("No additional weakness surfaced beyond the scoreboard; inspect per-golden ranks and lane evidence for follow-up hypotheses.");

  const report = {
    baseline_date: "2026-07-18",
    generated_at: new Date().toISOString(),
    runtime_root: RUNTIME_ROOT,
    golden_count: goldens.length,
    verification: {
      expected_count: expectedSet.size,
      verified_count: returnedSet.size,
      missing_ids: missingIds,
      error: verification.error || null,
    },
    lane_status: {
      lexical: verification.error ? "degraded" : "available",
      semantic: { status: embedProbe.status, detail: embedProbe.detail || "" },
      content: verification.error ? "degraded" : "available",
      date: verification.error ? "degraded" : "available",
      entity: verification.error ? "degraded" : "available",
    },
    metrics,
    lane_attribution: {
      counts: laneCounts,
      hit_count: queries.filter((query) => Number(query.first_expected_rank) > 0).length,
    },
    worst_performing: worst,
    observations,
    queries,
  };

  const jsonPath = path.join(EVAL_DIR, `${OUTPUT_STEM}.json`);
  const markdownPath = path.join(EVAL_DIR, `${OUTPUT_STEM}.md`);
  await Bun.write(jsonPath, `${JSON.stringify(report, null, 2)}\n`);
  await Bun.write(markdownPath, markdownReport(report));

  console.log(`Retrieval eval: ${goldens.length} goldens; verified ${returnedSet.size}/${expectedSet.size} expected IDs`);
  console.log(`recall@1 ${(metrics.recall_at_1 * 100).toFixed(1)}% | recall@5 ${(metrics.recall_at_5 * 100).toFixed(1)}% | recall@10 ${(metrics.recall_at_10 * 100).toFixed(1)}% | MRR ${metrics.mrr.toFixed(4)}`);
  console.log(`lane attribution ${JSON.stringify(laneCounts)} | semantic ${embedProbe.status}`);
  console.log(`worst-performing ${worst.slice(0, 5).map((item) => `${item.id}:${item.first_expected_rank ?? "miss"}`).join(", ")}`);
  console.log(`wrote ${jsonPath}`);
  console.log(`wrote ${markdownPath}`);

  if (verification.error || missingIds.length) process.exitCode = 1;
}

await main();
