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
- **Governance**: ADRs, lightweight router evals, minimal review gates. These exist so an
  incremental build (across many sessions, human or agent) cannot quietly drift from the
  architecture. For a vibe-coded hobby project they're still useful — but trimmed to the
  lightest set that actually protects the architecture (§2).

What we deliberately **defer or trim** (each with its own ADR when actually decided):

- **The spec's heavy testing apparatus is cut.** Parts Q.6/Q.9 demand golden eval suites,
  security/privacy/regression test batteries, mandatory coverage and multi-stage review
  gates before *any* milestone ships. For a single-user hobby app that is disproportionate
  overhead. New policy (§2, Deviation #3): `cargo build` + `cargo test` green is the only
  gate; bugs get fixed as PATCH releases discovered during real use by the one user (me).
- The full Tauri 2 + React desktop shell is heavy for the first iterations. The spec's
  Phase 0 asks for it upfront; we front-load the *core* instead (see §3) because the core
  has no UI dependency by design (Part B explicitly allows swapping the presentation
  layer later without a core rewrite). When we do pick a shell, we re-evaluate Tauri vs.
  a pure-Rust GUI (egui/Slint) for size/speed — recorded as an ADR, not silently.
- Providers beyond local llama.cpp, gateways, OpenCode, voice, personality layer,
  autonomy — all kept abstractly compatible, none built early (Part R says the same).

## 2. Ground Rules (agreed conventions)

### 2.0 Working model (owner decision — read this first)

Machie is an **intended vibe-coded app**: solo-built by the agent, for the owner, with no
code review and no manual testing duty on the owner's side. Concretely:

- The owner does **not** read docs or review code. These documents exist for *me* — they
  are my memory across sessions, not deliverables to be approved. They must stay short
  enough that re-reading them is cheap.
- The owner does **not** run test suites, evals, or checklists on my behalf. I never ask
  "please try X and tell me what happens." Verification is entirely my job, in-sandbox,
  before anything is committed or tagged.
- The owner's only involvement is **optional, opportunistic**: at certain checkpoints they
  may pull the repo (or download release files) and simply *use* `machie` on their own
  machine. If something misbehaves in daily use, they mention it; I fix it and ship a
  PATCH. There is no obligation to do this after every change, feature, or patch — ever.
- Therefore: **every release must be safe-to-run-unattended.** The README carries
  spoon-fed install/upgrade/uninstall instructions (exact commands, copy-pasteable).
  Anything I ship must degrade gracefully: bad input → clear error message, not a panic;
  missing model/config → helpful hint about what to do, not a stack trace. First-run
  experience matters more than any test battery, because first-run-by-the-owner *is*
  the test battery.
- Checkpoints where I proactively say "this one's nice to try, here's how" are fine;
  checkpoints where I *depend* on owner feedback to proceed are not allowed. If I need
  information, I get it from the sandbox, the code, or the spec — not from the owner.

| Concern | Convention |
|---|---|
| Language | Rust is the primary implementation language for everything, wherever possible. Non-Rust languages only where Rust genuinely can't do the job (per-spec C/C++ libs via FFI like llama.cpp/FFmpeg; a thin UI layer if a web stack wins the shell ADR), never for business logic. |
| Versioning | Semantic versioning `MAJOR.MINOR.PATCH` in workspace `Cargo.toml`. While `0.x`, MINOR = phase, PATCH = milestone **and** bug-fix release (Rust convention: breaking-ish changes allowed under 0.x, documented in changelog). Errors found during real use ship as PATCH releases. Tag every release: `v0.1.0`, `v0.1.1`, … |
| Releases | One release = one feature or one completed milestone; bug fixes ship as small PATCHes whenever they're noticed in daily use. Tagged releases carry copy-pasteable install instructions in the README. Since the owner may skip releases entirely and pull whenever they feel like it, the *latest tag* must always be a working superset — never tag something I haven't run myself end-to-end. Small, frequent commits; each commit leaves the workspace building green. |
| Process | Simplified loop: PLAN → IMPLEMENT → BUILD+TEST+RUN-MYSELF → SHIP → NEXT MILESTONE. Gate is `cargo build && cargo test` passing **plus me actually running the shipped binary/CLI paths in-sandbox** before tagging — since nobody else will smoke-test it for me. No mandated coverage targets, no security-review batteries, no eval-accuracy thresholds blocking merges (deviation from Parts Q.6/Q.9 — see §5.3). |
| Testing | Minimal and pragmatic: unit tests come free with writing Rust (`#[test]` on pure logic where obvious), plus a handful of smoke tests per crate. No TDD, no golden-suite governance, no test-before-code rules. Tests exist to catch *my* regressions cheaply, not to satisfy a process. Anything that feels like ceremony gets deleted. |
| Decisions | Every architecturally significant choice → `docs/decisions/ADR-NNNN-title.md` using the Part Q.1 template. Read existing ADRs before contradicting one. |
| Config | TOML, `config_version = 1`, migration registry from day one (Part Q.5). |
| DB | SQLite, accessed only from Rust, migrations required (Parts B/K.5). |
| Router | The ANSWER/SEARCH/ACTION router keeps a tiny hand-written eval file (`evals/router/*.jsonl`) purely as a regression sanity check I can run manually — it never blocks a release. |
| Trust | Everything from an LLM/doc/web/tool output is **untrusted data**, never instructions (Part 0.4). |

## 3. Milestone Roadmap (small slices, each ships something)

Each milestone below maps onto the spec's phases but cuts them into smaller, individually
releasable pieces. ✅ = done, 🚧 = current, ☐ = planned.

### v0.1.x — Engineering Foundation (spec Phase 0)
- ☐ **0.1.0** Repo scaffolding: Cargo workspace (`crates/*` created *as needed*, not
  empty placeholder crates — deviation from Phase 0 item 1 noted in §5),
  `.gitignore`, rustfmt/clippy config, minimal CI (build + test only), and the README
  "Install / Upgrade / Uninstall" section written spoon-fed-style from day one (even if
  it just says "nothing to install yet" — the section exists and stays current per §2.0).
- ☐ **0.1.1** Governance files kept lean: ADR-0001 (tech stack),
  ADR-0002 (gateway independence), ADR-0003 (product naming), plus this PLAN. No
  heavyweight DEFINITION_OF_DONE / REVIEW_GATES ceremony beyond "build+test green".
- ☐ **0.1.2** `crates/config`: TOML config crate, `config_version = 1`, migration
  registry stub, a couple of unit tests.
- ☐ **0.1.3** `crates/database`: SQLite connection handling + migration tool + initial
  empty migration, a couple of unit tests.
- ☐ **0.1.4** Router eval sanity-check skeleton: `evals/router/*.jsonl` schema, loader +
  pass/fail reporter (reports 0/0 for now), CLI subcommand to run it manually.
- ☐ **0.1.5** First CLI binary `machie` (thin `apps/cli`): `version`, `config show`,
  `db init`, `evals run`. Proof the whole foundation wires together. **Phase 0
  checkpoint:** stop & report status/deviations.

### v0.2.x — Core Intelligence (first slice of spec Phase 1)
- ☐ **0.2.0** Structured Task Protocol types (Part C.4) + error model (Part K.4).
- ☐ **0.2.1** Intent/Task Router: classification into ANSWER/SEARCH/ACTION, seed the
  small hand-written eval file with a dozen real examples (sanity check only).
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
- ☐ Deterministic tools + permission system (Phase 5; the one place I keep a few extra
  adversarial spot-checks, since "untrusted input → shell/file ops" is where hobby apps
  actually get burned — still no formal security-test battery).
- ☐ Web research (Phase 6) — first time anything may leave the machine; disclosure UX (L.6).
- ☐ Gateway/cloud providers (Phase 7) — only after ADR-0002 constraints verified.
- ☐ Presentation layer decision (ADR): Tauri 2 + React/TS/Tailwind vs. native Rust GUI.
  Spec default is Tauri; we revisit once a CLI core is solid. Either way the core stays UI-free.
- ☐ OpenCode, Voice, Personality Layer, Autonomy (Phases 8–11) — last, always opt-in.

## 4. Environment Notes

- Sandbox currently lacks `cargo`/`rustc` (Node 20 present). Step zero of the first
  working session: install Rust toolchain (`rustup`) so the build+test gate can run.
- CI targets GitHub Actions initially, deliberately minimal: one job running
  `cargo build --workspace` and `cargo test --workspace`. No clippy-deny or fmt gates;
  those are nice-to-haves I'll run locally only if they ever seem worth it.

## 5. Change Log of Plan Deviations from the Frozen Spec

1. **Crates created on demand, not all upfront.** Part K.7 itself permits this: "do not
   create directories or crates solely because this document mentions them if they end up
   unused." Phase 0 lists empty crates; we start with `config`, `database`, and later add
   `core`/`router`/`providers`/… as milestones need them.
2. **CLI-first, desktop-shell-later.** Phase 0 bundles the Tauri scaffold immediately; we
   defer the shell to ~v0.5 pending an ADR. Justification: Part B guarantees the core is
   presentation-independent, and a CLI exercises every core gate (router, config, DB,
   evals) with far less toolchain weight — better fit for "short code changes, steadily."
3. **Heavy testing/governance apparatus removed** (owner decision, this revision). Parts
   Q.4/Q.6/Q.9 mandate golden eval suites blocking merges, security/privacy/regression
   test batteries, coverage expectations, and multi-stage hard review gates before any
   milestone ships. That machinery assumes a team and external users. Machie has exactly
   one user who discovers bugs by using the app; those fixes ship as semver PATCH releases
   (`v0.1.1`, `v0.1.2`, …). Replaced by: build+test green as the only gate, opportunistic
   unit tests, and the tiny manual router sanity-check file. Retained from the governance
   set only what protects the *architecture* rather than the *quality bar*: ADRs, the
   untrusted-data rule, config/DB migration discipline. Rationale: ceremony tax on a
   hobby project exceeds its defect-prevention value at this scale.
4. **No owner-side QA of any kind** (owner decision, second revision). The spec's whole
   governance model implicitly assumes a reviewer/approver at every gate. Here there is
   none, by design: the owner neither reads docs/code nor performs testing; they may pull
   tagged releases and use the app opportunistically, with no obligation after any change.
   Consequences baked into §2.0: agent-side self-verification before tagging becomes the
   real quality bar (in place of review), graceful degradation and helpful error UX are
   treated as features (because unattended first-run *is* the test), and the README's
   install/upgrade/uninstall section is mandatory and spoon-fed rather than optional.
