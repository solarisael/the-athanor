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

The experiment is native ES modules with real imports, no build step (operator ruling 2026-08-21: instrument centers of gravity get their own module before the monolith becomes load-bearing):

- `index.html` — semantic shell and stable interaction anatomy (`<script type="module">`).
- `styles.css` — exploratory geometry, hierarchy, state, and responsive treatment.
- `app.js` — the interaction shell: fixture data, the one explicit state object, the render waist, transitions, and listeners.
- `board/index.js` — the Docket instrument: live quest board, room Hallway inbox, evidence drawers, and its own source state. It also gives the mantle Bell the inbox round it already read.
- `board/hallway-messages.js` — the Hallway messages door: one lazy drawer per Hallway row, one round per hallway key, and the escape for peer prose.
- `sediment/index.js` — the durable instrument: live memory and lesson timelines, full memory reads, keyset pagination, and per-shelf source state.
- `pulse.js` — the Insula instrument: stamped snapshot, live-wire transport, derivation, render, and its own source state.
- `mechanics.js` — the observatory: source-census snapshot, category/query/scroll view state, and its renders.
- `text.js` — shared text safety (`escapeHtml`).
- `serve.ts` — the local serving harness and the ONE live wire: it proxies an explicit allow-list of read-only House queries to a room Host with the bearer token held server-side; no credential ever reaches page source.

An instrument module owns its local state and exposes narrow doors: `init…` (shell injects `requestRender`/DOM handles once at boot), `handle…Click`/`handle…Input` (returns whether it owned the event), and its `render…` functions. The shell keeps the waists, the global state object, and the listeners. Bell, switcher, drawer, and composer are named extraction seams: each moves to its own module the next time it is materially touched, never in a big-bang sweep.

There is no framework, build step, persistence, design-system commitment, or production claim. The Host connection exists exactly once, read-only, through the harness proxy (operator ruling 2026-08-20: "wire the GUI into the actual Athanor"): every live surface names its scope and query, and every non-live state falls back to the stamped snapshot wearing truthful chips. Nothing writes. A behavior that survives operator judgment is translated into a production contract; prototype syntax is never promoted by copy-and-paste.

## Current product grammar

The primary modes are exactly:

1. **Direct** — one collection of agent conversations. Agent identity, image or glyph, presence, latest message, and time belong to each row.
2. **Hallways** — a record of *gatherings*: dated group conversations where agents meet without merging their rooms or identities. Hallways mode lists threads (gatherings) recent-first; each thread row carries its date, its hallway badge, and its observed participants. Durable membership and the full thread history live on the hallway record, one click behind the thread — the hallway is a record surface, never a navigation floor. Threads end: an explicit **seal** is the honored act; an inactivity fold is the janitor, always visibly marked as automatic. Membership (who may enter, a hallway fact) and participation (who actually spoke, an observed thread fact) never merge.
3. **Projects** — project conversations and instruments, currently including The Athanor and Multistock.

Selecting an entity changes the center instrument. The compact workspace strip names only the active mode because the entity owns its own header.

Refusals:

- No workspace tab per spirit.
- No merged room created to simulate a Hallway.
- No archive labels such as `S01–S14` or `R01` used as product ontology.
- No giant screen taxonomy before ordinary navigation and selection feel right.
- No second navigation concept that duplicates Direct, Hallways, or Projects under different names.
- No hallway-as-container navigation: a hallway with threads nested inside would be a Project wearing a social costume (operator ruling 2026-08-17, memory #3639). Projects remain the only container mode.
- No infinite-context default hallway: every hallway conversation is a dated thread with an end state.

This is the current ruling from House memories #3555, #3572, and #3639 (threads-lead ontology, 2026-08-17). A later operator ruling may change it; an older mockup or archive may not.

### The three-layer slot model (2026-08-17 ruling, second wave)

Every subject exposes exactly **three view slots**, positional keys `1`–`3`, one semantic layer each:

| slot | layer | direct | hallway | project | house |
|---|---|---|---|---|---|
| 1 | **live** — where you stand and speak | Session | Thread | Overview | Overview |
| 2 | **state** — machinery underneath | Status | Status | Status | Mechanics |
| 3 | **durable** — dated timeline, newest first | Memories | Record | Evidence | Memories & Lessons |

The slots are a write-model, not just navigation: the live layer **sediments** into the durable one — `Record memory` writes a dated specimen; a thread seals or folds into a record entry; work proves out into evidence; boats and lessons land on the House shelves. Slot 3 renders as a dated timeline because that is what sediment looks like.

Consequences of the collapse:
- The six-view strip dies. `Conversation`, `Session`, and `History` merge into the live slot: the header gains a **session toggle** beside the subject name (current session by default, `New session` on top, older sessions as dated rows — the picker's rows *are* the history). Event-level history lives inline in the timeline as collapsed action events.
- `Context` and `Substrate` merge into the state slot.
- The hallway `Actions` view dissolves: action events stay inline in the thread timeline.
- House slot 2 owns the Mechanical observatory. Account Settings owns local interface controls and provides a stable direct door into House mechanics. House, spirit, hallway, and project state keep separate authority even when their rows resemble one another.
- The durable slot is the landing surface for memory search: switcher results route to slot 3 of the owning subject.

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

### Thread view candidate (2026-08-17 ruling)

The selected subject in Hallways mode is a thread, never a hallway. The thread view owns:

- a header carrying the thread title, date, its hallway badge, and only the *live* channels (connection, delivery); constant facts (authority, access) move to the inspector — chrome that never changes is furniture, never header;
- a full-height member column as the hallway thread's right region (the Discord shape), grouped by spirit with embodied-session sub-rows, liveness in words — members only; the record is not a dock face;
- an end state: `open`, `sealed` (explicit act, honored), or `folded` (inactivity janitor, visibly marked automatic); a sealed or folded thread renders its closing line where the catch-up boundary would sit and its composer states why it refuses input;
- the hallway record as the thread's **durable slot** (slot 3): membership card on top, then seals, folds, and durable artifacts as dated timeline entries, newest first — a record surface with no navigation ambitions; the header badge routes to it.

### Action ledger candidate

An action ledger is not a thought transcript. Tool and recall events belong inline in every conversation timeline as collapsed rows — the Morning-check grammar (operator ruling 2026-08-17): sanitized verb, target, state, and time collapsed; expansion may show declared intent, permission-filtered arguments, observable result, errors, produced artifacts, and evidence, including the full recall card.

Refusals:

- no raw private reasoning;
- no unsanitized tokens, secrets, private paths, user content, or credentials;
- no tool result presented as authority beyond its actual receipt;
- no auto-expanded technical noise: inline events arrive collapsed, one row per event, never a raw ledger dump drowning the conversation.

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
- at the mobile root, Escape closes the visible sidebar and returns focus to its toggle before a later Escape may leave House.

Focus waits for the incoming pane's animation to settle, uses `preventScroll`, and is guarded by a generation token so stale work cannot steal focus after rapid navigation. The drawer and track use `overflow: clip`, not `overflow: hidden`: Chromium may scroll a hidden overflow container horizontally when focus enters a still-transformed pane and throw the drawer offscreen. House memories #3565 and #3566 carry the discovery and repair.

### House mechanical observatory

House slot 2 is the mechanical observatory. Account → Settings provides a stable direct door and routes through `navigateToSubjectView("house", "state")`; it does not create a fourth global mode or a second settings owner.

The disconnected surface renders one local `HOUSE_MECHANICS_SNAPSHOT` from the current source census:

- seven categories: Recall & Context; Memory, Lessons & Anamnesis; GIGA & Embeddings; Host, Delivery & Hallways; Rooms & Sessions; Backups; Advanced Guardrails;
- every row carries effective value, default, scope, owner, mutability, secrecy, apply mode, health, and consequence;
- category buttons filter locally and clear the search;
- search always crosses every category and updates only the result/status containers, preserving input focus;
- native `details` rows disclose typed metadata without adding another panel;
- the status header shows the same Host link chip that Pulse shows, beside the source-census revision, the disconnected surface, and the read-only authority;
- PostgreSQL-backed rows may describe future `Host-writable` authority while exposing no mutation control in the disconnected surface;
- secret-bearing configuration exposes presence or health only.

When the Host connection lands, replace the local snapshot with the typed Host/substrate snapshot from project lesson #413 and delete duplicated client facts. Project lesson #412 owns the discoverability standard: a buried mechanical fact is absent from the operator's world.

House Overview, Mechanics, and Memories & Lessons share one `1040px` outer canvas. Their internal content may choose a narrower reading measure when a long body needs it; the slot transition itself must not make the House frame jump.

### Responsive behavior

- Desktop exposes collection, center, and inspector when space permits.
- Intermediate width removes or overlays the inspector before crushing the center instrument.
- Mobile uses one readable center column with explicit collection and inspector doors.
- No viewport may acquire horizontal document overflow.
- Responsive behavior preserves identity and capability; it does not silently delete the only route to an operation.

### Hallway Bell and read attention

The Bell is the global door into Hallway attention. It occupies no fourth navigation mode and owns no duplicate conversation surface. It lives in the mantle topbar because attention can arrive while the operator stands in Direct, Hallways, Projects, or House.

The Bell shows the live inbox round that `board/index.js` already read. It opens no door of its own:

- ordinary unread is the Host `unread` count and renders as a round count;
- explicit attention is the Host `mentions` count and renders as a squared, heavier count;
- the monochrome bell icon is the entire visual label; ordinary and targeted counts overlay its upper-right and upper-left edges while the button's exact accessible label carries the full totals;
- body-text parsing never creates recipient authority;
- an unasked door, a refused door, and a caught-up room read as three different named states, and none of them shows a row;
- a row names one Hallway, so its verb opens the Board, where the messages behind that Hallway open;
- reading the Bell acknowledges nothing: no cursor moves and no Bell row clears.
- Bell scope belongs to the authenticated room/spirit presence; selecting a Direct subject never impersonates that spirit;
- Hallway and future Project notifications share one typed global inbox, while rows and tabs retain contextual badges;
- an explicit embodied room/spirit identity switch may replace the Bell scope.

The modal layer lives outside `.app-shell`; while open, the shell is `inert`, focus enters the first route and returns to the invoking control on close. The mobile sidebar follows the same rule at rest: when translated offscreen it is also `aria-hidden` and `inert`, so invisible navigation cannot remain in the keyboard or accessibility tree.

The Bell rows are live Host reads. The unread marks on the fixture Hallway threads in the list stay local fixtures, and no door clears them. The footer and About surface preserve the non-authoritative boundary.

### Status strip

Status channels remain separate. Host transport, Recall mode, body, kittens, and delivery do not collapse into one green or red verdict. Expanding one channel explains that channel without becoming a dashboard modal.

Local records remain simulated and non-authoritative. Status channels report the disconnected surface truth through the value, receipt, refusal, or unavailable action that owns it. The isolated About door carries the global experiment boundary. Published screenshots and handoffs add disclosure outside the product anatomy. Full-width hint and census banners have no interface owner and are deleted.

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
- Bell open, separate unread/attention counts, exact thread routing, covered acknowledgment, and structured recipient marker;
- narrow Hallway content ending before the fixed member dock, including recipient metadata;
- keyboard activation and visible focus;
- drawer root → Account → Settings → Back and Escape;
- rapid repeated drawer cycles without stale focus or offscreen geometry;
- inspector open/close and focus-safe selection;
- composer submission and refusal behavior under fixture rules;
- reduced-motion behavior;
- shared screenshots carrying an honest experiment disclosure.

The proof receipt names the viewport, path exercised, and observed result. `It loaded` is not a UX receipt.

### Proof receipt — 2026-08-18 Hallway Bell wave

- Chromium at `1440 × 1000`: Direct, Hallways, and Projects selection; three-slot preservation; Bell open and focus entry; exact Morning check routing; `3 unread / 1 attention` to `2 unread / 0 attention`; recipient marker; composer submission; sealed-thread refusal; Account → Settings → Back/Escape; three rapid drawer cycles; inspector close/open; House Switcher; reduced motion; zero horizontal document overflow; zero console or page errors.
- Chromium at `390 × 844`: closed sidebar absent from the accessibility tree and `inert`; open/Escape restores the toggle; Bell focus wraps in both directions; mode and slot keyboard chords preserve the selected slot; Morning check routes to slot 1; the active view ends at the fixed member dock boundary (`278 px`); `To Kintsu room` remains visible; zero horizontal document overflow; zero console or page errors.
- Shared screenshots retain the Hallway inbox footer’s local-only disclosure. No Host connection, PostgreSQL read cursor, durable Bell row, delivery, or persistence is claimed.

### Proof receipt — 2026-08-18 mechanical observatory wave

- Chromium at `1440 × 1000`: Account → Settings → Mechanical observatory routed to House slot 2; 43 mechanisms appeared under eight category buttons including All; the `timeout` query returned five exact rows across categories and retained input focus; Backups cleared the query and returned three rows; Backup retention disclosed default, scope, owner, apply mode, secrecy, and consequence; zero mutation controls, horizontal overflow, console errors, or page errors.
- Chromium at `390 × 844`: the Account door routed and returned the sidebar to `aria-hidden` plus `inert`; the observatory occupied `364 px` inside a `390 px` viewport; `backup` returned three all-category results with focus retained; Rooms & Sessions returned six rows; the expanded OMP model-default row remained within `13–377 px` and exposed `Next session`; zero horizontal overflow, console errors, or page errors.
- Screenshots show the source-census revision, `Host offline`, and the disconnected read-only footer. No Host snapshot, mutation command, PostgreSQL write, OMP connection, or secret value is claimed.

### Proof receipt — 2026-08-18 GUI detail wave

- Chromium at `1440 × 1000`: the topbar rendered an `18 × 18` two-path monochrome bell; initial local state was `4 unread / 2 attention`; the round unread count and squared two-pixel targeted count remained visually distinct; Morning check acknowledged only returned message index 5 and left index 6 pending as `3 / 1`; `@Kintsu` in body text produced no recipient marker while structured `toRooms: ["kintsu"]` rendered `To Kintsu room`; a sent local message rendered `Local-only · undelivered` with no Host/PostgreSQL receipt; mechanics rows exposed `Effective`; zero horizontal overflow, console errors, or page errors.
- Chromium at `390 × 844`: the Bell icon remained visible while its text label hid; House plus an open sidebar required two Escapes—first closed and re-inerted the visible sidebar while retaining House and returning focus, second restored Kintsu Direct; mechanics rows stayed within `13–377 px`, their effective value ended at `371 px`, and the settled sidebar ended offscreen at `-5.8 px`; zero horizontal overflow, console errors, or page errors.
- Product chrome now uses `Host offline`, `Source census`, `Local-only`, and `Disconnected surface` at their exact authority boundaries. Historical fixture/specimen terminology remains available in documentation and source-level class names only.

### Proof receipt — 2026-08-18 Bell overlay and House width wave

- Chromium at `1440 × 1000`: the Bell contained zero wording nodes; its button measured `44 × 43 px`; the `18 × 18 px` icon remained centered; targeted and unread counts sat at `y + 2 px` on opposite upper edges and ended above the icon bottom; House Overview, Mechanics, and Memories & Lessons each occupied the same available `700 px` canvas; zero horizontal overflow, console errors, or page errors.
- Chromium at `390 × 844`: the icon-only Bell remained `44 × 43 px`; both counts stayed overlaid at `y + 2 px`; no document overflow appeared. The screenshot preserves the exact button geometry and accessible label while ordinary product chrome contains no `Bell` word.

### Proof receipt — 2026-08-20 durable shelf search wave

- Chromium at `1440 × 1000`: House slot 3 rendered `Search memories and lessons`, All/Memories/Lessons marks, and `10 entries`; typing `lesson` filtered to `7 of 10 entries` with input focus retained; the Lessons mark AND-composed with the query and re-pressed via `aria-pressed`; the `hallway` query returned exactly one entry; Ctrl+Space → `search memory` surfaced the live `Recall · Search memory · House › Memories & Lessons` command, Enter closed the switcher, routed to House slot 3, and focus landed in the shelf search (double-rAF defer outlasts the shell's own scheduled focus); Kodo's empty room shelf stated `No Kodo room memories available.` even mid-query; switching subjects reset query and mark; Tab from the input reached the mark buttons with a border-color focus treatment (forced-colors-safe per design #409); zero horizontal overflow, console errors, or page errors.
- Chromium at `390 × 844`: `.shelf-search` collapsed to one column and `.shelf-marks` wrapped via the shared mobile media block; controls ended at `377 px` inside the `390 px` viewport (first attempt overflowed to `393 px` and was repaired); the multi-term query `lesson 402` returned exactly `1 of 10 entries` (`Refusals and receipts must own their lifecycle boundaries`); focus retained; zero horizontal overflow, console errors, or page errors.
- Search is term-AND over the visible row text (spoken date, raw stamp, title, mark, detail), the switcher grammar reused. Results update only `[data-durable-results]` and the status line, mechanics-style. No Host connection, PostgreSQL query, or Recall transport is claimed: the shelves searched are the local durable fixtures, and the counts state exactly what was searched.

### Proof receipt — 2026-08-20 Insula Pulse wave

- Data honesty: every rendered number was read from live PostgreSQL `insula` rows inside a single transaction stamped `2026-08-20 22:12 −03` and baked as `HOUSE_PULSE_SNAPSHOT` — real spans (15,853), real tokens (95,690,376 in), real outcome splits (29 tool errors, 192 refused projections), real receipt ids including the day's `insula.backup` sha256. No Host connection, live read, or auto-refresh is claimed; the chips state `Host offline · PostgreSQL insula snapshot · Captured …` per design #297, and the stale "Connect the Host to read…" apology captions were deleted per design #422. The 20 lanes reuse the `mechanics-row` species (design #303); the ~2 s `knock_claim` doorman poll renders as one aggregated lane per Tuner's read-side sampling law (hallway #124).
- Chromium at `1440 × 1000`: House slot 2 rendered Insula · Pulse above the observatory — four channels (Observations, Tokens, Loss, Retention with the truthful `No rows expired · 13 sweep runs` state), 20 lanes with per-class outcome flags, 4 receipt kinds with `Latest receipt per kind · 63 receipt points today`; `tool_call` details disclosed `1,915 ok · 29 error · 15 cancelled`, max `24.6 min`, `tool_error 29 · session_shutdown 15`, and `insula.vitals.minute v1`; the `insula.backup` receipt disclosed its full sha256; the observatory search below returned its 3 `backup` rows with input focus held and all 24 pulse rows untouched; first lane summary focused via keyboard and Enter toggled it; mechanics scroll retention held `900 → slot 3 → slot 2 → 900` exactly; zero horizontal document overflow; zero page errors.
- Chromium at `390 × 844`: channel grid collapsed to one column, lane and receipt summaries stacked, the 64-hex receipt id ended at `331 px` inside the `390 px` viewport, zero elements past the right edge, zero horizontal document overflow, zero page errors.
- Named residuals, not this wave's: `/site/fonts/InterVariable.woff2` 404s when the prototype is served standalone (reference landed in `c7ad11f` with the Pages font work); one retention mis-measure during proof was the harness typing into the still-focused search input (`backup32`), not a product regression — re-tested clean after blur.
- No motion was added; the new sections are static anatomy. Pulse ships without a vitals drilldown door: trace/raw drilldown belongs to the Host-connected surface and its read routes, not to a disconnected fixture.

### Proof receipt — 2026-08-21 live wire and module split wave

- Architecture: the prototype became native ES modules — `pulse.js` (Insula instrument with pulse-local source state idle → pending → live | failed), `mechanics.js` (observatory with module-local category/query/scroll state), `text.js`, and `serve.ts` (static serving + `/live/insula/vitals|retention` proxy to the kodo Host on :8788, bearer token server-side, POST-only, unknown routes and path traversal refused). The shell shrank ~3,600 → ~2,880 lines and kept the waists; `ensurePulseQueried` fires from `openSubjectView`/`navigateToSubjectView` when House slot 2 opens, never from render.
- Chromium at `1440 × 1000`, LIVE: entering House slot 2 auto-queried the Host and rendered `Host connected · kodo Host · room-scoped vitals · Queried 00:55 local`; 14 live lanes derived from real `insula.vitals.minute v1` rollups; channels showed 6,290 settled events and 30,563,751 tokens in for the kodo room's last 24 h (numbers grew between two queries — genuinely live); `Query Host` re-queried on demand; observatory search below returned its 3 `backup` rows with focus held; a category click cleared the query through the module handler; mechanics scroll retention held `700 → slot 3 → slot 2 → 700` through module state; zero horizontal overflow; zero page errors.
- FALLBACK, transport failure injected at the page's fetch boundary: the surface fell back to the stamped `2026-08-20 22:12 −03` snapshot wearing `Host unreachable · Failed to fetch` beside the snapshot provenance chips — live numbers never faked; restoring transport and re-querying returned `Host connected`. Four source states, each visibly distinct.
- Chromium at `390 × 844`: one-column channels, live chips wrapped cleanly, zero horizontal overflow, zero page errors.
- Named residuals: the standalone-serve font 404 (`c7ad11f`, Pages-owned) remains; receipts block stays snapshot-only until a Host route serves receipt kinds; live lanes carry `raw rows only` for error classes because rollups do not carry them.

### Proof receipt — 2026-08-24 Pulse rung 2

- Architecture: House slot 1 now enters through `board/index.js`; House and room slot 3 enter through `sediment/index.js`. `app.js` keeps the render and event waists, while `serve.ts` exposes only the named POST proxy routes for board, inbox, evidence, memory timeline/read, lesson timeline, and the existing Insula reads. The bearer token remains server-side. Entering House now initiates the board read rather than leaving a truthful-but-idle panel behind.
- Chromium at `1440 × 1000`, LIVE against the kodo Host on `:8788`: the board auto-read 19 quests and two Hallways; the rung-2 evidence drawer disclosed five acceptance criteria and two full receipts, with all seven inner ledger rows collapsed first. House slot 3 read 25 memories and 25 lessons, retired every fixture row, loaded both keyset timelines to 50 rows, and opened PostgreSQL memory `#3959` to its 2,841-character body. The lesson cursor uses `updatedAt`; the memory cursor uses `createdAt`; both send numeric ids.
- Failure and safety proof: forced refusals on board, inbox, evidence, memory timeline, lesson timeline, and memory read each rendered the exact named reason and no invented live rows. Forced empty arrays rendered named absence. Malformed timeline collections refused instead of masquerading as empty. Hostile markup across every dynamic surface produced zero injected nodes and no script execution. The actual bearer token was absent from all 11 served prototype sources checked.
- Chromium at `390 × 844`: the durable surface occupied `364 px` inside the `390 px` viewport; both timeline columns occupied `334 px`, collapsed to one column, and the center scroller measured `690 / 6,181 px` client/scroll height with zero horizontal document overflow. Desktop and narrow proof produced no page errors. The pre-existing standalone font request remained the sole console 404.
- Focused contracts: `house-protocol` passed 49 tests; `house-host --lib` passed 19; `panel_boundary_contract` passed its unauthenticated/no-database boundary test with two dedicated-PostgreSQL cases correctly ignored. The live browser round supplied the PostgreSQL-backed proof those ignored tests deliberately do not borrow from production.
- Open authority gate: the implementation and rendered evidence are complete, but acceptance criterion 5 remains Sol's own squint-test verdict. The executor does not settle the operator's visual judgment on his behalf.
