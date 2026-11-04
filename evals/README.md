# Memory evaluation

## Daily-use measurement

Daily memory quality is measured from real production turns, not memory quizzes.
Enable local capture per room with `"recallTelemetry": true` in
`.solarisael-room.json`. The adapter writes the exact compact recall viewport,
route, suppression diagnostics, session ID, and SHA-256 of the user message to
the room-local `.omp/runtime/recall-turns.jsonl`. It does not duplicate the raw
user message. The OMP session log remains the text authority.

Audit collected turns with:

```sh
bun evals/run-daily-usage.ts
```

After enough real, non-mutating conversational turns exist, create blind replay
pairs with:

```sh
bun evals/run-daily-usage.ts --replay
```

The replay uses the saved conversation prefix and the viewport that was actually
shown on that turn. The memory condition keeps normal House context and explicit
memory tools. The no-memory condition removes the House memory extension.
Mutating tools are excluded from both conditions. Sol's blind A/B preference is
the authority; a model judge does not decide daily usefulness.

Reports and sealed A/B mappings stay under the private room-local
`.omp/runtime/daily-memory-eval/` directory. There is no honest daily-use score
until enough post-instrumentation turns have been collected and judged.

## Synthetic single-turn diagnostic

Run from the repository root:

```sh
bun evals/run-end-to-end.ts
```

This is a controlled diagnostic, not a daily-use quality measurement. It
exercises production query routing, recall lanes, compaction, and viewport
filtering, but then places the viewport into an isolated one-turn model call
with tools disabled, forced JSON citations, no real conversation history, and
synthetic memory questions. Those constraints make failure attribution useful
for the viewport pipeline. They do not reproduce normal Solarisael use.

`living-scenarios.json` describes information needs, evaluation criteria, risks,
authority mode, and current memory threads. Current reference packets resolve
from active, unsuperseded thread members at run time. Historical scenarios use
explicit historical records as authority evidence. Memory IDs are never the
retrieval target.

Every scenario runs with full recall and without recall. Scenarios with a
historical distractor also run a stale-distractor condition. Reports include
first-response success, claim and evidence coverage, stale or unsupported
claims, failure attribution, citations, and memory lift. The evaluator records
the exact evidence viewport, authority packet, model identities, usage, and
latency in its JSON report.

Do not report its pass rate as daily memory quality or user-visible reliability.

The default is three independent first responses per condition. Configure a run
with:

```sh
SOLARISAEL_E2E_TRIALS=5 \
SOLARISAEL_E2E_CONCURRENCY=3 \
SOLARISAEL_E2E_ANSWER_MODEL=openai-codex/gpt-5.6-sol \
SOLARISAEL_E2E_JUDGE_MODEL=openai-codex/gpt-5.6-sol \
bun evals/run-end-to-end.ts
```

The evaluator requires the OMP adapter repository beside this repository
because it imports the production recall compactor and viewport filter.

## Legacy historical-row ranking

The former golden-query benchmark is retained only as a narrow diagnostic:

```sh
bun evals/run-id-ranking-evals.ts
```

It reads `legacy-id-ranking-queries.json` and measures whether specific
historical database rows appear in a candidate ranking. It does not measure
answer quality, authority selection, supersession handling, model judgment, or
memory usefulness. Its recall@k and MRR values must not be reported as overall
memory-quality scores.

The checked-in `baseline-2026-07-18.json` and `.md` are historical artifacts
from that legacy benchmark.

Both evaluators resolve the substrate from `SOLARISAEL_SUBSTRATE`, then from
`SOLARISAEL_OBSIDIAN_ROOT` or `SOLARISAEL_VAULT_ROOT`, then from the local
default.
