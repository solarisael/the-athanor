# Agent rules for the-athanor

## Version discipline (Sol's standing rule — HARDENED 2026-08-14)

Do NOT change version numbers on every change. Ordinary fixes and features
accumulate under `CHANGELOG.md` `[Unreleased]` with no version bump.

A version string changes only when a release payload is actually built AND
deployed, and then as the smallest possible step: prefer a sub-patch
(`0.9.6` → `0.9.6.1`) over a patch bump. Never move the minor version toward
`1.0` without Sol's explicit say-so. Never use `-rc` suffixes for follow-ups —
`0.9.x-rc1` sorts *before* `0.9.x` in semver and reads as a downgrade. The
installer treats versions as opaque strings, so four-segment sub-patches are
safe.

Doc "current source version" claims track the release train, not each hotfix.

**Agents do not change version strings. Ever.** Not in `package.json`, not in
build scripts, not in docs. Sol bumps versions himself when *he* decides a
release happens. An agent's job ends at `[Unreleased]`.

## Lesson queries — execute before each task

Read the root `LESSONS.jsonl` before implementation or fanout.
Each line is a complete argument object for `xd://lessons`.
Submit every line unchanged, once per task.
Check that every requested ID returns within its stated family.
Read every returned body before the first edit.
Give workers the complete returned bodies, inline or through a shared packet.
Do not skip a batch or replace bodies with bare IDs.
Stop before editing if a query fails or a requested record is missing.

PostgreSQL owns the lesson bodies.
Apply each lesson when its trigger matches the task.
Retrieval does not activate parked projects or authorize additional work.
Read `docs/LESSONS.md` for unavailable references and the query contract.
Read `docs/roadmap.md`, `docs/ARCHITECTURE.md`, and `docs/EVIDENCE.md` before implementation.
Use current owning documentation when historical notes disagree.

Query the registry for additional task-specific lessons when needed.
Use shape and technology filters when a lexical query misses relevant guidance.
Preserve the lesson family with each ID.

## Main surface (standing, 2026-08-28 — project lesson 462)

The web client at `gui-prototype/` is the House's main operator surface.
The parked Godot client moved to the private `solarisael/athanor-godot` repository.
The release does not ship it. Its rules, including typography, live in that repository's `AGENTS.md`.

## Lessons scopes: runtime vs GUI

Two project scopes, deliberately separate:

- **`the-athanor`** — runtime, substrate, delivery, Host, installer, adapter
  rules. Version discipline (lesson 358) lives here. The main-surface
  direction (lesson 462) also lives here.
- **`the-athanor-gui`** — Godot client taste: theme, typography, layout, screen
  contracts. These lessons apply only inside the private `solarisael/athanor-godot`
  repository, and only when Sol reopens the Godot client.

Record new lessons into the matching scope with the `remember` organ
(`kind: project-lesson`, `project` as above).
