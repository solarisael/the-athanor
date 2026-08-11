# Design Contracts

This document defines the neutral contract format for The Athanor client.

PostgreSQL design documents remain authoritative. Source code remains the implementation authority for each platform.

## Authority order

1. Use the Solarisael website as visual and interaction canon.
2. Use the design catalogue for reviewed semantic contracts.
3. Use the friend archive for documentation shape and composition evidence.
4. Use Godot resources and Rust Controls as the client implementation.

Do not copy platform syntax between layers.

## Contract record

Each design record uses these fields.

- **Identity:** Stable semantic name and group.
- **Purpose:** Meaning carried by the record.
- **Authority:** Owning catalogue documents and source files.
- **Anatomy:** Required named parts.
- **Variants:** Allowed semantic variants.
- **States:** Allowed runtime states.
- **Refusals:** Combinations that must not exist.
- **Fixed copy:** Text that cannot be replaced or softened.
- **Accessibility:** Contrast, motion, focus, and non-color requirements.
- **Composition:** Allowed parents, children, and import direction.
- **Tokens:** Semantic roles consumed by the record.
- **Examples:** One valid use.
- **Counterexamples:** One invalid use and its reason.
- **Web mapping:** Owning web tokens, selectors, and components.
- **Godot mapping:** Theme items, type variations, resources, and Rust Controls.
- **Proof:** Real observation that proves both mappings preserve the contract.
- **Open gaps:** Missing evidence or unsettled decisions.

The manifest supplies inventories, props, enum values, and token records.
The README supplies purpose, refusals, fixed copy, and accessibility facts.
The adherence rules supply executable prop and token restrictions.
Missing per-component prompts remain missing evidence.

## Record: theme root

- **Identity:** `theme-root`; group `foundation`.
- **Purpose:** Own the visual identity while independent axes stay separate.
- **Authority:** Catalogue #5 and #10; `src/styles/index.css`; `src/layouts/index.astro`.
- **Anatomy:** One root theme owner and independent phase, effect, shell, scale, text, and measure state.
- **Variants:** One visual identity now: `solarisael`.
- **States:** Phase values are `nigredo`, `albedo`, `citrinitas`, `rubedo`, and `codex`.
- **Refusals:** Phase cannot become the theme identity. A nested region cannot create a second root.
- **Accessibility:** Reduced motion stops motion without removing static meaning.
- **Composition:** Product components consume the root. Product components never redefine canon tokens.
- **Tokens:** The reliquary palette, font roles, text scale, and semantic accent.
- **Web mapping:** `<html>` owns theme, shell, effect, and scale. `<body>` or `<container>` owns phase.
- **Godot mapping:** One root `.tres` Theme serves the Control tree. Named phase variations supply semantic accent to bounded subtrees.
- **Proof:** The 2026-08-08 gallery uses one root Theme, semantic containers, and five phase accents without changing identity.
- **Open gaps:** Shell, effect, text scale, measure, and phase subtree behavior still need interactive scene proof.

## Record: reliquary palette

- **Identity:** `reliquary-palette`; group `foundation`.
- **Purpose:** Separate ground, surfaces, readable text, decoration, and accent.
- **Authority:** Catalogue #1, #2, and #4; `src/styles/tokens.css`; `src/styles/index.css`; `src/styles/typography.css`.
- **Anatomy:** Root ground, crust, mantle, soft surface, raised surface, primary text, secondary text, muted text, accent, and second accent.
- **Variants:** Five phase accents route through one semantic accent role.
- **Refusals:** Components cannot consume a phase hue directly. Required text cannot use the muted role.
- **Accessibility:** Muted text measures 4.46:1 and remains decorative only.
- **Composition:** Ornament gold remains identity-bound when the contract names it as phase-independent.
- **Web mapping:** Components consume `--site_style_accent` and `--ui_*` roles.
- **Godot mapping:** The root Theme owns ground, surface, text, and phase colors.
- **Proof:** The declarative gallery renders all canonical roles from `theme/athanor_theme.tres`.
- **Open gaps:** Required-text contrast needs a measured Godot frame check before this palette becomes stable.

## Record: typography roles

- **Identity:** `typography-roles`; group `foundation`.
- **Purpose:** Separate readable text from controlled ceremonial voices.
- **Authority:** Catalogue #3 and #9; `src/styles/fonts.css`; `src/styles/typography.css`.
- **Anatomy:** Body, heading, mono, literary, editorial, ritual, and artifact roles.
- **Variants:** Public size names are display, large, main, mid, sub, body, fine, lead, kicker, and caption.
- **Refusals:** Public components cannot expose structural `h1` through `h5` size names.
- **Accessibility:** Body and heading use the hyperlegible face. Expressive faces stay on controlled upper steps.
- **Composition:** The role and size axes remain independent.
- **Web mapping:** Font role tokens and `sol__text_*` classes own the public vocabulary.
- **Godot mapping:** The root `.tres` Theme owns fonts and sizes. Type variations expose semantic names.
- **Proof:** Godot loaded and rendered Atkinson Hyperlegible Next, Cinzel Decorative, and JetBrains Mono from bundled WOFF2 assets.
- **Open gaps:** Literary, editorial, and artifact roles remain unimported. Reader scaling and fallback behavior remain unproved.

## Record: ritual surface

- **Identity:** `ritual-surface`; group `primitive`.
- **Purpose:** Carry content inside one named ceremonial plane.
- **Authority:** Catalogue #6, #8, and #10; `src/styles/base.css`; design lessons #298, #300, and #304.
- **Anatomy:** Ground, surface, border, content inset, and optional functional ornament.
- **Variants:** Mantle, vessel, aether, and ornament describe distinct semantic roles.
- **States:** Surface intensity follows the shell axis. Phase accent remains separate.
- **Refusals:** Ornament cannot replace structure. Effects cannot become the content carrier.
- **Accessibility:** The surface remains complete with motion disabled and in greyscale.
- **Composition:** Product surfaces compose canon primitives. Canon primitives never import product vocabulary.
- **Web mapping:** Ritual elements and data-shape attributes select the canon treatment.
- **Godot mapping:** `StyleBoxFlat` subresources and Theme type variations map each semantic role.
- **Proof:** One real Forward+ frame renders a container-led two-column grid with four authored semantic surfaces.
- **Open gaps:** The gallery marks geometry provisional. Exact border, radius, shadow, and spacing roles still need canon extraction.

## First gallery boundary

Build only these four records first.

The gallery must show one theme root, two phase scopes, the palette hierarchy, typography roles, and ritual surfaces.
It must not show fake Host data or an operator workflow.

Use a real Godot render as proof.
Do not write regression tests before the rendered contract becomes stable.

This boundary is satisfied and no longer restricts the client. Screen S01 remains
the declarative gallery under it. Host-backed operator workflow begins on screen
S02 under the Recall Policy instrument record below, which keeps the same
prohibition in a stronger form: no fake Host data at all, not even a default.

## Record: operator shell

- **Identity:** `operator-shell`; group `composition`.
- **Meaning:** Keep system identity, operator controls, task navigation, work content, and live context visible together.
- **Refusal:** Do not merge authority, ranking, chronology, proposal state, dispatch state, or subsystem health.
- **Fixed copy:** Prototype and per-screen data-origin disclosures remain visible until the Host supplies real contracts for every screen. The top bar names S01 as synthetic and S02 as Host-only; neither clause may be dropped while the other screen exists.
- **Anatomy:** Top bar, view controls, grouped navigation, content viewport, and status dock.
- **Variants:** Desktop shell first. Narrow behavior remains unresolved.
- **States:** Active group, active screen, display mode, subsystem health, focus, scenario, and movement. Active screen is local presentation state owned by `AthanorProbe`, never derived from Host state.
- **Accessibility:** Controls use real Buttons. Group and screen selection remain separate focus surfaces.
- **Composition:** Persistent shell regions frame one current task surface. The status dock never becomes a decorative footer.
- **Web mapping:** The restored S01 prototype supplies composition evidence only.
- **Godot mapping:** One container-led `ApplicationShell` owns five persistent regions and one expanding content viewport. `AthanorProbe` binds `resume_screen_button`, `recall_policy_screen_button`, `resume_page`, and `recall_policy_page`, and toggles visibility plus the active type variation.
- **Proof:** A real 1100×650 Forward+ frame renders S01 with visible disclosures, grouped navigation, content, and context. That observation stands for S01; the default viewport is now 1100×760 to give S02 vertical room, so the S01 proof frame is no longer the default window size.
- **Open gaps:** Secondary navigation behavior, narrow layout, contrast measurement, and reduced-motion behavior remain unproved. Keyboard traversal is now asserted in code for the S02 controls but not yet observed. Host wiring is implemented in source for exactly one projection and otherwise remains open.

## Record: recall policy instrument

- **Identity:** `recall-policy-instrument`; group `product`.
- **Purpose:** Let an operator read the Host-owned Recall Policy projection and author exactly one durable change to `requested_mode`, without the client becoming an authority or inventing state.
- **Authority:** `docs/RUNTIME_ARCHITECTURE.md` sections 4, 4.1, and 4.5; `docs/GODOT_CLIENT.md` sections 2, 5, and 10; `crates/house-protocol/src/host.rs`; and the Rust Host in `crates/house-host`. Client binding lives in `src/protocol.rs`, `src/host_link.rs`, and `src/recall_policy.rs`.
- **Anatomy:** Fixed disclosure, link column (address field, connect, disconnect, request-snapshot, transport state, transport detail, projection cursor, Host binding), and policy column (requested mode, resolved mode, active project, working set, resolution reason, last refresh, recovery, subsystem health, four proposal controls, staged selection, one authorising control, command lifecycle, unavailable reason).
- **Variants:** One instrument. The four requested modes are values inside it, not variants of it.
- **States:** Transport is `idle`, `connecting`, `connected`, `ready`, `disconnected`, or `protocol refused`. Command lifecycle is `idle`, `pending`, `acknowledged`, `refused`, or `failed`. Projection readiness is present or absent. Subsystem health is the Host's `degraded` field. The four axes render on separate components and never share one channel.
- **Refusals:** No projection field is defaulted, guessed, or coerced; an unknown enum value, field, event type, or foreign `schema_version` refuses the whole envelope and drops applied state. Selecting a mode never sends it. A write is impossible without an authenticated Host binding, an applied projection version, and a selection that differs from the current `requested_mode`. The client reads `ATHANOR_HOST_TOKEN` from the process environment for the WebSocket `Authorization: Bearer` header; the token is never exported to or persisted in a scene resource. The client never infers a House, room, spirit, session, scope, visibility, or authority class, and never opens an address that is not `ws://` or `wss://`.
- **Fixed copy:** The non-authority and authenticated-snapshot disclosure is re-asserted from a Rust constant on every render and forced visible, so the scene cannot replace, empty, or hide it. It renders before any Host content.
- **Accessibility:** Every state carries a mark and a word, so it survives greyscale and stillness. Every disabled control keeps its label and carries its specific reason as a tooltip while the same reason is also visible as text. All controls are real `Button` and `LineEdit` nodes with `FocusMode::ALL`; the address field submits on Enter. Focus rings come from the `AthanorTab`, `AthanorTabActive`, and `AthanorField` focus styleboxes, none of which is empty.
- **Composition:** The instrument lives inside the existing content viewport as screen S02 and consumes only canon primitives. It adds no second root and no second transport path.
- **Tokens:** `AthanorVessel`, `AthanorStatusMargin`, `AthanorInstrumentColumn`, `AthanorInstrumentRow`, `AthanorKicker`, `AthanorStatusLabel`, `AthanorStatusValue`, `AthanorStatusGrid`, `AthanorStatusRow`, `AthanorNavigationRow`, `AthanorBody`, `AthanorMeta`, `AthanorField`, `AthanorTab`, and `AthanorTabActive`. No literal color, size, or font appears in Rust or in the scene.
- **Examples:** With no link open, every write control is disabled, each one says `SEM CONEXÃO COM O HOST`, and every projection value reads `—`.
- **Counterexamples:** Showing `AUTO` as the requested mode before a snapshot arrives, because a displayed default is indistinguishable from Host truth.
- **Web mapping:** The localhost administrative Recall Policy form in the OMP adapter supplies workflow evidence only. Its camelCase persisted shape is a local record, not the Host wire shape, and must not be copied into this client.
- **Godot mapping:** `AthanorRecallPolicy` extends `PanelContainer`, exports `host_url`, and binds twenty-five scene paths. Every exported name in `src/recall_policy.rs` matches its assignment in `main.tscn`; renaming one requires renaming both in the same pass.
- **Proof:** Compile-intended Host and client source share schema version, WebSocket path `/athanor/v1/ws`, command/event names, mutation vocabulary, sequence, and hash fields through `house-protocol`. No build, import, editor run, frame capture, or command was performed in this pass.
- **Open gaps:** Runtime and visual proof remain outstanding. Delta coverage is intentionally limited to `field_update` on this flat projection; every other mutation type is refused and forces an explicit resynchronise rather than partial application.

## Wire and display separation

Wire type and path constants live in `crates/house-protocol/src/host.rs` and are
imported by `src/protocol.rs`: `auto`, `conversation`, `work`, and `quiet` for
`requested_mode`, and `conversation`, `work`, `mixed`, and `quiet` for
`resolved_mode`. Display labels remain client-local in `display()` and are never
compared to, parsed from, or sent as wire values.

The instrument shows both, marked, as `CONVERSA (wire conversation)`, so an
operator can read the label and audit the value without the two collapsing into
one string. Projection field names on the wire are snake_case, matching the
envelope field names in section 4.1.
