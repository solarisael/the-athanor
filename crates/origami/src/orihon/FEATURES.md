# Orihon storage

The guarded deployment determines installed availability. Source and live state remain separate.

Orihon stores provenance. A record does not establish memory or canon authority.

## Public interface

- `append_pg` owns one PostgreSQL transaction.
- `append_pg_in_transaction` uses a savepoint in the caller's transaction.
- `read_pg` returns bounded records from PostgreSQL.
- `append_file` owns one locked file operation.
- `read_file` returns bounded records from the local book.
- `export_file(directory, room, records)` exports the exact accepted records from PostgreSQL.

The directory argument names `<room>/orihon`, not the room root.
The transaction caller must commit before it acknowledges acceptance.
A failed export does not reverse PostgreSQL acceptance.
Retry the capture to export the same accepted records.

## Records and replay

The source identity is `(room, harness, source_session, source_id)`.
The content digest does not form part of this identity.
Replay compares normalized source facts.
Replay preserves the first writer session and storage timestamp.
A changed source under the same identity refuses, except for the following reconciliation.
A replay can add `foldText` or `children` when the stored record has neither field.
Every original audit fact and digest must remain unchanged.
The store keeps the stored record unchanged and reports `reconciled`.
This rule applies to any record that meets these conditions.

Each room starts its sequence and page ordinal at zero.
A user record starts a page.
Each non-user record names an earlier user in the same harness and source session.
A parent must exist before its child in the book or capture batch.
The stores never reorder a batch to find a parent.

Source timestamps use normalized RFC3339 UTC text.
PostgreSQL keeps that exact text in `source_time`, beside its timestamp column.
Day reads use the exact UTC source day, without PostgreSQL precision rounding.

## Bounds and redaction

A capture contains 1 through 64 records.
Non-echo text cannot exceed 1 MiB before or after redaction.
The prepared batch cannot exceed 4 MiB.
Echoes retain equal head and tail portions within 30,000 Unicode characters.
The elision marker counts toward this limit.

Both stores apply the same credential patterns before echo truncation.
The Host removes its known private values before storage.
The redaction flag preserves both sources of redaction.
Pattern matching cannot identify arbitrary unmarked secrets.
Storage errors do not include source text.

JSON redaction preserves bytes outside changed string values, including duplicate keys.
JSON redaction keeps unchanged string tokens byte-exact and redacts credentials in surrounding strings. For strings with unpaired UTF-16 surrogates, credential redaction writes a lossy copy only when redaction changes the string. The copy uses one U+FFFD per unpaired unit and preserves valid surrogate pairs and literal escaped backslashes.

Reads combine page, sequence, and day filters.
`afterSeq` is exclusive; `untilSeq` is inclusive.
A read returns at most 200 records.
`nextAfter` names the last returned sequence only when more matching rows exist.

## File and database integrity

Daily files use `YYYY-MM-DD.jsonl`, with one serialized `Record` per line.
All file operations hold a standard-library lock on `.orihon.lock`.
Do not remove the lock file while a process can access the book.
Writes call `File::sync_all` before acknowledgment.
Retries also flush accepted files when no new row is necessary.
Unix writes also synchronize the directory.
Windows uses the file flush; the standard library does not expose a directory flush.

A retry can complete a batch after a prefix of complete records reaches disk.
A malformed or torn line refuses with its path and line number.
The refusal preserves existing bytes and does not print their content.

The file store scans daily files for each operation.
An index becomes necessary when measured capture latency exceeds the Host deadline.

Migration 0037 adds the PostgreSQL book and room cursor.
A room cursor lock serializes sequence allocation.
A trigger checks parent linkage.
Another trigger refuses updates, deletes, and truncation of the book.

## Exact fold snapshots

Migration 0040 extends immutable folds with binary child references.
Levels zero and one retain their exact message and page source references.
Higher levels contain two exact child revisions instead of copied descendant lists.
Both stores validate child geometry, digests, room identity, and replay.
Old page revisions remain readable after late source records arrive.

`folds::list_pg` and `folds::list_file` return bounded, deterministic fold snapshots.
The file reader validates source references against one loaded book snapshot.
No production path composes a view from these folds or generates new ones; the live tree replaced the page-fold worker and its book view.
Old revisions stay readable through the Host's exact `unfold`.

## Live views

The chat and compactor views keep separate persisted lines and batch states.
New units append to both views.
The chat view merges above 128,000 UTF-8 bytes and targets 64,000 bytes.
The compactor view merges independently above 32,000 bytes and targets 16,000 bytes.
A chat merge does not replace the compactor view.
Both views merge only sibling lines whose parent exists.

A view can remain above its target while the required parent nodes are absent.
Storage preserves originals independently of both views.
