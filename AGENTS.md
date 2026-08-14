# Agent rules for the-athanor

## Version discipline (Sol's standing rule)

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

Load-bearing rows for this repo's delivery spine: 365-369 (JetStream failure
contracts, retention authority, ack-deadline-as-lease, dedup-window-ends-
before-side-effect, diagnose-from-persisted-state), 349 (retry keys carry real
execution identity), 49/51 (inserts declare their key; migrations idempotent).

## Lessons scopes: runtime vs GUI

Two project scopes, deliberately separate:

- **`the-athanor`** — runtime, substrate, delivery, Host, installer, adapter
  rules. Version discipline (lesson 358) lives here.
- **`the-athanor-gui`** — Godot client taste: theme, typography, layout,
  screen contracts, design-system rules. Client taste must not blur into
  runtime rules and vice versa. Working under `gui/` → query BOTH (`the-athanor`
  for boundaries like "Godot is presentation-only", `the-athanor-gui` for taste).

Record new lessons into the matching scope with the `remember` organ
(`kind: project-lesson`, `project` as above).

## Typography (standing, 2026-08-14 — project lesson 375)

One face: the **system font** (Godot `SystemFont`, `["Segoe UI", "sans-serif"]`)
for everything. The theme's `default_font` is the single source; no per-style
`fonts/font` overrides; no bundled faces without Sol's explicit say-so.
Rendering: grayscale antialiasing, **light** hinting (full hinting produces
crushed small glyphs), auto subpixel positioning.
Two floors are law: no `font_size` below **13px**; no light-on-dark
`font_color` below **0.7** brightness (dark-on-light chip text is exempt).
Hierarchy comes from size and color above the floors, never face-switching.
