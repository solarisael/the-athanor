#!/usr/bin/env python3
"""Canonical embedding pass — embed all memories with Nemotron-3-Embed-1B.

Targets the canonical ``memory_chunks`` table with vector(2048) storage.
Documents use the model's required ``passage: `` retrieval prefix.

Idempotent: re-running only embeds chunks that don't yet have embeddings.
Resumable: process can be killed and restarted; picks up where it left off.

Usage:
    python3 embed_4b_pass.py [--dry-run] [--rooms room-a,room-b] [--batch N]
"""

from __future__ import annotations

import argparse
import atexit
import json
import os
import re
import sys
import subprocess
import time
from pathlib import Path
from urllib import request as urlreq
from urllib.error import HTTPError, URLError

import psycopg2
import psycopg2.extras


# -------- config --------
# Both Ollama's native endpoint and OpenAI-compatible embedding endpoints are
# accepted. The model and dimension must match the migrated vector space.
import os as _os

import state_paths


def configure_embedding() -> None:
    """Refresh embedding settings after an optional dotenv file is loaded."""
    global EMBED_URL, EMBED_MODEL, LMSTUDIO_URL, LMSTUDIO_MODEL, EMBED_DIM
    global _OLLAMA_TAGS_URL

    EMBED_URL = _os.environ.get(
        "SOLARISAEL_EMBED_URL",
        _os.environ.get("SOLARISAEL_LMSTUDIO_URL", "http://localhost:11434/api/embed"),
    )
    EMBED_MODEL = _os.environ.get(
        "SOLARISAEL_EMBED_MODEL",
        "hf.co/zenmagnets/Nemotron-3-Embed-1B-Q4_K_M-GGUF:latest",
    )
    raw_dimension = _os.environ.get("SOLARISAEL_EMBED_DIMENSION", "2048")
    try:
        EMBED_DIM = int(raw_dimension)
    except ValueError as exc:
        raise ValueError(
            f"SOLARISAEL_EMBED_DIMENSION must be an integer, got {raw_dimension!r}"
        ) from exc

    # Back-compat aliases — older callers may still read these names.
    LMSTUDIO_URL = EMBED_URL
    LMSTUDIO_MODEL = EMBED_MODEL
    suffix = "/api/embed"
    _OLLAMA_TAGS_URL = (
        f"{EMBED_URL[:-len(suffix)]}/api/tags"
        if EMBED_URL.rstrip("/").endswith(suffix)
        else None
    )


configure_embedding()
_OLLAMA_AUTOSTARTED = False  # True only if WE started it, so we can stop it later
_OLLAMA_PROCESS: subprocess.Popen | None = None


def _ollama_alive(timeout: float = 4.0) -> bool:
    if _OLLAMA_TAGS_URL is None:
        # Non-Ollama endpoints are probed by the real POST request. Never start
        # a local Ollama server on behalf of an unrelated compatible service.
        return True
    try:
        urlreq.urlopen(urlreq.Request(_OLLAMA_TAGS_URL, method="GET"), timeout=timeout)
        return True
    except Exception:
        return False


def _windows_ollama_exe() -> str | None:
    # WSL exposes the Windows profile root through /mnt/<drive>/Users. Do not
    # bake a maintainer username into a public runtime.
    configured = _os.environ.get("SOLARISAEL_OLLAMA_EXE")
    candidates = [configured] if configured else []
    candidates.extend(
        str(path)
        for drive in Path("/mnt").glob("[a-z]")
        for path in (drive / "Users").glob("*/AppData/Local/Programs/Ollama/ollama.exe")
    )
    for candidate in candidates:
        if candidate and _os.path.exists(candidate):
            return candidate
    return None


def ensure_ollama_up(wait_s: int = 30) -> bool:
    """Return True if Ollama is serving; auto-start it if down and possible.

    Sets the module flag _OLLAMA_AUTOSTARTED when we are the ones who started
    it, so a caller can stop it again with stop_ollama_if_autostarted().
    """
    global _OLLAMA_AUTOSTARTED, _OLLAMA_PROCESS
    if _ollama_alive():
        return True
    if _os.environ.get("SOLARISAEL_NO_AUTOSTART_OLLAMA"):
        return False

    exe = _windows_ollama_exe()
    command = [exe, "serve"] if exe else ["ollama", "serve"]
    try:
        process = subprocess.Popen(
            command,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
    except Exception as exc:
        print(f"  [ollama] could not auto-start: {exc}", file=sys.stderr)
        return False

    _OLLAMA_AUTOSTARTED = True
    _OLLAMA_PROCESS = process
    atexit.register(stop_ollama_if_autostarted)
    print("  [ollama] was down — auto-started, waiting for it to serve...", file=sys.stderr)
    for _ in range(wait_s):
        if _ollama_alive():
            _OLLAMA_AUTOSTARTED = True
            print("  [ollama] up.", file=sys.stderr)
            return True
        time.sleep(1)
    print("  [ollama] did not come up in time.", file=sys.stderr)
    stop_ollama_if_autostarted()
    return False


def stop_ollama_if_autostarted() -> None:
    """Stop Ollama only if THIS process auto-started it (leave a pre-existing
    server running)."""
    global _OLLAMA_AUTOSTARTED, _OLLAMA_PROCESS
    if not _OLLAMA_AUTOSTARTED:
        return
    process = _OLLAMA_PROCESS
    try:
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=5)
        print("  [ollama] stopped (was auto-started by this run).", file=sys.stderr)
    except Exception as exc:
        print(f"  WARN: could not stop auto-started Ollama: {exc}", file=sys.stderr)
    finally:
        _OLLAMA_AUTOSTARTED = False
        _OLLAMA_PROCESS = None

CHUNK_MAX_CHARS = 4000
SUBCHUNK_TARGET = 2200
SUBCHUNK_OVERLAP = 200

DEFAULT_BATCH = 6


def load_dotenv(path: Path) -> None:
    if not path.exists():
        return
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        k, _, v = line.partition("=")
        os.environ.setdefault(k.strip(), v.strip())


# -------- chunking (mirrors embed_8b_pass.py) --------
HEADING_RE = re.compile(r"^(##\s+.+)$", re.MULTILINE)


def split_by_headings(body: str) -> list[tuple[str, str, int, int]]:
    matches = list(HEADING_RE.finditer(body))
    chunks: list[tuple[str, str, int, int]] = []
    if not matches:
        chunks.append(("__preamble__", body, 0, len(body)))
        return chunks
    if matches[0].start() > 0:
        chunks.append(("__preamble__", body[: matches[0].start()], 0, matches[0].start()))
    for i, m in enumerate(matches):
        end = matches[i + 1].start() if i + 1 < len(matches) else len(body)
        chunks.append((m.group(1).strip(), body[m.start():end], m.start(), end))
    return chunks


def split_oversized(text: str, char_offset: int, target: int, overlap: int):
    if len(text) <= target * 1.4:
        return [(text, char_offset, char_offset + len(text))]
    paragraphs = re.split(r"\n\s*\n", text)
    pieces: list[tuple[str, int, int]] = []
    buf: list[str] = []
    buf_chars = 0
    buf_start = char_offset
    for para in paragraphs:
        para_len = len(para) + 2
        if buf and buf_chars + para_len > target:
            joined = "\n\n".join(buf)
            pieces.append((joined, buf_start, buf_start + len(joined)))
            tail = joined[-overlap:] if overlap and len(joined) > overlap else ""
            buf = [tail, para] if tail else [para]
            buf_chars = len(tail) + para_len
            buf_start = buf_start + len(joined) - len(tail)
        else:
            buf.append(para)
            buf_chars += para_len
    if buf:
        joined = "\n\n".join(buf)
        pieces.append((joined, buf_start, buf_start + len(joined)))
    return pieces


def chunk_memory(body: str, max_chars: int, target: int, overlap: int) -> list[dict]:
    out: list[dict] = []
    section_chunks = split_by_headings(body)
    for heading, sec_body, sec_start, sec_end in section_chunks:
        if len(sec_body) <= max_chars:
            out.append({
                "heading_path": heading, "body": sec_body,
                "char_start": sec_start, "char_end": sec_end,
            })
        else:
            for sub_text, sub_start, sub_end in split_oversized(sec_body, sec_start, target, overlap):
                out.append({
                    "heading_path": heading, "body": sub_text,
                    "char_start": sub_start, "char_end": sub_end,
                })
    out = [c for c in out if c["body"].strip()]
    for i, c in enumerate(out):
        c["chunk_index"] = i
        c["token_estimate"] = max(1, len(c["body"]) // 4)
    return out


# -------- embedding client (ollama or openai-compat) --------
def embed_batch(texts: list[str], retries: int = 3, timeout: int = 120) -> list[list[float]]:
    payload = json.dumps({"model": EMBED_MODEL, "input": [f"passage: {text}" for text in texts]}).encode("utf-8")
    headers = {"Content-Type": "application/json"}
    last_err: Exception | None = None
    for attempt in range(retries):
        try:
            # On the first attempt, make sure Ollama is actually serving — wake
            # it if a previous session left it down, so the write can't stall.
            if attempt == 0 and not _ollama_alive():
                ensure_ollama_up()
            req = urlreq.Request(EMBED_URL, data=payload, headers=headers, method="POST")
            with urlreq.urlopen(req, timeout=timeout) as resp:
                data = json.loads(resp.read().decode("utf-8"))
            # Ollama shape: {"embeddings": [[...], [...], ...]}
            if isinstance(data.get("embeddings"), list):
                embeddings = data["embeddings"]
            # OpenAI-compat shape: {"data": [{"embedding": [...]}, ...]}
            elif isinstance(data.get("data"), list):
                embeddings = [item["embedding"] for item in data["data"]]
            else:
                raise ValueError(f"unexpected embed response keys: {list(data.keys())}")
            for v in embeddings:
                if len(v) != EMBED_DIM:
                    raise ValueError(f"unexpected embedding dim {len(v)} != {EMBED_DIM}")
            return embeddings
        except (HTTPError, URLError, TimeoutError) as e:
            last_err = e
            wait = 2 ** attempt
            print(f"  embed retry {attempt+1}/{retries} after {wait}s ({e})", file=sys.stderr)
            time.sleep(wait)
    raise RuntimeError(f"embed failed after {retries} retries: {last_err}")


# -------- main --------
def main() -> None:
    p = argparse.ArgumentParser()
    p.add_argument("--rooms", help="comma-separated room keys; defaults to every room in memories")
    p.add_argument("--batch", type=int, default=DEFAULT_BATCH)
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--limit", type=int, default=0)
    p.add_argument("--env-file", default=str(state_paths.default_dotenv_path()))
    args = p.parse_args()

    rooms = [r.strip() for r in (args.rooms or "").split(",") if r.strip()]
    load_dotenv(Path(args.env_file))
    try:
        configure_embedding()
    except ValueError as exc:
        p.error(str(exc))
    conn = psycopg2.connect(
        host=os.environ["PGHOST"], port=os.environ["PGPORT"],
        user=os.environ["PGUSER"], password=os.environ["PGPASSWORD"],
        dbname=os.environ["PGDATABASE"],
    )

    # Sanity: extension + table.
    with conn.cursor() as cur:
        cur.execute("SELECT extversion FROM pg_extension WHERE extname='vector'")
        ver = cur.fetchone()
        if not ver:
            sys.exit("FATAL: pgvector extension not present")
        if ver[0] < "0.4":
            sys.exit(f"FATAL: pgvector {ver[0]} too old; vector support needs 0.4+")
        cur.execute("SELECT to_regclass('memory_chunks')")
        if cur.fetchone()[0] is None:
            sys.exit("FATAL: memory_chunks table missing — run run_migrations.py")
        if not rooms:
            cur.execute("SELECT DISTINCT room FROM memories ORDER BY room")
            rooms = [row[0] for row in cur.fetchall()]

    # ---- Phase A: chunk every memory not yet in memory_chunks ----
    print(f"[chunk] rooms={rooms}", file=sys.stderr)
    total_memories = 0
    total_new_chunks = 0

    with conn:
        with conn.cursor() as cur:
            cur.execute("""
                SELECT id, room, title, body
                FROM memories
                WHERE room = ANY(%s)
                  AND id NOT IN (SELECT DISTINCT memory_id FROM memory_chunks)
                ORDER BY id
            """, (rooms,))
            todo = cur.fetchall()

        if args.limit:
            todo = todo[: args.limit]

        for mem_id, room, title, body in todo:
            chunks = chunk_memory(body, CHUNK_MAX_CHARS, SUBCHUNK_TARGET, SUBCHUNK_OVERLAP)
            total_memories += 1
            total_new_chunks += len(chunks)
            if args.dry_run:
                print(f"  [dry] mem {mem_id} ({room}) -> {len(chunks)} chunks", file=sys.stderr)
                continue
            with conn.cursor() as cur:
                psycopg2.extras.execute_values(
                    cur,
                    """
                    INSERT INTO memory_chunks
                        (memory_id, chunk_index, heading_path, body, char_start, char_end, token_estimate)
                    VALUES %s
                    ON CONFLICT (memory_id, chunk_index) DO NOTHING
                    """,
                    [
                        (mem_id, c["chunk_index"], c["heading_path"], c["body"],
                         c["char_start"], c["char_end"], c["token_estimate"])
                        for c in chunks
                    ],
                )

    print(f"[chunk] {total_memories} memories chunked into {total_new_chunks} new chunks", file=sys.stderr)
    if args.dry_run:
        return

    # ---- Phase B: embed any chunk without a vector ----
    print(f"[embed] starting (batch={args.batch}, model={EMBED_MODEL}, url={EMBED_URL})", file=sys.stderr)
    with conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM memory_chunks WHERE body_embedding IS NULL")
        remaining = cur.fetchone()[0]
    print(f"[embed] {remaining} chunks pending", file=sys.stderr)

    embedded = 0
    started = time.time()
    while True:
        with conn.cursor() as cur:
            cur.execute(
                """
                SELECT id, body
                FROM memory_chunks
                WHERE body_embedding IS NULL
                ORDER BY id
                LIMIT %s
                """,
                (args.batch,),
            )
            rows = cur.fetchall()
        if not rows:
            break
        ids = [r[0] for r in rows]
        texts = [r[1] for r in rows]
        try:
            vectors = embed_batch(texts)
        except RuntimeError as e:
            print(f"[embed] FATAL: {e}", file=sys.stderr)
            sys.exit(2)
        with conn:
            with conn.cursor() as cur:
                for chunk_id, vec in zip(ids, vectors):
                    cur.execute(
                        """
                        UPDATE memory_chunks
                        SET body_embedding = %s::vector,
                            embedded_at    = NOW()
                        WHERE id = %s
                        """,
                        (vec, chunk_id),
                    )
        embedded += len(rows)
        elapsed = time.time() - started
        rate = embedded / elapsed if elapsed > 0 else 0
        eta = (remaining - embedded) / rate if rate > 0 else 0
        print(f"  [embed] {embedded}/{remaining} | {rate:.1f}/s | eta {eta:.0f}s", file=sys.stderr)

    elapsed = time.time() - started
    print(f"[embed] done. {embedded} chunks in {elapsed:.0f}s", file=sys.stderr)
    conn.close()


if __name__ == "__main__":
    main()
