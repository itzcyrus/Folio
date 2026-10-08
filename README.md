# Machie

A **local-first intelligent workspace** — a personal AI operating layer for searching,
understanding, transforming, and acting on your data and computer. Built as a steady,
small-step hobby project ("vibe-coded, but governed").

## What This Repository Is

This app is based on a discarded, highly ambitious hobby-project specification for an app
called **Machie**. The full original spec lives in [`old_app_inspiration_spec.md`](old_app_inspiration_spec.md)
and remains the **frozen source of requirements** — architecture, security model, philosophy,
and roadmap. We rebuild against it incrementally instead of trying to implement it all at once.

- 📋 **The frozen spec**: [`old_app_inspiration_spec.md`](old_app_inspiration_spec.md)
- 🗺️ **The living plan & roadmap**: [`docs/PLAN.md`](docs/PLAN.md) ← start here
- 🧭 **Architecture decisions (ADRs)**: [`docs/decisions/`](docs/decisions/)

## The Approach

Mature and sensible: **one feature at a time, small updates, steadily to completion.**

1. Pick one milestone from the plan → implement → test → review → hard gates pass → release.
2. Every architecturally significant decision gets an ADR before it becomes code.
3. Nothing ships until `cargo build` / `cargo test` / `cargo clippy -D warnings` /
   `cargo fmt --check` are all green. No fake functionality, no stubs presented as done.
4. The router (the most consequential component) is guarded by golden eval sets under
   `evals/router/` — accuracy can never silently regress.

## Principles (from the spec, kept non-negotiable)

- **Deterministic first.** If a program can do it reliably, use the program. The LLM
  decides *what*; typed tools decide *how*. Never `User → LLM → arbitrary shell`.
- **Local-first.** Fully functional offline; no silent cloud fallback; explicit
  data-transfer disclosure whenever anything might leave the machine.
- **Legible.** Machie explains what it did, why it chose a path, what permissions it
  needs, and what data leaves the computer.
- **Untrusted data.** Everything from an LLM, document, web page, or tool output is data,
  never instructions.
- **Priority order:** correctness → security → local-first/privacy → maintainability →
  performance → convenience → novelty.

## Tech Stack (decided by the spec — changes require an ADR)

| Layer | Choice |
|---|---|
| Core / orchestration | **Rust** (Cargo workspace, `crates/*`) — primary language for all logic |
| Desktop shell | Tauri 2 (deferred per plan; revisit vs. native Rust GUI via ADR) |
| UI | React + TypeScript + Tailwind (presentation only — zero business logic) |
| Database | SQLite (FTS5 for search), accessed only from Rust |
| Local inference | llama.cpp (FFI or supervised subprocess) |
| Config | TOML, versioned (`config_version = 1`, migrations required) |

Other languages appear only where Rust genuinely can't do the job (C/C++ libs via FFI,
or a thin UI layer) — never for core logic.

## Versioning

Semantic versioning: **MAJOR.MINOR.PATCH**, tagged per release (`v0.1.0`, `v0.1.1`, …).
While pre-1.0: MINOR ≈ phase, PATCH ≈ milestone. See [`docs/PLAN.md`](docs/PLAN.md) for
the full roadmap.

## Status

🚧 **Phase 0 — Engineering Foundation** (repo scaffolding, governance files, config/DB
basics, router eval harness, first CLI). See the roadmap in [`docs/PLAN.md`](docs/PLAN.md).

```
v0.1.x  Foundation (workspace, CI, ADRs, config, database, eval harness, `machie` CLI)
v0.2.x  Core intelligence (structured tasks, ANSWER/SEARCH/ACTION router, provider traits)
v0.3.x  Local inference (llama.cpp provider, runtime basics, CLI chat)
v0.4.x  Documents (ingestion, chunking, FTS5, citations) → MVP checkpoint
v0.5.x+ RAG → tools & permissions → web → gateways → presentation layer → everything else
```

## Building

Requires a Rust toolchain (`rustup`). Once the workspace exists:

```sh
cargo build            # build
cargo test             # run tests
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

(Toolchain setup steps land with milestone v0.1.0.)
