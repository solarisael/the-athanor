#!/usr/bin/env python3
"""Where mutable substrate state lives.

Product code is immutable once installed, so nothing the substrate writes at
runtime may land inside it. The installed layout is `<install-root>/the-athanor`
for product code, `<install-root>/rooms` for rooms, and
`<install-root>/state/substrate` for the dotenv and the PostgreSQL dumps.

`ATHANOR_STATE_DIR` names the Athanor state root explicitly and is the contract
an installer is expected to set. It must be absolute.

Without it there are exactly two acceptable structural answers, and neither is a
guess:

* an installed tree whose `<install-root>/state` already exists, i.e. the
  product path can actually reach the state root;
* a development checkout, recognised by the source artefacts (`Cargo.toml` and
  `crates/`) that a built product never ships.

Anything else raises. In particular the state root never degrades to a path
inside the immutable product tree just because `<install-root>/state` has not
been created yet — that would write mutable state into product code and only
surface much later.
"""
from __future__ import annotations

import os
import re
from pathlib import Path

#: These helpers run on the Windows host *and* inside WSL, so "absolute" cannot
#: mean whatever the current interpreter's platform happens to think. A drive
#: path, a UNC path, and a POSIX path are all absolute here — the same rule the
#: adapter applies in solarisael-house-proof/substrate.ts.
_ABSOLUTE_RE = re.compile(r"^(?:[A-Za-z]:[\\/]|\\\\|/)")


def _is_absolute(value: str) -> bool:
    return bool(_ABSOLUTE_RE.match(value))


STATE_DIR = "ATHANOR_STATE_DIR"

#: `<athanor-root>/substrate` — the immutable substrate assets.
SUBSTRATE_DIR = Path(__file__).resolve().parent
#: `<athanor-root>` — the immutable product checkout.
ATHANOR_ROOT = SUBSTRATE_DIR.parent


class StateRootError(RuntimeError):
    """The Athanor state root could not be resolved."""


def _is_development_checkout(root: Path) -> bool:
    """True when `root` is a source checkout rather than an installed product.

    A built product ships binaries, substrate assets, adapters, and docs. It does
    not ship the Cargo workspace manifest or the `crates/` source tree, so their
    presence is what separates a development run from an installed one.
    """
    return (root / "Cargo.toml").is_file() and (root / "crates").is_dir()


def resolve_state_root() -> tuple[Path, str]:
    """The Athanor state root and how it was decided.

    Raises `StateRootError` when no explicit, installed, or development answer
    is available, rather than inventing one.
    """
    configured = os.environ.get(STATE_DIR, "").strip()
    if configured:
        if not _is_absolute(configured):
            raise StateRootError(
                f"{STATE_DIR} must be an absolute path (got {configured})"
            )
        return Path(configured), "environment"

    installed = ATHANOR_ROOT.parent / "state"
    if installed.is_dir():
        return installed, "installed_tree"

    if _is_development_checkout(ATHANOR_ROOT):
        return ATHANOR_ROOT / "state", "development_checkout"

    raise StateRootError(
        f"{STATE_DIR} is not set and no state root could be resolved. An installed "
        f"Athanor must be told where its mutable state lives; set {STATE_DIR} to the "
        "absolute path of <install-root>/state."
    )


def state_root() -> Path:
    """The Athanor state root. See `resolve_state_root` for the reason."""
    return resolve_state_root()[0]


def substrate_state_dir() -> Path:
    """Mutable state owned by the substrate: `<state-root>/substrate`."""
    return state_root() / "substrate"


def default_dotenv_path() -> Path:
    """The dotenv every substrate script reads unless `--env-file` says otherwise."""
    return substrate_state_dir() / ".env"


def default_backup_dir() -> Path:
    """Where pg_dump archives land, honouring `SOLARISAEL_BACKUP_DIR`."""
    override = os.environ.get("SOLARISAEL_BACKUP_DIR", "").strip()
    return Path(override) if override else substrate_state_dir() / "backups"


def release_binary() -> Path:
    """The substrate executable produced by the root Cargo workspace."""
    release = ATHANOR_ROOT / "target" / "release"
    for name in ("athanor-substrate.exe", "athanor-substrate"):
        if (release / name).is_file():
            return release / name
    return release / "athanor-substrate"
