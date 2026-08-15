# Production GUI Lessons Map

This map governs only `gui/`: The Athanor's production Godot client, its native scenes and resources, and the `athanor-godot` Rust GDExtension.

It does not govern the repository root or the disposable browser experiment. Interaction discovery lives at `../gui-prototype/` and has its own map at `../gui-prototype/LESSONS_MAP.md`.

PostgreSQL remains authoritative for lesson and memory bodies. `DESIGN_CONTRACTS.md` owns the local semantic contract records. Source owns each platform's implementation. This map joins those authorities into one design, code-format, and taste path; it does not replace them.

Before consequential work, retrieve the named lesson bodies. Bare IDs are routing labels, not delivery.

## Product boundary

The production GUI is a thin operator client:

- Godot owns native presentation, input, focus, responsive composition, and platform integration.
- The Rust GDExtension owns the typed client boundary, Host transport, protocol translation, and native Godot-facing state.
- The Host owns commands, projections, synchronization, and durable effects.
- PostgreSQL owns installed durable authority behind the Host.

The GUI never reaches directly into PostgreSQL, NATS, providers, or harness internals. It never infers authoritative domain state from visual state. A successful animation, optimistic row, or conversational sentence is not a durable-write receipt.

`src/host_link.rs` is the only outbound network surface and accepts only WebSocket URLs. Keep that door singular.

## Current navigation ruling

The primary modes are exactly:

1. **Direct** — agent conversations.
2. **Hallways** — dated multi-agent conversations without merged rooms.
3. **Projects** — project conversations and instruments.

Selecting an entity changes the center instrument. Stable chrome names the mode; the entity header names the selected object.

Refusals:

- No workspace tab per spirit.
- No room merge disguised as a Hallway.
- No global mode per subsystem instrument.
- No archive label such as `S01–S14` or `R01` used as navigation or product ontology.
- No giant screen taxonomy substituted for ordinary object selection.

Existing `S01` files, preview scenes, and `Screen` routing are evidence from the earlier design laboratory. They may remain while a bounded slice still depends on them, but new product work must not deepen that taxonomy. Migrate a touched slice to semantic mode/entity/instrument names rather than adding another numbered screen.

House memories #3555 and #3572 carry this corrected product shape. Older archives remain evidence, not authority over the correction.

## Authority order

### Product and interaction

1. Current explicit operator ruling.
2. Current PostgreSQL-authoritative Athanor memories and project lessons.
3. Accepted records in `DESIGN_CONTRACTS.md`.
4. Browser-observed interaction evidence from `../gui-prototype/`.
5. Older design archives and numbered-screen compositions.

### Visual language

Follow the authority order already recorded in `DESIGN_CONTRACTS.md`:

1. Solarisael's current website for visual and interaction canon.
2. The design catalogue for reviewed semantic contracts.
3. The friend archive for documentation shape and composition evidence.
4. Godot resources and Rust Controls for platform implementation.

Do not copy platform syntax between layers. A CSS selector is not a Godot theme item; a DOM transition is not a scene-tree contract. Preserve the semantic anatomy and re-express it natively.

## Product composition

The operator shell owns four stable regions:

- **Left collection:** Direct, Hallways, Projects, their current entities, and the operator/account door.
- **Center instrument:** the selected entity's primary conversation or working surface.
- **Context inspector:** selected-entity or selected-item detail, never a second center authority.
- **Bottom status strip:** separate quiet channels for transport, Recall, body, kittens, delivery, and other explicit domains.

Deeper instruments compose inside a selected agent, Hallway, or Project. Recall policy, memory mapping, GIGA review, canon inspection, House health, sources, dispatch, and kitten work are instruments—not automatic new global navigation modes.

One root owns responsive docking, drawers, current mode/entity, inspector visibility, and local presentation preferences. Domain projections remain independent of those presentation axes.

## Design taste

### Familiar chrome, House-specific meaning

A new operator should recognize conversations, collections, composer, selection, inspector, drawers, and settings without study. Solarisael identity belongs in the surfaces, typography, texture, ornament, controlled motion, and language. Do not paste the website into a desktop shell; do not erase the House into generic enterprise UI.

### Quiet is the carrier

The shell stays calm enough for content and consequence to register. Glow, motion, ornamental framing, and phase treatment are spent sparingly and for named meaning. If every panel is ceremonial, no panel is.

### Dense, not cramped

Operational information may be dense. Preserve grouping, comparison, a clear downward scan, and enough negative space to mark completed thoughts. Do not solve density by shrinking required text below its contrast or readability floor.

### State must remain legible without effects

Selection, consequence, health, authority, proposal state, chronology, focus, and availability survive grayscale, reduced motion, and disabled environmental effects. Beauty may reinforce structured state; it never becomes the only signal.

### Ordinary language beats interface commentary

Name real objects and actions. Remove copy that explains the existence of a panel, drawer, card, or navigation system. Keep fixed disclosure, refusal reasons, provenance, and durable effects exact even when the surrounding voice is warm.

### Consequence owns action weight

Controls derive visual weight from durable, destructive, safe, and cancel semantics. There is no free visual variant that can dress a consequential action as a harmless one. Unavailable controls expose their reason in the same component.

### Epistemic channels do not blend

Authority, relevance, chronology, health, ranking, and proposal state use separate components and signals. A recent result is not authoritative; a ranked result is not canon; a backlog is not aggregate system health; a tool failure is not a write outcome.

## Contract before component

Every promoted component or composition gets a record in `DESIGN_CONTRACTS.md` with:

- stable identity and group;
- purpose and owning authority;
- required anatomy;
- allowed semantic variants and states;
- impossible/refused combinations;
- fixed copy;
- accessibility behavior;
- allowed parents, children, and import direction;
- semantic tokens;
- valid and invalid examples;
- web evidence and native Godot mapping where applicable;
- rendered proof;
- open gaps.

Do not create a component because two screenshots look similar. Extract only when one semantic correction must reach every use for the same owner. Domain-specific labels, policy, and variation stay at the composition.

## Layering

Use one import direction:

1. **Foundation:** theme root, palette roles, typography roles, spacing and motion metrics.
2. **Primitives:** surfaces, text actions, consequence buttons, disclosure, fields, status channels.
3. **Product components:** message, receipt, evidence, composer, source, health, Recall, dispatch, or related domain anatomy.
4. **Compositions/instruments:** Direct conversation, Hallway, Project, settings, inspector, and bounded workflows.
5. **Operator shell:** collection, center, inspector, status strip, responsive docking, and route/state ownership.

Product vocabulary may compose canon primitives. Product vocabulary never leaks downward into the reusable visual canon, and presentation-only types never become protocol values.

## Scene grammar

### `.tscn` owns stable anatomy

Scenes own:

- stable node hierarchy;
- container relationships;
- anchors, size flags, and minimums;
- theme type variations;
- static resources and component composition;
- owner-unique node names used by the script;
- fixed copy or initial empty/refusal state that belongs to the component contract.

Do not build static hierarchy imperatively in GDScript. Use code for dynamic records, state transitions, Host projection application, and behavior that cannot be expressed as scene composition.

Prefer containers over manual coordinates. A child that must shrink or grow receives explicit size flags. Exactly one scroll owner governs a reading region; nested scroll containers must have an explicit handoff or one is neutralized.

### Scene names carry semantics

Use names such as `direct_conversation`, `hallway`, `project_workspace`, `account_drawer`, or the stable contract identity. Do not create new `sNN_*` or `rNN_*` names. Node names identify their role in the component anatomy rather than their current visual appearance.

### Theme resources own stable appearance

Color, typography, spacing, borders, and reusable state treatment flow through `theme/`, type variations, and named `.tres` resources. Components consume semantic roles. Do not scatter raw visual values through scenes or GDScript.

A theme resource may be platform-specific. Its semantic role must still trace to a design contract or explicit local experiment; do not name an arbitrary value as canon merely because it moved into `.tres`.

## GDScript grammar

Follow one obvious top-level scan path:

1. `@tool` when editor synchronization is intentional;
2. `class_name` and `extends`;
3. contract comments that preserve non-obvious intent or refusal;
4. signals;
5. enums and constants;
6. exported semantic properties;
7. `@onready` node handles;
8. `_ready` and engine callbacks;
9. public component operations and queries;
10. signal handlers;
11. private state/application helpers.

Code format and taste:

- Use the repository's Godot formatter conventions: tabs for GDScript indentation and formatter-stable wrapping.
- Type parameters, return values, collections, and local values when the admitted shape matters.
- Names predict the effect or answer. Booleans read as propositions; reason strings name the refusal.
- Signals announce operator intent or completed local transitions. The caller owns Host commands and durable effects.
- Export semantic state, content, and reason fields—not free styling knobs that bypass the contract.
- An exported setter updates the rendered component when the node is ready. Keep the synchronization path singular.
- Public methods state what callers may do. Private `_apply_*` methods synchronize one owned visual contract.
- Use guards for unavailable node state and explicit refusal. Do not silently discard unknown fields or unsupported states.
- Blank lines separate signals, constants, properties, lifecycle, public API, handlers, and private machinery because those are different thoughts—not because a quota says so.
- Comments preserve ownership, honesty, platform hazards, or why a state exists. Delete comments that merely narrate the next line.
- Do not compress several transitions into a clever expression. The plain sequence is preferred when it makes ownership visible.

### Honest data

Scenes contain no plausible synthetic runtime history. Empty or absent Host contracts render an explicit disclosure/refusal state. Fixture data belongs in bounded smoke scripts or the browser prototype, never in the production scene as if delivered.

A display component renders Host-provided content verbatim unless its contract explicitly owns formatting. It does not guess timestamps, identities, authority, health, or durable effects.

## Rust GDExtension grammar

The `athanor-godot` crate is platform glue and client-state ownership, not a second Athanor core.

- Use `cargo fmt` as the mechanical floor; review formatter-stable code for scan path and ownership.
- Each module has one center of gravity: transport, protocol, session, routing, projection-specific state, or shell composition.
- Keep protocol constants and wire shapes sourced from `house-protocol`; do not tidy or duplicate them by hand.
- Keep transport phase, projection readiness, and subsystem health as separate types and channels.
- Public types name admitted facts or capabilities. Functions name their observable answer or effect.
- Validate at the boundary before touching a peer or scene. Unsupported schemes, fields, states, and command shapes refuse explicitly.
- Async and process lifecycles retain one owner, bounded shutdown, and observable completion. A signal sent is not an exit receipt.
- Avoid cloning, allocation, conversion, and Godot `Variant` traffic when ownership or a borrowed native type can remain clear.
- Keep Godot conversion at the platform seam. Domain decisions stay in the Rust core/Host, not in `VarDictionary` manipulation.
- Comments explain boundary ownership, wire compatibility, safety, or a platform trap. Routine Rust remains self-explanatory through names and structure.

## Prototype-evidenced interaction candidates

The browser experiment has established evidence for these candidate contracts. None is production-promoted by this map alone. Each remains a candidate until it crosses the gate below through an accepted production contract record, native Godot implementation, and rendered Godot proof.

### Nested drawers

- Rendered panes form one ordered state set.
- Exactly one pane is interactive and exposed to accessibility/focus.
- Back and Escape move one state outward.
- Opening records the return-focus owner.
- Mobile/root Escape closes the sidebar and returns focus to its toggle.
- Focus enters only after transition geometry settles; stale focus work is invalidated.
- Clipping must not create an unintended scroll owner.

The exact browser `overflow: hidden`/Chromium repair in memories #3565 and #3566 is evidence about the semantic focus-and-geometry contract, not a CSS instruction for Godot.

### Selection

Changing mode or entity updates collection selection, center instrument, context inspector, and local chrome as one owned transition. A previous entity's selected sub-item never leaks into the next entity.

### Responsive shell

The center instrument remains the priority. As width falls, the inspector yields before the center is crushed; the collection becomes an explicit drawer at compact width. No capability disappears without another visible route.

Godot is currently a desktop client. Do not claim mobile support merely because the HTML experiment works at 390 px. Prove the native window sizes the product actually supports.

### Status strip

Each status item reports one domain and opens its own detail. Never synthesize one aggregate House verdict from transport, Recall, body, workers, delivery, GIGA, Vault, or AKASHA state.

## Prototype-to-production gate

A browser behavior enters production only when:

1. Sol has accepted it on the rendered prototype surface.
2. Its product object and owner are named.
3. Its anatomy, states, refusals, focus, keyboard behavior, responsive behavior, and disclosure are explicit.
4. A `DESIGN_CONTRACTS.md` record owns the semantic contract or an existing record is deliberately extended.
5. The Godot implementation uses native scenes, containers, resources, focus APIs, and signals.
6. The Rust boundary consumes explicit Host commands and projections.
7. The real Godot surface proves the behavior again.

Do not copy HTML, CSS, or browser JavaScript into Godot-shaped wrappers. Translate meaning, not syntax.

## Lesson loadout

Retrieve these bodies before design or implementation work in this directory.

### Design — load together

- **Design #293:** visual weight follows consequence.
- **Design #294:** unsafe compositions should be unconstructible.
- **Design #295:** unavailable controls state why.
- **Design #296:** authority, relevance, chronology, and health use separate channels.
- **Design #297:** fixed disclosures cannot be softened; absent contracts refuse plausible fake content.
- **Design #298:** stable color and type roles belong to tokens.
- **Design #299:** one root owns global visual state; independent axes remain independent.
- **Design #300:** effects are spend; quiet is the carrier.
- **Design #301:** state survives grayscale and stillness.
- **Design #302:** required text meets measured contrast.
- **Design #303:** component, existing style, then local layout glue.
- **Design #304:** product vocabulary composes canon and never mutates protocol authority.

### Athanor product

- **Project #117:** combine familiar agent interaction, Solarisael visuals, and Athanor domain meaning.
- **Project #118:** mapping, provenance, ambiguity, and durable effects remain inspectable.
- **Project #119:** authored effects, local surfaces, and environmental overlays remain independent.
- **Project #120:** preserve machine-legible contracts and human authority/correction.

### Code format, structure, and proof

- **Coding #9:** plain line, clean door, sharp refusal.
- **Coding #10 and #11:** names predict the answer or effect and do not lie.
- **Coding #12:** each file has one center of gravity.
- **Coding #13:** an awkward helper name diagnoses mixed contracts; it does not convict by spelling alone.
- **Coding #14:** code makes its refusals and loyalties visible.
- **Coding #19 and #20:** keep unavoidable platform ugliness in one named seam; delete wrappers with no owned boundary.
- **Coding #27:** write the honest first shape, then compress without losing truth.
- **Coding #134:** frontend UX proof belongs to the rendered surface.
- **Coding #143:** centralize only semantic behavior with shared authority.
- **Coding #194:** comments preserve intent, danger, ritual, and unrecoverable constraints.
- **Coding #196 and #200:** prefer the plain line and use negative space as punctuation.
- **Coding #321:** keep the operator feedback loop to one bounded change and immediate real-surface proof.
- **Coding #381–385:** formatter floor, mental-operation line breaks, ownership-shaped indentation, stable alignment, and one top-level scan path.

### Conditional lifecycle lesson

- **Project #338** loads only when work touches the legacy `gui-server.ts` → `RustJsonlTransport` HTTP/process boundary or equivalent worker-replacement shutdown ordering. It does not govern the current Godot/Rust GDExtension `HostLink` transport and is not part of the default GUI loadout.

## Proof before calling production GUI work done

A build is not visual or interaction proof. Exercise the actual Godot surface.

For every changed slice:

- launch the real project and reach the surface through its real navigation path;
- inspect hierarchy, spacing, text legibility, density, selection, and visual weight;
- exercise keyboard navigation, focus entry, focus return, Escape, and visible focus;
- exercise the smallest and largest supported native window sizes named by the slice;
- verify no unintended nested scrolling, clipping, or offscreen control;
- verify reduced-motion/static treatment where motion or effects changed;
- exercise each refusal and confirm its reason is visible;
- verify empty, loading, connected, stale, error, and recovery states that the contract admits;
- verify Host-owned effects through an observable receipt rather than an optimistic visual change;
- confirm fixture or absent-contract content cannot masquerade as delivered data.

A screenshot proves appearance at one state. A keyboard walk proves focus behavior. A Host receipt proves a command or projection. Name which proof supports which claim.

## Updating this map

Update this file when a GUI-specific product ruling, design taste law, code-format decision, platform trap, or promotion gate changes. Do not move the rule to the repository-root map: root-wide convergence and GUI craft are different retrieval surfaces.

Before editing:

1. retrieve current design, project, and coding lesson bodies;
2. retrieve the newest Athanor GUI product memories;
3. inspect the current `DESIGN_CONTRACTS.md` and owning implementation;
4. distinguish operator ruling, accepted contract, experiment evidence, current code, and inference.

After editing, verify that this map and `../gui-prototype/LESSONS_MAP.md` agree on shared product grammar while preserving their different authority and platform boundaries.
