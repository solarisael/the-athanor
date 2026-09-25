import Base
import ./LAWS.bend as Laws
import ./wait.bend as W

def Laws.silence_keeps_waiting(n):
  match n:
    case 0n:
      {==}
    case 1n+p:
      Laws.silence_keeps_waiting(p)

def Laws.return_proceeds(n, rest):
  match n:
    case 0n:
      {==}
    case 1n+p:
      Laws.return_proceeds(p, rest)

def Laws.refusal_fails(n, rest):
  match n:
    case 0n:
      {==}
    case 1n+p:
      Laws.refusal_fails(p, rest)

def Laws.window_holds(n):
  match n:
    case 0n:
      {==}
    case 1n+p:
      Laws.window_holds(p)

def Laws.late_verify_proceeds(n, rest):
  match n:
    case 0n:
      {==}
    case 1n+p:
      Laws.late_verify_proceeds(p, rest)

def Laws.deadline_fails(n, rest):
  match n:
    case 0n:
      {==}
    case 1n+p:
      Laws.deadline_fails(p, rest)

def Laws.only_finish_or_deadline_fails(looks):
  match looks:
    case Nil{}:
      {==}
    case Con{look, rest}:
      match look:
        case W.Look{seen, passed}:
          match seen:
            case W.Verified{}:
              {==}
            case W.Finished{}:
              {==}
            case W.Undecided{}:
              match passed:
                case False{}:
                  Laws.only_finish_or_deadline_fails(rest)
                case True{}:
                  {==}
