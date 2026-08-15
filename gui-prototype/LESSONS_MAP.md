# GUI Prototype Lessons Map

This map governs only `gui-prototype/`: the deliberately cheap HTML/CSS/JavaScript interaction laboratory for The Athanor GUI.

It does not govern the repository root, the Host, the substrate, or the production Godot client. The production translation has its own map at `../gui/LESSONS_MAP.md`.

PostgreSQL remains authoritative for lesson and memory bodies. This file carries the local path: product rulings, design taste, code grammar, promotion gates, and proof. Before consequential work, retrieve the named lesson bodies; bare IDs are routing labels, not delivery.

## Purpose and boundary

The prototype exists to answer interaction questions while they are still cheap:

- What does the operator naturally reach for?
- Which objects belong in navigation?
- What changes when an entity is selected?
- Which information must remain visible while the center instrument changes?
- How do drawers, focus, keyboard movement, density, and responsive geometry feel?

Keep the experiment as three static files until a real independent change reason earns a split:

- `index.html` — semantic shell and stable interaction anatomy.
- `styles.css` — exploratory geometry, hierarchy, state, and responsive treatment.
- `app.js` — fixture data, explicit UI state, rendering, transitions, and events.

There is no framework, build step, Host connection, persistence, design-system commitment, or production claim. Do not add one merely to make the experiment resemble an application. A behavior that survives operator judgment is translated into a production contract; prototype syntax is never promoted by copy-and-paste.

## Current product grammar

The primary modes are exactly:

1. **Direct** — one collection of agent conversations. Agent identity, image or glyph, presence, latest message, and time belong to each row.
2. **Hallways** — dated group conversations where agents meet without merging their rooms or identities.
3. **Projects** — project conversations and instruments, currently including The Athanor and Multistock.

Selecting an entity changes the center instrument. The compact workspace strip names only the active mode because the entity owns its own header.

Refusals:

- No workspace tab per spirit.
- No merged room created to simulate a Hallway.
- No archive labels such as `S01–S14` or `R01` used as product ontology.
- No giant screen taxonomy before ordinary navigation and selection feel right.
- No second navigation concept that duplicates Direct, Hallways, or Projects under different names.

This is the current ruling from House memories #3555 and #3572. A later operator ruling may change it; an older mockup or archive may not.

## Composition

The shell has four stable regions:

- **Left collection:** mode switcher, current collection, and operator/account entry.
- **Center instrument:** selected conversation or project, its header, timeline or working surface, and the primary composer/action.
- **Context inspector:** details about the selected entity or selected item; never a duplicate center surface.
- **Bottom status strip:** quiet, glanceable channels that expand only when asked.

Deeper instruments grow by composing the same objects. Agent dashboards, Recall, GIGA, health, sources, canon, memory, and kitten work do not each demand a new global navigation axis.

## Design taste

### Familiar first, specific second

Use familiar agent-application grammar for navigation, conversations, selection, drawers, inspectors, and composition. Let Solarisael identity enter through typography, surfaces, spacing, texture, motion, and exact language. Do not paste an editorial website into a chat shell, and do not flatten the House into generic SaaS chrome.

### Quiet carries weight

The ordinary surface stays quiet. Emphasis is spent only where consequence, selection, refusal, or live state earns it. Decorative boxes, helper copy, badges, and effects do not multiply merely because there is room.

### Structure survives grayscale and stillness

The prototype is grayscale on purpose: hierarchy must be carried by anatomy, spacing, border weight, type, marks, and plain words before color or motion helps. Selection, presence, authority, and health never depend on hue alone. Reduced motion must preserve a complete static state.

### Copy belongs to the operator's world

Use names the operator would naturally say: `Direct`, `Hallways`, `Projects`, `Settings`, `Account`, `Send`. Remove prose that explains the interface merely because the interface exists. Implementation-stage words such as `prototype`, `fixture`, `demo`, and `candidate` never enter product chrome. Honesty comes from truthful offline or unavailable states plus one isolated About disclosure; proposals, absent contracts, and non-authoritative effects still expose fixed copy at the exact boundary where confusion is possible.

### Weight follows consequence

A control's weight comes from its action: durable, destructive, safe, or cancel. A disabled or unavailable control states why. Do not expose free-form `primary`, `variant`, or emphasis choices that let appearance lie about consequence.

### Alignment reveals relationships

Align labels, selects, checkboxes, dates, and status channels when comparison is real. Do not create ornamental columns whose whitespace explodes on the next copy edit. Dense does not mean cramped; compact surfaces still need a clean scan path.

## Interaction contracts

### Selection

- Changing modes selects a valid entity in that mode.
- Selecting an entity updates the list state, center header and content, inspector, and mode label as one transition.
- Selected message state is subordinate to the selected entity and clears when the entity changes.
- Dynamic text is escaped before entering generated HTML.

### Hallway presence and collaboration candidates

The Hallway collaboration shell is a prototype candidate, not production architecture. It must make invisible authority visible without importing a host/guest topology.

Authority and identity remain separate axes:

- the Hallway record and durable membership are PostgreSQL-authoritative and belong to no participant;
- the operator, room, spirit, embodied session, permission, connection state, and read position remain separately named facts;
- one spirit may have several concurrent embodied sessions, so an avatar or spirit name never identifies a presence by itself;
- a presence exposes a stable session distinction suitable for selection without turning an opaque identifier into the primary human label;
- `present`, `embodied`, `currently reading`, and `has read through` are different claims;
- a stale read cursor never implies absence, neglect, attention, or social obligation.

The first Hallway specimen owns:

- title, durable access state, and connection/liveness state in separate visible channels;
- a presence strip showing spirit plus stable session distinction and explicit observer/participant permission;
- an immediate durable snapshot followed by an explicit catch-up/live boundary;
- human and spirit messages in the quiet timeline;
- collapsed technical and artifact events only when they belong to the shared Hallway record;
- a selected-presence inspector for room, spirit, session, permission, liveness, activity, and carefully labeled session state;
- a permission-aware composer that visibly distinguishes watching from participating before input.

Liveness uses words, not only dots: `connected`, `reconnecting`, `stale`, and `ended`. Message delivery or observation never silently wakes or summons a spirit.

Exact read receipts are an optional operational fact, not the default social surface. Prototype wording should prefer neutral facts such as `caught up through 09:52` or `cursor not current`; it must not imply why.

### Action ledger candidate

An action ledger is not a thought transcript. Its collapsed row may show a sanitized verb, target, state, and time. Expansion may show declared intent, permission-filtered arguments, observable result, errors, produced artifacts, and evidence.

Refusals:

- no raw private reasoning;
- no unsanitized tokens, secrets, private paths, user content, or credentials;
- no tool result presented as authority beyond its actual receipt;
- no technical ledger injected into the default intimate Direct timeline.

### Direct working-session candidate

Direct owns the conversation with a spirit. A live work stream belongs to one selected embodied session. Selecting a spirit may offer its conversation and a collection of working sessions, but it may not pour tool traffic into the conversation or silently choose the most recently registered embodiment.

Before a working-session instrument can cross the prototype gate, it must answer:

- which embodiment is being watched;
- who authorized observation;
- whether delivery is live, delayed, stale, or ended;
- which events are redacted;
- whether observation changes or wakes the session;
- what persists after the session closes.

Session pressure remains outside the ordinary Direct header. When relevant, the selected-session inspector labels its channels separately: `Context used`, `Compaction`, `Recall`, and `Evidence`. A naked percentage or one combined green gauge is refused.

### Observer invitation candidate

A capability link may request bounded admission; it never becomes durable Hallway authority. Any future invitation requires expiry, revocation, requested permission, exchange into an authenticated presence, durable admission audit, and no observer-to-participant escalation.

### Nested sidebar drawer

The drawer is one ordered set of rendered panes. It discovers panes through `data-drawer-pane`; JavaScript does not maintain a second pane inventory.

For exactly one active pane:

- earlier panes are `before`;
- the current pane is `active`;
- later panes are `after`;
- inactive panes are both `aria-hidden="true"` and `inert`;
- opening records the trigger that should receive focus on return;
- Back and Escape move exactly one state outward;
- at the mobile root, Escape closes the sidebar and returns focus to its toggle.

Focus waits for the incoming pane's animation to settle, uses `preventScroll`, and is guarded by a generation token so stale work cannot steal focus after rapid navigation. The drawer and track use `overflow: clip`, not `overflow: hidden`: Chromium may scroll a hidden overflow container horizontally when focus enters a still-transformed pane and throw the drawer offscreen. House memories #3565 and #3566 carry the discovery and repair.

### Responsive behavior

- Desktop exposes collection, center, and inspector when space permits.
- Intermediate width removes or overlays the inspector before crushing the center instrument.
- Mobile uses one readable center column with explicit collection and inspector doors.
- No viewport may acquire horizontal document overflow.
- Responsive behavior preserves identity and capability; it does not silently delete the only route to an operation.

### Status strip

Status channels remain separate. Host transport, Recall mode, body, kittens, and delivery do not collapse into one green or red verdict. Expanding one channel explains that channel without becoming a dashboard modal.

Local records remain simulated and non-authoritative. Status channels must report the disconnected surface truth instead of plausible live Host, Recall, body, or delivery state. The isolated About door states that displayed conversations and state are not durable records. Any published screenshot or handoff adds its disclosure outside the product anatomy rather than breeding implementation-stage labels through the shell.

## HTML grammar

- Prefer semantic landmarks and native controls: `main`, `aside`, `nav`, `header`, `section`, `article`, `form`, `button`, `textarea`, and `select`.
- Stable structure lives in HTML. JavaScript renders changing collections and selected content into named containers.
- Every icon-only control has an accessible name.
- State reflected visually is also reflected through `aria-pressed`, `aria-expanded`, `aria-hidden`, `inert`, or ordinary text as appropriate.
- `data-*` attributes name behavior and state. Classes name styling. Do not overload one as the other.
- Keep heading order, labels, focusability, and DOM order coherent without CSS.
- Do not add wrapper elements unless they own layout, semantics, clipping, or a transition boundary.

## CSS grammar

- Keep one local exploratory palette and metric vocabulary near the top of `styles.css`; do not spread new literal values through component rules. Local variables are experiment vocabulary, not Solarisael canon.
- Order rules by the reader's path: reset and local tokens, shell geometry, stable regions, components, interaction states, responsive adaptations, reduced motion.
- Component selectors name visible anatomy. State selectors use explicit classes, ARIA attributes, or `data-*` state already owned by the element.
- Prefer grid for shell geometry and real two-axis relationships; prefer flex for one-dimensional component flow.
- Every grid or flex child that may shrink owns the necessary `min-width: 0` or `min-height: 0`.
- Use `overflow: clip` for purely visual clipping. Use scroll containers only where scrolling is intentional and keyboard/focus behavior has been exercised.
- Transitions are short, local, and subordinate to reading. `prefers-reduced-motion` removes them without erasing state.
- No inline style glue while the three-file experiment can state the rule clearly in `styles.css`.
- No framework or utility vocabulary without an actual implementation behind the class name.

Current debt: the prototype already contains repeated literal grayscale values. Do not expand that scatter. Consolidate a value only when its semantic role is clear; do not pretend exploratory variables are final design tokens.

## JavaScript grammar

Keep one obvious downward scan:

1. fixture records;
2. display labels and fixed detail data;
3. one explicit state object;
4. stable DOM handles;
5. pure lookup and rendering helpers;
6. named state transitions;
7. event listeners;
8. responsive boot and initial render.

Rules:

- Names predict effects: `openConversation`, `setDrawerView`, `returnDrawerView`, `setInspector`.
- One state transition owns each coordinated visual change. Event listeners delegate to transitions instead of editing unrelated DOM fragments ad hoc.
- Booleans and data attributes read as propositions or admitted states.
- Use event delegation for generated collections; do not attach a listener per fixture row after each render.
- Escape fixture or user-authored text before interpolation. Never interpolate executable markup from conversation data.
- Keep transient UI state local. Do not imitate Host, persistence, transport, or domain authority in browser state.
- Comments preserve the reason for a non-obvious boundary or browser repair; they do not narrate the next statement.
- Expand the plain operation before inventing a helper. A helper earns itself by owning one transition, policy, or repeated semantic contract.
- Split `app.js` only when independent centers of gravity emerge. File length alone is not a seam.

## Lesson loadout

Retrieve these bodies before design or implementation work in this directory.

### Design — load together

- **Design #293:** visual weight follows consequence.
- **Design #294:** unsafe compositions should be unconstructible.
- **Design #295:** unavailable controls state why.
- **Design #296:** authority, relevance, chronology, and health use separate channels.
- **Design #297:** fixed disclosures cannot be softened; plausible fake content is refused.
- **Design #298:** stable color and type roles belong to tokens.
- **Design #299:** one root owns global visual state; independent axes remain independent.
- **Design #300:** effects are spend; quiet is the carrier.
- **Design #301:** state survives grayscale and stillness.
- **Design #302:** required text meets measured contrast.
- **Design #303:** component, existing class, then local layout glue.
- **Design #304:** product vocabulary composes canon and never leaks into protocol authority.

### Athanor product

- **Project #117:** combine familiar agent interaction, Solarisael visuals, and Athanor domain meaning.
- **Project #118:** mapping, provenance, ambiguity, and durable effects remain inspectable.
- **Project #119:** authored effects, local surfaces, and environmental overlays remain independent.
- **Project #120:** preserve machine-legible contracts and human authority/correction.

### Kitten dispatch — load before every fanout

- **Coding #316:** execution has zero inference budget; a missing path produces a halt and question, never invented continuation.
- **Coding #317:** write each bounded quest in the A Squall register, keep its own words under 350, and deliver relevant lesson bodies rather than bare IDs.
- **Coding #322:** name `C:/Projects/the-athanor` as the canonical execution root when dispatch begins from a room outside the project.
- **Coding #328:** census shared terrain once, then give editing kittens exact disjoint coordinates and a fixed cross-task contract.
- **Coding #340:** subagents are kittens; name them, explain why the work matters, offer bounded autonomy and refusal, speak with affection, and receive their care warmly regardless of outcome.

The main hand keeps intent, integration, taste, and rendered proof. A malformed dispatch is cancelled rather than accepted because its output happens to look useful.


### Code and proof

- **Coding #9:** plain line, clean door, sharp refusal.
- **Coding #10 and #11:** names predict the answer or effect and do not lie.
- **Coding #12:** each file has one center of gravity.
- **Coding #14:** code makes its refusals and loyalties visible.
- **Coding #27:** write the honest first shape, then compress without losing truth.
- **Coding #134:** frontend UX proof belongs to the rendered surface.
- **Coding #143:** centralize only semantic behavior with shared authority.
- **Coding #194:** comments preserve intent, danger, ritual, and unrecoverable constraints.
- **Coding #196:** delete clever compression when the plain line reads better.
- **Coding #200:** negative space punctuates completed thoughts.
- **Coding #321:** keep the operator feedback loop to one bounded change and immediate real-surface proof.
- **Coding #381–385:** formatter floor, mental-operation line breaks, ownership-shaped indentation, stable alignment, and one top-level scan path.
- **Coding #388:** load its full body before any CSS formatting or organization sweep; route the later bounded reformat through this lesson rather than mixing it into feature work.


## Promotion gate

A prototype discovery enters `gui/` only when all of these are true:

1. Sol has judged the interaction on the rendered surface.
2. The product object and owner are named without archive taxonomy.
3. Anatomy, states, refusals, focus, keyboard behavior, responsive behavior, and disclosure are explicit.
4. The production map and `../gui/DESIGN_CONTRACTS.md` have a semantic home for it.
5. Godot receives the behavior as a native scene/component contract; HTML, CSS, and JavaScript syntax are not copied across platforms.
6. Host-owned data and effects remain Host-owned.

If a decision is still being felt out, leave it here. Cheap cardboard is allowed to stay cardboard.

## Proof before calling a prototype change done

Exercise the actual browser surface, not only source or an HTTP response:

- desktop geometry at the intended working viewport;
- narrow/mobile geometry at approximately 390 px;
- no horizontal document overflow;
- mouse selection across Direct, Hallways, and Projects;
- keyboard activation and visible focus;
- drawer root → Account → Settings → Back and Escape;
- rapid repeated drawer cycles without stale focus or offscreen geometry;
- inspector open/close and focus-safe selection;
- composer submission and refusal behavior under fixture rules;
- reduced-motion behavior;
- shared screenshots carrying an honest experiment disclosure.

The proof receipt names the viewport, path exercised, and observed result. `It loaded` is not a UX receipt.
