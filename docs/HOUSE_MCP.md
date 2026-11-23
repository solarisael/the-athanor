# The House as an MCP server

Status: plan, 2026-11-23. Sol approved the direction in the kodo room.

## Why

The House organs (recall, remember, canon, lessons, wake, hallway, and the
rest) reach only OMP, through `adapters/omp/house-proof/tools.ts`. An MCP
server lets any MCP harness reach the same organs. It is the second harness
adapter that `docs/HARNESS_EXTENSION.md` section 8, question 2, asks for: one
adapter per harness, and the Athanor owns every decision.

## Trust: no change

The MCP server holds exactly the trust that an OMP session holds today.

- The Host accepts one House-wide bearer token, `ATHANOR_HOST_TOKEN`
  (`crates/host/src/house.rs:234-247`, `server.rs:350-372`).
- `validate_command` (`server.rs:2738-2785`) checks that the envelope's
  room, spirit, and house match the room route. It requires only a non-blank
  `sender_session` of at most 256 bytes.
- The adapter chooses the room from the launch directory or from
  `ATHANOR_CLIENT_ROOM_DIR` (`adapters/omp/house-proof/room.ts:109-148`).

A process that runs as the operator and holds the token can act as any room,
whether it is OMP or MCP. Per-room secrets (the Docket capability) keep
working unchanged, because they are read from the room directory.

## Shape: one tool table, no copy

The MCP server does not redefine the tools. It calls the existing
`registerSolarisaelTools(pi, release)` against a small `pi` that captures the
definitions. The tests already work this way
(`adapters/omp/tests/house-tool-arguments.test.ts`). Each captured definition
becomes an MCP tool with the same name, description, and Zod schema. A change
in `tools.ts` reaches both harnesses, and the two lists cannot drift.

| Piece | Source |
|---|---|
| stdio server | `@modelcontextprotocol/sdk`, as in `adapters/workspace-search/src/mcp.ts` |
| Host access | `~/.omp/agent/athanor/client.json`, projected into the same environment keys that `installed-loader.ts` sets |
| room | `ATHANOR_CLIENT_ROOM_DIR`, or the launch directory, resolved by the existing room code |
| session | `mcp-<uuid>`, minted once for each server process |
| `instructions` | `<houseDir>/ORGANS.md`, read at startup |

## OMP-only tools stay out

Some tools need OMP state that an MCP host does not have: transcript files,
the model registry, agent and session controls, and in-process diagnostics.
The server leaves them out by name and lists them in one place. Every other
tool is exposed.

## ORGANS.md

ORGANS.md keeps the parts that do not go stale: Sub aliis astris, No
borrowed leash, the authority order, and the rule for each organ. The
Windows topology and the deployment law leave it, because they describe a
machine that no longer exists. The MCP `instructions` carry this file, so an
MCP client gets the same House core that OMP rooms load through `AGENTS.md`.
When slice 5 of `HARNESS_EXTENSION.md` moves the shared guide into the
spellbook `guide` field, the server reads it from there instead.

## Exit proof

1. With the live Host, an MCP client lists the tools. `room_state` answers
   for the room that the launch directory names.
2. `recall` returns memories for that room.
3. A request with an unknown argument key is refused, the same as in OMP.
4. The excluded tools are absent from the list.
5. The `initialize` result carries the ORGANS text.
