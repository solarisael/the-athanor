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
The source remains the authority for implementation behavior.
