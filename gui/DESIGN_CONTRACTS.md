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
- **Godot mapping:** The root `.tres` Theme owns fonts and sizes. Type variations expose semantic names. Native desktop UI disables stretch scaling, opts into high-DPI rendering, and imports the bundled faces with LCD antialiasing, normal hinting, automatic subpixel positioning, and viewport-default oversampling. Small static UI text does not use MSDF.
- **Proof:** Godot loaded and rendered Atkinson Hyperlegible Next, Cinzel Decorative, and JetBrains Mono from bundled WOFF2 assets. Native 1× Forward+ frames were inspected at 1440×900 and 640×760 on Sol's 1920×1080, 96-DPI RGB display.
- **Open gaps:** Literary, editorial, and artifact roles remain unimported. Reader scaling and fallback behavior remain unproved. English is the only source language until a deliberate localization pipeline exists. Effect-bearing text is a separate lane: survey `RichTextLabel`/`RichTextEffect`, `Label3D`, `TextMesh`, built-in MSDF, and maintained Godot addons before authoring custom glyph geometry.

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

This boundary is satisfied and no longer restricts the client. S01 now marks the
future active-conversation center explicitly; its chat surface remains an honest
placeholder until the Host conversation contract exists. Host-backed operator
workflow begins on S02 under the Recall Policy instrument record below.

## Record: operator shell

- **Identity:** `operator-shell`; group `composition`.
- **Meaning:** Keep navigation, the active conversation, contextual inspection, and live system state legible without making the client a second authority.
- **Refusal:** Do not merge authority, ranking, chronology, proposal state, dispatch state, subsystem health, or placeholder presentation with real Host data.
- **Fixed copy:** Prototype and per-screen data-origin disclosures remain visible until the Host supplies real contracts for every screen. The chat placeholder says exactly what does not exist.
- **Anatomy:** Top chrome; grouped screen navigation; left room/spirit/session sidebar; center active-conversation viewport; right context inspector; status dock. Auxiliary Recall, routing, GIGA, source, and system surfaces explicitly replace or overlay the center.
- **Variants:** Wide at 1200 px and above shows both sidebars. Compact from 800–1199 px keeps left navigation and hides the inspector. Narrow below 800 px makes the center primary and hides both sidebars plus nonessential chrome.
- **States:** Active screen, responsive class, subsystem health, focus, scenario, and movement. Active screen and responsive class are local presentation state owned by `AthanorProbe`, never derived from Host state.
- **Accessibility:** Controls use real Buttons. Group and screen selection remain separate focus surfaces. One center `ScrollContainer` owns vertical wheel and focus-follow scrolling; nested page scrollers are prohibited.
- **Composition:** The active conversation owns the center. Placeholder sidebars declare absent Host contracts rather than synthesizing room, session, source, or authority indexes.
- **Web mapping:** The website is visual-canon provenance only. Its WebGPU/WASM effect renderer is not the Athanor runtime; this client remains Godot.
- **Godot mapping:** `AthanorProbe` reparents the content viewport under one center scroller after screen-local bindings resolve, applies three width classes on root resize, adjusts Recall columns and state-grid density, and toggles sidebars and chrome. `HFlowContainer` owns wrapping action rows. One sibling `AthanorHostSession` owns the authenticated WebSocket.
- **Proof:** Real Forward+ frames render the chat-centered wide shell at 1440×900, compact Recall at 1000×760, and single-center narrow routing at 640×760. Debug-only scroll automation reached the bottom of S01, S02, and S03 after live Host content arrived.
- **Open gaps:** The compact inspector drawer trigger, narrow room/session drawer trigger, contrast measurement, reduced-motion behavior, and keyboard traversal observation remain unproved. Conversation, source, GIGA, lineage, and system projections remain open.

## Record: recall policy instrument

- **Identity:** `recall-policy-instrument`; group `product`.
- **Purpose:** Let an operator read the Host-owned Recall Policy projection and author exactly one durable change to `requested_mode`, without the client becoming an authority or inventing state.
- **Authority:** `docs/RUNTIME_ARCHITECTURE.md` sections 4, 4.1, and 4.5; `docs/GODOT_CLIENT.md` sections 2, 5, and 10; `crates/house-protocol/src/host.rs`; and the Rust Host in `crates/house-host`. Client binding lives in `src/protocol.rs`, `src/host_session.rs`, and `src/recall_policy.rs`.
- **Anatomy:** Fixed disclosure, shared-link column (address field, connect, disconnect, request-snapshot, transport state, transport detail, projection cursor, Host binding), and policy column (requested mode, resolved mode, active project, working set, resolution reason, last refresh, recovery, subsystem health, four proposal controls, staged selection, one authorising control, command lifecycle, unavailable reason).
- **Variants:** One instrument. The four requested modes are values inside it, not variants of it.
- **States:** Transport is `idle`, `connecting`, `connected`, `ready`, `disconnected`, or `protocol refused`. Command lifecycle is `idle`, `pending`, `acknowledged`, `refused`, or `failed`. Projection readiness is present or absent. Subsystem health is the Host's `degraded` field. The four axes render on separate components and never share one channel.
- **Refusals:** No projection field is defaulted, guessed, or coerced; an unknown enum value, field, event type, or foreign `schema_version` refuses the whole envelope and drops applied state. Selecting a mode never sends it. A write is impossible without an authenticated Host binding, an applied projection version, and a selection that differs from the current `requested_mode`. The root session reads `ATHANOR_HOST_TOKEN` from the process environment for the WebSocket `Authorization: Bearer` header; the token is never exported to or persisted in a scene resource. The client never infers a House, room, spirit, session, scope, visibility, or authority class, and never opens an address that is not `ws://` or `wss://`.
- **Fixed copy:** The non-authority and authenticated-snapshot disclosure is re-asserted from a Rust constant on every render and forced visible, so the scene cannot replace, empty, or hide it. It renders before any Host content.
- **Accessibility:** Every state carries a mark and a word, so it survives greyscale and stillness. Every disabled control keeps its label and carries its specific reason as a tooltip while the same reason is also visible as text. All controls are real `Button` and `LineEdit` nodes with `FocusMode::ALL`; the address field submits on Enter. Focus rings come from the `AthanorTab`, `AthanorTabActive`, and `AthanorField` focus styleboxes, none of which is empty.
- **Composition:** The instrument lives inside the existing content viewport as screen S02 and consumes only canon primitives. It adds no second root and no second transport path.
- **Tokens:** `AthanorVessel`, `AthanorStatusMargin`, `AthanorInstrumentColumn`, `AthanorInstrumentRow`, `AthanorKicker`, `AthanorStatusLabel`, `AthanorStatusValue`, `AthanorStatusGrid`, `AthanorStatusRow`, `AthanorNavigationRow`, `AthanorBody`, `AthanorMeta`, `AthanorField`, `AthanorTab`, and `AthanorTabActive`. No literal color, size, or font appears in Rust or in the scene.
- **Examples:** With no link open, every write control is disabled, each one says `NO HOST CONNECTION`, and every projection value reads `—`.
- **Counterexamples:** Showing `AUTO` as the requested mode before a snapshot arrives, because a displayed default is indistinguishable from Host truth.
- **Web mapping:** The localhost administrative Recall Policy form in the OMP adapter supplies workflow evidence only. Its persisted record is not the Host envelope and must not be copied into this client.
- **Godot mapping:** `AthanorRecallPolicy` extends `PanelContainer`, consumes `AthanorHostSession`, and binds its scene paths. Every exported name in `src/recall_policy.rs` matches its assignment in `main.tscn`; renaming one requires renaming both in the same pass.
- **Proof:** The real Host and client share schema version, WebSocket path `/athanor/v1/ws`, command/event names, mutation vocabulary, sequence, and hash fields through `house-protocol`. A live Forward+ frame applied `requestedMode=auto`, `resolvedMode=conversation`, working-set count 4, authenticated Kintsu binding, version 1, sequence 1, and state hash from the isolated Host.
- **Open gaps:** Delta coverage is intentionally limited to `field_update` on this flat projection; every other mutation type is refused and forces an explicit resynchronise rather than partial application.

## Wire and display separation

Wire type and path constants live in `crates/house-protocol/src/host.rs` and are
imported by `src/protocol.rs`: `auto`, `conversation`, `work`, and `quiet` for
`requested_mode`, and `conversation`, `work`, `mixed`, and `quiet` for
`resolved_mode`. Display labels remain client-local in `display()` and are never
compared to, parsed from, or sent as wire values.

The instrument shows both, marked, as `CONVERSA (wire conversation)`, so an
operator can read the label and audit the value without the two collapsing into
one string. Snapshot projection fields use the Host contract's camelCase names;
delta mutation fields remain the `house-protocol` snake_case field vocabulary.

## Record: worker-lane status instrument

- **Identity:** `worker-lane-status`; group `product`.
- **Purpose:** Make the House's bounded worker bodies and advisor boundary legible without dispatching or inventing availability.
- **Authority:** `house-core::routing::lane_status`, `ROUTING_STATUS`, `ROUTING_RESULT`, and `ROUTING_PROJECTION_ID`.
- **Anatomy:** Fixed read-only disclosure, connection/result state, scrollable lane details, advisor boundary, event/sequence receipt, and explicit refresh.
- **States:** Host unavailable, query pending, status applied, or exact protocol refusal.
- **Refusals:** No direct spellbook read, no folder-derived identity, no dispatch, no inferred health, no unknown result fields, and no result whose correlation ID does not match the pending request.
- **Fixed copy:** The screen states that it is read-only and cannot dispatch or initiate agents.
- **Accessibility:** Lane capability is written in text rather than encoded only by color; refresh is a real focused Button with a reason-bearing tooltip when disabled.
- **Composition:** S03 consumes the root `AthanorHostSession`; it does not open a socket or hold credentials.
- **Godot mapping:** `AthanorRoutingStatus` parses the typed routing result into a bounded view and renders exact lane name, model role, OMP agent, tools, context modes, edit/intent/acceptance permissions, and the non-dispatchable advisor.
- **Proof:** A live isolated Host returned four lanes through `athanor.routing.status`; the 1100×760 Forward+ frame rendered the authenticated result with event ID and sequence 1. The visible first lane was `smol-scout · pi/smol · scout · LEITURA`.
- **Open gaps:** Familiar spellbook status, actual dispatch receipts, quest lineage, and live worker lifecycle remain separate future screens/contracts.

## Record: effects laboratory

- **Identity:** `effects-laboratory`; group `research-product`.
- **Purpose:** Keep rendering experiments production-quality, measurable, and directly promotable without coupling decorative output to Host authority.
- **Authority:** Godot 4.7.1 built-ins, the pinned Juicee `1.4.2` runtime tree, and this record. The Juicee editor plugin, autoload, and updater remain disabled.
- **Anatomy:** Four source-labelled alchemical treatments, one real `phase_stoke.tres` sequence, overlap/cancel/restore controls, accessibility controls, non-authority disclosure, and live render instrumentation.
- **States:** Selected phase, active independent sequence instances, reduced motion, no flash, no chromatic separation, responsive viewport class, and sampled renderer metrics.
- **Refusals:** Effects never encode verification, routing, completion, or Host truth. Only one overlapping sequence owns the backbuffer lane. No runtime network updater, floating addon version, fake transcript, or placeholder effect mechanism is allowed.
- **Accessibility:** Flash is blocked by default. Reduced motion freezes procedural shader time and removes scale motion from duplicated runs. No-chromatic disables both Juicee separation and phase-shader channel splitting. Every phase and authority boundary is written in text.
- **Composition:** The durable scene lives at `effects_lab/effects_lab.tscn` inside this Godot project so it consumes the real theme and import pipeline without entering `main.tscn`.
- **Proof:** The headless contract runner proves overlay uniqueness and cleanup, accessibility blocking, ref-counted restoration, independent overlap, run retirement, disabled VSync, and 640/1440 responsive widths. A native D3D12 Forward+ frame renders the 1100×760 two-column gallery without engine errors.
- **Open gaps:** Material Maker and Phantom Camera have not entered the laboratory. Phase treatments are candidate mechanisms, not accepted product canon. Named-hardware budgets need representative animated-load captures before any treatment enters the operator shell.
