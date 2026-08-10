#!/usr/bin/env python3
"""Direct write to solarisael_memory.memories. The autonomy primitive.

Bypasses markdown-on-disk — for substrate-only entries (chat logs, ephemeral
notes, things that don't deserve a markdown twin). For markdown-backed entries,
edit the .md file and re-run import_markdown.py.

Usage examples:
    python3 record_memory.py --room room-key --type feedback \\
        --title "Rule: precise names" --source-path "db-only/precise_names.md" \\
        --body-file /tmp/body.md \\
        --thread "naming / precision / application" \\
        --meta-kv origin=direct-db-write --meta-bool no_markdown_twin=true

    echo "**Decision:** ..." | python3 record_memory.py --room room-key --type session \\
        --title "Architecture decision" --source-path "db-only/architecture_decision.md" \\
        --body-stdin --thread "architecture / decision / reason"
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from datetime import date, datetime, timezone
from pathlib import Path

import psycopg2

from backup_runner import run_backup

import state_paths

# Date extraction for the initial schema's `dates` array column.
# Matches any YYYY-MM-DD anywhere + the stitched cross-midnight pattern
# YYYY-MM-DD_DD_ where the second DD shares the year-month prefix.
_DATE_RE = re.compile(r"(\d{4})-(\d{2})-(\d{2})")
_STITCHED_RE = re.compile(r"(\d{4})-(\d{2})-(\d{2})_(\d{2})_")


def derive_dates(source_path: str, primary_date: date, also_dates: list[str]) -> list[date]:
    """Return sorted unique list of date objects to store in memories.dates.

    Includes:
      - primary_date (the --date arg / today)
      - every YYYY-MM-DD substring in source_path
      - stitched-date expansion: YYYY-MM-DD_DD_ → both days share year+month
      - explicit --also-date entries (parsed YYYY-MM-DD)
    """
    found: set[date] = set()
    if primary_date:
        found.add(primary_date)
    for token in also_dates or []:
        try:
            found.add(date.fromisoformat(token))
        except ValueError:
            sys.exit(f"--also-date {token!r} is not a valid YYYY-MM-DD")
    sp = source_path or ""
    for m in _DATE_RE.finditer(sp):
        try:
            found.add(date(int(m.group(1)), int(m.group(2)), int(m.group(3))))
        except ValueError:
            continue
    for m in _STITCHED_RE.finditer(sp):
        y, mo, d1, d2 = (int(m.group(1)), int(m.group(2)),
                         int(m.group(3)), int(m.group(4)))
        for d in (d1, d2):
            try:
                found.add(date(y, mo, d))
            except ValueError:
                continue
    return sorted(found)


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, _, v = line.partition("=")
        os.environ.setdefault(k.strip(), v.strip())


def parse_meta(kv_args, bool_args) -> dict:
    meta: dict = {}
    for kv in kv_args or []:
        k, _, v = kv.partition("=")
        meta[k] = v
    for kv in bool_args or []:
        k, _, v = kv.partition("=")
        meta[k] = v.lower() in {"true", "1", "yes"}
    meta.setdefault("origin", "direct-db-write")
    meta["recorded_at"] = datetime.now(timezone.utc).isoformat()
    return meta


def normalize_threads(values: list[str]) -> list[str]:
    threads: list[str] = []
    seen: set[str] = set()
    for value in values:
        thread = value.strip()
        if not thread:
            sys.exit("--thread values must be nonblank")
        if thread not in seen:
            seen.add(thread)
            threads.append(thread)
    return threads


def parse_continues(values: list[str] | None, threads: list[str]) -> list[dict]:
    continuations: list[dict] = []
    seen: set[str] = set()
    available = set(threads)
    for raw in values or []:
        try:
            continuation = json.loads(raw)
        except json.JSONDecodeError as exc:
            sys.exit(f"--continues must be valid JSON: {exc.msg}")
        if not isinstance(continuation, dict):
            sys.exit("--continues must be a JSON object")
        thread_value = continuation.get("thread")
        thread = thread_value.strip() if isinstance(thread_value, str) else ""
        if not thread:
            sys.exit("--continues thread must be nonblank")
        previous_value = continuation.get("previousMemoryId")
        if isinstance(previous_value, bool):
            sys.exit("--continues previousMemoryId must be a positive decimal integer")
        if isinstance(previous_value, int):
            previous_id = previous_value
        elif isinstance(previous_value, str) and re.fullmatch(r"[1-9]\d*", previous_value):
            previous_id = int(previous_value)
        else:
            sys.exit("--continues previousMemoryId must be a positive decimal integer")
        if previous_id <= 0 or previous_id > 9223372036854775807:
            sys.exit("--continues previousMemoryId must fit a positive PostgreSQL BIGINT")
        if thread in seen:
            sys.exit(f"--continues must contain at most one entry per thread: {thread!r}")
        if thread not in available:
            sys.exit(f"--continues thread must also be present in --thread: {thread!r}")
        seen.add(thread)
        continuations.append({"thread": thread, "previousMemoryId": previous_id})
    return continuations


def main() -> None:
    p = argparse.ArgumentParser(description="Direct write to solarisael_memory.memories.")
    p.add_argument("--room", required=True)
    p.add_argument("--type", required=True, dest="type_")
    p.add_argument("--title", required=True)
    p.add_argument("--source-path", required=True,
                   help="Use a 'db-only/...' prefix when there's no markdown twin.")
    p.add_argument("--date", help="YYYY-MM-DD; defaults to today")
    p.add_argument("--also-date", action="append", default=[],
                   help="Additional YYYY-MM-DD to include in memories.dates "
                        "(repeatable). Use for cross-day sessions where the "
                        "auto-parse from --source-path doesn't catch every day. "
                        "Stitched filenames like YYYY-MM-DD_DD_ are auto-expanded.")
    p.add_argument("--body-file", help="path to body markdown file")
    p.add_argument("--body-stdin", action="store_true",
                   help="read body from stdin instead of a file")
    p.add_argument("--thread", action="append", default=[],
                   help="thread key (slash-separated concept-variants); repeatable")
    p.add_argument("--supersedes", type=int, action="append", default=[],
                   help="memory id this new write supersedes; repeatable")
    p.add_argument("--continues", action="append",
                   help='JSON continuation {"thread": "...", "previousMemoryId": 123}; repeatable')
    p.add_argument("--canon-touches", action="append", default=[],
                   help="named_entity name this memory touches; repeatable. "
                        "Adds the new source_path to that entity's pointer_files "
                        "so the canon-assertion overlay surfaces this memory when "
                        "the entity is active. Match by entity name (room-scoped).")
    p.add_argument("--meta-kv", action="append", default=[],
                   help="meta key=value (string); repeatable")
    p.add_argument("--meta-bool", action="append", default=[],
                   help="meta key=true/false (bool); repeatable")
    p.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--no-embed", dest="embed", action="store_false",
                   help="skip inline chunk+embed (faster for batch ops; new chunks "
                        "won't be retrievable until embed_4b_pass.py runs)")
    p.add_argument("--no-backup", dest="backup", action="store_false",
                   help="skip backup.sh after success (default: backup runs)")
    p.set_defaults(backup=True, embed=True)
    args = p.parse_args()
    if any(memory_id <= 0 for memory_id in (args.supersedes or [])):
        sys.exit("--supersedes ids must be positive integers")

    if args.body_file and args.body_stdin:
        sys.exit("--body-file and --body-stdin are mutually exclusive")
    if args.body_stdin:
        body = sys.stdin.read()
    elif args.body_file:
        body = Path(args.body_file).read_text(encoding="utf-8")
    else:
        sys.exit("provide --body-file or --body-stdin")

    if not body.strip():
        sys.exit("body is empty; refusing to write")

    date_ = args.date or datetime.now().date().isoformat()
    try:
        primary_date = date.fromisoformat(date_)
    except ValueError:
        sys.exit(f"--date {date_!r} is not a valid YYYY-MM-DD")
    dates_array = derive_dates(args.source_path, primary_date, args.also_date)
    threads = normalize_threads(args.thread or [])
    continues = parse_continues(args.continues, threads)
    meta = parse_meta(args.meta_kv, args.meta_bool)

    canon_touches = args.canon_touches or []

    if args.dry_run:
        print(json.dumps({
            "room": args.room, "type": args.type_, "date": date_,
            "dates": [d.isoformat() for d in dates_array],
            "title": args.title, "source_path": args.source_path,
            "body_chars": len(body), "threads": threads,
            "supersedes": args.supersedes,
            "continues": continues,
            "canon_touches": canon_touches,
            "embed_inline": args.embed,
            "meta": meta,
        }, indent=2, ensure_ascii=False))
        return

    load_dotenv(Path(args.env_file))

    # Inline chunk+embed (2026-05-19 single-writer migration). Defer the
    # heavy imports to after dotenv so EMBED_URL/MODEL pick up env overrides
    # if present. embed_4b_pass.py only does module-level os.environ.get()
    # with hardcoded fallbacks, but the local-env-first pattern is the
    # canonical way scripts in this directory pick up config.
    if args.embed:
        sys.path.insert(0, str(Path(__file__).parent))
        from embed_4b_pass import (
            chunk_memory, embed_batch,
            CHUNK_MAX_CHARS, SUBCHUNK_TARGET, SUBCHUNK_OVERLAP,
            ensure_ollama_up, stop_ollama_if_autostarted,
        )
        # Wake Ollama if a previous session left it down, so the embed can't
        # stall the write. We restore the prior state (stop it) after the write
        # only if WE were the ones who started it.
        ensure_ollama_up()
    conn = psycopg2.connect(
        host=os.environ["PGHOST"], port=os.environ["PGPORT"],
        user=os.environ["PGUSER"], password=os.environ["PGPASSWORD"],
        dbname=os.environ["PGDATABASE"],
    )
    try:
        with conn, conn.cursor() as cur:
            cur.execute(
                """
                INSERT INTO memories
                    (room, type, date, dates, title, source_path, body, threads, meta)
                VALUES (%s, %s, %s, %s, %s, %s, %s, %s, %s::jsonb)
                ON CONFLICT (room, source_path) DO UPDATE SET
                    type = EXCLUDED.type,
                    date = EXCLUDED.date,
                    dates = EXCLUDED.dates,
                    title = EXCLUDED.title,
                    body = EXCLUDED.body,
                    threads = EXCLUDED.threads,
                    meta = EXCLUDED.meta,
                    updated_at = NOW()
                RETURNING id, room, type, title, source_path
                """,
                (args.room, args.type_, date_, dates_array, args.title,
                 args.source_path, body, threads,
                 json.dumps(meta, ensure_ascii=False)),
            )
            row = cur.fetchone()
            memory_id = row[0]
            superseded_ids = []
            if args.supersedes:
                cur.execute(
                    """
                    UPDATE memories
                    SET superseded_by = %s
                    WHERE id = ANY(%s) AND id <> %s
                    RETURNING id
                    """,
                    (memory_id, sorted(set(args.supersedes)), memory_id),
                )
                superseded_ids = [item[0] for item in cur.fetchall()]

            # Synchronize the thread registry, events, and direct-write refs.
            events: dict[str, int] = {}
            for thread in threads:
                cur.execute(
                    """
                    INSERT INTO threads (room, thread_key)
                    VALUES (%s, %s)
                    ON CONFLICT (room, thread_key) DO UPDATE
                    SET thread_key = EXCLUDED.thread_key
                    RETURNING id
                    """,
                    (args.room, thread),
                )
                thread_id = cur.fetchone()[0]
                cur.execute(
                    """
                    INSERT INTO thread_events (thread_id, memory_id)
                    VALUES (%s, %s)
                    ON CONFLICT (thread_id, memory_id) DO UPDATE
                    SET memory_id = EXCLUDED.memory_id
                    RETURNING id
                    """,
                    (thread_id, memory_id),
                )
                event_id = cur.fetchone()[0]
                events[thread] = event_id
                cur.execute("DELETE FROM memory_thread_refs WHERE event_id = %s", (event_id,))
                cur.execute(
                    """
                    INSERT INTO memory_thread_refs
                        (event_id, lines_start, lines_end, context)
                    VALUES (%s, NULL, NULL, '')
                    """,
                    (event_id,),
                )
            current_event_ids = list(events.values())
            if current_event_ids:
                cur.execute(
                    "DELETE FROM thread_events "
                    "WHERE memory_id = %s AND NOT (id = ANY(%s))",
                    (memory_id, current_event_ids),
                )
            else:
                cur.execute(
                    "DELETE FROM thread_events WHERE memory_id = %s",
                    (memory_id,),
                )


            # Replace only explicitly supplied incoming edges. An omitted
            # continuation preserves that thread's existing link on upsert.
            continued_ids = []
            for continuation in continues:
                thread = continuation["thread"]
                previous_memory_id = continuation["previousMemoryId"]
                cur.execute(
                    """
                    SELECT previous_event.id, current_thread.id
                    FROM threads AS current_thread
                    JOIN thread_events AS previous_event
                      ON previous_event.thread_id = current_thread.id
                    JOIN memories AS previous_memory
                      ON previous_memory.id = previous_event.memory_id
                    WHERE current_thread.room = %s
                      AND current_thread.thread_key = %s
                      AND previous_memory.room = %s
                      AND previous_memory.id = %s
                    """,
                    (args.room, thread, args.room, previous_memory_id),
                )
                previous = cur.fetchone()
                if previous is None:
                    raise ValueError(
                        f"continuation predecessor {previous_memory_id} must belong "
                        f"to room {args.room!r} and thread {thread!r}"
                    )
                previous_event_id, thread_id = previous
                next_event_id = events[thread]
                if previous_event_id == next_event_id:
                    raise ValueError("a memory cannot continue itself")
                cur.execute(
                    "DELETE FROM thread_event_links WHERE thread_id = %s AND next_event_id = %s",
                    (thread_id, next_event_id),
                )
                cur.execute(
                    """
                    INSERT INTO thread_event_links
                        (thread_id, previous_event_id, next_event_id)
                    VALUES (%s, %s, %s)
                    """,
                    (thread_id, previous_event_id, next_event_id),
                )
                continued_ids.append((thread, previous_memory_id))


            # Inline chunk + embed (single-writer migration, 2026-05-19).
            # On UPSERT we rebuild chunks from scratch — cleaner than diff
            # logic, and the FK cascade drops old rows when we DELETE first.
            chunk_count = 0
            if args.embed:
                cur.execute(
                    "DELETE FROM memory_chunks WHERE memory_id = %s",
                    (memory_id,),
                )
                new_chunks = chunk_memory(
                    body, CHUNK_MAX_CHARS, SUBCHUNK_TARGET, SUBCHUNK_OVERLAP,
                )
                if new_chunks:
                    # Phase A: insert chunk rows
                    inserted = []
                    for chunk in new_chunks:
                        cur.execute(
                            """
                            INSERT INTO memory_chunks
                                (memory_id, chunk_index, heading_path, body,
                                 char_start, char_end, token_estimate)
                            VALUES (%s, %s, %s, %s, %s, %s, %s)
                            RETURNING id
                            """,
                            (
                                memory_id, chunk["chunk_index"],
                                chunk["heading_path"], chunk["body"],
                                chunk["char_start"], chunk["char_end"],
                                chunk["token_estimate"],
                            ),
                        )
                        inserted.append((cur.fetchone()[0], chunk["body"]))

                    # Phase B: embed bodies + update rows. Single batch call;
                    # embed_batch chunks the bodies into one Ollama request.
                    bodies = [b for _, b in inserted]
                    vectors = embed_batch(bodies)
                    for (chunk_id, _), vec in zip(inserted, vectors):
                        vec_str = "[" + ",".join(f"{x:.6f}" for x in vec) + "]"
                        cur.execute(
                            """
                            UPDATE memory_chunks
                            SET body_embedding = %s::vector,
                                embedded_at = NOW()
                            WHERE id = %s
                            """,
                            (vec_str, chunk_id),
                        )
                    chunk_count = len(inserted)

            # Canon-touches: append this source_path to the pointer_files
            # array of each named_entity it touches. Idempotent — uses jsonb
            # path containment to avoid duplicate entries.
            canon_added = []
            for entity_name in canon_touches:
                cur.execute(
                    """
                    UPDATE named_entities
                    SET pointer_files = pointer_files
                        || jsonb_build_array(jsonb_build_object(
                            'file', %s::text,
                            'lines', jsonb_build_array(0, 0)
                        ))
                    WHERE room = %s AND name = %s
                      AND NOT (pointer_files @> jsonb_build_array(
                          jsonb_build_object('file', %s::text)
                      ))
                    RETURNING name
                    """,
                    (args.source_path, args.room, entity_name, args.source_path),
                )
                hit = cur.fetchone()
                if hit:
                    canon_added.append(hit[0])
    finally:
        conn.close()
    summary = f"recorded id={memory_id}  {row[1]}/{row[2]}  '{row[3]}'  ({row[4]})"
    if args.embed:
        summary += f"  chunks={chunk_count}"
    if dates_array:
        summary += f"  dates={[d.isoformat() for d in dates_array]}"
    if canon_touches:
        summary += f"  canon_touched={canon_added}/{canon_touches}"
    if args.supersedes:
        summary += f"  superseded={superseded_ids}/{sorted(set(args.supersedes))}"
    if continues:
        summary += f"  continues={continued_ids}"
    print(summary)
    # If we auto-woke Ollama just for this write, put it back to sleep so we
    # leave the host as we found it.
    if args.embed:
        stop_ollama_if_autostarted()
    if args.backup:
        run_backup(__file__)


if __name__ == "__main__":
    main()
