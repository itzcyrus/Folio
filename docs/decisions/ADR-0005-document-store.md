# ADR-0005: Document Store — Files on Disk, Index in SQLite

- Status: Accepted
- Date: 2026-10-09
- Deciders: agent (vibe-coded project; owner does not review)
- Supersedes: —
- Related: ADR-0004 (SQLite migrations), Owner decision #2 & #6 (PLAN §0)

## Context

The first user-facing feature is Docs + Search (owner decision #2). The owner chose
"SQLite + Files" storage (decision #6): documents must not be trapped as DB blobs;
they live at user-chosen paths the user owns and edits with any tool. Search must
work over content, filename, and metadata (tags), deterministically (no AI required).

## Decision

1. **Files stay on disk where the user put them.** `machie doc add <path>` records a
   document row; Machie never moves, copies, or rewrites the original file.
2. **SQLite holds the index** (schema v2, migration-gated per ADR-0004):
   - `documents(id, path UNIQUE, title, kind, size_bytes, mtime_unix, content_hash, tags, added_at, indexed_at, status)`
   - `doc_fts`: FTS5 virtual table over `(title, tags, body)` with `content='documents'`
     external-content pattern keyed by rowid.
3. **Content hash = SHA-256 of file bytes.** Cheap correctness check for "has this
   changed?" independent of mtime; used to skip re-indexing unchanged files.
4. **Stale detection without watchers:** every `search`/`list`/`show` reconciles rows
   against the filesystem lazily — missing file → `status='missing'`; changed
   size/mtime/hash → `status='stale'` and re-extract inline. No background daemon.
5. **Tags** are stored as a comma-separated list on the row (set via `doc tag`).
   Simple, searchable via FTS, no join-table ceremony until it hurts.
6. **Extraction scope for v0.2.x:** UTF-8 text (.txt/.md/code-ish) extracted directly;
   PDF/DOCX deferred to v0.2.3 with its own dependency ADR. Binary/unextractable files
   still get indexed by name+metadata (`kind='binary'`, empty body).
7. **Deletion semantics:** `doc remove` deletes only the index row; the file stays
   (user owns files). A `--delete-file` flag may come later behind confirmation.

## Consequences

- Re-indexing is O(dirty docs), triggered by real commands, so the CLI stays snappy.
- FTS5 gives us ranked full-text search for free (bm25); we merge ranks with
  filename/tag signal weights in v0.2.2.
- Paths are the identity; moving a file outside Machie shows up as missing+new rather
  than silently followed. Acceptable for v0.2; a `doc move` convenience can land later.
- Single-user local trust model: we index whatever the user points us at; extraction
  output is stored but treated as untrusted data (Part 0.4) when fed to models later.
