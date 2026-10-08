# ADR-0001: Rust as the primary language

- Status: accepted (2026-10-08)
- Context: Owner decision for the Machie rebuild; hobby project, single-maintainer, vibe-coded.
- Decision: All application logic is written in Rust. The workspace is a Cargo workspace
  (`crates/*`). Other languages are only permitted via FFI or as a thin UI layer, and only
  when a feature cannot reasonably be implemented in Rust. Release binaries are built with
  `strip`, `lto`, `codegen-units = 1`, `opt-level = "s"` for size efficiency.
- Consequences: rusqlite uses the `bundled` SQLite so builds need no system SQLite; C toolchain
  required to compile bundled SQLite (cc on macOS/Linux, MSVC on Windows).
