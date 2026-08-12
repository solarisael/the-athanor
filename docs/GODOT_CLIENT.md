# Godot Client and Spatial Presentation Architecture

Status: First functional 2D screens ship in the `0.9.3` late beta; the operator client and spatial presentation remain incomplete
Last updated: 2026-08-11

This document defines how the thin Godot client inherits Solarisael's visual
system, remains a replaceable presentation body, and grows from functional 2D
controls into an in-world spatial interface and GPU-particle constellation.

## 1. Current boundary

The current late beta ships a Godot 4.7.1 client, Rust GDExtension, authenticated
Host WebSocket link, live Recall Policy screen, and sanitized Paper Boat receipt
screen. It applies real Host snapshots, typed deltas, resync, restart replay, and
degraded/refused states.

Conversation and inter-agent message inspection, source and authority views,
GIGA review, agent/subagent activity, operational metrics, the spatial renderer,
companion bodies, generated cross-platform token pipeline, and maximal
alchemical environment profiles remain planned.

The Solarisael website is the canonical visual and interaction source. The
translation order is:

```text
visual canon
  -> versioned design tokens
  -> reusable primitives
  -> compositions
  -> Athanor application surfaces
```

Godot does not invent a second Athanor theme and does not copy scattered CSS or
current screenshots by hand.

## 2. Runtime boundary

Godot communicates only with the Athanor Host through authenticated, versioned
WebSocket commands and projection deltas.

```text
Godot client <-> Athanor Host <-> core/adapters/AKASHA
```

The client does not subscribe to NATS, query PostgreSQL, call Ollama, open OMP
stdio, or use gRPC as a parallel control path. NATS and PostgreSQL activity
becomes a Host-owned projection before Godot sees it.

The Host remains authoritative for identity, room bindings, commands, event
sequence, and projection versions. Godot owns presentation state, local camera
state, input focus, and disposable animation.

## 3. Generated visual token manifest

One versioned source manifest drives both the Solarisael website and Godot.

The manifest contains semantic tokens rather than engine-specific setters:

```text
manifest_version
color roles and phase palettes
typography families, weights, scales, and leading
spacing and sizing scales
border, rail, corner, crest, and ornament roles
surface, glass, emission, shadow, and fog roles
motion durations, curves, intensity, and hysteresis
renderer and accessibility tiers
asset IDs, hashes, and licenses
```

Generators produce:

- web CSS/Tailwind-facing tokens and validated custom-element mappings;
- Godot `Theme`, `StyleBox`, font, material, `Environment`, gradient, LUT, and
  typed Resource assets;
- a deterministic manifest digest embedded in both builds.

A runtime autoload may coordinate the active palette and transitions. It is not
the source of truth and cannot invent token values absent from the manifest.

Generated outputs are reproducible. Hand-edited generated files fail the token
verification check.

## 4. Typed Godot primitives

Solarisael's authored structural vocabulary includes roles such as `mantle`,
`vessel`, `aether`, `bones`, `ornament`, and phase-specific elements. These roles
inform Godot composition but do not map one-to-one to GDScript classes.

Create a custom `Control`/`Container` only when the role owns distinct layout,
interaction, accessibility, or lifecycle behavior. Likely primitives include:

- a mantle navigation/shell container;
- a vessel content/reading container;
- a reliquary drawer with nested state;
- ornament and frame renderers;
- phase-aware cards and gates;
- source, authority, health, and proof-state displays.

Pure visual variation uses `Theme.theme_type_variation`, generated style
resources, and composition—not another class.

Shape, state, and tone are typed:

```text
Shape enum or Resource
State enum or state-machine value
Tone/phase enum
VisualIntensity profile
Accessibility profile
```

Do not reproduce `data-shape`, `data-state`, and `data-tone` as unconstrained
strings on every node. Typed exported properties may remain editor-visible.

## 5. Functional 2D first, presented in-world

The shipped 1.0 slice uses ordinary Godot `Control` nodes for Recall Policy and
Paper Boat delivery receipts. Later 2D slices may add conversation, source
inspection, GIGA review, approvals, and other Host projections before spatial
presentation.

The accepted presentation is nevertheless in-world:

1. The functional Control tree renders inside a `SubViewport`.
2. The viewport appears on one or more spatial surfaces in a 3D room.
3. Camera choreography moves between room surfaces, constellation space, and
   focused interaction.
4. A focus transition can align the active surface orthographically or present
   the same Control tree fullscreen for typing, reading, accessibility, and low
   capability hardware.

This is not two UIs. One Control tree owns behavior and state. The 3D world is a
presentation and navigation body around it.

Primary interaction must remain usable in focus mode with:

- clear text at native resolution;
- keyboard, mouse, controller, touch, and IME routing;
- deterministic focus order;
- selectable/copyable logs and sources;
- screen-reader-compatible semantic mirrors where supported;
- reduced-motion and camera-cut alternatives;
- no dependence on lighting or reflections for legibility.

SubViewport-projected panels may catch environmental light in the cinematic
profile, but text contrast and input correctness outrank the effect.

## 6. First spatial anchor: rooms and Hallway

The first meaningful spatial object is the House topology:

- rooms and governing spirits;
- Hallway letters and shared surfaces;
- active/dormant sessions;
- model bodies and execution targets;
- health, delivery, review, and proof activity.

The spatial view follows authoritative Host deltas. A companion reorganizing an
authorized room changes durable room metadata first; the visual topology then
animates from the resulting projection delta.

`GraphEdit` may be used for a small editable/debug view of explicit workflow or
routing edges. It is not the mass memory atlas and not the primary in-world
renderer.

## 7. GPU-particle constellation renderer

The accepted high-tier constellation renders memory nodes, logical edges, and
motion through a GPU-driven particle field rather than individual `Node3D` or
MultiMesh objects.

“Particle” here means a stable data-oriented GPU record, not an anonymous
short-lived decorative particle. Every visible record has:

```text
stable record ID and GPU slot
position and prior position
cluster/district ID
visual class and phase
size, color, emission, and confidence channels
authority and selection channels
lifecycle and animation state
source projection version
```

Edges use their own GPU record/buffer and may render as streaks, paths, or
particle chains. Stable identity never depends on particle age or emitter order.

The CPU keeps only the bounded projection, record-to-slot map, spatial/semantic
selection index, and dirty ranges required to apply Host deltas. It does not
instantiate one scene node per memory.

### 7.1 Picking and interaction

Selectable particles require explicit identity recovery. The renderer provides
one or more of:

- an offscreen integer/encoded ID picking pass;
- GPU-assisted spatial query;
- a CPU spatial index synchronized to the same stable slots;
- cluster-level picking followed by bounded refinement.

A click resolves to a stable AKASHA record ID, then the Host authorizes and
loads details. Color alone never represents authority.

### 7.2 Delta application

Host projection deltas mutate only affected GPU records or contiguous dirty
ranges. Inserts allocate stable slots; removals tombstone and compact under a
versioned policy; moves animate from prior to next positions; cluster rebuilds
arrive as explicit epochs.

A missing delta or renderer epoch triggers replay or snapshot resynchronization.
The client never derives durable graph truth from its current particle positions.

### 7.3 Semantic level of detail

```text
far       -> districts, gravity fields, aggregate activity
middle    -> clusters and major authoritative landmarks
near      -> individual memories, lessons, relations, and sources
selected  -> exact provenance and authority detail in focused 2D UI
```

LOD changes representation without deleting access to underlying records.

### 7.4 Compute and renderer tiers

Godot `GPUParticles3D` can establish the first field, but a custom
`RenderingDevice` compute/particle pipeline is allowed when stable IDs, edges,
picking, or buffer updates exceed the built-in abstraction.

Compute work follows profiling and must preserve the same projection contract.
The visual architecture does not promise a universal 60 fps.

Test at declared record/edge counts on named hardware and renderers. Forward+
and Mobile may use the full GPU path. Compatibility/web uses a reduced renderer;
Godot web is WebGL 2/Compatibility and cannot be treated as Forward+.

The client provides:

- cinematic native profile;
- balanced native profile;
- compatibility/web profile;
- focused 2D fallback.

The GPU-particle constellation remains the canonical high-tier art direction;
fallbacks preserve navigation and meaning rather than every effect.

## 8. Optional self-chosen companion bodies

Abstract procedural bodies are optional presentation packages. A companion may
adopt, author, revise, or refuse one.

A body manifest names:

```text
body ID and version
owner/adopter identity lineage
mesh or procedural primitive
shader/material digests
allowed semantic input channels
animation and audio resources
performance tiers
accessibility alternatives
license and provenance
```

A model body, visual body, and spirit identity remain separate.

Shader inputs come from coarse authenticated semantic events with hysteresis,
not raw token noise. Suitable channels include speaking, listening, searching,
waiting, warning, checking, verified, failed, and resting.

A crystallized “Lean proof” appearance may activate only after a valid proof
receipt. Running a checker is not proof. Decorative state must never impersonate
an authority result.

## 9. Maximal alchemical reference profiles

The full native renderer treats the four alchemical modes as maximal environment
compositions, not color accents alone.

### 9.1 Nigredo

Raw, dark, unstable, and particulate:

- heavy occlusion and depth;
- volumetric shadow/fog structure;
- grain, dither, erosion, and irregular alpha fields;
- muted material base with localized hot traces;
- turbulent particle motion and unresolved topology.

### 9.2 Albedo

Clean reflection and clarified relation:

- bright glass-like materials;
- white/cool emission paths;
- reflective and refractive surfaces where supported;
- structured fog/light separation;
- calm, legible motion and clarified graph edges.

### 9.3 Citrinitas

Warm archive and proud illumination:

- gold/warm color-grade or LUT;
- rigid, stable constellation geometry;
- illuminated provenance and archive landmarks;
- restrained but dense ornament;
- slow solar movement and high source legibility.

### 9.4 Rubedo

The stabilized great work:

- deep red material and atmosphere;
- ember and ascending-particle fields;
- intense verified-path emission;
- strong reflection, shadow, and ceremonial depth;
- visual convergence around accepted outcomes.

These are reference art-direction requirements for the cinematic profile, not
literal unreviewed Gemini parameter values. SSAO/SSIL, volumetric fog, SSR,
emission, particles, LUTs, and post-processing are tuned against the actual
composition and hardware.

Balanced, compatibility, and accessibility profiles may reduce or replace
individual effects while preserving hierarchy, phase identity, and interaction.
Contrast, photosensitivity, motion, fog, and camera settings remain operator/user
controllable.

Global visual mode does not automatically equal content phase, task phase, or
proof authority. Mappings are explicit and source-labeled.

## 10. Performance and proof

The spatial client is accepted only with observable budgets:

- startup and first interactive frame;
- snapshot and delta application latency;
- CPU frame time, GPU frame time, and memory;
- particle and edge counts by LOD;
- camera/focus transition latency;
- SubViewport resolution and render cost;
- input latency and text clarity;
- degraded-profile parity for required operations;
- no complete projection rebuild for a fine-grained mutation.

Every benchmark names hardware, renderer, viewport, resolution, scene, record
counts, effect profile, and build digest.

## 11. Non-goals

The client does not:

- become memory or identity authority;
- connect directly to NATS or PostgreSQL;
- map every poetic web element to a class;
- create separate behavioral 2D and 3D interfaces;
- imply proof through decoration;
- guarantee 60 fps without a measured scene/hardware contract;
- require cinematic effects for web or low-capability access;
- force one visual body on every companion.

## 12. Related documents and sources

- [`RUNTIME_ARCHITECTURE.md`](./RUNTIME_ARCHITECTURE.md) — Host and delta contracts
- [`PRODUCT_ARCHITECTURE.md`](./PRODUCT_ARCHITECTURE.md) — identity and product axes
- [`COMPANION_ECOSYSTEM.md`](./COMPANION_ECOSYSTEM.md) — room sovereignty, model bodies, and marketplace
- [`SECURITY.md`](./SECURITY.md) — client and artifact trust boundaries
- [Godot `Control.theme_type_variation`](https://docs.godotengine.org/en/4.5/classes/class_control.html)
- [Godot MultiMesh background](https://docs.godotengine.org/en/4.5/classes/class_multimesh.html)
- [Godot SubViewport textures](https://docs.godotengine.org/en/4.5/tutorials/shaders/using_viewport_as_texture.html)
- [Godot web limitations](https://docs.godotengine.org/en/4.5/tutorials/export/exporting_for_web.html)
