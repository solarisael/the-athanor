# Connected GUI navigation

This map describes the connected web surface. `LESSONS_MAP.md` holds its interaction rules.
Hearth names the control center. Pulse serves its web interface.
The Host and each domain organ retain their existing authority.

## Scope

The current cutover connects existing capabilities. It does not implement new backend capabilities.
The interface removes simulated records and local actions that imply durable writes.
Unavailable actions stay unavailable. The roadmap tracks their missing contracts.

## Navigation

Direct, Hallways, and Projects remain the primary modes.
The House mark opens the House surface and preserves the previous selection.
Each subject has three slots. Subject changes preserve the selected slot.

| Slot | Direct | Hallways | Projects | House |
|---|---|---|---|---|
| Live | Chat | Thread | Overview | Overview |
| State | Status and sessions | Status | Status | Mechanics |
| Durable | Memories | Record | Evidence | Memories and lessons |

Number keys select slots outside editable controls.
Control plus arrow keys select modes or subjects.
Control plus Space opens the switcher.
Escape closes the current overlay or moves outward through the drawer.

## Screen inventory

| Family | Existing source or operation | Owner |
|---|---|---|
| Direct Chat | Room chat snapshot and operator chat submission | `chat.js` |
| Direct Status | Room identity, policies, health, and portable session status | `mechanics-live.js`, `sessions/index.js` |
| Direct Memories | Selected room timeline and full memory read | `sediment/index.js` |
| Hallway Thread | Inbox and bounded message reads | `hallways.js`, `board/hallway-messages.js` |
| Hallway Status | Inbox counts, members, timestamps, and message source | `hallways.js` |
| Hallway Record | Members from the current inbox response | `hallways.js` |
| Project Overview | Docket quests grouped by state | `projects.js` |
| Project Status | Docket counts and returned evidence | `projects.js` |
| Project Evidence | Existing receipt reads, one request at a time | `projects.js`, `board/index.js` |
| House Overview | Shared Docket and inbox rounds | `board/index.js` |
| House Mechanics | Health, room state, Insula vitals, and retention receipts | `mechanics.js`, `mechanics-live.js`, `pulse.js` |
| House Record | House-scoped memories and the lesson timeline | `sediment/index.js` |
| Bell | The same inbox round as the House board | `app.js`, `board/index.js` |
| Switcher | Existing navigation transitions | `app.js` |
| Account and Settings | Reported operator and browser-local preferences | `app.js`, `preferences/index.js` |
| Inspector | Selected subject, message, and observed source facts | `app.js` |
| Status strip | Separate Host, Recall, body, worker, and delivery channels | `health.js` |
| Repair | Existing desktop repair operations when the proxy reports support | `repair.js` |

`app.js` owns the shell transitions and event delegation.
`index.html` owns the semantic regions. `styles.css` owns the layout and visible states.

## Room selection

`POST /local/rooms` returns configured room keys, the default room, and the repair capability.
It returns no credentials or filesystem paths.
The Direct collection comes from those configured keys.
The selected room uses the `room` URL parameter.
`askHost` sends that value in `X-Athanor-Room`.
Both proxies refuse keys outside the configured room list.
An absent header selects the launch room.
A room change reloads the document. Previous room caches cannot become the next room's state.
An unsent message requires confirmation before that reload discards it.
Selecting a room never changes the operator into its spirit.

## Session controls

Direct Status lists exact registered instances. No instance starts selected.
The operator selects an instance and its conversation before proposing a control.
Only controls reported by that instance appear.
A separate confirmation names the target, payload, and effect.
The interface shows command receipts and permits an explicit receipt check.
It does not automatically send or repeat commands.
Session controls do not change the room chat target.
Session creation remains unavailable.

The proxies expose only `status`, `command`, and `commandStatus` through `/live/session`.
They refuse attachment, delivery-owner changes, resource writes, and harness acknowledgments through that route.
The existing Host validates command targets, payloads, capabilities, and retry identity.

## Record boundaries

House memories explicitly use the `house` scope.
Room memories explicitly use the selected room scope.
Search and filters cover loaded pages only. Counts describe loaded rows, not database totals.
Older-page requests preserve the server cursor.
Memory expansion stays bound to its source row and scope.
Lesson bodies remain unavailable because this proxy has no full-body read route.
Reading Hallway messages acknowledges nothing.
Hallway posting, sealing, folding, and presence controls remain unavailable.

## Mechanics and failures

The observatory uses observed responses instead of client-owned defaults.
Missing fields remain unreported. Missing counts never become zero.
Vitals and retention failures remain independent.
Retention days and receipts come from returned retention records.
A lane opens spans. A span opens its trace.
Long Direct diagnostics remain collapsed beneath named scalar facts.

## Proxy and public boundaries

Bun serves the assets from disk. The desktop proxy embeds the same assets.
Both preserve the canonical loopback Host, Origin, and Fetch Metadata checks.
The bearer remains inside the proxy.
The desktop supports local repair. Bun reports that capability as unavailable.
The public build blocks Host requests and includes no server files.
Public interface preferences remain browser-local. Public pages contain no durable House records.
