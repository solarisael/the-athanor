# omp-keeper

The keeper owns the console seam. It starts omp as a child, waits for the exit,
asks the House for a restart intent, and starts omp again. No service can do
this work, because the terminal belongs to the operator, not to a service.

## What the keeper does

1. Start omp with the exact command from the config file. The child inherits the
   console. The keeper never detaches the child and never hides its window.
2. Wait for the child exit. Exit code 87 means "armed exit, restart me". The code
   is only a hint. The keeper asks `restart_status` after every exit code.
3. Resolve the current substrate program. Read `<programRoot>/current.json`, then
   use `versions/<version>/bin/athanor-substrate.exe`. The keeper resolves this
   path again for every ask, so a release change during a session takes effect.
4. Start the substrate as a child and speak newline-delimited JSON to it.
5. Read the answer:
   - No pending intent: print one line and exit.
   - Pending intent in `requested` or `exiting`: claim it, transition it to
     `relaunching`, then start omp again with the same command.
   - Pending intent in another state: print one line and exit. Only `requested`
     and `exiting` accept a claim.
6. Hold the relaunch open until the House confirms the successor. The successor
   proves itself with `restart_verify`. The keeper then asks `restart_status`
   about its own intent by id, because only that read reports a terminal state.
   One answer is proof: our own intent id, reported `verified`. Anything else is
   not. A successor that is never confirmed inside the intent's relaunching
   deadline is killed, and the attempt counts as failed.
7. Stop when the House refuses. A storm-guard refusal ends the loop with a plain
   message that says omp is not running.

## The config file

The keeper reads `omp-keeper.json` beside its own program file. The option
`--config <path>` selects a different file. The file holds one JSON object.

| Field | Need | Meaning |
| --- | --- | --- |
| `ompLaunch` | yes | The literal omp command. The first entry is the program. |
| `workspace` | yes | The omp workspace path. The keeper asks about this workspace and starts omp in it. |
| `programRoot` | yes | The installed Athanor program root that holds `current.json`. |
| `stateRoot` | yes | The Athanor state root. The keeper exports it to every substrate child. |
| `capabilityPath` | one of two | The file that holds the keeper capability secret. |
| `capability` | one of two | The keeper capability secret, in the file itself. |
| `claimant` | no | The keeper principal name. Default `omp-keeper`. |
| `watchIntervalSecs` | no | Seconds between status asks while omp runs. Default 30. Use 0 to stop the asks. |

Example:

```json
{
  "ompLaunch": ["C:/Users/Administrador/.bun/bin/omp.exe"],
  "workspace": "C:/Solarisael/Obsidian/obsidian/kodo",
  "programRoot": "C:/Program Files/Solarisael/Athanor",
  "stateRoot": "C:/Solarisael/Obsidian/obsidian/house/state",
  "capabilityPath": "C:/Solarisael/Obsidian/obsidian/kodo/.omp/runtime/restart-capability",
  "watchIntervalSecs": 30
}
```

For a local House, deploy the release first. Then provision one room:

```powershell
pwsh crates/omp-keeper/scripts/provision-local.ps1 `
  -RoomDir C:/Solarisael/Obsidian/obsidian/kodo `
  -OmpProgram C:/Users/Administrador/AppData/Roaming/npm/omp.cmd
```

The script provisions four operation secrets. It writes `omp-keeper.json` in
the room runtime directory. Start the printed keeper command instead of `omp`.

An installed release carries the same door in its keeper component:

```powershell
$root = "$env:ProgramFiles/Solarisael/Athanor"
$version = (Get-Content "$root/current.json" -Raw | ConvertFrom-Json).version
pwsh "$root/versions/$version/components/omp-keeper/provision-omp-keeper.ps1" `
  -RoomDir C:/Solarisael/Obsidian/obsidian/kodo `
  -OmpProgram C:/Users/Administrador/.bun/bin/omp.exe
```

Give exactly one capability field. The operator provisions the secret into the
row `restart.principal_capabilities`, principal `omp-keeper`, operation class
`restart_claim`. The keeper reads the secret at claim time. The secret stays out
of every log line, every error message, and every debug print.

Name the program file with its extension in `ompLaunch`, not a bare shell word:
the keeper starts a program, and no PATHEXT search happens for it. A `.cmd` or
`.bat` shim — which is what an npm-installed `omp` is — is a valid entry and
needs no shell of your own. Rust's process spawn hands a shim to the command
processor for you and escapes the arguments for it, so a path with spaces and
trailing flags both survive. `crates/omp-keeper/tests/smoke.rs` proves that with
a real `.cmd` in a directory whose name has a space.

The `claimant` name must be a lowercase slug. This shape is the shape the
substrate accepts for a principal name.

## The handshake

The keeper speaks the substrate protocol over stdin and stdout. Every request is
one line: `{"protocol":1,"id":"1","method":"restart_status","params":{...}}`.
Every answer is one line that holds `result` or `error`.

| Method | When | Answer the keeper uses |
| --- | --- | --- |
| `restart_status`, workspace only | after every child exit, and on every watch tick | the pending intent, or none |
| `restart_status`, with `intentId` | on every verify watch poll | that one intent, in whatever state it reached |
| `restart_claim` | intent in `requested` or `exiting` | the claim token and the claim epoch |
| `restart_transition` | after the claim | the new state |

The state order is `requested -> exiting -> claimed -> relaunching -> verified`.
The adapter writes `exiting` without a token. The keeper claims from `exiting`,
or from `requested` when a crash left the adapter no time to arm. The keeper
transitions to `relaunching` with its claim token. A transition to `failed` also
carries the token and is legal from `claimed` or `relaunching`. The successor
session writes `verified`; the keeper never does.

Deadlines and refusals:

- Deadlines are instants, not stopwatches. `restart_status` publishes
  `exitingDeadlineAt` and `relaunchingDeadlineAt` as absolute RFC3339 times, and
  the keeper obeys those. A keeper that starts after the adapter already armed is
  therefore late on its first look, and acts on it.
- The keeper kills the omp child only when the intent says `exiting` and the
  published instant has passed. The contract's stage length is 60 seconds; the
  keeper's own 60 is only a net for an answer that carries no instant at all.
- A relaunch attempt fails three ways: omp will not start, omp starts and the
  House never confirms it before `relaunchingDeadlineAt`, or the keeper loses the
  House after the spawn. Every one of them kills the child, retries one time, and
  transitions the intent to `failed` on the second failure.
- Each attempt enters `relaunching` again, because the intent row counts
  `relaunch_attempts` and mints a fresh `relaunchingDeadlineAt` on every
  `relaunching` transition. The retry runs inside the House's new window; the
  keeper never opens a second window of its own.
- The relaunching window has no net. When a window read fails or carries no
  instant, the deadline the House last published stands. Where the House has
  never named one, there is no window to wait inside and the attempt ends. A
  window the keeper invents is time the House never granted.
- The idempotency key for a claim is `<claimant>:claim:<intentId>`. A repeated
  claim for one intent carries the same key.
- `ompLaunch` is the base command and carries no session selector.
- `resume` appends `--resume <sessionId>` from the intent.
- `fresh` adds no selector after the adapter confirms a paper boat.

## The ceilings

- `# enough:` one keeper for each harness kind. One keeper process owns one omp
  child. Two keepers on one workspace fight for one intent.
- `# enough:` one request in flight. The keeper matches every answer to its own
  request id and never sends a second request first.
- `# enough:` one substrate child for each ask. The keeper starts the substrate,
  asks, and closes it. This keeps the resolution fresh and costs one process
  start for each ask.
- The storm guard belongs to the House. The keeper holds no local restart count.
  It answers a `restart_storm` refusal, wherever in the loop it arrives, with one
  operator sentence and no retry.
- The keeper holds no stage clock. Every deadline it obeys is an instant read off
  the intent; `src/clock.rs` is the only place that parses one and asks whether it
  has passed.

## Tests

```
cargo test -p omp-keeper
```

The unit tests cover the config file, the substrate resolution in temporary
directories, the House clock, and the decision functions. The smoke tests start
the real keeper program against two fixtures and need no database:
`examples/fake_omp.rs` is the child (it arms an exit, or overstays on demand),
and `examples/fake_substrate.rs` answers the wire. That fixture builds every
answer from a real `house_protocol::restart` struct and validates every request
with the real door's own `validate()`, so a request the House would refuse is
refused in the smoke too.
