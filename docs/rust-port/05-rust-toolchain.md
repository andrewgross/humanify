# Toolchain, workspace, and code standards

How the Rust code is built, linted, tested, and gated — and exactly where it
plugs into the one existing gate. The crate shape comes from
`02-rust-target-architecture.md` (section 2); the phase gates it must serve are
in `03-migration-plan.md`. Two goals order every choice below, in this
priority: **confidence** (exact, non-statistical checks wherever a check
exists) and **speed** (an edit-to-verdict loop measured in seconds).

The governing repo rule carries over unchanged: `npm run check` is the gate,
and if a check is not in `STAGES` in `scripts/check.ts` it does not run
(CLAUDE.md; `scripts/check.ts:56-98`). The Rust toolchain does not get a
second gate beside it — it gets four new entries in that array (section 8).

## 1. Workspace layout

The Cargo workspace lives at the repo root, beside the npm project, for the
whole migration; `src/` is deleted at phase 6 (`03-migration-plan.md`), the
workspace stays.

```
humanify/
  Cargo.toml            # [workspace] virtual manifest
  Cargo.lock            # COMMITTED — this builds binaries, not a library
  rust-toolchain.toml   # the pin (section 2)
  rustfmt.toml          # section 3
  clippy.toml           # section 4
  deny.toml             # section 5
  bacon.toml            # section 7
  crates/
    humanify-core/      # pipeline: model, hash, graph, match, name, place, emit
    humanify-llm/       # provider client, cache, rate limiting, wave scheduler
    humanify-cli/       # arg parsing, config, kill switches, subcommands
  src/ package.json ... # the TS project, untouched until phase 6
```

Coexistence facts, checked against the current tree:

- **`.gitignore` gains one line: `/target/`.** The current file has no target
  entry (it ignores `dist`, `output`, `node_modules/`, eval logs — repo
  `.gitignore`). `Cargo.lock` is committed, never ignored.
- **The JS toolchain cannot wander into `crates/`.** Biome is scoped to
  `src/**`, `test/**`, `experiments/lib/**`, `scripts/**` (biome.json:40-49);
  prettier's lint script globs the same roots (package.json:20); knip ignores
  what its config names and analyzes TS/JS only. No JS-side config change is
  needed. One benign overlap: lint-staged runs prettier on staged `*.md` and
  `*.json` anywhere (package.json:91-94), which may touch docs inside
  `crates/`; insta snapshots are `.snap` files and are not matched.
- Workspace manifest sketch (`resolver = "3"` is the edition-2024 resolver;
  a virtual manifest must name it explicitly):

```toml
[workspace]
resolver = "3"
members = ["crates/humanify-core", "crates/humanify-llm", "crates/humanify-cli"]

[workspace.package]
edition = "2024"
rust-version = "1.92"   # example — always equals the toolchain pin (section 2)

[workspace.lints.clippy]
cognitive_complexity = "deny"   # nursery lint: silent unless raised (section 4)
```

Member crates set `[lints] workspace = true`. Cargo's lints inheritance is
all-or-nothing — a crate cannot inherit the table and override one entry — so
per-crate deviations are crate-root attributes (`#![forbid(unsafe_code)]` in
`humanify-core`, per `02-rust-target-architecture.md` section 9), which also
keeps every deviation visible in the source it applies to.

## 2. Toolchain pin: rust-toolchain.toml

```toml
[toolchain]
channel = "1.92.0"   # example — pin whatever stable is current on adoption day
components = ["clippy", "rustfmt", "llvm-tools"]
profile = "minimal"
```

Policy:

- **Pin an exact stable version, never a channel name.** `channel = "stable"`
  ages the same way `baseline-main` aged ("current main" from 2026-07-21 with
  no run-status files — CLAUDE.md's own cautionary label). Everyone — the
  maintainer, Claude agents, CI, the bahadur devcontainer — resolves the
  identical compiler, because rustup auto-installs from this file on the first
  `cargo` invocation.
- **MSRV is the pin.** `rust-version` in `[workspace.package]` equals the
  pinned version exactly; there is no supported range. This is a binary
  shipped from this repo, not a library with downstream consumers.
- **Toolchain bumps are scheduled events**, handled like oxc bumps (section
  5): a dedicated commit that re-runs `npm run check` (which includes the
  fixture parity stage, section 8), and — post-port — a neutrality run,
  because a compiler upgrade is exactly a should-change-nothing edit.
- `llvm-tools` — rustup resolves it to the official component name
  `llvm-tools-preview` via a rename mapping, so either spelling works in this
  file. It is there for `cargo-llvm-cov` coverage later; it costs nothing to
  pin now and avoids a toolchain re-download when coverage is first wanted.

## 3. rustfmt: defaults, one-line config

`rustfmt.toml`, complete contents:

```toml
edition = "2024"   # bare `rustfmt` (lint-staged) does not read Cargo.toml
```

Everything else is defaults, deliberately. The precedent is the repo's own
`.prettierrc`, whose sole content is one setting (.prettierrc:1-3): formatting
is a solved problem and every option is future churn. Two concrete reasons to
hold this line:

- **Default rustfmt output is what every editor and every agent produces**
  with zero configuration, so formatting diffs cannot appear between authors.
- **No unstable options, ever.** Unstable rustfmt options require nightly and
  reformat when the pin bumps — formatting churn in a repo whose entire
  product is diff stability.

## 4. Clippy policy: the house rules as lints

The port's stated aim is making review rules structural
(`02-rust-target-architecture.md`, opening question). Lints are the cheapest
form of that. Total policy: six enforced rules — four via `clippy.toml` +
lint levels, two via crate-root attributes — with warnings fatal at the gate
only.

| lint / setting                                                                              | where configured                                                                    | level                | house rule it enforces                                                                                                                                                                                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `clippy::cognitive_complexity` with `cognitive-complexity-threshold = 15`                   | `clippy.toml` + `[workspace.lints.clippy]`                                          | deny                 | biome's `noExcessiveCognitiveComplexity` ceiling of 15 (biome.json:13-20; CLAUDE.md "complexity <= 15"). Nursery lint, off by default — it must be raised explicitly or it enforces nothing.                                                                                                                                                                     |
| `clippy::disallowed_types`: `std::collections::HashMap`, `std::collections::HashSet`        | `clippy.toml`                                                                       | warn (fatal at gate) | the determinism hazard of `02-rust-target-architecture.md` section 5: HashMap iteration order is randomized per process; decision paths iterate `BTreeMap`/`IndexMap`/ID-ordered arenas. The ban catches hasher-swapped aliases too (the lint resolves the underlying type), which is intended — a fixed-seed map is still insertion-order-fragile.              |
| `clippy::disallowed_methods`: `std::env::{var, var_os, vars, vars_os, set_var, remove_var}` | `clippy.toml`                                                                       | warn (fatal at gate) | the kill-switch registry rule. Today a test greps all of `src/` for `process.env` and fails on any reader outside a named allow-list with a stated reason (src/kill-switches.test.ts:95-134). The lint is the same guard with the allow-list spelled `#[allow]` + comment at the one env module (`02-rust-target-architecture.md` section 3 maps this directly). |
| `clippy::unwrap_used`                                                                       | `#![deny]` in `humanify-core` root; `allow-unwrap-in-tests = true` in `clippy.toml` | deny (core only)     | biome's `noNonNullAssertion` error (biome.json:30-37): no "trust me" narrowing on the decision path. `expect("why this cannot fail")` stays legal — the message is the justification the TS allow-lists demand in comment form.                                                                                                                                  |
| `unsafe_code` (rustc lint)                                                                  | `#![forbid]` in `humanify-core` root                                                | forbid               | "No `unsafe` in `humanify-core`" (`02-rust-target-architecture.md` section 9). `forbid` cannot be re-allowed downstream, which is the point.                                                                                                                                                                                                                     |
| warnings-as-errors                                                                          | the gate command only: `cargo clippy --workspace --all-targets -- -D warnings`      | gate/CI only         | unused imports/variables/parameters are errors in biome today (biome.json:25-29). In Rust they stay warnings in the editor and under `bacon` — a half-finished function must not block the edit loop — and become fatal at `npm run check` and in CI, where the repo has always enforced them.                                                                   |

`clippy.toml`, complete contents:

```toml
cognitive-complexity-threshold = 15
allow-unwrap-in-tests = true

disallowed-types = [
  { path = "std::collections::HashMap", reason = "iteration order is randomized per process; decision paths use BTreeMap/IndexMap/ID-ordered arenas (02-rust-target-architecture.md section 5). Non-decision uses: #[allow] the site with a comment saying why order cannot matter." },
  { path = "std::collections::HashSet", reason = "same hazard as HashMap; use BTreeSet or IndexSet." },
]

disallowed-methods = [
  { path = "std::env::var",        reason = "environment access goes through the env module only — the kill-switch registry rule (TS analog: src/kill-switches.test.ts)." },
  { path = "std::env::var_os",     reason = "see std::env::var." },
  { path = "std::env::vars",       reason = "see std::env::var." },
  { path = "std::env::vars_os",    reason = "see std::env::var." },
  { path = "std::env::set_var",    reason = "the process never mutates its own environment; configuration is argv." },
  { path = "std::env::remove_var", reason = "see std::env::set_var." },
]
```

Where the banned things ARE allowed: `humanify-llm` and `humanify-cli` may
`#[allow(clippy::disallowed_types)]` at specific sites (in-flight request
maps, cache indexes) with a comment stating why iteration order cannot reach a
decision — mirroring the TS test's each-exception-names-its-reason discipline
(src/kill-switches.test.ts:100-112). The env module in `humanify-cli` carries
the single `#[allow(clippy::disallowed_methods)]`. There is no blanket
crate-level allow for either.

One honest caveat, pre-registered: clippy's cognitive-complexity metric is not
biome's metric. A function at biome-15 may score differently under clippy-15.
The threshold starts at 15 for rule continuity and gets calibrated during
phase 1 against ported functions that biome accepted; if it moves, the commit
that moves it says why (see Open questions).

## 5. Dependency policy

Fifteen proposed dependency entries total (counting the seven `oxc_*` crates
as one toolkit) — eleven runtime, four dev-only:

| crate(s)                                                                                            | purpose                                                                                                                                                                                                        | tier       |
| --------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| `oxc_parser`, `oxc_semantic`, `oxc_traverse`, `oxc_codegen`, `oxc_allocator`, `oxc_ast`, `oxc_span` | the toolkit the target architecture is built on (`02-rust-target-architecture.md` section 1)                                                                                                                   | core       |
| `rayon`                                                                                             | data-parallel evidence generation (`02-rust-target-architecture.md` section 6)                                                                                                                                 | core       |
| `serde`, `serde_json` (with `preserve_order`)                                                       | the frozen JSON contract — stats, ledgers, trails must keep the TS shapes, and `preserve_order` (indexmap-backed) keeps key order deliberate, matching JS insertion-order emission                             | core       |
| `indexmap`                                                                                          | deterministic insertion-ordered maps on decision paths                                                                                                                                                         | core       |
| `thiserror`                                                                                         | per-module errors, loud at the CLI boundary (`02-rust-target-architecture.md` section 9)                                                                                                                       | core       |
| `xxhash-rust` (XXH3)                                                                                | the hash function under the three key newtypes; the choice is free because phase 1 gates on partition equality, not hash bytes (`03-migration-plan.md` phase 1)                                                | core       |
| `tokio`, `reqwest`                                                                                  | async LLM client. Deliberately NOT an OpenAI SDK crate: a hand-rolled client is what keeps request bytes — and therefore the shared cache key — under our control (`02-rust-target-architecture.md` section 7) | core (llm) |
| `clap`                                                                                              | the `commander` analog for `humanify-cli`                                                                                                                                                                      | core (cli) |
| `anyhow`                                                                                            | error aggregation at the binary boundary only, never in `humanify-core`                                                                                                                                        | core (cli) |
| `insta`                                                                                             | snapshot tests: prompt renders, canonical hash serializations, trail JSON                                                                                                                                      | dev        |
| `similar`                                                                                           | diff rendering inside parity test failures                                                                                                                                                                     | dev        |
| `tempfile`                                                                                          | tree-emitting tests                                                                                                                                                                                            | dev        |
| `trybuild`                                                                                          | compile-fail proofs that the type-level promises hold (`06-testing-strategy.md` §1, gate-mandatory there)                                                                                                      | dev        |

What remains outside `Cargo.toml` is installed tooling: `cargo-nextest`
(runner), `cargo-deny` (audit), `cargo-insta` (snapshot review), `bacon`
(watcher), `cargo-mutants` (on-demand mutation testing over the guard
modules, `06-testing-strategy.md` §5), `cargo-fuzz` (the low-priority fuzz
targets, 06 §7), optionally `sccache`, later `cargo-llvm-cov`;
`cargo-hakari` is noted only as a later option if feature-unification
rebuilds ever hurt.

**oxc pinning.** oxc is pre-1.0 and moves fast, and it sits under every hash
and every span the pipeline trusts. Policy: exact pins (`=x.y.z`) in
`[workspace.dependencies]`, plus the committed `Cargo.lock`. **An oxc upgrade
is a scheduled event, never a drive-by**: its own branch, `npm run check`
(which includes fixture parity, section 8), then the full parity gate of
whatever migration phase is current — and post-port, a warm neutrality run
plus an eval score, because a parser/codegen bump is precisely the class of
change that can move decisions while looking like a chore. This is the same
discipline the eval applies to its references (CLAUDE.md: re-score rather
than trust the name).

**cargo-deny.** `deny.toml` checks four things: `advisories` (RUSTSEC),
`licenses` (allowlist — MIT/Apache-2.0 plus BSL-1.0 for `xxhash-rust` and
Unicode-3.0 for the transitive `unicode-ident`; populate the final list from
the first `cargo deny check` run rather than trusting this enumeration),
`bans` (duplicate-version warnings), `sources` (crates.io only). It runs in
CI and by hand (`cargo deny check`), not in `STAGES`: the advisory database
is a network fetch, and the gate must stay fast and offline-clean. One
forward note: the allowlist governs the SOURCE graph; distributing a compiled
binary (`08-operations-runbook.md` §9, deferred) adds notice/attribution
obligations per bundled license — generate a third-party notices file (e.g.
`cargo-about` (verify)) as part of whatever distribution option is chosen,
and list licensing among that decision's factors.

## 6. Build profiles and compile times

```toml
[profile.dev]
# defaults: opt-level 0, incremental — the edit loop

[profile.dev.package."*"]
opt-level = 3   # dependencies (oxc, tokio) compiled optimized ONCE, then cached;
                # parsing a 19.5 MB bundle under an opt-0 oxc would make every
                # fixture run painful for zero rebuild benefit

[profile.release]
lto = "thin"    # fat LTO / codegen-units=1 buy little here and slow the build

[profile.profiling]
inherits = "release"
debug = true    # release speed + symbols, for samply / Instruments on macOS
```

Expected compile times — **all estimates; no measurement exists until the
first workspace commit, at which point these numbers get replaced with
measured ones** (the repo's own rule about projections,
`04-performance-model.md` closing section):

| operation                                       | estimate              |
| ----------------------------------------------- | --------------------- |
| cold `cargo build` (full dep tree, laptop)      | 2–5 min               |
| cold `cargo clippy --workspace`                 | similar to cold build |
| warm `cargo check` after a `humanify-core` edit | 2–10 s                |
| warm `cargo clippy`                             | 5–20 s                |
| `cargo nextest run -p humanify-core` (warm)     | seconds               |
| cold `cargo build --release`                    | 3–8 min               |

The levers, in the order to reach for them: incremental compilation (on by
default in dev — do not disable it), `cargo-nextest` as the test runner
(faster scheduling, per-test process isolation, clean failure output),
`sccache` (`RUSTC_WRAPPER=sccache`) only if cold builds across worktrees or
the laptop/devcontainer pair become a real cost, and `cargo-hakari` only if
feature unification ever causes visible rebuild cascades — both of the last
two are deferred until the pain is measured, not adopted on spec.

## 7. The dev loop

The develop-quickly payoff, concretely. Steady state while porting a module:

1. **Watch:** `bacon` in a side terminal — `bacon clippy` for diagnostics, or
   the nextest job below for tests-on-save. (`cargo watch -x "nextest run -p
humanify-core"` is the fallback if bacon is unavailable.)

```toml
# bacon.toml
[jobs.nextest]
command = ["cargo", "nextest", "run", "-p", "humanify-core"]
need_stdout = true
```

2. **Test the touched crate, seconds:** `cargo nextest run -p humanify-core`
   — or one module: `cargo nextest run -p humanify-core hash::`. Red/green
   TDD unchanged (CLAUDE.md development workflow; `02-rust-target-architecture.md`
   section 9 keeps tests colocated as `#[cfg(test)]`).
3. **Snapshot review when output shapes change intentionally:**
   `cargo insta review`. Snapshots cover the external contracts — prompt
   renders (a contract with the LLM cache, `02-rust-target-architecture.md`
   section 7), canonical hash serializations, trail/stats JSON. A snapshot
   diff on any of these is a contract change and gets the scrutiny of one.
4. **Fixture-scale parity, tens of seconds:**
   `tsx scripts/rust-parity.ts --fixture <name>` — runs the Rust binary on a
   committed fixture, diffs its decision dump against the committed TS oracle
   dump (`03-migration-plan.md` phase 0), prints the first divergence with
   `similar`-rendered context. Exact equality, no thresholds.
5. **Before commit:** `npm run check` — which now includes all of the above
   at workspace scope (section 8). Iterating on one corner first:
   `npm run check -- --only rust:fmt,rust:clippy,rust:unit`, which the
   summary labels PARTIAL exactly so it cannot be mistaken for green
   (scripts/check.ts:191-198).

Steps 1–4 are seconds each. That is the property `03-migration-plan.md`
("Sequencing realities") already banks on — phases 1–3 gates "checkable in
seconds per run" — and this section is where it is cashed.

## 8. check.ts integration

The gate grows from 8 stages to 12. Adding a stage is one entry in `STAGES`
(CLAUDE.md), the interface is
`{ name, why, run, advisory? }` (scripts/check.ts:34-48), and the summary
counts derive from `STAGES.length` (scripts/check.ts:195-198) — so the four
entries below are the entire integration. They land in the same commit that
creates the workspace; none is advisory.

```ts
{
  name: "rust:fmt",
  run: "cargo fmt --all --check",
  why: "rustfmt defaults across crates/ — the prettier analog; formatting drift never reaches review"
},
{
  name: "rust:clippy",
  run: "cargo clippy --workspace --all-targets -- -D warnings",
  why: "the house rules as lints: complexity ceiling, HashMap/env bans, no unsafe in core; warnings fatal at the gate only"
},
{
  name: "rust:unit",
  run: "cargo nextest run --workspace",
  why: "every #[cfg(test)] and tests/ target in crates/ — the Rust half of what `unit` promises for TS"
},
{
  name: "rust:parity",
  run: "tsx scripts/rust-parity.ts",
  why: "Rust decisions byte-equal the committed TS oracle dumps on the fixture corpus — exact, per the 03 phase gates"
},
```

Placement in the array keeps the cheapest-first ordering by kind: `rust:fmt`
and `rust:clippy` directly after `lint`, `rust:unit` after `unit`,
`rust:parity` after `fingerprint` and before `e2e`. The existing `--only`
selector mechanism picks them up by name with no further work
(scripts/check.ts:100-114).

Notes on the parity stage:

- `scripts/rust-parity.ts` is new work, created in phase 0/1
  (`03-migration-plan.md`) alongside `--dump-artifacts`. Until it exists the
  first three stages still land — a workspace with no gate is how drift
  starts.
- It compares **fixture-scale** corpora only, so the stage stays in seconds
  and the whole gate stays ~25s-class (CLAUDE.md). The four-pair oracle
  parity runs — multi-GB trees, warm caches — are phase-gate instruments run
  on the workstation, not check stages, exactly as the eval and neutrality
  are today.
- Its comparisons are exact-equality over deterministic artifacts, which is
  what keeps it out of reach of measurement rules 10 and 11
  (`docs/measurement-pitfalls.md`): no LLM variance is present (dumps are
  mechanical-decision records), so there is no noise floor to resolve and no
  cache to lie with.

**lint-staged** gains one entry beside the existing two (package.json:91-94):

```json
"*.rs": "rustfmt"
```

Bare `rustfmt` formats the staged files in place and picks up
`rustfmt.toml` — including the edition line, which exists precisely because
this invocation never sees `Cargo.toml` (section 3).

## 9. CI: GitHub Actions

Today the repo has **no CI at all** — `.github/` does not exist, and the only
automated gates are the husky pre-commit hook and manually-run
`npm run check`. The port is the moment to add a minimal workflow, scoped to
what hosted runners can actually verify.

Proposed workflow (`.github/workflows/rust.yml`): matrix over `macos-latest`
and `ubuntu-latest` — macOS because development is on darwin, Linux because
the bahadur devcontainer is where long runs live, and the pair flushes out
platform-order bugs (directory listing order, filesystem case-sensitivity) that the
HashMap ban alone does not cover. Steps:

1. checkout; rustup resolves `rust-toolchain.toml` automatically (the pinned
   toolchain, section 2)
2. `Swatinem/rust-cache` for the cargo cache
3. `taiki-e/install-action` for `cargo-nextest` and `cargo-deny`
4. `cargo fmt --all --check`
5. `cargo clippy --workspace --all-targets -- -D warnings`
6. `cargo nextest run --workspace`
7. `cargo deny check`
8. Node 22 + `npm ci`, then `tsx scripts/rust-parity.ts` — the fixture parity
   stage, which needs the TS side present but no network and no model

What CANNOT run in CI, stated explicitly so nobody wires it up and trusts a
green badge:

- **Big-bundle parity** (the four oracle pairs): multi-GB inputs, dumps, and
  output trees; minutes-to-hours per pair.
- **Anything touching the LLM**: the eval (`npm run eval -- score`),
  neutrality runs, warm-replay equivalence (phase 4) — the model is a local
  vLLM endpoint on the LAN, unreachable from hosted runners, and per
  measurement rule 10 a cached stand-in would make the verdict a lie anyway.
- **Walk segments** (phase 6 validation).

Those run on the workstation or the bahadur devcontainer. The container
mirrors the laptop's paths — a standing project constraint that is
load-bearing here: warm LLM caches, oracle dumps, and eval references carry
between the two machines without path surgery. CI is therefore an added
platform-coverage net, not the gate; **the gate remains `npm run check` run
locally before commit**, same as today.

## 10. rust-analyzer

Three settings, then it is done:

- `rust-analyzer.check.command`: `"clippy"` — the editor shows the same
  diagnostics the gate enforces, instead of a second opinion.
- `rust-analyzer.cargo.targetDir`: `true` — rust-analyzer builds into its own
  target subdirectory so editor analysis never holds the lock a terminal
  `cargo nextest run` is waiting on.
- No `linkedProjects` needed: the workspace `Cargo.toml` is at the repo root
  and discovery finds it.

Since `.idea/` is already gitignored and no editor config is committed today,
these live in personal editor settings; documenting them here is the shared
copy.

## Open questions

1. **Clippy-15 vs biome-15.** The two cognitive-complexity metrics differ;
   the calibrated threshold after phase 1 may not be 15. Whoever calibrates
   it records the mapping in the commit that changes it.
2. **Umbrella `oxc` crate vs individual `oxc_*` crates.** The umbrella crate
   with feature flags may version more cleanly than seven exact pins that
   must move in lockstep. Decide at workspace creation; either way the pin is
   exact.
3. **Oracle-dump size vs the repo.** Fixture-scale dumps should be small
   enough to commit; if any are not, the parity script needs a
   content-addressed store on the workstation and CI runs a reduced fixture
   set. Measure at phase 0 before choosing.
4. **Doctests.** `cargo nextest` does not run them. Default position: no
   doctests in `humanify-core` (examples live in unit tests); revisit only if
   rustdoc examples accumulate anyway, in which case `rust:unit` gains
   `&& cargo test --doc`.
5. **A `rust:deny` check stage.** cargo-deny stays CI/manual for now because
   of the advisory-DB fetch; if an offline-cached mode proves reliable it
   could join `STAGES` as advisory.
6. **sccache adoption.** Deferred until laptop/devcontainer cold-build cost
   is measured and hurts; the levers list (section 6) orders it after
   nextest for a reason.
