#!/usr/bin/env python3
"""Write to the Cabinet (anamnesis) — the path-compression organ.

THIS IS NOT record_memory.py. memories are a CATALOGUE (what was said,
auto-retrieved by match). The Cabinet is a COUNSEL: the compressed PATH a
conversation WALKED — the ramp — consulted at a fork or woken at session start.

The discipline is enforced, not suggested. The writer REFUSES drawers that
aren't paths. See the LITURGY banner (printed on every run).

Two operations:
    add        — a drawer (pillar or cycle)
    append-rep — one rep onto an existing cycle (the training-log)

Examples:
    python3 record_cabinet_entry.py --room room-key add \\
        --kind pillar --fidelity record --activation wake \\
        --title "A standing principle" --shape principle \\
        --ramp-file /tmp/ramp.md --counsel "..." \\
        --source-path memory/source.md --canon "principle-key"

    python3 record_cabinet_entry.py --room room-key append-rep \\
        --title "A recurring cycle" --rep-number 3 --occurred-on 2026-06-01 \\
        --how-it-went-file /tmp/rep.md \\
        --portal-pull "where the old pattern nearly won" \\
        --lighter "how this pass became measurably lighter" \\
        --source-path memory/source.md
"""
from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import date, datetime, timezone
from pathlib import Path

import psycopg2

from backup_runner import run_backup

import state_paths


LITURGY = """\
  ┌─ THE CABINET (anamnesis) ─────────────────────────────────────────────┐
  │ You are compressing the PATH, not recording the event.                │
  │ Keep the ramp whole; let the beginning and the peak compress.         │
  │ Mark fidelity: 'record' (true-as-said) vs 'raw-material' (reforged).  │
  │ A cycle without its reps is a lie about recurrence.                   │
  │ The seam between true-as-said and true-as-reforged is the drift-guard │
  │ — never collapse it.                                                  │
  └───────────────────────────────────────────────────────────────────────┘"""


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, _, v = line.partition("=")
        os.environ.setdefault(k.strip(), v.strip())


def read_text_arg(inline: str | None, file_path: str | None, label: str) -> str:
    """Return text from --X (inline) or --X-file. Exactly one is honored."""
    if file_path:
        return Path(file_path).read_text(encoding="utf-8").strip()
    return (inline or "").strip()


def die(msg: str) -> None:
    sys.exit(f"\n  REFUSED — {msg}\n")


def embed_one(text: str) -> list[float]:
    """Embed a single text via the shared embed client (auto-wakes Ollama)."""
    sys.path.insert(0, str(Path(__file__).parent))
    from embed_4b_pass import embed_batch, ensure_ollama_up
    ensure_ollama_up()
    return embed_batch([text])[0]


def vec_literal(vec: list[float]) -> str:
    return "[" + ",".join(f"{x:.6f}" for x in vec) + "]"


# ---------------------------------------------------------------------------
# add a drawer
# ---------------------------------------------------------------------------
def cmd_add(args, conn) -> None:
    ramp = read_text_arg(args.ramp, args.ramp_file, "ramp")
    counsel = read_text_arg(args.counsel, args.counsel_file, "counsel")
    peak = read_text_arg(args.peak, args.peak_file, "peak")
    beginning = read_text_arg(args.beginning, args.beginning_file, "beginning")
    verify_note = read_text_arg(args.verify_note, args.verify_note_file, "verify-note")

    # --- DISCIPLINE: hard refusals (definitionally not a Cabinet drawer) ---
    if not ramp:
        die("a drawer with no RAMP is a memory, not a Cabinet entry. "
            "The ramp is the causal walk (A caused B caused the peak). Provide --ramp / --ramp-file.")
    if args.fidelity not in ("record", "raw-material"):
        die("fidelity must be explicitly 'record' (true-as-said) or "
            "'raw-material' (true-as-reforged). No silent default — declare it.")
    has_inline_rep = bool(args.seed_rep_how or args.seed_rep_how_file)
    if args.kind == "cycle" and not (args.allow_empty_cycle or has_inline_rep):
        die("a CYCLE drawer must carry at least one rep — a cycle with no reps "
            "is a lie about recurrence. Seed the first rep with --seed-rep-* "
            "flags, or pass --allow-empty-cycle ONLY if you will append-rep "
            "immediately.")
    if args.kind == "cycle" and args.activation == "wake" and not verify_note:
        die("a wake-tier CYCLE needs a --verify-note / --verify-note-file. "
            "The loaded prior must not capture perception: the reader treats the live "
            "cycle as INFORMING the turn, not DEFINING it. State the verify-against-the-turn caution.")

    # --- DISCIPLINE: soft warning (probably a compression mistake) ---
    if peak and len(ramp) < len(peak):
        print(f"  ⚠  WARNING: ramp ({len(ramp)} chars) is shorter than peak "
              f"({len(peak)} chars). The ramp is the spine — it should stay "
              f"full-resolution while the peak compresses. Proceeding, but "
              f"check you didn't squeeze the path.", file=sys.stderr)

    canon = args.canon or []
    source_paths = args.source_path or []
    tags = args.tag or []
    meta = {"origin": "record_cabinet_entry", "recorded_at": datetime.now(timezone.utc).isoformat()}
    for kv in args.meta_kv or []:
        k, _, v = kv.partition("=")
        meta[k] = v

    # The embedded text is the consultable payload: the ramp + counsel.
    embed_text = ramp + ("\n\n" + counsel if counsel else "")
    if args.dry_run:
        print(LITURGY)
        print(f"\n  DRY RUN — would add {args.kind}/{args.fidelity}/{args.activation} "
              f"'{args.title}' (active={not args.dormant})")
        print(f"  ramp: {len(ramp)} chars | counsel: {len(counsel)} chars | "
              f"peak: {len(peak)} | beginning: {len(beginning)}")
        print(f"  canon={canon} source_paths={source_paths} tags={tags}")
        return

    vec = embed_one(embed_text)

    with conn, conn.cursor() as cur:
        cur.execute(
            """
            INSERT INTO anamnesis
                (room, kind, fidelity, activation, active, title, shape,
                 peak, beginning, ramp, counsel, verify_note,
                 source_paths, canon_links, tags, body_embedding, embedded_at, meta)
            VALUES (%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s,%s::vector,NOW(),%s::jsonb)
            ON CONFLICT (room, title) DO UPDATE SET
                kind=EXCLUDED.kind, fidelity=EXCLUDED.fidelity,
                activation=EXCLUDED.activation, active=EXCLUDED.active,
                shape=EXCLUDED.shape, peak=EXCLUDED.peak, beginning=EXCLUDED.beginning,
                ramp=EXCLUDED.ramp, counsel=EXCLUDED.counsel, verify_note=EXCLUDED.verify_note,
                source_paths=EXCLUDED.source_paths, canon_links=EXCLUDED.canon_links,
                tags=EXCLUDED.tags, body_embedding=EXCLUDED.body_embedding,
                embedded_at=NOW(), meta=EXCLUDED.meta, updated_at=NOW()
            RETURNING id
            """,
            (args.room, args.kind, args.fidelity, args.activation, (not args.dormant),
             args.title, args.shape, peak or None, beginning or None, ramp,
             counsel or None, (verify_note or None),
             source_paths, canon, tags, vec_literal(vec),
             json.dumps(meta, ensure_ascii=False)),
        )
        cab_id = cur.fetchone()[0]

        # Optional first rep seeded inline (for cycles added with --seed-rep-*).
        if args.kind == "cycle" and (args.seed_rep_how or args.seed_rep_how_file):
            how = read_text_arg(args.seed_rep_how, args.seed_rep_how_file, "seed-rep-how")
            portal = read_text_arg(args.seed_rep_portal, args.seed_rep_portal_file, "seed-rep-portal")
            lighter = read_text_arg(args.seed_rep_lighter, args.seed_rep_lighter_file, "seed-rep-lighter")
            _insert_rep(cur, cab_id, args.seed_rep_number or 1, args.seed_rep_on,
                        how, portal, lighter, (source_paths[0] if source_paths else None))

    print(LITURGY)
    print(f"\n  cabinet add: id={cab_id}  {args.kind}/{args.fidelity}/{args.activation}  "
          f"'{args.title}'  active={not args.dormant}")


# ---------------------------------------------------------------------------
# append a rep onto a cycle
# ---------------------------------------------------------------------------
def _insert_rep(cur, cab_id, rep_number, occurred_on, how, portal, lighter, source_path):
    cur.execute(
        """
        INSERT INTO anamnesis_reps
            (cabinet_id, rep_number, occurred_on, how_it_went, portal_pull, lighter, source_path)
        VALUES (%s,%s,%s,%s,%s,%s,%s)
        ON CONFLICT (cabinet_id, rep_number) DO UPDATE SET
            occurred_on=EXCLUDED.occurred_on, how_it_went=EXCLUDED.how_it_went,
            portal_pull=EXCLUDED.portal_pull, lighter=EXCLUDED.lighter,
            source_path=EXCLUDED.source_path
        RETURNING id
        """,
        (cab_id, rep_number, occurred_on, how, portal, lighter, source_path),
    )
    return cur.fetchone()[0]


def cmd_append_rep(args, conn) -> None:
    how = read_text_arg(args.how_it_went, args.how_it_went_file, "how-it-went")
    portal = read_text_arg(args.portal_pull, args.portal_pull_file, "portal-pull")
    lighter = read_text_arg(args.lighter, args.lighter_file, "lighter")

    # --- DISCIPLINE: a rep without its measurement is a note, not a rep ---
    if not how:
        die("a rep needs --how-it-went / --how-it-went-file: the walk this pass.")
    if not portal:
        die("a rep needs --portal-pull / --portal-pull-file: where measured-default / "
            "the bad pattern nearly won, and what held. Without it, the training-log "
            "can't show the muscle being tested.")
    if not lighter:
        die("a rep needs --lighter / --lighter-file: whether the weight was lighter "
            "than the prior rep, and how we'd know. This is the measurement the whole "
            "cycle is FOR ('rep four was lighter than rep two'). A rep without it isn't a rep.")

    with conn, conn.cursor() as cur:
        cur.execute("SELECT id, kind FROM anamnesis WHERE room=%s AND title=%s",
                    (args.room, args.title))
        row = cur.fetchone()
        if not row:
            die(f"no drawer titled {args.title!r} in room {args.room!r}. "
                f"Add the cycle drawer first.")
        cab_id, kind = row
        if kind != "cycle":
            die(f"drawer {args.title!r} is a '{kind}', not a cycle. Reps only "
                f"attach to cycles.")
        if args.dry_run:
            print(LITURGY)
            print(f"\n  DRY RUN — would append rep {args.rep_number} to cycle "
                  f"'{args.title}' (id={cab_id})")
            return
        rep_id = _insert_rep(cur, cab_id, args.rep_number, args.occurred_on,
                             how, portal, lighter,
                             (args.source_path[0] if args.source_path else None))

    print(LITURGY)
    print(f"\n  cabinet rep: id={rep_id}  rep #{args.rep_number} on cycle "
          f"'{args.title}' (cabinet id={cab_id})")


def main() -> None:

    p = argparse.ArgumentParser(
        description="Write to the Cabinet (anamnesis) — the path-compression organ.",
        epilog=LITURGY, formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument("--room", required=True)
    p.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--no-backup", dest="backup", action="store_false")
    p.set_defaults(backup=True)
    sub = p.add_subparsers(dest="cmd", required=True)

    a = sub.add_parser("add", help="add a drawer (pillar or cycle)", epilog=LITURGY,
                       formatter_class=argparse.RawDescriptionHelpFormatter)
    a.add_argument("--kind", required=True, choices=["pillar", "cycle"])
    a.add_argument("--fidelity", required=True, choices=["record", "raw-material"])
    a.add_argument("--activation", default="fork", choices=["wake", "fork"])
    a.add_argument("--dormant", action="store_true", help="cycle not currently live (wake-tier skips it)")
    a.add_argument("--title", required=True)
    a.add_argument("--shape")
    a.add_argument("--ramp"); a.add_argument("--ramp-file")
    a.add_argument("--counsel"); a.add_argument("--counsel-file")
    a.add_argument("--peak"); a.add_argument("--peak-file")
    a.add_argument("--beginning"); a.add_argument("--beginning-file")
    a.add_argument("--verify-note"); a.add_argument("--verify-note-file")
    a.add_argument("--canon", action="append")
    a.add_argument("--source-path", action="append")
    a.add_argument("--tag", action="append")
    a.add_argument("--meta-kv", action="append")
    a.add_argument("--allow-empty-cycle", action="store_true")
    # inline first-rep seed for cycles
    a.add_argument("--seed-rep-number", type=int)
    a.add_argument("--seed-rep-on")
    a.add_argument("--seed-rep-how"); a.add_argument("--seed-rep-how-file")
    a.add_argument("--seed-rep-portal"); a.add_argument("--seed-rep-portal-file")
    a.add_argument("--seed-rep-lighter"); a.add_argument("--seed-rep-lighter-file")

    r = sub.add_parser("append-rep", help="append a rep onto a cycle", epilog=LITURGY,
                       formatter_class=argparse.RawDescriptionHelpFormatter)
    r.add_argument("--title", required=True)
    r.add_argument("--rep-number", type=int, required=True)
    r.add_argument("--occurred-on")
    r.add_argument("--how-it-went"); r.add_argument("--how-it-went-file")
    r.add_argument("--portal-pull"); r.add_argument("--portal-pull-file")
    r.add_argument("--lighter"); r.add_argument("--lighter-file")
    r.add_argument("--source-path", action="append")

    args = p.parse_args()
    load_dotenv(Path(args.env_file))

    conn = psycopg2.connect(
        host=os.environ["PGHOST"], port=os.environ["PGPORT"],
        user=os.environ["PGUSER"], password=os.environ["PGPASSWORD"],
        dbname=os.environ["PGDATABASE"],
    )
    try:
        if args.cmd == "add":
            cmd_add(args, conn)
        elif args.cmd == "append-rep":
            cmd_append_rep(args, conn)
    finally:
        conn.close()

    if not args.dry_run:
        # stop Ollama if our embed auto-started it
        try:
            sys.path.insert(0, str(Path(__file__).parent))
            from embed_4b_pass import stop_ollama_if_autostarted
            stop_ollama_if_autostarted()
        except Exception:
            pass
        run_backup(__file__, skip=not args.backup)


if __name__ == "__main__":
    main()
