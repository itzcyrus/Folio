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

This is a vibe-coded hobby app with exactly one user. That means:

- **You never review anything.** No code reading, no doc reading, no test running, no
  approval steps. Everything in this repo is written by the agent, for the agent.
- **You just use it, whenever you feel like it.** At checkpoints you may pull the repo or
  grab the latest release binary and run `machie` normally. There is zero obligation to
  do this after every change or feature — skip as many releases as you like.
- **Bugs found in daily use → mention them → PATCH release.** That's the whole QA loop.
- Because nobody smoke-tests on request, every tagged release is self-verified end-to-end
  before it's tagged, and error messages are written to be helpful when things go wrong
  unattended.

Development conventions:

1. Pick one milestone from the plan → implement → build + test + run it myself → tag a release.
2. Every architecturally significant decision gets an ADR before it becomes code.
3. Testing is minimal and pragmatic — unit tests where they're free, smoke tests per
   crate, no test batteries, no coverage targets, no eval-accuracy gates.
4. The router keeps a tiny hand-written sanity-check file under `evals/router/` that the
   agent can run manually — it informs development but never blocks a release.

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

✅ **v0.1.0 — Foundation shipped** (Cargo workspace, `machie-config`, `machie-db`,
`machie` CLI with status/init/config-check/session commands, ADR-0001..0004).
🎯 Direction set by the owner planning interview: the first real feature is
**Docs + Search** (deterministic full-text / filename / metadata search over your files)
— AI comes later, as an accelerator on top of solid app logic.
Next: v0.2.x doc store + `machie search`. Full roadmap in [`docs/PLAN.md`](docs/PLAN.md).

```
v0.1.x  Foundation (workspace, ADRs, config, database, `machie` CLI)   ← you are here
v0.2.x  Docs + Search (doc store, `machie doc`, multi-signal `machie search`)
v0.3.x  Core intelligence (structured tasks, ANSWER/SEARCH/ACTION router, provider traits)
v0.4.x  Local inference (llama.cpp provider, runtime basics, CLI chat)
v0.5.x  Retrieval augmentation (chunking, citations, vector/hybrid search) → MVP checkpoint
v0.6.x+ Tools & permissions → web → gateways → GUI (pure-Rust vs Tauri via ADR) → everything else
```

## Getting Machie (Install / Upgrade / Uninstall)

*This section is the only part of the docs you ever need. It is kept up to date at every
release with exact, copy-pasteable commands.*

### Install (build from source — simplest right now)

```sh
# One-time: install the Rust toolchain (then restart your shell):
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

git clone https://github.com/<your-username>/machie.git
cd machie && git checkout v0.1.0
cargo install --path crates/machie-cli     # installs the `machie` binary on your PATH
machie status                              # should print version, config and data paths
```

### First run

```sh
machie init                                # writes a starter config (~/.config/machie/config.toml)
machie session new "scratchpad"            # sessions work already; chat/tasks arrive in v0.2
```

Your data lives in one plain SQLite file plus TOML configs — locations shown by
`machie status` and `machie db-path`. Nothing leaves your machine. Ever. (v0.1 has no network code at all.)

### Upgrade

```sh
cd machie
git pull && git checkout <newest tag>      # e.g. git tag --sort=-v:refname | head -3
cargo install --path crates/machie-cli
# ...or just download the newest release tarball when prebuilt binaries start shipping,
# unpack it, and replace the old `machie` binary.
```

Config and database survive upgrades automatically; schema migrations run on first launch
after an upgrade. If a newer app ever wrote your database and you downgraded, Machie will
tell you plainly instead of corrupting anything.

### Uninstall

```sh
rm "$(which machie)"                               # remove the binary (Linux/macOS)
rm -rf ~/.config/machie ~/.local/share/machie      # optional: remove config + data
```

(Windows: delete the `machie.exe` you placed on PATH; data lives under
`%APPDATA%\machie`.) Your data is plain files + one SQLite DB — inspect or back it up
anytime before deleting.

## Building (for whoever edits the code — i.e., the agent)

Requires a Rust toolchain (`rustup`). Once the workspace exists:

```sh
cargo build            # build
cargo test             # run tests (the only gate)
```

(Toolchain setup steps land with milestone v0.1.0.)
