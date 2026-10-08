# Machie — Working Plan (Living Document)

> This is the driving plan for rebuilding Machie as a small, steady hobby project.
> Source of inspiration and frozen requirements: [`old_app_inspiration_spec.md`](../old_app_inspiration_spec.md).
> Update this document at the end of every phase/milestone. It is a plan, not a spec —
> the spec stays frozen; deviations get recorded as ADRs in `docs/decisions/`.

---

## 1. My Take on the Project

The old spec describes a **personal AI operating layer**, not a chat UI. Its most valuable
ideas are worth keeping verbatim even at hobby scale:

- **Deterministic-first.** "If a program can do it reliably, use the program." The LLM
  decides *what*, typed tools decide *how*. This single rule prevents most of the mess
  that generic "AI wrapper" apps fall into.
- **The ANSWER / SEARCH / ACTION router** with structured tasks and task graphs. This is
  the heart of the system and the most testable piece of it.
- **Local-first + honest network policy.** No silent cloud fallback, data-transfer
  disclosure, capability flags gating the UI.
- **Governance**: ADRs, golden router evals, Definition of Done, hard review gates. These
  exist precisely so an incremental build (across many sessions, human or agent) cannot
  quietly drift from the architecture. For a vibe-coded hobby project they're even more
  important — they're what makes "little by little" safe.

What we deliberately **defer or trim** (each with its own ADR when actually decided):

- The full Tauri 2 + React desktop shell is heavy for the first iterations. The spec's
  Phase 0 asks for it upfront; we front-load the *core* instead (see §3) because the core
  has no UI dependency by design (Part B explicitly allows swapping the presentation
  layer later without a core rewrite). When we do pick a shell, we re-evaluate Tauri vs.
  a pure-Rust GUI (egui/Slint) for size/speed — recorded as an ADR, not silently.
- Providers beyond local llama.cpp, gateways, OpenCode, voice, personality layer,
  autonomy — all kept abstractly compatible, none built early (Part R says the same).

## 2. Ground Rules (agreed conventions)

| Concern | Convention |
|---|---|
| Language | Rust is the primary implementation language for everything, wherever possible. Non-Rust languages only where Rust genuinely can't do the job (per-spec C/C++ libs via FFI like llama.cpp/FFmpeg; a thin UI layer if a web stack wins the shell ADR), never for business logic. |
| Versioning | Semantic versioning `MAJOR.MINOR.PATCH` in workspace `Cargo.toml`. While `0.x`, MINOR = phase, PATCH = milestone (Rust convention: breaking-ish changes allowed under 0.x, documented in changelog). Tag every release: `v0.1.0`, `v0.1.1`, … |
| Releases | One release = one feature or one completed milestone. Small, frequent commits; each commit leaves the workspace building green. |
| Process | Per spec Part Q.9: PLAN → IMPLEMENT → TEST → REVIEW → SECURITY REVIEW → HARD GATES → NEXT MILESTONE. Hard gates (build/test/clippy -D warnings/fmt) block milestones, no exceptions. |
| Decisions | Every architecturally significant choice → `docs/decisions/ADR-NNNN-title.md` using the Part Q.1 template. Read existing ADRs before contradicting one. |
| Config | TOML, `config_version = 1`, migration registry from day one (Part Q.5). |
| DB | SQLite, accessed only from Rust, migrations required (Parts B/K.5). |
| Router | Golden eval sets under `evals/router/` must exist before the router does; no router change merges with an accuracy regression (Part Q.6). |
| Trust | Everything from an LLM/doc/web/tool output is **untrusted data**, never instructions (Part 0.4). |

## 3. Milestone Roadmap (small slices, each ships something)

Each milestone below maps onto the spec's phases but cuts them into smaller, individually
releasable pieces. ✅ = done, 🚧 = current, ☐ = planned.

### v0.1.x — Engineering Foundation (spec Phase 0)
- ☐ **0.1.0** Repo scaffolding: Cargo workspace (`crates/*` created *as needed*, not
  empty placeholder crates — deviation from Phase 0 item 1 noted in §5),
  `.gitignore`, rustfmt/clippy config, CI (build · test · clippy · fmt-check).
- ☐ **0.1.1** Governance files written verbatim from the spec: ADR-0001 (tech stack),
  ADR-0002 (gateway independence), ADR-0003 (product naming), DEFINITION_OF_DONE,
  REVIEW_GATES, plus this PLAN.
- ☐ **0.1.2** `crates/config`: TOML config crate, `config_version = 1`, migration
  registry stub, tests.
- ☐ **0.1.3** `crates/database`: SQLite connection handling + migration tool + initial
  empty migration, tests.
- ☐ **0.1.4** Router eval harness skeleton: `evals/router/*.jsonl` schema, loader +
  pass/fail reporter (reports 0/0 for now), CLI subcommand to run it.
- ☐ **0.1.5** First CLI binary `machie` (thin `apps/cli`): `version`, `config show`,
  `db init`, `evals run`. Proof the whole foundation wires together. **Phase 0 gate:**
  stop & report status/deviations.

### v0.2.x — Core Intelligence (first slice of spec Phase 1)
- ☐ **0.2.0** Structured Task Protocol types (Part C.4) + error model (Part K.4).
- ☐ **0.2.1** Intent/Task Router: classification into ANSWER/SEARCH/ACTION, seed the
  golden eval sets with real examples, wire eval runner to router (Part Q.6 finally pays off).
- ☐ **0.2.2** Provider abstraction traits (Part C.1) + capability-based model registry
  (Part C.2) — trait-level only, with a mock provider for tests.
- ☐ **0.2.3** Sessions + workspaces data model (Parts H.1/H.2) persisted in SQLite.

### v0.3.x — Local Inference (rest of spec Phase 1)
- ☐ **0.3.0** `LlamaCppProvider` via supervised subprocess (chat completion, streaming,
  cancellation per Part K.3).
- ☐ **0.3.1** Model runtime basics: load/unload lifecycle, memory estimation (Part K.2).
- ☐ **0.3.2** CLI chat loop: real ANSWER requests end-to-end, local-only. Execution
  trail printing ("why did Machie do this", Part L.5, CLI edition).

### v0.4.x — Documents (spec Phase 2)
- ☐ **0.4.0** Ingestion + extraction (TXT/MD first; PDF/DOCX after dependency ADRs).
- ☐ **0.4.1** Chunking + metadata + provenance (Part G.3).
- ☐ **0.4.2** FTS5 indexing + basic retrieval + citations (Part G.7).
- ☐ **0.4.3** Strict source mode (Part G.10 Phase-2 slice). → **MVP checkpoint** begins.

### v0.5.x+ — Later phases (spec Phases 3–11), one line each, expanded when reached
- ☐ Vector search + hybrid retrieval + reranking (Phase 3; vector index chosen here, ADR).
- ☐ Deterministic tools + full permission system (Phase 5; security tests per Q.4 mandatory).
- ☐ Web research (Phase 6) — first time anything may leave the machine; disclosure UX (L.6).
- ☐ Gateway/cloud providers (Phase 7) — only after ADR-0002 constraints verified.
- ☐ Presentation layer decision (ADR): Tauri 2 + React/TS/Tailwind vs. native Rust GUI.
  Spec default is Tauri; we revisit once a CLI core is solid. Either way the core stays UI-free.
- ☐ OpenCode, Voice, Personality Layer, Autonomy (Phases 8–11) — last, always opt-in.

## 4. Environment Notes

- Sandbox currently lacks `cargo`/`rustc` (Node 20 present). Step zero of the first
  working session: install Rust toolchain (`rustup`) so hard gates can actually run.
- CI targets GitHub Actions initially: plain `cargo fmt --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`.

## 5. Change Log of Plan Deviations from the Frozen Spec

1. **Crates created on demand, not all upfront.** Part K.7 itself permits this: "do not
   create directories or crates solely because this document mentions them if they end up
   unused." Phase 0 lists empty crates; we start with `config`, `database`, and later add
   `core`/`router`/`providers`/… as milestones need them.
2. **CLI-first, desktop-shell-later.** Phase 0 bundles the Tauri scaffold immediately; we
   defer the shell to ~v0.5 pending an ADR. Justification: Part B guarantees the core is
   presentation-independent, and a CLI exercises every core gate (router, config, DB,
   evals) with far less toolchain weight — better fit for "short code changes, steadily."
