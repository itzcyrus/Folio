# ADR-0004: SQLite with forward-only migrations

- Status: accepted (2026-10-08)
- Context: Local-first single-file database; old spec Part J.3 chose SQLite. Kept — it is the
  right call and costs nothing.
- Decision: One database file at `<data_dir>/machie.db`. Schema tracked by the
  `PRAGMA user_version` integer; migrations are numbered, applied automatically on open, and
  move forward only. If the file carries a version newer than the running build understands,
  Machie refuses to open it (never silently downgrades or corrupts). WAL journal, foreign keys
  ON, busy timeout 5 s.
- Consequences: adding a table = append a numbered block in `migrate()` and bump
  `SCHEMA_VERSION`. Never edit an already-shipped migration block.
