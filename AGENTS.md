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

Load-bearing rows for this repo's delivery spine: 365-369 (JetStream failure
contracts, retention authority, ack-deadline-as-lease, dedup-window-ends-
before-side-effect, diagnose-from-persisted-state), 349 (retry keys carry real
execution identity), 49/51 (inserts declare their key; migrations idempotent).

## Main surface (standing, 2026-08-28 — project lesson 462)

The web client at `gui-prototype/` is the House's main operator surface.
The Godot client under `gui/` is PARKED, not deleted: `athanor.exe` stays the
canonical desktop owner of harness processes, but the face it fronts is the
web prototype. Do not extend Godot screens, themes, or scenes without Sol
explicitly reopening the Godot client.

## Lessons scopes: runtime vs GUI

Two project scopes, deliberately separate:

- **`the-athanor`** — runtime, substrate, delivery, Host, installer, adapter
  rules. Version discipline (lesson 358) lives here. The main-surface
  direction (lesson 462) also lives here.
- **`the-athanor-gui`** — parked Godot client taste: theme, typography,
  layout, screen contracts. Dormant while the client is parked; applies only
  when Godot work is explicitly resumed. Client taste must not blur into
  runtime rules and vice versa.

Record new lessons into the matching scope with the `remember` organ
(`kind: project-lesson`, `project` as above).

## Typography (parked Godot client only — project lesson 375)

One face: **Atkinson Hyperlegible Next as an MSDF FontFile**
(`multichannel_signed_distance_field=true`, `hinting=0`) wrapped in a
`FontVariation` with `variation_embolden = 0.35` — the embolden compensates
Godot's un-gamma'd dark-theme blending and was the decisive lever. The theme's
`default_font` is the single source; no per-style `fonts/font` overrides; no
additional faces without Sol's explicit say-so.
Two floors are law: no `font_size` below **14px**; no light-on-dark
`font_color` below **0.7** brightness (muted tier is 0.78; dark-on-light chip
text exempt). Hierarchy comes from size and color, never face-switching.
Rendering law: any surface showing the UI (including viewport-textured 3D
quads) must map **1:1 texels to pixels at rest** — orthographic camera sized
to the quad, nearest filtering. Depth and focus belong to motion, never to a
resting frame.
