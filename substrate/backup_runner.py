#!/usr/bin/env python3
"""Transport legacy Python writers to the Rust-owned backup operation."""
from __future__ import annotations

import os
import subprocess
import sys
from pathlib import Path

import state_paths

def _substrate_executable() -> Path:
    configured = os.environ.get("ATHANOR_SUBSTRATE_EXE", "").strip()
    return Path(configured) if configured else state_paths.release_binary()


def run_backup(anchor_file: str | Path, *, skip: bool = False) -> None:
    """Request a Rust backup without aborting the legacy writer on failure."""
    if skip:
        return

    executable = _substrate_executable()
    if not executable.is_file():
        print(f"  WARN: Rust substrate executable not found at {executable}, skipping backup", file=sys.stderr)
        return
    output_dir = Path(
        os.environ.get("SOLARISAEL_BACKUP_DIR", "").strip()
        or state_paths.substrate_state_dir() / "backups"
    )
    keep = os.environ.get("SOLARISAEL_BACKUP_KEEP", os.environ.get("KEEP", "3"))
    try:
        result = subprocess.run(
            [str(executable), "backup", "--output-dir", str(output_dir), "--keep", str(keep)],
            capture_output=True,
            text=True,
        )
    except OSError as err:
        print(f"  WARN: Rust backup command unavailable: {err}", file=sys.stderr)
        return

    if result.returncode != 0:
        print(
            f"  WARN: Rust backup failed (rc={result.returncode}): "
            f"{result.stderr.strip()}",
            file=sys.stderr,
        )
        return

    first_line = result.stdout.strip().splitlines()[0] if result.stdout.strip() else ""
    print(f"  backup: {first_line}")
