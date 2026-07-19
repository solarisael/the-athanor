# Retrieval evaluation harness

Run from the repository root:

```sh
bun evals/run-evals.ts
```

The command reads `golden-queries.json`, invokes the live Postgres recall path through `runRecallQuery`, verifies expected memory IDs via read-only WSL SQL, and writes a JSON/Markdown report (default stem `baseline-2026-07-18`; set `SOLARISAEL_EVAL_OUTPUT` to override). It also prints the scoreboard. The checked-in `baseline-2026-07-18.json` and `.md` are the 2026-07-18 baseline.
