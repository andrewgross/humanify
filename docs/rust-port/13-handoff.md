# Handoff: state of the plan on 2026-09-18

**Status: PROPOSAL — no build started, no decision made, no toolchain set up
in the environment that runs evals.** This document exists so an agent can
pick up the migration and its testing cold. It records what changed after the
plan was written, the facts that are not derivable from this repository, and
the open questions consolidated from the twelve planning docs. It does not
restate the architecture; the README's reading order does that.

## 1. What this branch is and where to start

- Thirteen docs under `docs/rust-port/`, written 2026-08-27/28 from `main` at
  `f3ebff0` (plus `feat/electron-unpacking` for the Electron notes). The
  branch touches nothing outside `docs/rust-port/`.
- On 2026-09-18 the branch was rebased onto `main` at `1671d71` with the docs
  unchanged, so a checkout of this branch carries the CURRENT `CLAUDE.md`,
  eval reference and boot gate. Section 2 lists what that rebase changed
  under the docs' feet.
- Start with [`README.md`](./README.md) (the pitch and the reading order).
  When picking up a work package, follow
  [`10-work-breakdown.md`](./10-work-breakdown.md) §4b. `PORTING.md`, the
  ledger that §4b step 1 reads, DOES NOT EXIST YET — phase 0 creates it.
- The decision the plan asks for is unchanged: run phases 0–2 (contract and
  oracle dumps in TS, then the Rust core model and matching cascade), gated
  on exact decision parity. Milestone M1's exact-match gate
  ([`10-work-breakdown.md`](./10-work-breakdown.md) §5) is the go/no-go.

## 2. What moved on `main` after the plan was written

`f3ebff0..1671d71` is seven commits, 2026-09-17 to 2026-09-19, and NONE of
them touch `src/`. The pipeline the docs inventory is byte-for-byte the
pipeline on `main` today, so every LOC count, file citation and stage table
still holds. What did change is the measurement environment around it:

| change on `main`                                                                                                                                                                                                             | commit                          | which doc statements it touches                                                                                                                                                                                                                                                                                                                                       |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| The gpt-oss-20b endpoint died 2026-09-17; the measurement LLM moved to GLM-5.3-Flash-NVFP4 on `:8100` (exp090), then BACK to gpt-oss-20b on `:8000` on 2026-09-18. GLM stays up on `:8100` for quality runs.                 | `a97ab99`, `f6b0db3`, `1813577` | 04's headline (LLM wall 75–135 s at concurrency 32, CPU share ~85%) was measured against gpt-oss-20b and is valid again as written. Against GLM, per-call output roughly doubles (cold hop ~3 h vs ~1 h 9 min per exp090), which shrinks the CPU share and the projected speedup ratio. Every speedup claim from here on must name the model it was measured against. |
| exp090 recorded that `novel` / `realLines` were byte-identical across the model swap on both hops, while `churnExBuild` and `nameOnlyLines` moved (−228 / −170 busy).                                                        | `f6b0db3`                       | Direct evidence for 07's premise that the deterministic decisions are separable from the model: the hold columns did not move. It is NOT evidence that emitted trees are model-independent — the churn columns moved — so the phase-5a byte-identity gate still needs the warm cache, exactly as 11 R14 says.                                                         |
| The cold eval reference was re-pointed to `main-2026-09-18`, scored at `1813577`. Hold columns byte-equal to all three prior references (novel 4,188 / realLn 416,377).                                                      | `fd3ac32`                       | 09 §8 names `main-2026-08-20` as the historical comparison point; it is now one reference older. The rule in `CLAUDE.md` stands: check a label's `*-run-status.json` before citing it.                                                                                                                                                                                |
| The boot gate now pins `BOOT_GATE_MODEL` (default `claude-haiku-4-5-20251001`) because since 2026-09-18 the API refuses the account-default model to any CLI older than 2.1.251 — which is every version this project walks. | `fd3ac32`                       | Every boot gate in the plan (03 phase 5b, 10 M3 and M5, 09's mini-walk) inherits this: an unpinned prompt fails on every tree regardless of what the pipeline did. The Rust-era runbook (08) must pass the model through; see `experiments/lib/boot-gate.sh`.                                                                                                         |
| exp089 (line-witness prompt hints) and exp091 (prior-name avoid-list) were REFUTED; no code merged.                                                                                                                          | `e569ebb`, `1671d71`            | Nothing in the port changes. 10 §6 ("what continues on main during the port") is unaffected; the ask-side lever family is closed, which removes one source of prompt churn during the parity era.                                                                                                                                                                     |

## 3. Referenced work that is still unmerged

- **Electron directory input** (`feat/electron-unpacking`, `2db84af`) is PR
  #1, open since 2026-08-23, not merged. 07's exemption table already says it
  "enters the oracle when merged, via re-dump". That is still the rule: if it
  merges while phase 0 is in flight, the oracle dump must be re-cut under
  07 §10's lifecycle, and 01's stage inventory gains the `electron` adapter
  and `resolveElectronAppLayout` as an unpack-stage owner.
- The plan's Electron notes (01, 08 §"Directory input") describe the state
  of that branch on 2026-08-27, which is the same commit as today.

## 4. Environment facts the repository does not record

- **Where runs happen.** Evals, walks and neutrality runs execute in a
  devcontainer on the `bahadur` server (Tailscale alias `bahadurt`), on
  `/data4/humanify/` (7.3 TB NVMe). Container paths deliberately MIRROR the
  laptop: the repo is `/Users/andrewgross/Development/humanify` inside the
  container too. Twenty-odd experiment scripts and `pairs.json` embed that
  path. Attach with
  `ssh -t bahadurt 'docker exec -it humanify-dev tmux new -A -s main'` and
  run long jobs inside tmux. The container's checkout was at `1671d71`
  (current `main`) on 2026-09-18.
- **Rust toolchain: present on the laptop, ABSENT in the container.** Laptop:
  rustc/cargo 1.94.0 via rustup, with clippy, fmt, fuzz and miri installed.
  Container: no `cargo`, no `rustc`; Node v24.18.0. 05 §2 pins an exact
  stable on adoption day and gives `1.92.0` only as an example — the pin is
  a phase-0 decision, and it has to be installed where the evals run before
  any gate can be trusted (05 §2's "toolchain must be the same everywhere"
  rule is currently unmet by construction).
- **Install wrinkle.** `isolated-vm` (transitive via `webcrack`) has never
  loaded on any machine here and fails to build on Linux + Node 24. Install
  with `npm ci --ignore-scripts && npm rebuild esbuild @swc/core`. Do not
  downgrade Node or add an override to make it compile: the exact Node pin
  protects byte-identical output.
- **The LLM server is owned, not rented.** Two vLLM endpoints on the same
  box: `http://192.168.1.234:8000/v1` serves `openai/gpt-oss-20b` (the
  measurement model, pinned in `experiments/034-eval-harness/pairs.json`);
  `:8100` serves GLM-5.3-Flash-NVFP4 (quality runs). Inside the container
  the LB is `host.docker.internal:8000` via `EVAL_ENDPOINT`. Tokens cost
  nothing; the only cost is wall-clock. Check it is up with
  `curl -s -m 3 http://192.168.1.234:8000/v1/models`; if unreachable, ask
  Andrew to power it on. API key is `dummy`.
- **Fixtures.** Claude Code bundles live under
  `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.<v>/`
  (older: `npm/package/cli.js`; newer: `binary-decompiled/src/entrypoints/index.js`).
  The multi-hop walk operates in a separate checkout,
  `~/Development/unpacked-claude-code` — this is the "walk driver outside
  this repo" that 10's open questions mention.
- **GitHub from the container** signs through a forwarded ssh-agent socket
  (re-armed at boot by `.devcontainer/setup-host-agent.sh`,
  `SSH_AUTH_SOCK` exported from `.profile`, not `.bashrc`). There is no key
  on the server to copy.

## 5. House rules that bind the port

These are in `CLAUDE.md` and `docs/`, and the plan already cites them; they
are listed here because each has bitten once:

- `npm run check` is the only gate and `scripts/check.ts` `STAGES` is the only
  list of what it covers; 05 §8 says how cargo stages join it. Nothing
  outside that list runs.
- Red/green: write the test, watch it fail, then port. 10 §4b step 4 says the
  same for gates: run the gate red first.
- No backwards compatibility, no parallel maintenance: TS is a read-only
  oracle during the port and is deleted at phase 6 (03). 02 §9 limits
  TS↔Rust artifact compatibility to migration scaffolding.
- Measurement rules that decide verdicts: `docs/measurement-pitfalls.md` rule
  8 (enumerate what the harness does not look at), rule 10 (a cache left on
  for the verdict is a lie — and the parity era is the ONE place the plan
  deliberately uses warm replay, for determinism with the model held fixed),
  rule 11 (a gate cannot resolve an effect below its own noise floor).
  Neutrality verdicts are valid only WARM, and only the baseline leg's zero
  cache writes is load-bearing.
- Reporting: totals first, remaining last. PORTING.md is specified that way
  in 10 §4. Descriptive names, never review codes, in anything written for
  a human.
- No emoji anywhere in code, docs, logs or commit messages.

## 6. Open questions, grouped by when they must be decided

Docs 05 through 12 each keep their own "Open questions" section. This is the
union, ordered by the phase that needs the answer. Where two docs disagree,
section 11 says which one wins.

**Before phase 0 opens**

- The go/no-go on running phases 0–2 at all (README). Nothing else in this
  list starts until it is made, except the probes in section 7 marked
  runnable today, which pay for themselves either way.
- How the Rust binary ingests TS-beautified text during phases 1–5a (07
  OQ1): a `--beautified-input` flag that skips stage 6, or shelling out to a
  TS beautify-only command. Coupled to 11 OQ1: comment regions come from the
  ORIGINAL pre-beautify text and beautify strips comments, so a parity-era
  Rust leg cannot recompute them. Either the dump catalog (07 §2) gains
  `commentRegions` plus the bun banner classifications, or the binary
  ingests the original text. Both must be settled before phase 1
  scaffolding, and both shape WP0.2's dump schema.
- The toolchain pin (05 §2): an exact stable chosen on adoption day; the
  `1.92.0` in the doc is an example. The laptop has 1.94.0, the container has
  nothing (section 4). Umbrella `oxc` crate vs seven individual `oxc_*`
  exact pins (05 OQ2): decide at workspace creation.
- Release-profile debug symbols: 08 §1 and §7 and 09 §6 assume
  `[profile.release] debug`; 05 §6 puts symbols on a separate `profiling`
  profile. Pick one before the workspace lands (section 11).
- WP0.2's record-format freeze decides three things at once: whether the
  `layout` table persists derived line numbers or only ranks and slice
  lengths (12 OQ); which check stage invokes `humanify-parity` and on which
  fixture set (10 OQ, at WP0.2 review); who writes
  `test/e2e/fixtures/manifest.json` and whether it pins minifier versions
  (06 OQ), which must exist before a `rust:parity` stage can.
- Oracle dump size vs the repo (05 OQ3, 07 OQ5): fixture-scale dumps should
  be small enough to commit and keep `npm run check` near 25 s. Measure at
  phase 0 before choosing a content-addressed store.
- Schema ownership of `phase-times.json` and of the scorecard's
  predicted-KPI block: 08 hands it to 09, 09 OQ5 asks whether it lands in TS
  or Rust first and never fixes a shape, 12 OQ2 defers to 09. Nobody owns it.
- Whether CI is a port deliverable: 05 §9 proposes
  `.github/workflows/rust.yml` (the repo has no CI today); 06 §1 says nothing
  in the plan depends on it.

**During phase 1**

- Clippy's cognitive-complexity 15 is not biome's 15 (05 OQ1). Calibrate
  after phase 1 instead of assuming the thresholds agree.
- Lone surrogates in bundle text (07 OQ3): a Rust `String` cannot hold one;
  no oracle bundle is known to contain one; decide WTF-8 vs fail-loud. R3's
  census found the corpus non-ASCII is BMP-only, so the supplementary-plane
  fixture must be PLANTED before the converter's green means anything (11
  OQ2). Where the UTF-16 to UTF-8 conversion lives in the TS dumper is also
  open (06 OQ).
- `src/split/cjs-emit.ts:233`'s 8-hex sha256 alias suffix (07 OQ2): confirm
  its input is path/name text, not AST serialization. If the latter, alias
  comparison moves from the byte diff to `emit.json`.
- Which of `snapshot.ts:9-38`'s fields carry into insta snapshots, and
  whether `cacheSavingsRate` / `hashCollisionRate` stay meaningful once
  canonical serialization replaces beautified-text hashing (06 OQ).
  cargo-nextest is assumed, not required (06 OQ).

**Before phase 4**

- The npm SDK's hidden inner retry (`maxRetries` 2) changes cold attempt
  counts, and a failed call halves `adaptiveBatchSize`: reproduce the layered
  envelope or consciously drop it, recorded in the pipeline contract doc (11
  OQ3, R9).
- Whether any phase needs a COLD prompt corpus beyond warm replay (07 OQ4;
  none is expected to).
- A stub `--llm-endpoint` server for binary tests vs a committed warm cache
  (06 OQ): the cache short-circuits the retry and timeout paths.
- `[QUEUE-STATE]` is a dormant log format (formatter defined, no emitter):
  re-arm it from the Rust wave scheduler or retire it (08 OQ).

**Before phase 5a and 5b**

- The fresh-grouping path (`cluster-assign.ts`, the no-prior case) cannot
  gate on the oracle pairs because all four have priors (10 OQ). Probably a
  fifth oracle input, a no-prior run of one version, dumped at WP0.4. Decide
  before 5a starts.
- `statement-twin.ts` has gates in two phases; re-cut the WP2.3/WP3.2
  boundary at phase-2 exit if the pairing/apply split is awkward in Rust
  module terms (10 OQ).
- The reconcile-on-plan flip (12 §5) needs its warm byte-identity proof
  pre-registered as a named gate in 09 when the port reaches it (12 OQ).
- `using`-desugar: port the small desugar or keep the Node scaffold step;
  the boot gate decides (02 §8, 11 R10).
- Which machines are blessed for timing bands and the criterion suite, and
  where host provenance lives in `noise-bands.json` (09 OQ2); whether the
  shared devcontainer is quiet enough for regression thresholds (09 OQ6).

**At phase 6, or deferred past it**

- Usage-error exit code: the plan pins every failure to exit 1 by
  intercepting clap's 2; confirm nothing distinguishes usage errors from
  runtime failures (08 OQ). Also the `babel-transforms` span name outliving
  Babel and `--heap-mb`'s end state in `SCORE_FLAGS` (08 OQ).
- npm distribution: napi-rs, a sidecar binary, or stop publishing, plus the
  licence-notice obligations that ride along (08 §9; 05 §5 marks
  `cargo-about` "verify"). Deferred past phase 6.
- The walk driver lives outside this repo (`~/Development/unpacked-claude-code`);
  confirm its variable name for the binary path when phase 6 opens (10 OQ).
  Fixed 20-hop segment vs rolling latest for `eval -- walk` (09 OQ3).
  Whether to parallelize the TS `analyze` step, which dominates a post-port
  sweep, and whether anything in it assumes serial execution (09 OQ1).
  Whether concurrency stays a `pairs.json` constant or becomes per-machine
  config if the server sustains more than 32 (09 OQ4).
- Doctests (05 OQ4), a `rust:deny` stage (05 OQ5, blocked on cargo-deny's
  network fetch), sccache (05 OQ6).
- Team iteration speed in Rust for post-port lever work (03 risks table):
  judge after phase 2.

**Declared unknowable until a phase reaches it** (11 §1, 10 §7): live-draw
behavior under the Rust client (only 5b sees it), multi-hop feedback (only
the phase-6 walk), agent-assisted Rust throughput on this codebase and
time-per-parity-divergence (only phase 1 measures them; 10 §7 re-forecasts
at M1).

## 7. The risk register: fourteen risks, one probe run (R14, 2026-09-19)

11 §1 says eight of the probes are runnable today, each under a day, and the
register's convention is that a result edits its row in place, dated. No row
carries a date. R3 is the one partial measurement.

| id  | risk (11 §)                                 | the cheap probe                                                                                                                                                                                                                                                                             | run?                                                                                                                                                                  |
| --- | ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1  | Babel vs oxc parse divergence (§2)          | parse all 8 corpus bundles with npm `oxc-parser` beside `@babel/parser`: error count, top-level statement count normalizing directives and `using`, MB/s; also one TS-beautified `humanified.js`. "A day-one task": the phase-1 gate sits on it. Doubles as R7's API canary per oxc upgrade | no (corpus presence verified 2026-08-28 only)                                                                                                                         |
| R2  | comment/trivia reliance (§3)                | tsx script running the existing `findCommentRegions` and bun banner classifier over the 8 bundles; count classifications that consulted an attachment                                                                                                                                       | no                                                                                                                                                                    |
| R3  | UTF-16 vs UTF-8 offsets (§4)                | generalized byte-delta scan over every text the dumps anchor; wire it as 07 §1's identity fast-path assert                                                                                                                                                                                  | partial: minified inputs are pure ASCII; 2.1.215 `humanified.js` is +69 bytes over 15 non-ASCII lines, zero surrogates. The scan and the planted fixture are not done |
| R4  | cache-key / canonical-JSON bit-parity (§5)  | JSONL of (typed request, expected key) from the TS `canonicalJson` + `keyOf`, with adversarial cases; a ~100-line scratch Rust bin re-derives every key. "Runnable today, before phase 0"                                                                                                   | no                                                                                                                                                                    |
| R5  | formatter-swap semantics at 5b (§6)         | throwaway cargo bin: oxc parse then `oxc_codegen` over one TS-beautified file; categorize divergences; re-feed and assert idempotence                                                                                                                                                       | no                                                                                                                                                                    |
| R6  | hash porting / ledger continuity (§7)       | half-day census of every READER of persisted hash fields; the table goes into WP0.1. 12 §2 supersedes R6's one-time-transition mitigation: re-derivation from the prior tree is permanent infrastructure                                                                                    | no                                                                                                                                                                    |
| R7  | oxc version churn (§8)                      | R1's harness re-run per candidate upgrade                                                                                                                                                                                                                                                   | no (needs R1)                                                                                                                                                         |
| R8  | rayon order leaking into decisions (§9)     | per-component double-run byte-equality unit gate, planted red first; lands with the first parallel component                                                                                                                                                                                | no                                                                                                                                                                    |
| R9  | reqwest vs openai SDK differences (§10)     | byte-capture both clients against a local HTTP sink; failure envelope against a dead port and a 500-then-200 stub                                                                                                                                                                           | no                                                                                                                                                                    |
| R10 | webcrack boundary and `using` desugar (§11) | none by design: R1 confirms `using` parses; fixture and boot gate cover the rest                                                                                                                                                                                                            | n/a                                                                                                                                                                   |
| R11 | plausible-but-wrong agent translation (§12) | day one of phase 1: port one leaf module (`statement-hash.ts`'s serialization walk) against dumped vectors; count divergences caught by gate vs review; time-to-green is the first velocity datum                                                                                           | no                                                                                                                                                                    |
| R12 | performance projection wrong (§13)          | R1's parse throughput first; the full replacement is the phase-1 criterion bench with a back-to-back noise floor                                                                                                                                                                            | no                                                                                                                                                                    |
| R13 | two-stacks limbo (§13)                      | the probe IS the milestone: M1 at 25.5% of the surface, one to two months in                                                                                                                                                                                                                | no                                                                                                                                                                    |
| R14 | LLM endpoint dependence during parity (§13) | one warm replay pair with the endpoint at a dead port; success with +0 cache writes proves the parity loop is server-free. ~10 min                                                                                                                                                          | **YES 2026-09-19: PASSED** — both legs +0 writes, exit 0, 0/0 diff, endpoint dead (RUNBOOK §5)                                                                        |

## 8. Gates: where they are, and the three ways the plan falsifies early

The exit gates per phase are in 03 and repeated per work package in 10 §2;
the milestone entry/exit table is 10 §5. Four rules sit above all of them:

- **Exact, no tolerance.** M1 (phase 2) requires identical match sets
  pair-for-pair, tier-for-tier, and identical `resolutionStats` on all four
  oracle pairs. 07 §9: the comparer has no epsilon parameter by design. Phase
  3 compares every REJECTED rename with its reason, not only the applies.
- **The cache write count is the proof.** M2 (warm-replay equivalence) and
  M3 (byte-identical emitted trees) both require zero cache writes on the
  Rust leg; 07 §6 requires BOTH legs +0 and matching exit codes, with
  `split-ledger.json` and `vendor/_bun-modules.json` compared by
  `humanify-parity compare-ledger` rather than by byte diff.
- **No status without a gate-run citation** (10 §4). "It compiles" is not a
  status; a `parity-green` row citing a superseded oracle label is stale; a
  rebase invalidates green (10 §4c).
- **The differ must fail first** (07 §4). `humanify-parity selftest` ships
  known-diverged pairs and must exit 1 on each, and WP0.3's gate requires it
  to report nonzero across two different commits before any zero is believed.
  Phase 0's own gate is neutrality NEUTRAL for the dump flag plus the flag-ON
  inertness run (07 §3), which needs the per-leg-command extension of
  `neutrality.sh` because plain neutrality refuses same-commit comparisons.

Early falsifiers (03):

1. Phase-1 partitions disagreeing for reasons that are neither bugs nor
   conscious forks, e.g. beautified-text hashing encodes a distinction the
   canonical serialization cannot reproduce. That forces hash-compat mode and
   weakens 02 §4a.
2. Phase-4 prompt parity failing on ordering traceable to something
   nondeterministic on the TS side. That inverts the oracle relationship; the
   TS fix comes first.
3. The phase-2 gate needing tolerance. It should not; if it does, stop and
   understand why before proceeding.

The abort path (03): on a phase-2 no-go the branch is archived, not deleted;
the `--dump-artifacts` flag and the `humanify-parity` differ are kept because
they pay for themselves in the TS era; the README's STATUS line records the
reason.

## 9. Projections: none replaced by a measurement yet

04's status line still holds: projections current as of 2026-08-27, none
replaced by a measurement.

- **Measured, source named:** per-pair wall 532–1,013 s; LLM service time
  ÷ 32 = 75–135 s; CPU share 82–89%; naming pass 495 s wall vs 3,615
  call-seconds (7.3x effective parallelism against 32); peak RSS 15–30 GB
  under a 64 GB heap; walk median 7.8 min per hop. The 215→216 phase anatomy
  (parse and graph 53 s, prior 137 s, matching 91 s, naming window 173 s,
  floor/reconcile/sweep 81 s, split 15 s, relink 12 s) is a log-timestamp
  RECONSTRUCTION, not a trace.
- **Projected, labelled estimates:** hop ~17 min to ~2.5–3.5 min; neutrality
  pair ~10 min to ~1–2 min (the best case, ~10x); 4-pair eval sweep ~81 to
  ~25–35 min with the TS scoring step (~27 min) unchanged; RSS to a few GB
  (02 §6 amends 04's 2–4 GB downward). Soft numbers: the 5–20x per-core
  native factor and rayon scaling across an assumed 8–12 cores. Hard
  numbers: the LLM wall (measured) and parse/codegen (benchmarked upstream).
  The projection assumes the port keeps the algorithms.
- **04's own 2026-08-28 note:** the component table predates the
  prior-as-tables decision (12 §2) and overstates the projected parse cost.
  The 137 s prior parse leaves the budget entirely under 12 §2, but the row
  still includes it.
- **Two "today" figures are easy to conflate:** the ~17 min hop total is the
  2026-08-03 cold eval wall (1,013 s); the anatomy is of a 9 m 34 s hop.
  Every speedup ratio must say which base it divides, and, since section 2,
  which model it was measured against.
- **Upkeep rule** (04, 10 §4): when a probe or bench overturns a number, the
  owning doc is edited in place with a dated note the day the measurement
  lands.

## 10. The two no-port-needed wins, still not run

**Span profiler sweep.** `src/profiling/profiler.ts` already has spans for
parse, graph-build, prior-version, rename, generate and split, and nothing
passes `--profile`: neither the eval nor the walk. One profiled run per pair
turns 04's reconstructed anatomy into a Chrome trace. 09 §4 goes further:
write `.humanify/phase-times.json` unconditionally and lift it into
`-run.json` the way `run-pipeline.ts:222-233` lifts placement stats; add its
basename to `DIAGNOSTIC_ONLY` in `neutrality.sh:243` or timing jitter fails
every byte-identity verdict; record the HOST in band provenance; plant a
slowdown once and watch the alarm fire. Open sub-decision: TS now or Rust
first (09 OQ5).

**vLLM concurrency sweep** (09 §9). One pair, 215→216, COLD, `-c` in {32,
64, 96, 128, 32}; the repeated c=32 run last measures serving-state drift,
the sweep's own noise floor. If the naming wall does not drop from 32 to 64,
the CLIENT is the binding constraint and the server question is answered by
an offline replay probe that drives captured prompts from the disk cache at
controlled concurrency. If it drops with flat latency, the post-port floor
halves per doubling (~113 s to ~57 s at c=64, projected). Two things changed
since the doc was written: there are now two servers (gpt-oss-20b on `:8000`,
GLM on `:8100`), so the sweep must name its target; and exp090 measured a
32-call BURST at ~12 req/s against GLM during the swap, which is a burst, not
a sustained above-32 probe. The question is still open.

## 11. Inconsistencies between the docs — FIXED 2026-09-19

Every item below was applied in place with a dated `amended 2026-09-19` note;
the list is kept as the record of what changed.

Later docs amend earlier ones in place and win (rule 9). Each item below was
verified against the text on 2026-09-18. Fix them in one editing pass at
WP0.1 review, with dated notes:

- **Three vs five crates.** 08 §1 still opens "One workspace, three crates";
  02 §2 and 05 §1 say five (the 2026-08-28 review).
- **02's section numbers drifted when §9 (compatibility posture) was
  inserted.** 05 §2, the `unsafe_code` row of 05 §4, the `thiserror` row of
  05 §5, 05 §7 and 08 §7 cite "02 section 9" for idioms that now live in 02
  §10. 05's bump-gate citation and 08 §2's "surface evolves freely" citation
  of §9 are correct.
- **Release-profile symbols.** 08 §1 and §7 and 09 §6 assume
  `[profile.release] debug`; 05 §6's profile table sets `debug = true` only
  on a separate `profiling` profile.
- **Symbol-name privacy was retracted in 02 §2** ("it cannot": `Scoping`
  publicly exposes `rename_symbol`; the enforceable fact is that no module
  outside `core::emit` holds `&mut Scoping`), but WP3.1's exit gate in 10 §2
  still reads "symbol name field private to this module (compile-checked)"
  and 06 §1 layer 2 still promises "symbol-name writes outside
  `rename::validated` must not compile". Restate both against the overlay.
- **Dependency list.** 05 §5 counts fifteen entries (eleven runtime, four
  dev). 06 needs `proptest` and `assert_cmd`; 08 and 09 need `tracing`,
  `tracing-subscriber`, `tracing-chrome` and `dhat`; 08 needs `dotenvy` and a
  `vergen` build script; 07 and 11 need `sha2`. None are in 05's table.
- **Clippy policy.** 05 §4 says six enforced rules; 06 §4 property 4 adds
  `clippy::iter_over_hash_type` at deny in the decision modules, a seventh.
- **Spawn sites.** 09 §1 lists seven hardcoded pipeline spawn sites, adding
  `experiments/076-statement-placement/walk.sh:76`; 07 §7 and 08 §6 list six.
- **Ledger continuity at 5b.** 02 §4a and 11 R6 describe a one-time
  transition; 12 §2 supersedes it (re-derivation is permanent infrastructure)
  and 03 phase 5b now says continuity is a non-issue under 02 §9. R6's
  mitigation column is stale, and 08 §7 mis-cites the mechanism as "07 §7"
  (that section is eval integration).
- **Noise bands.** 09 §8: `noise-bands.json` was computed from TWO repeats;
  `CLAUDE.md` says three. Standardize on three.
- **Memory projection** appears as ~2–4 GB (04, 08 §5) and as "well under"
  that (02 §6).
- **Concurrency.** 08 §2's CLI table defaults `-c` to 50; every measurement
  uses 32 from `pairs.json`. Not a contradiction, but "concurrency 32"
  throughout 04 and 09 is a configured value, not the default.

## 12. A first session, concretely

1. Read the README, this document, then 10 §4b.
2. The go/no-go on phases 0–2 is Andrew's call, not the agent's. Everything
   below is independent of it and pays for itself in the TS era.
3. Run the three probes the register marks runnable today: R14 (dead-port
   warm replay, ~10 min; it also establishes whether the standing cache at
   `/work/neutrality-cache` is warm for `1671d71`), R4 (cache-key bit-parity;
   its ~100-line scratch bin is the first Rust written), R1 (oxc parse census
   over the eight bundles, the day-one task). Record each result in its row
   of 11, dated.
4. Run the span-profiler sweep and the concurrency sweep (section 10). Both
   are TypeScript-side and each replaces a projection with a measurement.
5. Install rustup in the container, choose and commit the pin (05 §2), and
   settle the release-profile question. Build inside the container; macOS
   and Linux binaries are not interchangeable (08 §6).
6. Fix section 11's inconsistencies in one editing pass with dated notes.
7. If the decision is go: WP0.1 (the pipeline contract doc, folding in R6's
   reader census), then WP0.2 with the ingestion and comment-region
   decisions made first, then WP0.3, then WP0.4 and `PORTING.md`. Port work
   and lever work never mix (09 §2): any decision-changing merge on `main`
   forces an oracle re-dump.
