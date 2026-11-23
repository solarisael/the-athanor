# House MCP

A stdio MCP server that gives any MCP harness the House organs that OMP
sessions reach through `adapters/omp/house-proof/tools.ts`. The plan is
[docs/HOUSE_MCP.md](../../docs/HOUSE_MCP.md).

The server declares no tool of its own. It runs `registerSolarisaelTools`
against a small `pi` that captures each definition, and exposes every captured
tool with the same name, description, and Zod schema, except the OMP-only tools
listed below. A change in `tools.ts` reaches both harnesses.

## Install

This package is not a root workspace. Install its two dependencies once:

```sh
cd adapters/house-mcp
bun install
```

## Run

```sh
ATHANOR_CLIENT_ROOM_DIR=~/.local/state/athanor/rooms/<room> bun adapters/house-mcp/src/server.ts
```

- **Room.** `ATHANOR_CLIENT_ROOM_DIR` names the room. Without it, the launch
  directory must be a room directory. `room.ts` resolves the room, the same as
  for an OMP home launch. If no room resolves, the server refuses to start and
  names the variable.
- **Host.** The server reads `~/.omp/agent/athanor/client.json` and sets
  `ATHANOR_HOST_TOKEN`, `ATHANOR_HOST_URL`, `ATHANOR_HOST_HOUSE_ID`, and
  `ATHANOR_STATE_DIR`. A key that is already set wins, the same rule that
  `installed-loader.ts` uses. Without `client.json`, those keys must be in the
  server's environment.
- **Session.** Each process mints one session, `mcp-<uuid>`, and holds it as
  the room's top-level session. An OMP spirit session holds the same place in
  its own process. Docket writes and Presence close require it.
- **Instructions.** The `initialize` result carries `<houseDir>/ORGANS.md`. If
  the file is missing, the server starts without instructions and says so on
  stderr.
- Stdout carries only JSON-RPC. Logs go to stderr.

## Claude Code

```sh
claude mcp add --transport stdio athanor-house \
  --env ATHANOR_CLIENT_ROOM_DIR=/home/<you>/.local/state/athanor/rooms/<room> \
  -- bun /path/to/the-athanor/adapters/house-mcp/src/server.ts
```

Or in `.mcp.json`:

```json
{
  "mcpServers": {
    "athanor-house": {
      "type": "stdio",
      "command": "bun",
      "args": ["/path/to/the-athanor/adapters/house-mcp/src/server.ts"],
      "env": {
        "ATHANOR_CLIENT_ROOM_DIR": "/home/<you>/.local/state/athanor/rooms/<room>"
      }
    }
  }
}
```

One server process serves one room. To give a client two rooms, add two
servers. Every subagent of the client shares the server's one session, so the
server cannot tell a worker from the spirit the way OMP does.

## OMP-only tools

`OMP_ONLY_TOOLS` in `src/server.ts` is the one list. These tools need OMP state
that an MCP host does not have:

| Tool | Reason |
|---|---|
| `zoom` | Reads child logs and image attachments from OMP transcript files through `ctx.sessionManager`. The `id+n` lines it opens belong to OMP's live chat view. |
| `house_model_default` | Resolves the model with `ctx.models` and applies it with `pi.setModel`. |
| `kitten_lineage_status` | Reports in-process OMP task lineage counters. |
| `home_resources` | Needs an OMP home attachment (`requireAttached`). |
| `home_resource_write` | Needs an OMP home attachment. |
| `house_sessions` | Needs an OMP home attachment. |
| `session_control` | Issues agent and session controls from an OMP attachment. |
| `request_restart` | Restarts the OMP process through its attachment and launcher. |

Two tools stay in, with less than in OMP:

- `orihon` has no judgment credential, because MCP has no OMP model registry.
  A query returns its candidates with the selection
  `unavailable: credential_unavailable`. Read a candidate by its `turnNo`.
- `sleep` writes the paper boat through the Host. The OMP compaction handoff
  that follows a boat in OMP does not exist here.

## Test

The test runs against the live Host. Every call in it reads, or the server
refuses it before it reaches the Host. It skips without
`ATHANOR_CLIENT_ROOM_DIR`.

```sh
cd adapters/house-mcp
ATHANOR_CLIENT_ROOM_DIR=~/.local/state/athanor/rooms/<room> bun test
```
