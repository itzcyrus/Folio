# ADR-0002: CLI-first, GUI deferred

- Status: accepted (2026-10-08)
- Context: The old spec assumed a Tauri 2 + React desktop shell from day one. That is far too
  heavy for incremental vibe-coding.
- Decision: Ship a CLI binary (`machie`) first. Any future GUI (Tauri or otherwise) will be a
  thin shell over the same core crates, introduced no earlier than ~v0.5 behind its own ADR.
  Core logic never depends on any UI (per the old spec's Part B, which we keep as good sense).
- Consequences: v0.1.x–v0.4.x are terminal apps; install instructions stay simple
  (`cargo install --path` / prebuilt binary).
