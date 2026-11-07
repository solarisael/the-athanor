# State machines

This folder holds one contract file for each state machine in the Athanor.
A state machine is a stored or long-lived value that decides which operation is legal next.

The first census ran on 2026-11-06.
The raw census reports are outside the repository, in `C:/Projects/the-athanor-dev/census-2026-11-06-state-machines/`.

## Keep these files current

A change to a machine's states, transition writers, or rules updates its file in the same commit.
A finding that you cannot confirm stays marked `unverified`.
When a defect is fixed, move it from "Defects" to "Rules" with the commit that fixed it.

## Status values

- `census`: the file records the census. Nobody has ruled on it.
- `ruled`: Sol ruled on the open questions.
- `modeled`: a Bend2 model proves the rules.
- `hardened`: the code has one transition owner, and a test compares it with the model.

## Machines

The order is by risk. Persisted and cross-process machines come first.

| # | Machine | File | Layers | Status |
|---|---|---|---|---|
| 1 | Chat say lifecycle | [chat-say.md](chat-say.md) | GUI, Host, adapter | census |
| 2 | Hallway knock | [hallway-knock.md](hallway-knock.md) | SQL, origami, Host, adapter | census |
| 3 | Restart | [restart.md](restart.md) | SQL, akasha, Host, adapter | census |
| 4 | Docket | [docket.md](docket.md) | SQL, akasha | census |
| 5 | GIGA queue and review | [giga.md](giga.md) | SQL, hearth, akasha | census |
| 6 | Memory and canon supersession | [memory-supersession.md](memory-supersession.md) | SQL, akasha | census |
| 7 | Install `current.json` | [install-current.md](install-current.md) | athanor-install | census |
| 8 | Worker lifecycle | [worker-lifecycle.md](worker-lifecycle.md) | SQL, hearth, akasha, adapter | census |
| 9 | Medium and low machines | [minor.md](minor.md) | all | census |

## File shape

Each machine file has these sections, in this order:

1. **What it is**: one paragraph.
2. **States**: each layer's declaration, with `file:line`.
3. **Transitions today**: every writer, with `file:line`.
4. **Rules**: numbered invariants `L1`, `L2`, and so on, each marked `holds` or `broken`, with evidence.
5. **Defects**: illegal moves and illegal combinations that the code permits today.
6. **Target shape**: the single transition owner and the Bend2 scope.
7. **Open rulings**: the decisions that Sol must make.

## Cross-layer finding

No Postgres trigger guards a state transition.
CHECK constraints limit the values of a column; they do not limit the moves between values.
Rust code owns every transition rule, and some writers use string literals instead of the enum.

## Reliability order

Most defects have one of two shapes:

- **A wait with no exit.** A machine enters a waiting state, and no deadline or owner moves it out.
- **Two copies that disagree.** Two processes keep their own copy of one state, and the copies drift.

Fix order, by harm that Sol has seen:

1. Waits with no exit: chat-say D1, D2; hallway-knock D1, D2, D4; restart D2; docket submitted-without-exit; worker-lifecycle D2.
2. Loss of classification or authority: giga D2 (purge retires turn identities), memory-supersession cycles.
3. Copies that disagree: worker-lifecycle adapter replay, chat-say D3, install-current D1 and D6.
4. SQL transition guards for each persisted machine, after its Rust owner exists.
5. The `minor.md` machines.
