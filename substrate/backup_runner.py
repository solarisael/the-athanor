#!/usr/bin/env python3
"""Shared backup runner for substrate write helpers."""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def _backup_command(script: Path) -> list[str] | None:
    if sys.platform != "win32":
        return ["bash", str(script)]

    win_path_forward = str(script).replace("\\", "/")
    try:
        translated = subprocess.run(
            ["wsl.exe", "--", "wslpath", "-a", win_path_forward],
            capture_output=True,
            text=True,
        )
    except FileNotFoundError as err:
        print(f"  WARN: WSL is unavailable: {err}", file=sys.stderr)
        return None
    if translated.returncode != 0:
        print(
            f"  WARN: wslpath failed (rc={translated.returncode}): "
            f"{translated.stderr.strip()}",
            file=sys.stderr,
        )
        return None

    return ["wsl.exe", "--", "bash", translated.stdout.strip()]


def run_backup(anchor_file: str | Path, *, skip: bool = False) -> None:
    """Run backup.sh next to anchor_file without aborting the caller on failure."""
    if skip:
        return

    script = Path(anchor_file).resolve().parent / "backup.sh"
    if not script.exists():
        print(f"  WARN: backup.sh not found at {script}, skipping", file=sys.stderr)
        return

    command = _backup_command(script)
    if command is None:
        return

    try:
        result = subprocess.run(command, capture_output=True, text=True)
    except FileNotFoundError as err:
        print(f"  WARN: backup command unavailable: {err}", file=sys.stderr)
        return

    if result.returncode != 0:
        print(
            f"  WARN: backup failed (rc={result.returncode}): "
            f"{result.stderr.strip()}",
            file=sys.stderr,
        )
        return

    first_line = result.stdout.strip().splitlines()[0] if result.stdout.strip() else ""
    print(f"  backup: {first_line}")
