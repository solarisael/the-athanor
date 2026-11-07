# Athanor anatomy

`anatomy.jsonl` maps the Athanor source into nodes and directed connections.
`anatomy.graphml` contains the same graph for tools that accept GraphML.
`anatomy.html` shows the graph in a browser and reads `anatomy.jsonl` from the same HTTP server.

From this directory, install the pinned dependencies with `npm ci --ignore-scripts`.
Start the local server with `npm start`.
Bun must be installed.
Open `http://127.0.0.1:4187/`.
Restart the server after JavaScript changes. The server builds the browser bundle at startup.

The viewer uses Cytoscape.js canvas rendering and the fCoSE force layout.
The server serves the library locally. The browser does not contact a CDN or send graph data outside this machine.
Every node appears as a labeled rectangular card.

Select a card to highlight its neighbors and inspect its source pointers.
Drag the background to pan. Scroll to zoom. Drag a card to move it.
The **Organs** view shows the main domains in a grid.
The **Neighborhood** view arranges one selected node and its direct neighbors.
Use **Whole graph** to restore the complete force layout.
Use **Spread out** to arrange the current view again.
Use the search field and kind selector to find a specific part.
The catalogue provides keyboard access to every node.

The map describes the checked source. It does not certify the installed runtime.
Source references carry repository-relative paths and inclusive line ranges.
The metadata record stores the repository revision and a SHA-256 digest for each referenced file.
A file digest identifies the working-tree content, including changes outside a commit.

## Records

Each JSONL line contains one JSON object.

| `type` | Fields | Meaning |
|---|---|---|
| `meta` | `schemaVersion`, `capturedAt`, `revision`, `sourceFiles` | Source identity and format version |
| `node` | `id`, `kind`, `label`, `description`, `sources` | One organ, component, module, tool, store, or dependency |
| `edge` | `id`, `from`, `to`, `relation`, `description`, `sources` | One directed connection |

A source reference contains `path`, `line`, and `endLine`.
Node IDs name entities. Edge endpoints use those IDs.
Additional edge fields retain dependency conditions when the manifest declares them.

`contains` names structural membership.
`depends_on` names a declared code dependency.
`calls` names an execution connection.
`reads` and `writes` name access to state.
Other relation names describe their specific operation.
A dependency edge does not prove that a process sends a request.

## Read the graph

Import `anatomy.graphml` into a graph tool that accepts directed GraphML.
Use `label` for display text.
Use `kind` to separate organs, tools, storage, and implementation detail.
Use `relation` to filter connections.
Open the `sources` data to inspect the supporting code.

For a custom viewer, parse each JSONL line separately.
Load the node records before the edge records.
Keep edge direction when the viewer supports it.
Treat `meta` as graph metadata, not as a node.

This is a source snapshot. Refresh affected records after the source changes.

Run `npm run stale` to find the records that need a refresh.
The check hashes each cited file and compares it with the digest in the `meta` record.
It lists every node and edge that cites a changed or missing file.
It exits with code 1 when at least one file differs.
The source remains the authority for implementation behavior.

## Constellation view

Open `http://127.0.0.1:4187/constellation` while the server runs.
Each organ is a bubble. Its parts are stars inside the bubble.
A bubble shows more of its inside as it grows on the screen. Scroll to zoom, or drag the rim to resize the bubble.
The camera moves only when you move it. Double-click a bubble to frame it.

The view reads four files:

| File | Owner | Content |
|---|---|---|
| `anatomy.jsonl` | the map snapshot | nodes and edges |
| `anatomy.overlay.jsonl` | the state-machine census and the operator | added organs, ownership rules, state machines, defects, and tags |
| `anatomy.layout.json` | the operator | bubble positions, size scales, and star positions; the view saves it after each move |
| `/anatomy.stale.json` | the server | cited files that changed since the snapshot; computed per request |

Each defect in `anatomy.overlay.jsonl` comes from one file in `docs/state-machines/`.
When a defect is fixed or a contract file changes, update the overlay in the same commit.

A bubble's size follows its part count. When you drag the rim, the layout file keeps your change as a `scale` on that size.
When an organ gets more parts, it grows by itself. When a bigger bubble covers a neighbour, the view moves them apart once and saves. **Undo** puts them back.

### Who owns a part

The `ownership` record in the overlay sorts the organs into three tiers:

1. Named organs, such as GIGA, Hallway, or Chat.
2. Carriers: the processes and crates that carry the named organs, such as Host, AKASHA, and the OMP adapter.
3. The shared organ, Hearth.

The view finds the owner of each part in this order:

1. The `assign` list in the `ownership` record. An entry there wins over every rule below.
2. Membership. The organ that reaches the part in the fewest `contains` or `implemented_by` steps owns it. On a tie, a named organ wins.
3. Connections. The part joins the organs that its other connections reach. A package counts as its crate organ.
   The best tier present decides. In that tier, an organ with more than half of the connections owns the part.
   If no organ has more than half, the common parent in `parents` owns the part. With no common parent, the shared organ owns it.

The header shows how many parts the view placed. A part with no connection to any organ stays off the map.

### State machines

Each `machine` record has one or more layers, for example SQL, Host, and Adapter.
One layer has `authority: true`. Each layer lists its states, its terminal states, and its moves.
Each move names the code that writes it, with a source range.

The view finds the states with no way out. Such a state is not terminal, and no move leaves it.
Do not list these states by hand. When the code gets a new exit, add the move, and the red mark goes away.

### Layout safety

- A press that moves less than 5 pixels is a click. A click does not change the layout file.
- **Undo** (Ctrl+Z) puts back the layout from before the last move, **Tidy up**, or **Arrange by links**.
- **Arrange by links** changes the layout only when the new layout has fewer crossing cables.
- When the layout file is not valid JSON, the view ignores it and shows a message. Your next move replaces the file.
- The server accepts requests only for the host names `127.0.0.1:4187` and `localhost:4187`.
