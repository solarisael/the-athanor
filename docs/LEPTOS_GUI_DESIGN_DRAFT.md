# Athanor GUI — Leptos design draft

Status: proposed design for Sol's review. This document does not authorize implementation.

## 1. Decision boundary

The GUI uses Rust and Leptos. Components share contracts, visual rules, and interaction behavior.
Screens compose components. Host services retain domain authority. PostgreSQL retains durable authority.

This document defines the proposed screen set. Sol must approve its scope and unresolved decisions before builder dispatch.
Names below identify design contracts. They do not assert that corresponding Host endpoints or Rust types exist.

### Settled direction

- Build a self-contained GUI architecture in Rust and Leptos.
- Define every screen, component, and composition before assigning implementation.
- Reuse components where behavior and ownership share one contract.
- Give builders approved specifications, applicable lessons, and typed service contracts.
- Keep platform integration separate from shared UI components.
- Restore the last valid destination on startup. Use the House overview when no valid destination can be restored.
- Replace the center view when navigating. Keep ongoing conversations accessible from the left collection.
- Support multiple independent conversations with the same spirit, including Houses with one spirit.
- Show only active ongoing conversations in the left conversation list.
- Keep default chrome quiet. Inspectors, session activity, and diagnostics open on request.
- A successfully stored paper boat marks its associated conversation as finished.
- During use, several conversations can remain open. On application reopen, restore only the latest conversation if it has no boat.

### Proposed choices

- Use client-rendered WASM for the Windows application.
- Use a persistent application shell with routed center views.
- Use Direct, Hallways, and Projects as primary collections.
- Give House-wide records and mechanics a separate global entry.
- Keep the Android application as a future client target, subject to its own connection contract.
- Keep draft text in application memory during navigation. Restart persistence requires a separate privacy decision.

### Exclusions from this design

This document does not define backend implementation, endpoint names, database migrations, deployment commands, or Android authentication.
It does not claim that all proposed actions have backend support.
Unsupported actions require a specific unavailable reason. Never simulate a receipt or a successful write.

## 2. Product objects

| Object | Identity and meaning |
|---|---|
| House | The shared domain containing rooms, records, policies, and services. |
| Room | A scope for conversation, access, and room-owned records. |
| Spirit | A named participant. A spirit name is not a session identifier. |
| Operator | The human principal. Selecting a room never changes the operator's identity. |
| Session | One identified harness instance and its conversation binding. |
| Conversation | An independently identified dialogue associated with a room and spirit. Several conversations may share that association. |
| Hallway | A shared gathering context with explicit membership. |
| Hallway thread | One dated conversation with an open, sealed, or automatically folded state. |
| Project | A work container. A work item belongs to the scope reported by its authority. |
| Record | A typed durable entity with provenance and revision semantics. |
| Proposal | A change awaiting authority. It is never displayed as settled fact. |
| Receipt | An observed outcome for one operation. Delivery and durable storage have different receipts. |

Use stable identifiers in routes, cache keys, selection, and commands. Display labels never substitute for identifiers.
Only show relationships that the Host reports. Do not infer project membership or session identity from nearby content.

## 3. Ownership and module boundaries

| Layer | Owns | Must not own |
|---|---|---|
| Visual system | Tokens, typography, surfaces, focus, and motion rules | House identity, wire schemas, or permissions |
| Interaction components | Layout, controls, overlays, and local interaction | Service credentials or durable authority |
| House components | Typed presentation of records, identities, receipts, and observations | Backend business rules |
| Feature modules | Screen composition, requests, and feature state | Another feature's private state |
| Client boundary | Typed requests, subscriptions, cancellation, and response association | Rendering or implicit retries of writes |
| Platform boundary | Secure connection setup, native facilities, and application lifecycle | Screen-specific rendering |
| Host | Authorization, commands, queries, and domain outcomes | Browser focus or local layout |

Each concern has a module with one public entry. Do not create a crate merely to split a file.
Share protocol types only when their dependencies and authority fit both compilation targets.
Keep secrets and operating-system paths outside WASM and component props.

## 4. Screen grammar

### Shell geometry

The wide layout has four regions:

1. A collection sidebar with primary navigation and subject selection.
2. A center instrument with its subject header and routed content.
3. An inspector opened explicitly for the selected object or item; closed by default.
4. Minimal status chrome. Expanded service observations appear on request.

The center owns the main reading and working area. Its scroll container does not move global navigation.
The inspector provides context. It does not repeat the full center view.
The default conversation surface emphasizes messages and the composer.
Do not reserve an empty inspector column or show persistent diagnostic panels.
Sol's current terminal is the requested density reference. Exact visual measurements remain unverified.

At constrained widths, the inspector becomes an explicit overlay before the center becomes unreadable.
On phones, show one center plane. Collection and inspector controls open accessible overlays.
Place the composer above the keyboard and safe area. Keep focused inputs visible.
Do not introduce horizontal document scrolling.

### Navigation

Primary collections select subjects. Subject views select the instrument for that subject.
Use semantic labels specific to each subject. Avoid an additional navigation taxonomy for every service.

The Direct collection lists ongoing conversation identities, rather than one fixed entry per spirit.
Provide a New conversation action and permit repeated creation for the same spirit.
Each conversation owns its history, draft, read position, and delivery association.
Switching conversations does not end them. Leaving the center for another instrument does not remove ongoing conversations from the list.
A successfully stored paper boat closes its associated conversation and removes it from the ongoing list.
A failed boat write does not close the conversation. Boat text in a message is not a stored boat receipt.
Inactivity, application close, and harness exit do not independently mark a conversation finished.
The boat receipt must identify its conversation. A room-wide latest-boat lookup cannot distinguish simultaneous conversations.
The spirit window provides access to previous conversations, including finished ones.
Viewing a finished conversation does not reopen it or remove its boat.
Q10 governs receipt association, creation effects, and any explicit continuation of finished conversations.
Single-spirit Houses use the same conversation model without requiring artificial additional identities.

Navigation replaces the center view. Split views and workspace tabs are outside the approved interaction direction.
On startup, restore the last valid destination for the authenticated principal and House.
If no valid destination can be restored, open S10.
When restoring conversations, reopen only the latest conversation if it has no stored boat and remains accessible.
Do not repopulate every previously open conversation or scan backward to open an older one automatically.
If that latest conversation has a boat, use S10. An absent sidebar entry does not delete conversation history.
Temporary connection failure must not erase the saved destination; show connection recovery before resolving it.

Proposed route identity:

`destination + subject identity + view + optional selected record identity`

The final URL grammar is an approval gate. Never put drafts, credentials, or sensitive query text in URLs.
Back and Forward restore a valid destination. A deleted or inaccessible destination shows a recoverable state.

A route change updates selection, header, content, and inspector context together.
An inspector selection clears when its owning subject changes.
A deliberate screen change focuses the destination heading. Background updates never steal focus.

### Transition behavior

Keep the shell stationary during center-view changes.
Use a short local crossfade for deliberate navigation. Default proposed duration: 140 milliseconds.
Use directional movement only for nested overlay navigation.
Reduced motion produces an immediate, complete state change.
Streaming updates and query refreshes do not animate the entire screen.
Rapid navigation must settle on the latest destination without delayed focus from an abandoned transition.

## 5. Shared state contracts

These are conceptual Rust shapes. The implementation must bind them to verified protocol types.

| State contract | Variants and required facts |
|---|---|
| QueryState<T> | Idle; Loading(request identity); Ready(value, observation time); Refreshing(previous value, request identity); Failed(reason, optional previous value) |
| Availability | Available; Unavailable(reason) |
| CommandState<R> | Editing; Reviewing(target, consequence, revision); Submitting(request identity); Succeeded(receipt); Rejected(reason); Uncertain(request identity, reason) |
| ConnectionState | Connecting; Connected; Reconnecting; Offline(reason); Closed |
| ThreadState | Open; Sealed(receipt); Folded(observed reason and time) |
| RecordSubject | A tagged identity for the exact record family and scope |
| OverlayState | Closed; Open(kind, entry focus, navigation stack) |

A successful empty query is Ready with an empty collection. Render its empty state without treating it as failure.
Only use Refreshing when retaining prior content is safe for that scope.
Label retained content with its observation time. Never carry it across a different room or principal.

A command timeout does not prove rejection. Use Uncertain until a query or receipt resolves its outcome.
Do not automatically repeat consequential commands. Use the Host's reconciliation and idempotency contract.

### Lifetimes

| Owner | State | Release or reset condition |
|---|---|---|
| Application | Reader preferences, local draft collection, and navigation history | Application close, explicit preference change, or approved clearing action |
| Navigation restoration | Persisted last destination, scoped by principal and House | Replace after valid navigation; validate access and existence during startup |
| Authenticated connection | Principal, capabilities, and connection status | Sign-out, credential replacement, or connection retirement |
| Room binding | Room-scoped subscriptions and request generation | Room change or access loss |
| Conversation | Draft text and read position, keyed by principal, room, and conversation | Explicit clear, acknowledged submission rules, or approved retention policy |
| Screen | Query criteria, selected result, and scroll position | Route lifetime or an explicit saved-view policy |
| Overlay | Temporary choices and return-focus anchor | Close or completion |
| Command | Target, revision, request identity, and outcome | Observed terminal receipt or explicit reconciliation |

Room change cancels cancellable reads and retires subscriptions.
Every response must match its request identity and owning scope before it can update visible state.
A draft cannot move to a different conversation because the display label matches.
The same rule applies when two conversations belong to the same room and spirit.
Component unmount must release its listeners. Shared subscriptions belong to a longer-lived named owner.

## 6. Component catalogue

Every component inherits the rules in sections 4, 5, and 9.
Props below describe required information. Events describe intent; feature owners execute authorized effects.

### Structural components

| ID | Component | Inputs | Events and owned state |
|---|---|---|---|
| C01 | AppShell | Region content and responsive mode | Layout only; emits collection and inspector requests |
| C02 | NavigationRail | Destinations, selected destination, and availability | SelectDestination; owns no domain data |
| C03 | SubjectList | Typed subject rows, selection, query state, and admitted creation action | SelectSubject, RequestCreate, RequestNextPage; Direct rows identify ongoing conversations |
| C04 | SubjectHeader | Subject identity, view choices, and selected view | SelectView, OpenContext |
| C05 | Inspector | Typed context and visibility | Close, FollowReference; owns local disclosure state |
| C06 | StatusStrip | Independent service observations | OpenStatus(channel); never derives one aggregate health claim |
| C07 | ScreenFrame | Heading, actions, and body | Owns heading focus target and scroll container |
| C08 | Surface | Semantic surface role and content | No state or domain effects |

### Interaction components

| ID | Component | Inputs | Events and owned state |
|---|---|---|---|
| C09 | ActionControl | Label, semantic action kind, availability, and pending state | Activate; unavailable reason remains visible |
| C10 | QueryControls | Typed criteria, field definitions, and availability | SubmitCriteria, ResetCriteria; owns editable criteria before submission |
| C11 | ResultList | Stable row keys, renderable rows, selection, and next-page availability | Select, RequestNextPage; no hidden fetching |
| C12 | QueryBoundary | QueryState and content renderer | Retry only when supplied; renders loading, empty, stale, and failure states |
| C13 | OverlayFrame | Title, return-focus anchor, and nested panes | Back, Close; owns focus containment and overlay stack |
| C14 | CommandReview | Exact target, consequence, revision, and CommandState | Confirm, Cancel, Reconcile; generic children cannot replace consequence or receipt |
| C15 | ReceiptView | Tagged receipt and inspectable evidence | OpenEvidence; distinguishes delivery, write, and command outcomes |
| C16 | FieldControl | Typed value, label, validation, and availability | Change; no independent durable writes |
| C17 | Disclosure | Label, summary, and body | Toggle; body visibility only |
| C18 | ReferenceLink | Typed destination and accessible label | Navigate; unavailable destination carries its reason |
| C19 | ViewSelector | Named view choices and selected view | SelectView; keyboard order follows visible order |

### House components

| ID | Component | Inputs | Events and owned state |
|---|---|---|---|
| C20 | IdentityMark | Explicit room, spirit, operator, or session identity | Optional OpenIdentity; never infers identity from an avatar |
| C21 | MessageTimeline | Ordered message identities, delivery state, and read position | InspectMessage, ReadThrough, RequestHistory; owns viewport-follow behavior |
| C22 | MessageRow | Authored content, attribution, time, and delivery facts | Inspect, FollowReference; renders permitted content only |
| C23 | Composer | Draft binding, recipient identity, availability, and submission state | EditDraft, RequestSend; no direct transport access |
| C24 | SessionList | Identified session observations and selection | SelectSession; never silently selects the newest instance |
| C25 | SessionControls | Exact session target, allowed commands, and command state | RequestCommand; uses C14 for consequential commands |
| C26 | ActivityTimeline | Permission-filtered events and observation status | InspectEvent, RequestHistory; disclosures start collapsed |
| C27 | MembershipPanel | Members, access facts, and permitted membership actions | InspectMember, RequestChange; separates membership from participation |
| C28 | ThreadSummary | Thread identity, time range, participants, and ThreadState | OpenThread, OpenRecord |
| C29 | WorkBoard | Authoritative work items, available grouping, and query state | OpenWorkItem; never invents lifecycle transitions |
| C30 | WorkItemView | Scope, work identity, state, evidence, and allowed actions | OpenEvidence, RequestCommand |
| C31 | RecordView | Tagged record, revision, authority, and provenance | FollowReference, RequestRecordAction |
| C32 | ProvenanceView | Sources, attribution, revisions, and supersession links | OpenSource, OpenRevision; display only observed relationships |
| C33 | ProposalReview | Proposed changes, conflicts, evidence, and allowed decisions | RequestDecision; settled authority stays visually distinct |
| C34 | SourcePreview | Source identity, permitted content, and mapping proposal | SelectRange, EditMapping; no automatic ingestion |
| C35 | SettingRow | Effective value, owner, scope, mutability, apply mode, and consequence | EditValue, RequestApply; never invents defaults |
| C36 | HealthChannel | One named domain, observation time, status, and diagnostics | OpenDiagnostics, RequestRefresh |
| C37 | InboxList | Attention items with exact destinations and acknowledgment state | OpenItem, AcknowledgeCoveredItems |

### Composition contracts

- C04 composes C20 and C19.
- C03 and C11 compose C12 where asynchronous data is present.
- C21 composes C22 and C17 for permitted technical events.
- C25, C27, C30, C31, C33, and C35 use C14 for consequential actions.
- C14 renders its completed outcome through C15.
- C31 and C33 compose C32 where provenance is present.
- C10 and editable feature panels compose C16.
- C05 uses C20, C31, C32, or C36 according to its tagged context.
- C13 owns overlay mechanics. Its content never creates a second focus trap.
- C08, C09, C17, and C18 are shared primitives. Use them only where their contracts apply.

A feature cannot restyle an action to conceal its consequence.
A component cannot fetch credentials or inspect another feature's private signals.
Do not introduce a generic panel with flags for unrelated record families.

## 7. Screen catalogue and composition map

All routed screens use C01 and C07. Connected screens also use C02 and C06.
Subject screens use C03 and C04. Overlays use C13 instead of creating another shell.
The Components field names the additional required feature components.

### S01 — Connection and recovery

Purpose: establish the authenticated House connection or recover an unavailable connection.
Entry: application start without a usable connection; connection failure action.
Components: C09, C12, C16, C20, C36.
Layout: House identity, connection state, available recovery action, and explicit diagnostic detail.
Actions: Connect, Retry connection, Open diagnostics, Return when permitted.
State: pending connection, connected, rejected, offline, and unavailable recovery capability.
Data: platform connection provider and Host identity. Credentials never appear in fields or diagnostic output.
Exit: restore the last valid destination or enter S10. Retain unrelated drafts during recoverable failures.

### S02 — Direct conversation

Purpose: converse within one independently identified conversation associated with a spirit and room.
Entry: ongoing conversation list, successful conversation creation, restored destination, or an authorized conversation reference.
Components: C20, C21, C22, C23, C15, C16, C17, C18.
Layout: ongoing conversations on the left; a compact header, message timeline, and composer in the center. Inspector closed by default.
Actions: New conversation, Select spirit, Create, Cancel creation, Switch conversation, Send, Load earlier messages, Inspect message, Open spirit, Open sessions, Open records.
State: creating, creation rejected, initial load, live, catching up, offline, pending delivery, delivery failure, empty history, and finished-history viewing.
Data: conversation inventory, creation receipt, conversation-specific snapshot, permitted stream, delivery receipts, and associated boat receipts.
Exit: retain the scoped draft and read position. Leaving the view does not imply message delivery or acknowledgment.
Creation replaces the center with an inline form. C16 selects the spirit; C09 supplies Create and Cancel through shared primitives.
Only an authoritative creation receipt adds a new conversation identity to the list.
Creation never silently reuses another conversation for the same spirit.
The creation contract must state whether creation starts a harness or waits for the first message.
Completed conversations stay outside the ongoing list. S19 provides their history entry.
Opening finished history shows its transcript and associated boat without starting a harness or enabling submission.
An explicit continuation action requires a separately approved contract. Reading history never changes its finished state.

### S03 — Room sessions

Purpose: select one harness instance and inspect its permitted activity.
Entry: Direct subject view selector.
Components: C20, C24, C25, C26, C14, C15, C17.
Layout: session selector above the selected activity surface; controls beside exact session identity.
Actions: Select session, Inspect event, Load history, Request an admitted session command.
State: no selection, loading, live, stale, ended, inaccessible, and command outcome.
Data: session registry, capability facts, and permission-filtered activity.
Exit: release screen-owned observation. Preserve unresolved command identity for reconciliation.
Never present session activity as a conversation transcript. Session and conversation identities remain distinct.

### S04 — Room records

Purpose: browse records within the selected room's permitted scope.
Entry: Direct subject view selector.
Components: C10, C11, C12, C18, C31, C32.
Layout: criteria above a bounded result list; selection opens S12.
Actions: Query, Reset, Load more, Open record, Open source.
State: idle, loading, empty, ready, refreshing, and failed.
Data: typed record queries. Room memory and House lessons remain separate families.
Exit: retain criteria and scroll for this route only. Clear incompatible selection on scope change.

### S05 — Hallway collection

Purpose: find dated shared conversations without merging participant rooms.
Entry: Hallways collection.
Components: C10, C11, C12, C20, C28.
Layout: filter controls above dated thread summaries.
Actions: Query, Open thread, Open hallway record, Load more.
State: loading, empty, ready, unavailable access, and failed.
Data: thread inventory and permitted participation facts.
Exit: selected thread routes to S06. The hallway relationship routes to S07.
Gate: thread enumeration requires a verified Host contract.

### S06 — Hallway conversation

Purpose: read or participate in one identified shared thread.
Entry: S05 or an exact inbox destination.
Components: C20, C21, C22, C23, C27, C28, C15, C17, C18.
Layout: thread header, shared timeline, optional member region, and permission-aware composer.
Actions: Send when permitted, Inspect participant, Load history, Open record, Read through covered messages.
State: loading, catching up, live, observer-only, sealed, folded, offline, and failed delivery.
Data: membership, participant observations, thread state, messages, and delivery receipts.
Exit: preserve thread read position. Membership never follows from sending or reading.
A closed thread shows its closing state and prevents submission with the stated reason.

### S07 — Hallway record

Purpose: inspect membership, access, closure events, and durable references for one hallway.
Entry: thread header or S05.
Components: C27, C28, C31, C32, C14, C15, C18.
Layout: access and membership first; dated record entries below.
Actions: Inspect member, Open thread, Open artifact, Request an explicitly permitted membership change.
State: loading, ready, empty record, inaccessible, and command outcome.
Data: authoritative hallway record and allowed actions.
Exit: return to the originating thread when it still exists.

### S08 — Projects and work board

Purpose: find work within the scopes the Host actually reports.
Entry: Projects collection.
Components: C10, C12, C20, C29.
Layout: scope identity and filters above work groups.
Actions: Query, Refresh, Open work item.
State: loading, empty, ready, refreshing, and failed.
Data: Docket queries and reported project relationships.
Exit: work selection opens S09.
Do not fabricate projects when the available scope is House-wide work.

### S09 — Work item

Purpose: inspect one work item's requirements, state, evidence, and available operations.
Entry: S08 or an authorized reference.
Components: C30, C32, C14, C15, C17, C18.
Layout: identity and state, acceptance criteria, evidence, then available actions.
Actions: Open evidence; request only commands explicitly admitted for the current principal.
State: loading, ready, inaccessible, stale revision, and command outcome.
Data: authoritative work detail and capability facts.
Exit: return to the board with its criteria and position intact.

### S10 — House overview

Purpose: expose shared records, work, and mechanics through stable visible doors.
Entry: global House control.
Components: C18, C20, C29, C36.
Layout: House identity followed by concise work and service summaries with destinations.
Actions: Open work, Open records, Open mechanics, Open source mapping, Open proposals.
State: independent loading or failure per region. One failure does not erase unrelated content.
Data: independently sourced summaries. Avoid one combined health score.
Exit: restore the previous valid subject destination when the operator returns.

### S11 — House records

Purpose: browse shared record families with explicit scope and authority.
Entry: S10.
Components: C10, C11, C12, C18, C31, C32.
Layout: record-family selector, criteria, and results.
Actions: Select family, Query, Load more, Open record.
State: idle, loading, empty, ready, and failed.
Data: family-specific queries for canon, lessons, and other approved shared records.
Exit: selection opens S12 with its tagged identity.
Gate: approve the exposed record families and their read contracts.

### S12 — Record detail

Purpose: inspect one typed record and its provenance without conflating record families.
Entry: S04, S07, S11, or a typed reference.
Components: C31, C32, C14, C15, C17, C18.
Layout: record identity and authority, body, provenance, revision relationships, available actions.
Actions: Open source, Follow revision, Request an admitted correction or supersession.
State: loading, ready, inaccessible, absent, stale revision, and command outcome.
Data: exact family read, provenance, and admitted commands.
Exit: return to the originating query without losing its criteria.
Record actions use family-specific command inputs. There is no universal untyped record writer.

### S13 — Source mapping

Purpose: inspect source material and propose its assignment before durable ingestion or repair.
Entry: S10 or a source reference with mapping permission.
Components: C10, C11, C12, C16, C32, C34, C14, C15.
Layout: source selection, content preview, mapping fields, and explicit review.
Actions: Select source, Preview ranges, Propose scope/type/ownership, Review, Confirm when admitted.
State: no source, loading, preview, invalid mapping, conflict, review, and command outcome.
Data: permission-filtered source content and authoritative mapping commands.
Exit: warn before discarding unsaved mapping edits. A preview alone never writes.
Gate: approved mapping vocabulary and ingestion contracts are required before implementation.

### S14 — Proposal review

Purpose: inspect candidates, conflicts, and proposed durable effects.
Entry: S10 or a proposal reference.
Components: C10, C11, C12, C33, C32, C14, C15.
Layout: proposal list beside or above the selected proposal, evidence, conflicts, and decision controls.
Actions: Select proposal, Open evidence, Request an admitted review or publication decision.
State: loading, empty, selected, stale proposal, unresolved conflict, and command outcome.
Data: proposal-family contracts. A review transition and durable publication remain distinct effects.
Exit: preserve query selection where valid. Never publish because the operator merely opened a proposal.
Gate: approve the proposal families and decision capabilities.

### S15 — House mechanics

Purpose: inspect effective settings, service status, and their distinct owners.
Entry: S10 or a status-channel destination.
Components: C10, C12, C35, C36, C14, C15, C17.
Layout: separate Settings and Status views. Each setting names scope, owner, and apply mode.
Actions: Query settings, Inspect diagnostics, Refresh a channel, Review an admitted setting change.
State: independently loading sections, ready, stale, unavailable mutation, and command outcome.
Data: one authoritative configuration snapshot and independent service observations.
Exit: retain local filtering. Pending mutations retain their request identities.
Never expose raw secret values or a general environment-file editor.

### S16 — Interface preferences

Purpose: adjust local reading and presentation without changing House policy.
Entry: account menu.
Components: C16, C19, C35.
Layout: grouped Appearance, Reading, and Accessibility controls with immediate preview.
Actions: Change a local preference; restore an approved default.
State: loaded, changed, persistence failure, and unsupported platform option.
Data: typed local preferences and the approved token catalogue.
Exit: retain successfully applied preferences. Report persistence failure without implying success.
Gate: approve which preferences persist and where they are stored.

### S17 — Account and connection

Purpose: show the operator, current House, access context, and connection controls.
Entry: account menu.
Components: C20, C36, C09, C14, C15, C18.
Layout: identity and connection facts, interface preferences entry, then consequential session actions.
Actions: Open preferences, Open diagnostics, Disconnect or change connection when supported.
State: connected, offline, unavailable action, and command outcome.
Data: authenticated principal and platform connection provider.
Exit: a connection change invalidates scope-bound data before another principal can see it.
Gate: approve disconnect behavior for local drafts and unresolved commands.

### S18 — Diagnostics

Purpose: inspect an observed fault and the recovery actions available for that domain.
Entry: status strip, S01, S15, or S17.
Components: C36, C17, C18, C14, C15.
Layout: affected domain, observation time, redacted diagnostic facts, and admitted recovery actions.
Actions: Refresh, Inspect evidence, Request repair when explicitly available.
State: loading, healthy observation, degraded observation, unavailable repair, and command outcome.
Data: redacted diagnostics and bounded recovery capabilities.
Exit: return to the referring screen. Opening diagnostics never starts repair.

### S19 — Spirit window and conversation history

Purpose: inspect one spirit and navigate its previous conversations.
Entry: the spirit identity control in a conversation; a permitted spirit destination in O05.
Components: C20, C10, C11, C12, C18, C15.
Layout: spirit identity above a queryable conversation history. Each row identifies its conversation and observed lifecycle state.
Actions: Query history, Load more, Open conversation, Inspect boat, New conversation with this spirit.
State: loading, empty history, ready, refreshing, inaccessible, and failed.
Data: authorized conversation inventory scoped to the spirit and room, distinct conversation identifiers, and associated boat receipts.
Exit: selecting a conversation replaces the center with S02. Return restores history criteria and position.
Finished conversations open in history mode. They do not enter the ongoing sidebar merely because someone reads them.
Starting a new conversation uses S02's creation flow with the spirit selected.
History remains available when the ongoing sidebar is empty and when the House contains only one spirit.
Gate: verify conversation enumeration and boat association contracts. Do not infer associations from titles or timestamps alone.

## 8. Overlay catalogue

| ID | Surface | Entry and composition | State, actions, and exit |
|---|---|---|---|
| O01 | Collection overlay | Narrow-layout collection control; C13, C02, C03 | Open collection; choose subject; close and focus the destination or return trigger |
| O02 | Context inspector | Inspect action; C13, C05 | Show selected context; follow references; close to the originating control |
| O03 | Account menu | Operator control; C13, C20, C18 | Open S16 or S17; nested Back returns one level; Escape closes at root |
| O04 | Attention inbox | Bell control; C13, C37, C20, C18 | Loading, empty, ready, failed; open exact destination; acknowledge only covered items |
| O05 | Destination switcher | Global switcher control; C13, C10, C11, C18 | Filter permitted destinations; keyboard select; navigate; cancel restores focus |
| O06 | Command review | Consequential action; C13, C14, C15 | Review target and consequence; confirm once; show receipt, refusal, or unresolved outcome |
| O07 | Identity detail | Identity action; C13, C20, C27 | Show explicit identity and permitted context; close without changing selection |
| O08 | Source or evidence preview | Reference action; C13, C12, C32, C34, C18 | Loading, ready, inaccessible, failed; open full destination; close returns focus |

Opening O04 does not wake a participant. Reading an item does not acknowledge unrelated items.
O05 lists known destinations. It never invents capabilities from a label match.
Closing O06 while a command is pending does not cancel the server operation.
The owning feature keeps the request identity and exposes its unresolved outcome when reopened.

## 9. Visual and accessibility contracts

Use one theme root. Palette identity, semantic phase, reader scale, surface intensity, and motion remain separate axes.
Use semantic tokens for typography, color, spacing, and motion.
Keep required text above the approved contrast floor. Muted decoration cannot carry unique information.

Use native landmarks and controls. Keep heading order and accessible names meaningful without styling.
Show selection, authority, failure, and availability through text and structure as well as color.
Maintain a visible outline under forced colors.

An overlay makes covered content inert. Its Escape behavior follows its own navigation depth.
Restore focus to a surviving trigger. If that trigger disappears, use the nearest valid owning control.

Do not announce a streaming transcript token by token through a live region.
Announce delivery outcomes and important state changes without interrupting reading continuously.
Preserve text selection during unrelated updates.

Shared visual specifications must define spacing, typography, density, icons, and responsive thresholds before visual implementation dispatch.
The current catalogue supplies principles. Exact GUI token bindings remain approval gate Q02.

## 10. Reuse and scale

Use a shared component when consumers share behavior, state meaning, and reasons to change.
Keep family-specific authority and validation in tagged feature contracts.
Add a variant only when the semantic contract changes. Do not add visual flags for one screen's convenience.

Use bounded queries and stable row identities. Require pagination where collections can grow without a fixed bound.
Do not create subscriptions per rendered row.
Introduce virtualization only after measured rendering cost requires it. Preserve selection and keyboard behavior when doing so.

Retain query results only within their scope and stated freshness policy.
Use request association and cancellation before introducing caches or retry machinery.
Avoid whole-screen updates for one streaming message or one setting observation.

The composition map in sections 7 and 8 is the source for reverse component usage.
Generate consumer lists from that map. Do not maintain a second handwritten usage matrix.

## 11. Acceptance journeys

These are required future checks, not claims of completed verification.

| ID | Journey | Observable result |
|---|---|---|
| A01 | Open Direct; enter a draft; visit records; return | Exact draft and conversation identity remain together |
| A02 | Start a room read; change rooms before completion | The late response cannot populate the destination room |
| A03 | Switch rooms during a live subscription | Retired events stop updating the UI; the new binding has one subscription owner |
| A04 | Disconnect during a send; reconnect | Delivery remains unresolved until a receipt or reconciliation establishes the outcome |
| A05 | Open sessions without selecting one | No activity stream silently attaches to an arbitrary instance |
| A06 | Open an observer-only or closed hallway thread | The composer refuses submission with the exact reason |
| A07 | Open an inbox item | Navigation reaches its exact thread; acknowledgment covers only the reported range |
| A08 | Query each approved record family | Scope, empty results, errors, and provenance remain correctly distinguished |
| A09 | Review a consequential action; change its target revision | Confirmation rejects stale review or requires a fresh review |
| A10 | Close and reopen a pending command review | The existing request remains identifiable; no duplicate command is sent |
| A11 | Navigate using Back, Forward, keyboard, and nested Escape | Valid destination and focus return without hidden focus or dead ends |
| A12 | Use a 390-pixel viewport and an on-screen keyboard | No document overflow; inputs and actions remain reachable |
| A13 | Enable reduced motion and forced colors | Every state and focus position remains legible |
| A14 | Lose one service while another remains available | Status channels retain independent observations and recovery controls |
| A15 | Preview source mapping and cancel | No durable write occurs |
| A16 | Inspect and review a proposal | Review status cannot masquerade as publication authority |
| A17 | Add a second consumer of a shared component | Behavior and accessibility come from the same contract without screen-specific forks |
| A18 | Start the packaged Windows application | WASM assets, navigation, Host access, and shutdown work through the actual desktop boundary |
| A19 | Restart after visiting a conversation or House instrument | A valid instrument restores; only the latest unfinished conversation restores; otherwise use S10 |
| A20 | Create two conversations with one spirit; switch between them | Histories, drafts, read positions, and delivery outcomes remain separate |
| A21 | Open a record from a conversation | The center changes; ongoing conversations remain available on the left |
| A22 | Open a conversation with default preferences | Inspector and diagnostics remain closed; messages and composer dominate |
| A23 | Render a mixture of ongoing and completed conversations | Only ongoing entries appear in the left conversation list |
| A24 | Store a boat for one of two conversations with the same spirit | Only the associated conversation finishes and leaves the ongoing list |
| A25 | Fail a boat write, exit a harness, or leave a conversation idle | None of these events alone marks the conversation finished |
| A26 | Reopen the application after using several conversations | Restore only the latest if it has no boat; otherwise show House without resurrecting older conversations |
| A27 | Open a spirit window and select a finished conversation | Its transcript and boat remain readable; its finished state and ongoing sidebar membership do not change |
| A28 | Open history in a single-spirit House with no ongoing conversations | Previous conversations remain reachable through the spirit destination |

Every screen also requires its loading, empty, ready, inaccessible, and failure paths where those states can occur.
Exercise each visible action. A rendered button is not proof of a reachable outcome.

## 12. Approval gates

| ID | Decision | Proposed resolution or required evidence |
|---|---|---|
| Q01 | Screen scope and navigation | Center replacement, multiple conversations per spirit, and spirit-window history are approved; review remaining screens and labels |
| Q02 | GUI visual specification | Quiet default chrome is approved; inspect the terminal reference and bind exact tokens and geometry |
| Q03 | Service capability map | Verify request, response, identity, revision, permission, and subscription contracts for every action |
| Q04 | Routes and history | Last valid destination with House fallback is approved; approve URL encoding and detailed history restoration |
| Q05 | Draft retention | Proposed: memory across view changes; approve restart persistence, clearing, and sign-out behavior separately |
| Q06 | Command continuity | Verify idempotency, reconciliation, cancellation, and permission-revocation behavior by command family |
| Q07 | House record scope | Approve exposed record and proposal families; verify mapping and correction capabilities |
| Q08 | Platform boundary | Approve Windows client rendering and native integration; specify Android connectivity separately |
| Q09 | Visual motion | Approve local crossfade and nested-overlay movement; measure reduced-motion and rapid-navigation behavior |
| Q10 | Conversation lifecycle | Boat-based finish, latest-only restoration, and spirit-window history are approved; verify receipt association, creation effects, and explicit continuation semantics |

Unresolved gates block only the affected implementation slice.
Builders receive explicit approved decisions. They must not resolve product questions through inference.

## 13. Builder packet contract

Each packet contains:

1. The exact screen or component IDs in scope.
2. The approved specification bodies and applicable House lesson bodies.
3. Typed service contracts with authoritative sources.
4. Exact file ownership and permitted dependency surfaces.
5. Component inputs, events, state lifetime, and composition.
6. Observable acceptance journeys and verification ownership.
7. Explicit missing prerequisites and refusal conditions.

Use a bounded source set containing only approved inputs.
A sparse checkout controls accidental exposure; it does not provide security isolation.
A missing contract returns to the design owner before implementation proceeds.

## 14. Design provenance

The architectural direction and draft status come from Sol and Kintsu's design discussion.
The component and screen proposals in this document await review.

Applicable House lesson anchors include:

- Coding 9, 12, 47, 143, 158, 159, 195, 330, and 446: plain contracts, shared ownership, explicit lifetimes, and module boundaries.
- Coding 134, 215, 257, 258, and 354: real-boundary proof, reachability, and complete interaction states.
- Coding 316, 317, 322, 328, and 340: bounded worker packets and explicit integration ownership.
- Design 293–304: consequence, refusal, authority separation, tokens, and visual state.
- Design 409 and 422: visible focus and truthful affordances.
- Project 117–120, 412, and 413: House product grammar, mapping, authority, and configuration visibility.

Retrieved Solarisael catalogue contracts: text-scale (9), mobile-composition (8), side-menu (7), and theme-axes (5).
Their site-specific geometry and naming do not automatically define application components.
This draft carries applicable principles. Q02 governs their exact application to the GUI.
