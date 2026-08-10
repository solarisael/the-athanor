#!/usr/bin/env python3
"""Return one explicit health verdict for the AKASHA substrate."""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import time
import urllib.error
import urllib.request
from pathlib import Path

import state_paths

try:
    import psycopg2
except Exception as exc:  # pragma: no cover - exercised by installations
    psycopg2 = None
    PSYCOPG_ERROR = str(exc)
else:
    PSYCOPG_ERROR = None

REQUIRED_SCRIPTS = (
    "record_memory.py",
    "catch_boat.py",
    "record_coding_lesson.py",
    "record_project_lesson.py",
    "record_writing_lesson.py",
    "record_design_lesson.py",
    "record_audio_lesson.py",
    "record_cabinet_entry.py",
)
REQUIRED_TABLES = (
    "memories",
    "threads",
    "thread_events",
    "memory_thread_refs",
    "thread_event_links",
    "memory_chunks",
    "named_entities",
    "lessons",
    "semantic_vocabulary",
    "anamnesis",
    "anamnesis_reps",
    "design_documents",
)


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        os.environ.setdefault(key.strip(), value.strip())


def connect():
    return psycopg2.connect(
        host=os.environ.get("PGHOST", "127.0.0.1"),
        port=os.environ.get("PGPORT", "5432"),
        user=os.environ["PGUSER"],
        password=os.environ["PGPASSWORD"],
        dbname=os.environ["PGDATABASE"],
        connect_timeout=3,
    )


def probe_embedding(timeout: float) -> dict:
    # Canon is the Windows-native GPU Ollama on 11434. The old 11435 fossil is a
    # WSL CPU service that often still answers, so guessing it makes this probe
    # report green against a server nothing else uses. Measured 2026-07-26.
    url = os.environ.get("SOLARISAEL_EMBED_URL", "http://127.0.0.1:11434/api/embed")
    model = os.environ.get("SOLARISAEL_EMBED_MODEL", "hf.co/zenmagnets/Nemotron-3-Embed-1B-Q4_K_M-GGUF:latest")
    raw_expected = os.environ.get("SOLARISAEL_EMBED_DIMENSION", "2048")
    try:
        expected = int(raw_expected)
    except ValueError:
        return {
            "ok": False,
            "url": url,
            "model": model,
            "error": f"SOLARISAEL_EMBED_DIMENSION must be an integer, got {raw_expected!r}",
        }
    payload = json.dumps({"model": model, "input": "passage: solarisael house health"}).encode("utf-8")
    request = urllib.request.Request(url, data=payload, headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            body = json.loads(response.read().decode("utf-8"))
        vectors = body.get("embeddings") or [item.get("embedding") for item in body.get("data", [])]
        vector = vectors[0] if vectors else None
        dimension = len(vector) if isinstance(vector, list) else None
        return {
            "ok": dimension == expected,
            "url": url,
            "model": model,
            "dimension": dimension,
            "expectedDimension": expected,
            **({"error": f"embedding dimension is {dimension}, expected {expected}"} if dimension != expected else {}),
        }
    except (OSError, ValueError, urllib.error.URLError) as exc:
        return {"ok": False, "url": url, "model": model, "expectedDimension": expected, "error": str(exc)}


def probe_database() -> dict:
    """Report reachability and schema state as two separate facts.

    `reachable` says whether a connection was made at all. Without it the two
    failures are only distinguishable by whether `schemaVersion` happens to be
    present, which reads downstream as "required 13; got undefined" and blames
    the schema for an unreachable server.
    """
    if psycopg2 is None:
        return {"ok": False, "reachable": False, "error": f"psycopg2 unavailable: {PSYCOPG_ERROR}"}
    try:
        conn = connect()
        with conn, conn.cursor() as cur:
            cur.execute("SELECT current_database(), current_user")
            database, user = cur.fetchone()
            cur.execute("SELECT extname FROM pg_extension WHERE extname IN ('vector', 'pg_trgm') ORDER BY extname")
            extensions = [row[0] for row in cur.fetchall()]
            cur.execute("SELECT table_name FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = ANY(%s)", (list(REQUIRED_TABLES),))
            tables = {row[0] for row in cur.fetchall()}
            cur.execute("SELECT coalesce(max((substring(version::text from '^[0-9]+'))::integer), 0) FROM schema_migrations")
            schema_version = cur.fetchone()[0]
        conn.close()
        missing = sorted(set(REQUIRED_TABLES) - tables)
        ok = not missing and {"vector", "pg_trgm"}.issubset(extensions) and schema_version >= 13
        return {
            "ok": ok,
            "reachable": True,
            "database": database,
            "user": user,
            "schemaVersion": schema_version,
            "extensions": extensions,
            "missingTables": missing,
            **({"error": "database schema is incomplete"} if not ok else {}),
        }
    except Exception as exc:
        return {"ok": False, "reachable": False, "error": str(exc)}


def newest_dump(directory: Path) -> Path | None:
    dumps = sorted(directory.glob("*.dump"), key=lambda p: p.stat().st_mtime, reverse=True)
    return dumps[0] if dumps else None


def resolve_backup_directory(state_root: Path | None) -> Path | None:
    """The one backups directory the writers fill.

    The canonical merge collapsed the two drifted substrate trees into a single
    product tree with mutable state outside it, so there is no longer an empty
    sibling to read green off. The resolved path is still reported.

    `None` means the state root is unresolved and no override was given, so
    there is no directory to name. That is reported, never guessed.
    """
    override = os.environ.get("SOLARISAEL_BACKUP_DIR", "").strip()
    if override:
        return Path(override)
    return None if state_root is None else state_root / "substrate" / "backups"


def probe_backup(max_age_hours: float, state_root: Path | None) -> dict:
    """The safety net has no nerve of its own.

    backup_runner.py warns and returns on every failure so a dead dump can
    never abort a memory write. That is the right call, and it is why backups
    once stayed dead five hours while health read green. This is the alarm.
    """
    directory = resolve_backup_directory(state_root)
    if directory is None:
        return {"ok": False, "directory": None, "error": "state root is unresolved, so no backup directory can be named"}
    if not directory.is_dir():
        return {"ok": False, "directory": str(directory), "error": "backup directory does not exist"}
    dump = newest_dump(directory)
    if dump is None:
        return {"ok": False, "directory": str(directory), "error": "no dump files present"}
    stat = dump.stat()
    age_hours = (time.time() - stat.st_mtime) / 3600
    with dump.open("rb") as handle:
        header = handle.read(5)
    problems = []
    if header != b"PGDMP":
        problems.append("newest dump is not a pg_dump custom-format archive")
    if age_hours > max_age_hours:
        problems.append(f"newest dump is {age_hours:.1f}h old, past the {max_age_hours:.0f}h bound")
    return {
        "ok": not problems,
        "directory": str(directory),
        "newest": dump.name,
        "ageHours": round(age_hours, 2),
        "bytes": stat.st_size,
        **({"error": "; ".join(problems)} if problems else {}),
    }


def resolve_substrate_binary(explicit: str | None = None) -> tuple[Path, str]:
    """The substrate executable, and how it was chosen.

    `ATHANOR_SUBSTRATE_EXE` is a Windows process variable and does not cross
    into WSL, so the adapter forwards the canonical installed value as an argv
    argument instead. Without it this probe falls back to the structural
    development location and reports `<product>/target/release/...` while the
    installed binary actually lives under `adapters/omp/bin/<platform>/` —
    a false negative that reads as a missing executable.

    The adapter is forwarding a value it already owns for diagnostics. This is
    not a third configuration owner: nothing here decides the path, it only
    reports the one it was handed.
    """
    if explicit and explicit.strip():
        return Path(explicit.strip()), "argument"
    override = os.environ.get("ATHANOR_SUBSTRATE_EXE", "").strip()
    if override:
        return Path(override), "environment"
    # The substrate binary is a Windows .exe even when this probe is invoked
    # from WSL, which runs it happily. Keying the suffix off os.name made the
    # retrieval organ report dead from one side of the loopback and alive from
    # the other. Look for what exists instead of guessing from the host.
    return state_paths.release_binary(), "release_build"


def resolve_state(env_file: str | None) -> tuple[Path | None, Path | None, str | None, str | None]:
    """Decide the dotenv to read and the state root, without ever raising.

    Returns `(dotenv, state_root, source, error)`.

    An explicit `--env-file` is authoritative and is honoured even when nothing
    else about the state root can be worked out. This matters across the
    Windows/WSL boundary: `ATHANOR_STATE_DIR` is a Windows process variable and
    does not cross into WSL, so the adapter passes the dotenv as an argv value
    instead. The argument must therefore be usable on its own.

    The canonical dotenv is `<state-root>/substrate/.env`, so an explicit file
    sitting in a `substrate` directory also names its state root. A file
    anywhere else names only itself, and the state root falls back to
    structural resolution.

    Nothing here raises. health.py must always answer with a verdict the
    adapter can parse; a traceback would reach it as malformed JSON.
    """
    if env_file:
        dotenv = Path(env_file)
        if dotenv.parent.name == "substrate":
            return dotenv, dotenv.parent.parent, "explicit_env_file", None
        try:
            state_root, source = state_paths.resolve_state_root()
        except state_paths.StateRootError as exc:
            return dotenv, None, "explicit_env_file", str(exc)
        return dotenv, state_root, source, None
    try:
        state_root, source = state_paths.resolve_state_root()
    except state_paths.StateRootError as exc:
        return None, None, None, str(exc)
    return state_root / "substrate" / ".env", state_root, source, None


def probe_topology(
    dotenv: Path | None,
    state_root: Path | None,
    source: str | None,
    error: str | None,
    substrate_exe: str | None = None,
) -> dict:
    """Report the paths this process actually resolved, and how.

    An operator debugging a wrong database, a missing dotenv, or an executable
    reported dead needs to see which value won and why. Everything here is
    resolved at runtime from this file's own location, from an explicit
    argument, or from the environment, so nothing baked in at build time can
    leak into the report.
    """
    binary, binary_source = resolve_substrate_binary(substrate_exe)
    return {
        "athanorRoot": str(state_paths.ATHANOR_ROOT),
        "substrateDir": str(state_paths.SUBSTRATE_DIR),
        "executable": str(binary),
        "executableFound": binary.is_file(),
        "executableSource": binary_source,
        "ok": state_root is not None,
        "stateRoot": None if state_root is None else str(state_root),
        "stateRootSource": source,
        "substrateStateDir": None if state_root is None else str(state_root / "substrate"),
        "dotenv": None if dotenv is None else str(dotenv),
        "dotenvExists": bool(dotenv is not None and dotenv.is_file()),
        **({"error": error} if error else {}),
    }


def phrase_from_corpus() -> tuple[str, str] | None:
    """Lift a probe query verbatim out of the newest embedded chunk.

    A phrase taken from the corpus must match itself. That is what makes this
    probe rot-proof: no golden string to go stale as memories change, and a
    failure means the lane is broken rather than the query gone cold.
    """
    try:
        conn = connect()
        with conn, conn.cursor() as cur:
            cur.execute(
                """
                SELECT m.room, mc.body
                FROM memory_chunks mc
                JOIN memories m ON m.id = mc.memory_id
                WHERE mc.body_embedding IS NOT NULL AND length(mc.body) > 200
                ORDER BY mc.embedded_at DESC NULLS LAST, mc.id DESC
                LIMIT 1
                """
            )
            row = cur.fetchone()
        conn.close()
    except Exception:
        return None
    if not row:
        return None
    room, body = row
    words = [word for word in body.split() if not set(word) <= set("#*-_`")]
    return (room, " ".join(words[:16])) if len(words) >= 8 else None


def probe_retrieval(timeout: float, substrate_exe: str | None = None) -> dict:
    """Drive the real binary, because a SQL imitation reads green while it is dark.

    The lane went dark for a full night behind a working database and a working
    embedder: the reason string below is emitted only for semantic
    contributions, and it was absent from every sample. Assert on that string.
    """
    binary, _ = resolve_substrate_binary(substrate_exe)
    if not binary.is_file():
        return {"ok": False, "binary": str(binary), "error": "substrate binary not found"}
    probe = phrase_from_corpus()
    if probe is None:
        return {"ok": False, "error": "no embedded chunk available to build a probe query"}
    room, query = probe
    envelope = json.dumps(
        {"protocol": 1, "id": "health", "method": "recall", "params": {"room": room, "query": query}}
    )
    try:
        completed = subprocess.run(
            [str(binary)],
            input=envelope + "\n",
            capture_output=True,
            text=True,
            timeout=timeout,
            cwd=str(state_paths.SUBSTRATE_DIR),
        )
    except (OSError, subprocess.SubprocessError) as exc:
        return {"ok": False, "binary": str(binary), "query": query, "error": str(exc)}
    line = next((raw for raw in reversed(completed.stdout.splitlines()) if raw.strip()), "")
    try:
        payload = json.loads(line)
    except ValueError:
        detail = completed.stderr.strip() or completed.stdout.strip()
        return {"ok": False, "query": query, "error": f"unparseable recall response: {detail[:200]}"}
    result = payload.get("result") or {}
    warnings = result.get("warnings") or []
    candidates = result.get("retrievalCandidates") or []
    semantic = [c for c in candidates if "semantic cosine similarity" in (c.get("reasons") or [])]
    problems = []
    if payload.get("error"):
        problems.append(str(payload["error"])[:200])
    if warnings:
        problems.append("; ".join(str(w) for w in warnings)[:200])
    if not semantic:
        problems.append("no candidate carried a semantic cosine similarity reason")
    return {
        "ok": not problems,
        "room": room,
        "query": query,
        "candidates": len(candidates),
        "semanticCandidates": len(semantic),
        **({"error": "; ".join(problems)} if problems else {}),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    # Resolved lazily, after parsing. An eagerly evaluated default is computed
    # even when --env-file IS supplied, so a state root this process cannot
    # work out would raise before the explicit argument was ever read — which
    # makes the explicit argument decorative. See resolve_state.
    parser.add_argument("--env-file", default=None,
                        help="absolute path to the substrate dotenv; overrides structural resolution")
    # Same boundary as --env-file: ATHANOR_SUBSTRATE_EXE is a Windows process
    # variable and does not reach this process inside WSL. Without it the
    # structural fallback reports <product>/target/release/... while an
    # installed binary lives under adapters/omp/bin/<platform>/, so a perfectly
    # healthy executable reads as missing.
    parser.add_argument("--substrate-exe", default=None,
                        help="absolute path to the substrate executable; overrides structural resolution")
    parser.add_argument("--skip-embedding", action="store_true")
    # Off by default, and deliberately so. This verdict feeds substrateHealth(),
    # which the adapter calls on a 3s lane-status budget and an 8s diagnostic
    # budget to decide whether memory is usable at all. The retrieval probe
    # spawns the substrate binary and embeds a query; a cold model alone has
    # measured ~10s. In a system whose doctrine is that retrieval must never
    # block a conversation, the liveness ping must never be the reason memory
    # looks dead. Deep organ checks belong to the release checklist, not the
    # hot path — see docs/RELEASE.md.
    parser.add_argument("--retrieval", action="store_true",
                        help="exercise the real recall lane; slow, for release checks")
    parser.add_argument("--timeout", type=float, default=8.0)
    # A cold Nemotron load measured ~10s, and the content lane can take 5s on
    # top of it. The embed probe's 8s budget is far too tight for the real door.
    parser.add_argument("--retrieval-timeout", type=float, default=45.0)
    parser.add_argument("--max-backup-age-hours", type=float, default=24.0)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent
    dotenv, state_root, state_source, state_error = resolve_state(args.env_file)
    if dotenv is not None:
        load_dotenv(dotenv)

    missing_scripts = [name for name in REQUIRED_SCRIPTS if not (root / name).is_file()]
    scripts = {"ok": not missing_scripts, "missing": missing_scripts}
    database = probe_database()
    embedding = {"ok": None, "skipped": True} if args.skip_embedding else probe_embedding(args.timeout)
    retrieval = (
        probe_retrieval(args.retrieval_timeout, args.substrate_exe)
        if args.retrieval
        else {"ok": None, "skipped": True, "reason": "not requested; pass --retrieval"}
    )
    backup = probe_backup(args.max_backup_age_hours, state_root)
    topology = probe_topology(dotenv, state_root, state_source, state_error, args.substrate_exe)
    reasons = []
    if not topology["ok"]:
        reasons.append("Athanor state root is unresolved")
    if not scripts["ok"]:
        reasons.append("required substrate scripts are missing")
    if not database["ok"]:
        reasons.append("PostgreSQL substrate is unavailable or incomplete")
    if embedding.get("ok") is False:
        reasons.append("embedding service is unavailable or incompatible")
    if retrieval.get("ok") is False:
        reasons.append("retrieval lane returned no semantic match")
    if backup.get("ok") is False:
        reasons.append("backup safety net is stale or missing")
    mode = "full" if not reasons else "degraded"
    result = {
        "ok": not reasons,
        "mode": mode,
        "substrateApi": 1,
        "scripts": scripts,
        "database": database,
        "embedding": embedding,
        "retrieval": retrieval,
        "backup": backup,
        "topology": topology,
        "degradedReasons": reasons,
    }
    print(json.dumps(result, ensure_ascii=False))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
