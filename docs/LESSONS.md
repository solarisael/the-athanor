# House Lessons

Lessons preserve reusable guidance in typed stores. They are not a second name for memories.

## Choose the store

| Kind | Use it for | Required boundary |
|---|---|---|
| `coding-lesson` | Transferable engineering or process rules | Should remain correct in another repository using the same tools |
| `project-lesson` | Rules, constraints, and decisions owned by one project | Requires a stable project key |
| `writing-lesson` | Durable prose, register, voice, and taste rules | Describes when the preference should shape writing |
| `design-lesson` | Reusable design-system taste and rationale | Names the design system whose catalogue entries the rule governs |
| `audio-lesson` | Reusable speech and audio-pipeline rules | Names the pipeline condition and observable result |
| `memory` | An event, decision, realization, or thing that happened | Preserves retrieval-bearing context rather than an abstract rule |

Use this test:

- “Would this still be correct in another repository using the same tools?” → coding lesson.
- “Is this true because of this repository's architecture, client, deployment, naming, or history?” → project lesson.
- “Did this happen?” → memory.

## Coding lesson contract

Write coding lessons through `remember` with `kind: "coding-lesson"`.

| Field | Meaning |
|---|---|
| `title` | Stable, short lesson name |
| `body` | The action, boundary, and reason |
| `shape` | Compact retrieval taxonomy such as `process`, `testing`, `naming`, `tooling`, `security`, or `refusal` |
| `scope` | `shared` or one room key; omitted scope defaults to `shared` |
| `project` | Optional project label or provenance; it does not make a coding lesson project-exclusive |
| `proofPattern` | Observable evidence that the rule was followed |
| `triggerContext` | The operation, uncertainty, or decision boundary used to retrieve the lesson |
| `voice` | Optional originating craft voice; not model selection |
| `tags` | A few specific technologies, operations, or failure modes |

Example:

```json
{
  "title": "Verify archive layout from the extracted artifact",
  "body": "A bundle is not verified by inspecting its build script. Build it, extract it into a clean temporary directory, and invoke the documented command from the documented working directory.",
  "kind": "coding-lesson",
  "shape": "testing",
  "scope": "shared",
  "project": "the-athanor",
  "proofPattern": "The generated archive is extracted; required paths are asserted; the documented verifier exits successfully from the adapter directory.",
  "triggerContext": "Changing packaging, installers, archive layout, or release instructions.",
  "tags": ["release", "archive", "installer", "artifact"]
}
```

## Project lesson contract

A project lesson belongs to one project rather than transferable craft. Write it through `remember` with `kind: "project-lesson"` and a stable `project` key.

Example:

```json
{
  "title": "Preserve legacy validation boundaries",
  "body": "Treat uncertain legacy behavior as a validation question against the existing systems and their data. Do not invent compatibility rules from table names.",
  "kind": "project-lesson",
  "project": "example-project",
  "proofPattern": "The implementation or decision cites observed legacy behavior, data, or an explicit stakeholder answer.",
  "triggerContext": "Inferring behavior while adapting a legacy flow.",
  "tags": ["legacy", "validation"]
}
```

A coding lesson with `project` set remains a coding lesson. The project label improves provenance and general recall; it does not convert a global craft rule into a project-only constraint.

## Writing, design, and audio lessons

Writing lessons preserve a specific prose decision and its retrieval context. Avoid vague instructions such as “write better.” Name the register, sentence behavior, humor mechanism, or prohibited shape.

Design lessons preserve reusable design-system taste: why a token, component, contract, or guideline is shaped the way it is, and when the rule should govern new work. Bind each one to a named design system. The catalogue itself is read through the `design_doc` organ and written through `design_doc_write`; a design lesson refers to those entries rather than replacing them.

Audio lessons preserve reusable pipeline behavior such as model choice, preprocessing, pronunciation, pacing, loudness, or export boundaries. Include the observable audio property that proves the rule.

## Importing skills and workspaces

The Athanor can absorb `SKILL.md` files, agent rules, postmortems, snippets, governance, and framework notes. Import the semantics, not the file count.

For each source rule, determine:

1. when it applies and should be retrieved;
2. what the agent should do or avoid;
3. what evidence proves compliance;
4. whether it is transferable, room-specific, or project-bound;
5. which original source remains authoritative.

A good import prompt is:

> Read these skill and governance files. Extract only rules that still apply to the current tools and environment. Split compound instructions into independently retrievable lessons, preserve important boundaries, reject obsolete or contradictory advice, and show the proposed lessons, projects, scopes, retrieval contexts, proof patterns, and sources before storing them.

Import in bounded batches. Deduplicate by meaning, preserve source provenance, and test representative retrieval before continuing. A smaller set of atomic lessons retrieves better than copied documents with overlapping language.

Generic workspace import profiles can map:

- governance to project lessons and policies;
- reusable methods to canonical skills or coding lessons;
- project-specific conventions to project lessons;
- living facts to typed source documents and memories;
- deliverables to authoritative artifacts;
- local, temporary, or archived material to exclusion rules.

The original source remains available for exact evidence unless the operator performs a verified authority cutover.

## Retrieve and enforce lessons

Use the `lessons` organ when durable guidance matters to the current decision. Apply the smallest useful filter for the task.

Trigger fields are reviewed TTSR authoring data. A lesson enters native OMP enforcement only when its tags include `ttsr-approved`.

The Athanor adapter loads approved rules into the active native `TtsrManager`. OMP owns matching, interruption, reminder injection, and retry.

Use OMP `/omfg <complaint>` to author a new guard from an observed offense. Review positive and ordinary negative examples before approval.

## Updating lessons

`update_lesson` revises one coding, project, writing, or design lesson in place.

The request envelope is `{ kind, id, expectedTitle, patch }`. The guard requires:

- the exact lesson kind and numeric ID;
- the exact current title in `expectedTitle`;
- at least one explicit replacement field inside `patch`.

Omitted patch fields remain unchanged. The row ID remains stable. `alwaysOn` changes stored eligibility metadata; it does not inject the lesson into OMP.

Coding and project lessons accept `project`. Use `clearProject: true` to write SQL `NULL`. The fields are mutually exclusive.

Update a lesson when its rule keeps the same identity but its wording or typed metadata changes. Cross-store fields are refused.

## Deleting lessons

`delete_lesson` permanently deletes exactly one coding, project, writing, or design lesson. It requires the numeric ID and exact current title.

Use deletion for an obsolete rule that should no longer remain recoverable in the active lesson store. For consolidation, update and verify the surviving lesson first, then delete the redundant row.

Deletion is not a bulk-cleanup mechanism.

## Verifying an import

After importing lessons:

1. retrieve several distinctive shapes;
2. retrieve project names and exact lesson titles;
3. confirm shared and room scopes;
4. confirm project-only constraints did not become global craft;
5. inspect proof patterns and trigger contexts;
6. confirm copied prose became actionable atomic rules;
7. confirm the original source path remains available where exact evidence matters.
