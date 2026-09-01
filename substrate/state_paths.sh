# shellcheck shell=bash
# Where mutable substrate state lives. Mirrors
# crates/house-substrate/src/state.rs.
#
# Product code is immutable once installed, so nothing written at runtime may
# land inside it. The installed layout is <install-root>/the-athanor for product
# code and <install-root>/state/substrate for the dotenv and PostgreSQL dumps.
#
# ATHANOR_STATE_DIR names the state root explicitly and must be absolute.
# Without it there are exactly two acceptable structural answers: an installed
# tree whose <install-root>/state already exists, or a development checkout
# recognised by the source artefacts (Cargo.toml and crates/) a built product
# never ships. Anything else is a hard error — the state root never degrades to
# a path inside the immutable product tree.
#
# Sources set: SUBSTRATE_DIR, ATHANOR_ROOT, STATE_ROOT, SUBSTRATE_STATE_DIR.

SUBSTRATE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ATHANOR_ROOT="$(cd "$SUBSTRATE_DIR/.." && pwd)"

if [[ -n "${ATHANOR_STATE_DIR:-}" ]]; then
  if [[ "$ATHANOR_STATE_DIR" != /* ]]; then
    echo "ATHANOR_STATE_DIR must be an absolute path (got $ATHANOR_STATE_DIR)" >&2
    return 1 2>/dev/null || exit 1
  fi
  STATE_ROOT="$ATHANOR_STATE_DIR"
elif [[ -d "$(dirname "$ATHANOR_ROOT")/state" ]]; then
  STATE_ROOT="$(dirname "$ATHANOR_ROOT")/state"
elif [[ -f "$ATHANOR_ROOT/Cargo.toml" && -d "$ATHANOR_ROOT/crates" ]]; then
  STATE_ROOT="$ATHANOR_ROOT/state"
else
  echo "ATHANOR_STATE_DIR is not set and no state root could be resolved." >&2
  echo "Set ATHANOR_STATE_DIR to the absolute path of <install-root>/state." >&2
  return 1 2>/dev/null || exit 1
fi

SUBSTRATE_STATE_DIR="$STATE_ROOT/substrate"
SUBSTRATE_DOTENV="${ATHANOR_SUBSTRATE_DOTENV_PATH:-$SUBSTRATE_STATE_DIR/.env}"
