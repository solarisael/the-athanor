"""Shared substrate path and PostgreSQL environment resolution.

The TypeScript callers launch these helpers from either Windows or WSL.  Keep
all path selection here so every helper agrees about the substrate: an explicit
``--substrate-dir``/``ATHANOR_SUBSTRATE_ROOT`` wins, otherwise the structural
``<athanor-root>/substrate`` directory inside the product tree is used.
"""
from __future__ import annotations

import os
import re
import shutil
import subprocess
from pathlib import Path
from typing import Mapping

PG_ENV_KEYS = ("PGHOST", "PGPORT", "PGUSER", "PGPASSWORD", "PGDATABASE")
_WINDOWS_DRIVE_RE = re.compile(r"^[A-Za-z]:[\\/]")


class SubstrateConfigError(ValueError):
    """Raised when an explicitly selected or default substrate is unusable."""


def windows_path_to_wsl(value: str | os.PathLike[str]) -> str:
    """Convert a Windows path for a POSIX/WSL Python process when necessary.

    ``wslpath`` is preferred because it handles UNC and mounted paths.  The
    drive-letter fallback keeps this deterministic in test environments and
    on minimal WSL installations where the command is unavailable.
    """
    raw = os.fspath(value)
    if os.name == "nt" or not _WINDOWS_DRIVE_RE.match(raw):
        return raw

    try:
        if shutil.which("wslpath"):
            converted = subprocess.run(
                ["wslpath", "-u", raw],
                check=True,
                capture_output=True,
                text=True,
                timeout=2,
            ).stdout.strip()
            if converted:
                return converted
    except (OSError, subprocess.SubprocessError):
        # The pure conversion below is sufficient for ordinary drive paths.
        pass

    drive = raw[0].lower()
    remainder = raw[2:].replace("\\", "/")
    return f"/mnt/{drive}{remainder}"


def _resolved_path(value: str | os.PathLike[str]) -> Path:
    converted = windows_path_to_wsl(value)
    return Path(converted).expanduser().resolve(strict=False)


#: `<athanor-root>/substrate` — the substrate assets shipped inside the product
#: tree. This module lives at `<athanor-root>/src`, so the answer is structural
#: and holds in a development checkout and an installed `<target>/the-athanor`
#: alike. No sibling checkout and no room directory participate in it.
DEFAULT_SUBSTRATE_DIR = Path(__file__).resolve().parent.parent / "substrate"


def resolve_substrate_dir(
    substrate_dir: str | os.PathLike[str] | None = None,
    *,
    environ: Mapping[str, str] | None = None,
) -> Path:
    """Resolve and validate the substrate directory.

    An explicit ``substrate_dir`` (the CLI flag) takes precedence over
    ``ATHANOR_SUBSTRATE_ROOT``.  A non-empty environment override is otherwise
    used.  With neither, the structural default ``<athanor-root>/substrate`` is
    used.

    Invalid explicit configuration never silently falls back to the default;
    callers that have a fail-open contract can catch ``SubstrateConfigError``.
    """
    env = os.environ if environ is None else environ
    configured = substrate_dir
    source = "--substrate-dir"
    if configured is None and "ATHANOR_SUBSTRATE_ROOT" in env:
        candidate = os.fspath(env["ATHANOR_SUBSTRATE_ROOT"]).strip()
        if candidate:
            configured = candidate
            source = "ATHANOR_SUBSTRATE_ROOT"

    if configured is not None:
        raw = os.fspath(configured).strip()
        if not raw:
            raise SubstrateConfigError(f"{source} must name a substrate directory")
        converted = windows_path_to_wsl(raw)
        if not Path(converted).is_absolute():
            raise SubstrateConfigError(f"{source} must be an absolute path")
        resolved = Path(converted).expanduser().resolve(strict=False)
    else:
        resolved = DEFAULT_SUBSTRATE_DIR.resolve(strict=False)

    if not resolved.is_dir():
        raise SubstrateConfigError(
            f"substrate directory does not exist or is not a directory: {resolved}"
        )
    return resolved


def read_env_file(path: str | os.PathLike[str]) -> dict[str, str]:
    """Read simple KEY=VALUE entries from a substrate ``.env`` file."""
    env_path = Path(path)
    values: dict[str, str] = {}
    try:
        lines = env_path.read_text(encoding="utf-8").splitlines()
    except OSError:
        return values

    for line in lines:
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        key = key.strip()
        if not key:
            continue
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[key] = value
    return values


def resolve_state_dotenv(
    substrate_dir: str | os.PathLike[str],
    *,
    environ: Mapping[str, str] | None = None,
) -> Path:
    """Resolve the mutable-state dotenv paired with a product substrate.

    Installed topology supplies ``ATHANOR_STATE_DIR``. Without it, source
    checkouts keep development state under ``<athanor-root>/state`` while an
    installed product keeps state beside ``<install-root>/the-athanor``.
    """
    env = os.environ if environ is None else environ
    configured = env.get("ATHANOR_STATE_DIR", "").strip()
    if configured:
        state_root = _resolved_path(configured)
    else:
        product_root = _resolved_path(substrate_dir).parent
        development = (product_root / "Cargo.toml").is_file() and (
            product_root / "crates"
        ).is_dir()
        state_root = product_root / "state" if development else product_root.parent / "state"
    return state_root / "substrate" / ".env"



def load_postgres_env(
    substrate_dir: str | os.PathLike[str],
    *,
    environ: Mapping[str, str] | None = None,
) -> dict[str, str]:
    """Load the mutable-state dotenv, with product dotenv as compatibility.

    The installed state file is authoritative when present. The old
    ``<product>/substrate/.env`` location remains a read-only compatibility
    fallback for development trees that have not moved their credentials yet.
    Explicit process PostgreSQL values always win.
    """
    env = os.environ if environ is None else environ
    state_dotenv = resolve_state_dotenv(substrate_dir, environ=env)
    product_dotenv = _resolved_path(substrate_dir) / ".env"
    values = read_env_file(state_dotenv if state_dotenv.is_file() else product_dotenv)
    for key in PG_ENV_KEYS:
        value = env.get(key)
        if value:
            values[key] = value
    return values


def substrate_env(
    substrate_dir: str | os.PathLike[str] | None = None,
) -> dict[str, str]:
    """Compatibility helper returning the resolved PostgreSQL environment."""
    return load_postgres_env(resolve_substrate_dir(substrate_dir))


# Short aliases for callers that prefer the noun used in the install contract.
resolve_substrate = resolve_substrate_dir
postgres_env = load_postgres_env

__all__ = [
    "PG_ENV_KEYS",
    "SubstrateConfigError",
    "load_postgres_env",
    "postgres_env",
    "read_env_file",
    "resolve_substrate",
    "resolve_state_dotenv",
    "resolve_substrate_dir",
    "substrate_env",
    "windows_path_to_wsl",
]
