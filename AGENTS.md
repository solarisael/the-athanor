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

## Lessons map — query it every time you work here

Before writing or changing ANY code in this repo, query the lessons registry
(the `lessons` organ, or `python house/substrate/query_coding_lessons.py` /
`query_project_lessons.py` from the Obsidian workspace). Once per task, not
once per session.

Retrieval discipline: the lexical query is weak. Widen before concluding
"no lessons": query by `shape` (e.g. `idempotency`, `refusal`, `verification`,
`process`), and know the technology-key vocabulary — NATS lessons are keyed
`nats`/`jetstream`/`nats-jetstream` (lessons 365-369), Go lessons are keyed
`go-toolchain`/`go-modules` or carry no keys at all (370-374). When a filter
returns nothing, direct SQL against the `lessons` table is ground truth.
Versions do not need exact spelling: a trailing number is a version, so
`bend` finds lessons keyed `bend-2`. Different names still need their own key.

Load-bearing rows for this repo's delivery spine: 365-369 (JetStream failure
contracts, retention authority, ack-deadline-as-lease, dedup-window-ends-
before-side-effect, diagnose-from-persisted-state), 349 (retry keys carry real
execution identity), 49/51 (inserts declare their key; migrations idempotent).

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
