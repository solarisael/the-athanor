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
- **Host.** The server needs the Host token and the House id. The server also
  uses the Host URL. Without a URL, the server uses `ws://127.0.0.1:8787`.
  The server takes each key from the first place in this list that holds the
  key:
  1. The environment: `ATHANOR_HOST_TOKEN`, `ATHANOR_HOST_HOUSE_ID`, and
     `ATHANOR_HOST_URL`. A key in the environment wins.
  2. `~/.omp/agent/athanor/client.json`, if this file exists. The Windows
     installer writes this file. The server also sets `ATHANOR_STATE_DIR` from
     this file, with the rule that `installed-loader.ts` uses.
  3. The Linux data root, if `client.json` does not exist. The data root is
     `$ATHANOR_DATA_ROOT`, else `$XDG_STATE_HOME/athanor`, else
     `~/.local/state/athanor`. The file `host.token` holds the token. The file
     `config/runtime.json` holds `hostPort` and `houseId`.

  If no place holds the token or the House id, the server does not start. The
  refusal names each place. The log on stderr shows the key names. The log
  never shows the token.
- **Session.** Each process mints one session, `mcp-<uuid>`, and holds it as
  the room's top-level session. An OMP spirit session holds the same place in
  its own process. Docket writes and Presence close require it.
- **Instructions.** The `initialize` result carries `<houseDir>/ORGANS.md`. If
  the file is missing, the server starts without instructions and says so on
  stderr.
- Stdout carries only JSON-RPC. Logs go to stderr.

## Another harness on Linux

On Linux, the server reads the Host keys from the data root. The harness gives
only the room folder. Do not put the Host token in a harness configuration.

1. Install the dependencies once. Refer to [Install](#install).
2. Add the server to the harness. For Claude Code, run this command. Replace
   `<room>` with the room key.

   ```sh
   claude mcp add --transport stdio athanor-house \
     --env ATHANOR_CLIENT_ROOM_DIR="$HOME/.local/state/athanor/rooms/<room>" \
     -- bun "$HOME/Projects/the-athanor/adapters/house-mcp/src/server.ts"
   ```

`$HOME/Projects/the-athanor` is the checkout that `athanor-nix` runs the House
from. If your checkout is in a different folder, change the path. The room
folder is the folder that holds `.athanor-room.json`.

For a different MCP harness, use these values:

| Field | Value |
|---|---|
| Transport | stdio |
| Command | `bun` |
| Arguments | `/home/<you>/Projects/the-athanor/adapters/house-mcp/src/server.ts` |
| Environment | `ATHANOR_CLIENT_ROOM_DIR=/home/<you>/.local/state/athanor/rooms/<room>` |

If the harness cannot find `bun`, use the full path that `command -v bun`
shows.

The same server in `.mcp.json`:

```json
{
  "mcpServers": {
    "athanor-house": {
      "type": "stdio",
      "command": "bun",
      "args": ["/home/<you>/Projects/the-athanor/adapters/house-mcp/src/server.ts"],
      "env": {
        "ATHANOR_CLIENT_ROOM_DIR": "/home/<you>/.local/state/athanor/rooms/<room>"
      }
    }
  }
}
```

To check the launch without a harness, start the server with only `HOME`,
`PATH`, and the room folder in its environment:

```sh
env -i HOME="$HOME" PATH="$PATH" \
  ATHANOR_CLIENT_ROOM_DIR="$HOME/.local/state/athanor/rooms/<room>" \
  bun "$HOME/Projects/the-athanor/adapters/house-mcp/src/server.ts"
```

The server writes `ready on stdio` to stderr. Then the server waits for JSON-RPC
on stdin. To stop the server, push Ctrl+D.

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

`bun test` runs two files:

- `tests/host-environment.test.ts` checks the order of the places for the Host
  keys. The file uses an injected environment and a temporary folder. The file
  needs no Host.
- `tests/house-mcp.test.ts` runs against the live Host. Every call in it reads,
  or the server refuses it before it reaches the Host. It skips without
  `ATHANOR_CLIENT_ROOM_DIR`. One test starts the server with only the room
  folder in its environment, and calls `room_state`.

```sh
cd adapters/house-mcp
ATHANOR_CLIENT_ROOM_DIR=~/.local/state/athanor/rooms/<room> bun test
```
