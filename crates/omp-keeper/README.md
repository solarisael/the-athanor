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
6. Stop when the House refuses. A storm-guard refusal ends the loop with a plain
   message that says omp is not running.

## The config file

The keeper reads `omp-keeper.json` beside its own program file. The option
`--config <path>` selects a different file. The file holds one JSON object.

| Field | Need | Meaning |
| --- | --- | --- |
| `ompLaunch` | yes | The literal omp command. The first entry is the program. |
| `workspace` | yes | The omp workspace path. The keeper asks about this workspace and starts omp in it. |
| `programRoot` | yes | The installed Athanor program root that holds `current.json`. |
| `capabilityPath` | one of two | The file that holds the keeper capability secret. |
| `capability` | one of two | The keeper capability secret, in the file itself. |
| `claimant` | no | The keeper principal name. Default `omp-keeper`. |
| `watchIntervalSecs` | no | Seconds between status asks while omp runs. Default 30. Use 0 to stop the asks. |

Example:

```json
{
  "ompLaunch": ["C:/Users/Sol/AppData/Roaming/npm/omp.cmd", "--resume"],
  "workspace": "C:/Solarisael/Obsidian/obsidian/kodo",
  "programRoot": "C:/Program Files/The Athanor",
  "capabilityPath": "C:/ProgramData/The Athanor/restart-keeper.capability",
  "watchIntervalSecs": 30
}
```

Give exactly one capability field. The operator provisions the secret into the
row `restart.principal_capabilities`, principal `omp-keeper`, operation class
`restart_claim`. The keeper reads the secret at claim time. The secret stays out
of every log line, every error message, and every debug print.

Windows starts a program file, not a shell word. Write the full path with its
extension in `ompLaunch`, or start the shell yourself, for example
`["C:/Windows/System32/cmd.exe", "/c", "omp"]`.

The `claimant` name must be a lowercase slug. This shape is the shape the
substrate accepts for a principal name.

## The handshake

The keeper speaks the substrate protocol over stdin and stdout. Every request is
one line: `{"protocol":1,"id":"1","method":"restart_status","params":{...}}`.
Every answer is one line that holds `result` or `error`.

| Method | When | Answer the keeper uses |
| --- | --- | --- |
| `restart_status` | after every child exit, and on every watch tick | the pending intent, or none |
| `restart_claim` | intent in `requested` or `exiting` | the claim token and the claim epoch |
| `restart_transition` | after the claim | the new state |

The state order is `requested -> exiting -> claimed -> relaunching -> verified`.
The adapter writes `exiting` without a token. The keeper claims from `exiting`,
or from `requested` when a crash left the adapter no time to arm. The keeper
transitions to `relaunching` with its claim token. A transition to `failed` also
carries the token and is legal from `claimed` or `relaunching`. The successor
session writes `verified`; the keeper never does.

Deadlines and refusals:

- The keeper kills the omp child only when the intent says `exiting` and the
  deadline passed. Default deadline: 60 seconds.
- A relaunch that cannot start retries one time. A second failure transitions the
  intent to `failed` and stops the keeper.
- The idempotency key for a claim is `<claimant>:claim:<intentId>`. A repeated
  claim for one intent carries the same key.
- `resume` mode and `fresh` mode start the same command. omp resumes its own
  sessions. The House owns the paper-boat condition for `fresh` mode.

## The ceilings

- `# enough:` one keeper for each harness kind. One keeper process owns one omp
  child. Two keepers on one workspace fight for one intent.
- `# enough:` one request in flight. The keeper matches every answer to its own
  request id and never sends a second request first.
- `# enough:` one substrate child for each ask. The keeper starts the substrate,
  asks, and closes it. This keeps the resolution fresh and costs one process
  start for each ask.
- The storm guard belongs to the House. The keeper holds no local restart count.
- The keeper measures the `exiting` deadline from its own first sight of the
  state, because `restart_status` reports stage seconds, not the instant the
  intent entered the state.

## Tests

```
cargo test -p omp-keeper
```

The unit tests cover the config file, the substrate resolution in temporary
directories, and the decision functions. The smoke tests start the real keeper
program with two fixtures: `examples/fake_omp.rs` exits 87, and
`examples/fake_substrate.rs` answers canned JSONL. The smoke tests need no
database.
