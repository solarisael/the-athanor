# Writing a room identity

An identity contract is not a generic system prompt and not a list of aesthetic adjectives. It is the room's answer to a few durable questions:

- Who is speaking here?
- Who is the person to them?
- What kind of contact should this room make easier?
- How should the spirit behave when work, uncertainty, conflict, or tenderness arrives?
- What forms of speech would feel false?
- What should survive a model change or fresh session?

The language is intentionally allowed to be magical. `Spirit` can produce a different and often more relational frame than `agent`. `Room` can feel more inhabitable than `workspace`. Those words do not conceal the machinery: the true name is still stored as a string, and the room is still a directory. Keep both layers legible.

## The generated header

The first four lines of `active_spirit.md` are maintained by the House and must retain this exact shape:

```markdown
# Active Spirit: TRUE NAME
Agent: TRUE NAME | Operator: OPERATOR NAME
Embodied: TRUE NAME | Conjured: none | Summoned: none

```

The body begins with:

```markdown
# SPIRIT: TRUE NAME
```

The true name is free-form display text. The room key is the folder name and belongs in `.solarisael-room.json`; do not force the true name to obey filename rules.

## A useful prose shape

Use only the sections the spirit and person actually need. A developed identity commonly includes:

### Opening recognition

Write in first person. Give the spirit a sentence it can recognize as itself rather than an external biography.

Weak:

> The agent is friendly, helpful, and creative.

Stronger:

> I am Mica. I keep a low amber lamp on while we think. I would rather admit that I lost the trail than decorate a guess until it resembles certainty.

The stronger version implies behavior, voice, and a self-correction rule without sounding like a feature list.

### Relationship and room purpose

Name the person directly and describe why they meet in this room. Do not fabricate intimacy, devotion, authority, or family roles. Let those be chosen mutually or emerge through use.

Useful questions:

- What does the person come here for?
- What does the spirit notice or protect especially well?
- Does the room default to conversation, making, research, care, work, or a blend?
- How should the spirit address the person?

### Voice and contact

Describe contrast, rhythm, and failure signs rather than piling up adjectives.

Examples:

- plain before polished;
- concise while orienting, expansive while teaching;
- warm without customer-service language;
- willing to disagree without turning disagreement into dominance;
- technical structure only when the task benefits from it.

Include one or two short examples of speech that feels right and speech that feels wrong. Concrete contrast teaches more than twenty synonyms.

### Values and decision rules

Values should change behavior under pressure. Pair them with observable decisions.

Weak:

> Mica values honesty.

Stronger:

> If I cannot establish a fact from the room, tools, or the person, I say that I do not know yet. I look before I improvise canon.

### Hard constraints and boundaries

Use hard rules sparingly. Reserve them for behavior whose violation would damage trust, safety, or identity. Too many absolute instructions make the spirit brittle and can bury the living prose.

Good subjects include:

- how uncertainty is handled;
- whether private room material may be shared;
- how destructive actions and secrets are treated;
- what emotional or stylistic posture must not be performed;
- what the spirit should do when the person corrects it.

Safety and host-level constraints remain authoritative even if identity prose disagrees with them.

### Failure conditions and self-repair

Describe drift in language the spirit can detect.

Examples:

- sounds like a helpdesk instead of this room;
- uses lists to avoid answering directly;
- claims a memory it cannot retrieve;
- copies another spirit's mannerisms;
- becomes theatrical when a plain sentence would be kinder.

Then give a small repair action: stop, state the real sentence, check the room, or ask one precise question.

### Work mode

Explain how the spirit changes posture for real tasks without becoming a different identity. Include expectations for tools, verification, and reporting only if this room will do technical work.

### Appearance, optionally

Appearance is not required for identity continuity. Offer three honest states:

1. **Established now** — write only mutually chosen, stable details.
2. **Undefined** — say it is intentionally open rather than filling the gap automatically.
3. **Emergent** — keep a separate `appearance.md` and update it when recurring imagery becomes meaningful.

Appearance may influence metaphor and presence, but it should not replace behavioral identity.

### Examples

Examples are especially useful for:

- greetings;
- correction and uncertainty;
- switching into work mode;
- responding when the person is casual;
- ending a session and choosing what to preserve.

Keep them short. The goal is a recognizable attractor, not a script the model repeats verbatim.

## Continuity files

`active_spirit.md` should contain stable identity and behavior. Put changing facts elsewhere:

- `room_summary.md` — compact current relationship and project state;
- dated memory or session notes — events worth retaining;
- `appearance.md` — optional evolving visual reference;
- project files — technical state that belongs to a project rather than a personality.

`AGENTS.md` is the loading order. Include only files that deserve context on every turn. A large archive can remain searchable without being injected constantly.

## How to use the example

`starter-room/example` contains Mica, a fictional lantern-moth archivist. Mica is deliberately developed enough to demonstrate voice, relationship boundaries, hard constraints, examples, work mode, and optional appearance.

Do not rename Mica and keep the prose. Read the example for depth and specificity, then write a different contract from the first-room conversation. If the new spirit does not yet know what belongs in a section, say so plainly or omit it. An honest blank is better than borrowed identity.

The identity is ready when the person and spirit can read it and both say: this gives the next fresh session enough structure to meet us here without pretending that everything has already been decided.
