# Law 2 of quest 50c26913: an unreachable House means wait, never exit; only a
# refusal or the deadline fails the attempt.

import Base
import ./wait.bend as W

# A move: silence never ends the wait.
law silence_keeps_waiting:
  for n: Nat
  {W.move(W.silent(n, Nil{})) == W.KeepWaiting{} : W.Verdict}

# A move: however long the House was away, its answer lets the keeper proceed.
law return_proceeds:
  for n: Nat
  for rest: List<W.Heard>
  {W.move(W.silent(n, Con{W.Answered{}, rest})) == W.Proceed{} : W.Verdict}

# A move: a refusal ends the loop, however long the silence before it.
law refusal_fails:
  for n: Nat
  for rest: List<W.Heard>
  {W.move(W.silent(n, Con{W.Refused{}, rest})) == W.FailAttempt{} : W.Verdict}

# The watch: undecided reads inside the window never end the wait.
law window_holds:
  for n: Nat
  {W.watch(W.undecided(n, Nil{})) == W.KeepWaiting{} : W.Verdict}

# The watch: a verify after any number of undecided reads proceeds.
law late_verify_proceeds:
  for n: Nat
  for rest: List<W.Look>
  {W.watch(W.undecided(n, Con{W.Look{W.Verified{}, False{}}, rest})) == W.Proceed{} : W.Verdict}

# The watch: the window's end fails the attempt.
law deadline_fails:
  for n: Nat
  for rest: List<W.Look>
  {W.watch(W.undecided(n, Con{W.Look{W.Undecided{}, True{}}, rest})) == W.FailAttempt{} : W.Verdict}

# The watch: the attempt fails exactly when a finish without a verify, or the
# window's end, comes before any verify. Nothing else fails it.
law only_finish_or_deadline_fails:
  for looks: List<W.Look>
  {W.is_fail(W.watch(looks)) == W.ruinous(looks) : Bool}
