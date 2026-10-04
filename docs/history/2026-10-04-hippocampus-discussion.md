# GIGA and Hippocampus: problem, goals, and release plan

Archived on 2026-10-04 from `docs/HIPPOCAMPUS.md`. These sections are discussion and plan, not current claims. Section numbers are the original ones. "Section 14.2" refers to [`HIPPOCAMPUS.md`](../HIPPOCAMPUS.md), which keeps the contract. Stage 1 has shipped (HIPPOCAMPUS §28). Stages 2-7 and §24-27 remain open plan.

## 3. Problem

House logs exact turns and supports deliberate memory. Long sessions can still hide important events from the final consolidation step.

The active model can miss a decision, correction, or lesson when it prepares a paper boat. A later recall query can also miss an event that never became a durable record.

Hippocampus creates low-authority pointers near the time of the event. Later consumers use those pointers to fetch exact source spans.

## 4. Goals

Hippocampus must:

1. Mark possible durable events after exact turns are logged.
2. Use a local model by default.
3. Run outside the conversation hot path.
4. Point to exact source turns.
5. Keep generated annotations non-authoritative.
6. Support memory, lesson, correction, entity, and thread candidates.
7. Use harness task metadata when an adapter provides it.
8. Help `remember`, `recall`, task completion, and `sleep` find evidence.
9. Support review, promotion, Curios, dismissal, expiry, and reprocessing.
10. Preserve room, project, and source authority boundaries.
11. Expose enough provenance to reproduce each annotation.
12. Measure benefit, cost, misses, and false-positive burden.

## 24. Evaluation

### 24.1 Candidate quality fixture

Create a sanitized set of sessions with human labels for:

- durable events;
- non-durable chatter;
- coding lessons;
- project lessons;
- corrections;
- supersessions;
- entity updates;
- thread updates.

Lock the human labels before any treatment run starts. Label authors must not inspect treatment output.

Outcome adjudicators must not know which output used Hippocampus. Reveal the treatment only after scoring finishes.

Measure:

- candidate precision;
- candidate recall;
- missed high-priority events;
- false-positive review burden;
- kind classification accuracy;
- source-span accuracy;
- duplicate cluster quality.

### 24.2 Consolidation outcome

Compare paper boats and durable writes with and without Hippocampus.

Measure:

- recovered human-labeled events;
- unsupported promoted claims;
- duplicate durable records;
- reviewer time;
- source fetch count;
- consolidation latency.

### 24.3 Retrieval outcome

Run later recall queries against events that did not receive manual memory writes.

Measure whether candidate pointers improve exact-source recovery. Do not count a generated gist as successful evidence recovery.

### 24.4 Agent benchmark

Run the paired House-on and House-off benchmark before GIGA where practical.

Repeat the treatment with GIGA and Hippocampus enabled. Keep model, harness, tasks, tools, and budgets constant.

Report these scores separately:

```text
No House
AKASHA House
AKASHA + GIGA House
```

### 24.5 Performance

Measure:

- enqueue overhead;
- annotation latency;
- local model tokens;
- local compute time;
- memory and storage growth;
- queue recovery after downtime;
- effect on active-turn latency.

Response generation must not await classifier completion.

On named reference hardware, total incremental active-turn overhead must stay below 25 ms at p95 and 100 ms at p99.

The enqueue portion must stay below 15 ms at p95 and 50 ms at p99.

Measure both budgets over at least 10,000 events with the declared release configuration.

## 25. Security review

The implementation requires a security review before release.

The review must cover:

- prompt injection inside visible turns;
- malicious task metadata;
- candidate poisoning;
- room boundary bypass;
- project scope confusion;
- classifier endpoint compromise;
- API key permissions;
- queue payload tampering;
- source hash validation;
- promotion without review;
- telemetry data leakage.

Classifier output must remain data. It must never become an executable instruction without a separate trusted policy step.

## 26. Delivery stages

### Stage 1: Event and candidate contracts

Implement versioned event validation, candidate validation, source references, and review states.

Acceptance:

- schema tests cover every required field;
- invalid candidates fail closed;
- exact turn references resolve;
- room isolation tests pass;
- no classifier is required yet.

### Stage 2: Promotion handlers

Implement every promotion handler from Section 14.2.

Acceptance:

- fixtures cover all seven candidate kinds;
- only an authorized deliberate-write path changes durable state;
- invalid input leaves durable state unchanged;
- every handler checks source hashes and scope;
- every handler records provenance and authorization;
- correction and supersession updates are atomic.

### Stage 3: Local classifier worker

Add the replaceable provider boundary and one tested local provider.

Acceptance:

- classification runs after exact logging;
- the active turn does not wait;
- zero-candidate output succeeds;
- invalid output creates no candidate;
- provider failures remain replayable.

### Stage 4: Sleep and remember consumers

Add candidate slates to `sleep` and explicit `remember` flows.

Acceptance:

- consumers fetch exact sources;
- generated gists remain non-authoritative;
- review actions preserve provenance;
- one durable record can merge several candidates.

### Stage 5: Recall candidate lane

Add low-authority candidate pointers to recall source selection.

Acceptance:

- recall fetches exact evidence;
- candidates never override canon or project authority;
- archived and superseded behavior remains unchanged;
- cross-room candidates remain inaccessible.

### Stage 6: Harness task metadata

Add optional OMP task, todo, subagent, tool, and verification events as the harness exposes them.

Acceptance:

- adapters can omit unsupported events;
- fixture events produce one compact intent for the correct worker, role, and phase;
- the selected packet contains no more than four promoted lessons;
- the same intent digest reuses the same packet;
- a changed worker, role, phase, project, task kind, risk, target, change, or proof contract refreshes the packet;
- ordinary tool calls do not refresh the packet;
- unreviewed lessons never become task policy.

### Stage 7: Evaluation and release

Run the candidate, consolidation, retrieval, performance, and agent evaluations.

Acceptance:

- sanitized fixtures and methods are public;
- limitations are explicit;
- AKASHA behavior remains available when GIGA is disabled;
- upgrade and rollback preserve exact logs and candidate provenance.
- each public result names its method, fixture or corpus, hardware, date, limitations, and sanitized artifact;
- public demonstrations use a sterile synthetic House;
- one evidence package supplies a short presentation, reproducible live demonstration, and public posts;
- GUI, avatar, marketplace, organizational import, and perfect installer work do not block this gate.

## 27. Release acceptance

GIGA can ship before 1.0 when all these statements are true:

1. GIGA remains optional and requires AKASHA.
2. Hippocampus runs asynchronously and fails open.
3. Every candidate points to exact durable sources.
4. Every candidate remains non-authoritative before promotion.
5. Room and project isolation tests pass.
6. The local classifier works without a remote provider.
7. Sleep and remember fetch exact evidence before promotion.
8. Recall labels candidate pointers as low authority.
9. Review actions preserve provenance and history.
10. Diagnostics omit raw private text by default.
11. Candidate quality has a sanitized measured baseline.
12. AKASHA works unchanged when GIGA is disabled.
13. The supported profile meets its declared quality and resource thresholds.
14. GIGA meets the active-turn overhead budget on named hardware.
15. Only an authorized deliberate-write path can promote a candidate.
16. Room-local review requires the authenticated governing spirit or a scoped principal.
17. Curios remain pointer-only and cannot promote themselves through resonance.
