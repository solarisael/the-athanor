#!/usr/bin/env python3
"""Propose ranking-death cleanup without deleting memory rows.

The default pass is read-only. It reports likely stale state claims and dense
session sediment that may be compressed into an arc. A human may then add or
edit explicit ``SUPERSEDE old_id -> new_id`` and ``ARCHIVE memory_id`` lines in
the report and pass that file to ``--apply``. Only those line items are ever
written; all other prose is ignored.
"""
from __future__ import annotations

import argparse
import os
import re
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path

import psycopg2
import psycopg2.extras

import state_paths

STATE_TYPES = frozenset({
    "state", "status", "decision", "rule", "feedback", "project", "reference",
    "plan", "config", "current", "claim",
})
STATE_MARKERS = re.compile(
    r"\b(current|currently|status|enabled|disabled|active|inactive|latest|now|"
    r"decision|rule|uses|using|will|must|should|configured|canonical)\b",
    re.IGNORECASE,
)
SUPERSEDE_RE = re.compile(
    r"^\s*SUPERSEDE\s+(\d+)\s*->\s*(\d+)\s*(?:\|\s*(.*?))?\s*$",
    re.IGNORECASE,
)
ARCHIVE_RE = re.compile(
    r"^\s*ARCHIVE\s+(\d+)\s*(?:\|\s*(.*?))?\s*$",
    re.IGNORECASE,
)


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, value = line.partition("=")
        os.environ.setdefault(key.strip(), value.strip())


def detect_erasure_columns(conn) -> dict[str, bool]:
    """Feature-detect erasure columns so read-only scans survive older databases."""
    try:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT column_name
                FROM information_schema.columns
                WHERE table_schema = current_schema()
                  AND table_name IN ('memories', 'named_entities')
                  AND column_name IN ('superseded_by', 'archived_at', 'summary_as_of')
                """
            )
            found = {row[0] for row in cur.fetchall()}
    except Exception:
        conn.rollback()
        found = set()
    return {
        "superseded_by": "superseded_by" in found,
        "archived_at": "archived_at" in found,
        "summary_as_of": "summary_as_of" in found,
    }


def memory_erasure_select(columns: dict[str, bool]) -> str:
    archived = "m.archived_at" if columns["archived_at"] else "NULL::timestamptz"
    superseded = "m.superseded_by" if columns["superseded_by"] else "NULL::bigint"
    return f"{archived} AS archived_at, {superseded} AS superseded_by"


def memory_erasure_filter(columns: dict[str, bool]) -> str:
    filters = []
    if columns["archived_at"]:
        filters.append("m.archived_at IS NULL")
    if columns["superseded_by"]:
        filters.append("m.superseded_by IS NULL")
    return " AND ".join(filters) or "TRUE"


def connect_from_env() -> object:
    return psycopg2.connect(
        host=os.environ["PGHOST"],
        port=os.environ["PGPORT"],
        user=os.environ["PGUSER"],
        password=os.environ["PGPASSWORD"],
        dbname=os.environ["PGDATABASE"],
        connect_timeout=3,
    )


def fetch_memories(conn, rooms: list[str], columns: dict[str, bool]) -> list[dict]:
    sql = f"""
        SELECT m.id, m.room, m.type, m.date, m.title, m.source_path,
               m.body, m.threads, m.created_at, m.updated_at,
               {memory_erasure_select(columns)}
        FROM memories m
        WHERE m.room = ANY(%s)
          AND {memory_erasure_filter(columns)}
        ORDER BY m.created_at ASC NULLS LAST, m.id ASC
    """
    with conn.cursor(cursor_factory=psycopg2.extras.RealDictCursor) as cur:
        cur.execute(sql, (rooms,))
        return [dict(row) for row in cur.fetchall()]


def likely_state_claim(memory: dict) -> bool:
    kind = str(memory.get("type") or "").strip().lower()
    title = str(memory.get("title") or "")
    body = str(memory.get("body") or "")
    if kind in STATE_TYPES:
        return True
    # Session prose is story by default. Only a state-shaped title crosses the
    # boundary, keeping the proposal conservative.
    if kind == "session":
        return bool(STATE_MARKERS.search(title))
    return bool(STATE_MARKERS.search(f"{title}\n{body[:1200]}"))


def memory_label(memory: dict) -> str:
    title = str(memory.get("title") or memory.get("source_path") or "").strip()
    title = " ".join(title.split())
    return title[:180] or f"memory {memory['id']}"


def state_claim_proposals(memories: list[dict]) -> list[dict]:
    by_thread: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for memory in memories:
        for thread in memory.get("threads") or []:
            thread = str(thread).strip()
            if thread:
                by_thread[(str(memory.get("room") or ""), thread)].append(memory)

    proposals: dict[int, dict] = {}
    for (_room, thread), entries in by_thread.items():
        entries.sort(key=lambda item: (item.get("created_at") or datetime.min.replace(tzinfo=timezone.utc), item["id"]))
        for index, old in enumerate(entries[:-1]):
            if not likely_state_claim(old):
                continue
            newer = next(
                (candidate for candidate in reversed(entries[index + 1:]) if likely_state_claim(candidate)),
                None,
            )
            if newer is None or newer["id"] == old["id"]:
                continue
            current = proposals.get(old["id"])
            if current is None or newer["id"] > current["new_id"]:
                proposals[old["id"]] = {
                    "old_id": old["id"],
                    "new_id": newer["id"],
                    "thread": thread,
                    "old_label": memory_label(old),
                    "new_label": memory_label(newer),
                }
    return sorted(proposals.values(), key=lambda item: (item["old_id"], item["new_id"]))


def sediment_clusters(memories: list[dict], window_days: int, min_count: int) -> list[dict]:
    now = datetime.now(timezone.utc)
    since = now - timedelta(days=max(1, window_days))
    groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for memory in memories:
        if str(memory.get("type") or "").lower() != "session":
            continue
        created = memory.get("created_at")
        if created is not None and created.tzinfo is None:
            created = created.replace(tzinfo=timezone.utc)
        if created is not None and created < since:
            continue
        for thread in memory.get("threads") or []:
            thread = str(thread).strip()
            if thread:
                groups[(str(memory.get("room") or ""), thread)].append(memory)

    clusters = []
    for (room, thread), entries in groups.items():
        if len(entries) < min_count:
            continue
        entries.sort(key=lambda item: (item.get("created_at") or datetime.min.replace(tzinfo=timezone.utc), item["id"]))
        clusters.append({
            "room": room,
            "thread": thread,
            "ids": [item["id"] for item in entries],
            "dates": sorted({str(item["date"]) for item in entries if item.get("date")}),
            "count": len(entries),
        })
    return sorted(clusters, key=lambda item: (-item["count"], item["room"], item["thread"]))


def build_report(
    memories: list[dict],
    columns: dict[str, bool],
    state_proposals: list[dict],
    clusters: list[dict],
    window_days: int,
    min_count: int,
) -> str:
    lines = [
        "The Athanor digest pass",
        f"Generated: {datetime.now(timezone.utc).isoformat()}",
        "Mode: read-only proposal; no rows were changed.",
        "",
        "Rule: state claims may be superseded; session/story sediment is an arc-compression candidate.",
        "Erasure columns: "
        f"superseded_by={'yes' if columns['superseded_by'] else 'missing'}, "
        f"archived_at={'yes' if columns['archived_at'] else 'missing'}.",
        "",
        f"STALE STATE CLAIMS ({len(state_proposals)}):",
    ]
    if state_proposals:
        for item in state_proposals:
            lines.append(
                f"SUPERSEDE {item['old_id']} -> {item['new_id']} | "
                f"shared thread {item['thread']!r}; {item['old_label']!r} is older than {item['new_label']!r}"
            )
    else:
        lines.append("(none)")

    lines.extend([
        "",
        f"SESSION SEDIMENT ({len(clusters)} clusters; window={window_days}d, minimum={min_count}):",
    ])
    if clusters:
        for cluster in clusters:
            dates = ", ".join(cluster["dates"]) or "undated"
            ids = ", ".join(str(memory_id) for memory_id in cluster["ids"])
            lines.append(
                f"ARC-CANDIDATE room={cluster['room']!r} thread={cluster['thread']!r} "
                f"count={cluster['count']} dates={dates} ids=[{ids}] | proposal only; compress before archiving"
            )
    else:
        lines.append("(none)")

    lines.extend([
        "",
        "Apply format (only these explicit lines are actionable):",
        "  SUPERSEDE <older_id> -> <newer_id> | reason",
        "  ARCHIVE <memory_id> | reason",
        "Everything else is report prose and is ignored by --apply.",
    ])
    return "\n".join(lines) + "\n"


def parse_actions(path: Path) -> tuple[list[tuple[int, int, str]], list[tuple[int, str]]]:
    supersedes: list[tuple[int, int, str]] = []
    archives: list[tuple[int, str]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        match = SUPERSEDE_RE.match(line)
        if match:
            old_id, new_id = int(match.group(1)), int(match.group(2))
            if old_id > 0 and new_id > 0 and old_id != new_id:
                supersedes.append((old_id, new_id, (match.group(3) or "explicit proposal").strip()))
            continue
        match = ARCHIVE_RE.match(line)
        if match:
            memory_id = int(match.group(1))
            if memory_id > 0:
                archives.append((memory_id, (match.group(2) or "explicit proposal").strip()))
    return supersedes, archives


def apply_actions(conn, columns: dict[str, bool], supersedes, archives) -> None:
    if (supersedes and not columns["superseded_by"]) or (archives and not columns["archived_at"]):
        raise RuntimeError("superseded_by and archived_at columns are required before applying erasure proposals")
    print("APPLY PLAN")
    for old_id, new_id, reason in supersedes:
        print(f"  SUPERSEDE {old_id} -> {new_id} | {reason}")
    for memory_id, reason in archives:
        print(f"  ARCHIVE {memory_id} | {reason}")
    if not supersedes and not archives:
        print("  (no explicit SUPERSEDE or ARCHIVE line items)")
        return

    with conn, conn.cursor() as cur:
        for old_id, new_id, _reason in supersedes:
            cur.execute(
                "UPDATE memories SET superseded_by = %s WHERE id = %s AND id <> %s",
                (new_id, old_id, new_id),
            )
            print(f"  updated superseded_by on {old_id}: {cur.rowcount} row(s)")
        for memory_id, _reason in archives:
            cur.execute(
                "UPDATE memories SET archived_at = COALESCE(archived_at, NOW()) WHERE id = %s",
                (memory_id,),
            )
            print(f"  set archived_at on {memory_id}: {cur.rowcount} row(s)")


def main() -> int:
    parser = argparse.ArgumentParser(description="Propose/apply non-destructive memory digestion.")
    parser.add_argument("--room", action="append", default=[], help="room to scan (repeatable; defaults to every room in memories)")
    parser.add_argument("--window-days", type=int, default=14, help="session sediment date window (default: 14)")
    parser.add_argument("--min-cluster-size", type=int, default=3, help="minimum same-thread session count (default: 3)")
    parser.add_argument("--out", type=Path, help="also write the human-readable proposal report here")
    parser.add_argument("--apply", type=Path, help="apply only explicit SUPERSEDE/ARCHIVE lines from this proposal file")
    parser.add_argument("--env-file", type=Path, default=state_paths.default_dotenv_path())
    args = parser.parse_args()

    load_dotenv(args.env_file)
    conn = connect_from_env()
    rooms = list(args.room)
    if not rooms:
        with conn.cursor() as cur:
            cur.execute("SELECT DISTINCT room FROM memories ORDER BY room")
            rooms = [row[0] for row in cur.fetchall()]
    try:
        columns = detect_erasure_columns(conn)
        if args.apply:
            supersedes, archives = parse_actions(args.apply)
            apply_actions(conn, columns, supersedes, archives)
            return 0

        memories = fetch_memories(conn, rooms, columns)
        proposals = state_claim_proposals(memories)
        clusters = sediment_clusters(memories, args.window_days, args.min_cluster_size)
        report = build_report(memories, columns, proposals, clusters, args.window_days, args.min_cluster_size)
        print(report, end="")
        if args.out:
            args.out.write_text(report, encoding="utf-8")
            print(f"Wrote proposal report: {args.out}")
        return 0
    finally:
        conn.close()


if __name__ == "__main__":
    raise SystemExit(main())
