# The keeper's wait for the House, as a model small enough to prove.
#
# The keeper meets the House in two ways, and silence means the same in both:
# it is not a decision, so it never ends the keeper's wait by itself.
#
# A move (the status ask after an armed exit, the claim, each transition) waits
# through silence until the House answers. An answer lets the keeper proceed; a
# refusal is the House's decision and ends the loop. No window stands over a
# move, so nothing else ends one. `move` mirrors `keeper::until_answered`.
#
# The verify watch holds a relaunched omp inside the House's relaunching window.
# A verify proceeds; a finish without a verify fails the attempt; a read the
# House refused or could not answer decides nothing; the window's end fails the
# attempt. `watch` mirrors `keeper::watch_relaunched`.
#
# This certifies the decision order, not the Rust that carries it out.

import Base

type Verdict is Data:
  Proceed{}
  FailAttempt{}
  KeepWaiting{}

type Heard is Data:
  Answered{}
  Refused{}
  Unreachable{}

type Seen is Data:
  Verified{}
  Finished{}
  Undecided{}

type Look is Data:
  Look{seen: Seen, passed: Bool}

def move(heard: List<Heard>) -> Verdict:
  match heard:
    case Nil{}:
      KeepWaiting{}
    case Con{first, rest}:
      match first:
        case Answered{}:
          Proceed{}
        case Refused{}:
          FailAttempt{}
        case Unreachable{}:
          move(rest)

def watch(looks: List<Look>) -> Verdict:
  match looks:
    case Nil{}:
      KeepWaiting{}
    case Con{look, rest}:
      match look:
        case Look{seen, passed}:
          match seen:
            case Verified{}:
              Proceed{}
            case Finished{}:
              FailAttempt{}
            case Undecided{}:
              match passed:
                case False{}:
                  watch(rest)
                case True{}:
                  FailAttempt{}

# `n` asks the House could not answer, then `after`.
def silent(n: Nat, after: List<Heard>) -> List<Heard>:
  match n:
    case 0n:
      after
    case 1n+p:
      Con{Unreachable{}, silent(p, after)}

# `n` undecided reads inside the standing window, then `after`.
def undecided(n: Nat, after: List<Look>) -> List<Look>:
  match n:
    case 0n:
      after
    case 1n+p:
      Con{Look{Undecided{}, False{}}, undecided(p, after)}

# Does a finish without a verify, or the window's end, come before any verify?
def ruinous(looks: List<Look>) -> Bool:
  match looks:
    case Nil{}:
      False{}
    case Con{look, rest}:
      match look:
        case Look{seen, passed}:
          match seen:
            case Verified{}:
              False{}
            case Finished{}:
              True{}
            case Undecided{}:
              match passed:
                case False{}:
                  ruinous(rest)
                case True{}:
                  True{}

def is_fail(verdict: Verdict) -> Bool:
  match verdict:
    case Proceed{}:
      False{}
    case FailAttempt{}:
      True{}
    case KeepWaiting{}:
      False{}
