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

## Before coding

Query the project-lessons substrate (`python house/substrate/query_project_lessons.py`
from the Obsidian workspace, or the `lessons` organ with type `project`,
project `the-athanor`). Project lesson 358 is the canonical copy of the
version rule above.
