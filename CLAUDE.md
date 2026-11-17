# CLAUDE.md

## Checks

**One command. Run it before every commit:**

```bash
cargo dev check
```

It runs thirteen stages, cheapest first — shims, no-private-addresses,
rust:fmt, rust:clippy, census:clones (ADVISORY), census:concurrency,
census:effects, census:slop (ADVISORY), census:comments (ADVISORY), rust:unit, rust:build,
format-golden, e2e — and prints a summary saying which ran, which failed and
which it never reached (fail-fast). The three ADVISORY stages print `REVIEW`
(never FAIL): an automated mini code-review for Claude/agents to act on —
`census:clones` by unifying the code or allowlisting the finding with a
justification (`dev/census/clone-allowlist.json`), `census:slop` by reading
why a crate's erosion or clone share rose against the merge-base,
`census:comments` by rewriting each NEW comment line it lists (an experiment
id, date or `finding #`; a file not in the tree; a never/must/do not with no
`(enforced: <check>)` tag; a dead link in the kept docs; a Markdown file
outside `dev/census/docs-allowlist.json`). `census:concurrency` is
pass/fail: an unlisted concurrency site in `crates/` FAILS until it is
confirmed (see Code style, "Concurrency"). `census:effects` is pass/fail:
an effect site or a `Result<_, String>` in a product crate that is not on
`dev/census/effects-allowlist.json` FAILS, and so does a stale entry (the
pure crates' list only shrinks; `docs/responsibility.md`, "May this code do
I/O?"). All other stages are pass/fail.
Nothing is outside it.

`cargo dev` is the measurement harness: the `humanify-ag-dev` crate
(`crates/humanify-ag-dev`, `publish = false`, never part of the release
artifact), run through the alias in `.cargo/config.toml`. It replaced the
TypeScript / Python / bash harness (`npm run check`, `scripts/check.ts`,
`experiments/lib/`, the 034 TS scorer, the 096 Python scorer) in harness
lanes L1-L5 (exp195-exp199, exp203); tag `experiments-code-final` is the last
tree that has the old harness and the old experiments' code.

**Setup, once per clone:**

```bash
git config core.hooksPath .githooks   # the pre-commit hook: cargo dev precommit
npm ci --ignore-scripts               # webcrack + tsx, for the webcrack shim only
```

The hook rustfmts staged `.rs` files, runs the concurrency census over them
and refuses a staged private network address. It is not the gate: it never
runs the tests.

**The pipeline is the Rust binary** (`crates/`, built as
`target/release/humanify-ag` — the one product command since exp186, with
the offline index builder as its `index` subcommand: `humanify-ag index
build --spec …`). The TypeScript pipeline (`src/`) was deleted at the
cutover, 2026-09-26 — the ledger of what was deleted, moved or retired and
why is `git show 6c4b4d13f:docs/rust-port/19-cutover.md`; tag `m4` is the last
tree that has it. The
only JavaScript left is `tools/webcrack-shim.ts`, which the binary runs (with
`npx tsx`) to unpack webpack / browserify bundles, and node itself, which e2e
uses to import every output. `package.json` holds exactly the shim's runtime.
(The index builder's bundler recipes, `crates/humanify-ag-index/recipes/`,
are configs it hands to rollup / webpack, not harness code.) The library truth
layers, corpus freezes, bundle builds and npm caches are `cargo dev library
layers|corpus freeze|corpus build|npm`; every committed layer re-freezes
byte-identically on the corpora this host still has (exp202).

What the stages prove:

- `shims` — the remaining JavaScript (the webcrack shim) parses.
- `no-private-addresses` — no RFC 1918 address in a tracked file (the repo is
  public).
- `rust:clippy` — the house rules as lints, over every crate including the
  harness: complexity ceiling, hash-iteration ban in decision code, env reads
  confined to their owners, the concurrency bans.
- `rust:unit` — `cargo test --workspace`: the pipeline's tests, the replays of
  the frozen `test/parity/` specs, and the harness's own tests (every old
  `*.test.ts` rule is ported there or recorded as moot).
- `rust:build` — the RELEASE binary, `--locked`, the build the eval runs.
- `format-golden` — that binary's formatter against
  `test/parity/format-goldens.json` (the TS beautifier's captured bytes, now
  the formatter's frozen spec), plus a planted perturbation that must be caught.
- `e2e` — the release binary on the committed e2e fixtures
  (fresh, then with `--prior-version`) against the one stub LLM
  (`humanify-ag-testkit`) that names every identifier, run twice for
  byte-determinism, every launch held equal to `dev/eval/goldens/e2e.golden.txt`,
  and every output imported by Node and required to export the input's surface
  EXACTLY — an export name is the module's API (finding #55, fixed; the
  `esm-exports` fixture holds every ESM export form).

There used to be three commands and none of them ran everything: `test:e2e` sat
outside the documented gate entirely, and `knip` sat outside the one people
actually ran — which is how `check:all` came to be red on main for two findings
nobody had seen. The split was never about speed.

A subset is available for iteration and is **labelled PARTIAL** so it cannot be
mistaken for a green gate:

```bash
cargo dev check --only rust:fmt,rust:clippy
```

Adding a check is one entry in `STAGES` in
`crates/humanify-ag-dev/src/gate/check.rs`. If it is not in that list it does
not run, and that list is the only place to look to find out what the gate
covers.

Individual stages, if you need to run one directly:

```bash
cargo dev shims
cargo dev no-private-addresses
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo dev census clones [--loose]  # cross-function twins in crates/ vs dev/census/clone-allowlist.json
cargo dev census concurrency       # concurrency sites in crates/ vs dev/census/concurrency-allowlist.json
cargo dev census slop [--base REF] # per-crate erosion / clone share vs the merge-base
cargo dev census comments [--all]  # comment and docs rules; --all lists every finding
cargo test --workspace             # rust:unit
cargo build --release --locked -p humanify-ag-cli   # rust:build
cargo dev format-golden            # needs rust:build
cargo dev e2e                      # needs rust:build
```

Dev tools beside the gate (each a `cargo dev` verb; `cargo dev` with no
arguments prints every verb with what it proves and what it CANNOT prove):
`stub-llm serve [--port N]`, `build-bin <ref>`, `run stub`.

## Validating cross-version changes

**One harness. Every supported instrument is a verb on it:**

```bash
cargo dev                               # lists the verbs, what each proves and CANNOT prove
cargo dev eval score <label>            # cold scored run of the binary
cargo dev eval neutrality <ref>         # byte-identity gate for should-change-nothing edits
cargo dev diff label <from-tree> <to-tree> <from-input-dir> <to-input-dir> [out.json]   # one label per changed line
cargo dev eval walk <from>..<to> --walk-dir D   # a real version walk; its report flags the hops to read
cargo dev library score <label> --index D --rule agreement|structural   # library naming P/R on a frozen corpus
```

**The library scorer is scorer 16 (`exp096-score/16`, exp239).** It scores
exactly as scorer 15 and writes plain serde JSON (no Python spelling); only
the corpus and layer identities keep their frozen spelling
(`crates/humanify-ag-dev/src/json/frozen.rs`), so recorded hashes stay valid.
Scorer 15 (exp214) is scorer 14's cards plus two truth corrections, each its own column: a
source-map DEFECT (the label's package — its map files, its published files
in the index literal table and its installed copy — holds at most 20% of the
unit's literals) is out of both sides (`defectExcludedBytes`); a COPY CREDIT
(the claimed package's own published file holds the unit's print and >= 90%
of its literals, the truth's file holds the code, and a dependency / fork /
vendored / same-file link says the truth package ships a copy) counts as
named (`namedAsCopyCreditBytes`; `precisionNoCopyCredit` charges it as
wrong). Every corrected unit is listed in `truth15.json` beside the cards,
with the wrong claims left wrong by reason. Labels of scorer 14 and 15 do
not share a leaderboard, and `library proof` refuses a label of another
version: rescore it with `library rescore <label> --out DIR`. The defect
check reads the corpus apps' installed trees and the copy credit reads the
npm registry-manifest cache, NEITHER of which is frozen: on a host without
them scorer 15 finds no defects. On index 4736dcfa, bun-dev node reads
0.998138 (0.997511 without copy credit), bun-held node 0.999032.

The verb table in `crates/humanify-ag-dev/src/main.rs` is the only place to
look — an instrument not listed there is not supported. Every setting is a
harness key (`crates/humanify-ag-dev/src/settings/keys.rs`, one parse point):
an unknown flag or a retired one fails upfront, naming why, and no instrument
reads an ambient environment variable (`env_guard`). The harness puts the
user tools (`~/.cargo/bin`, `~/.bun/bin`, without which boot gates silently
skip) on its children's PATH, and `eval score` refuses to score into a label
whose cards came from a different commit, pipeline, base mode, naming mode or
library index (`--force-mixed` overrides). One comparison guard
(`identity::guard`) decides every "may these two be compared" question.

For any change that could affect deobfuscation output (naming, matching,
splitting), the final gate on top of `cargo dev check` is the eval harness — it
scores the pipeline on a fixed set of version transitions and grades the
cross-version diff as real change vs reducible noise.

**For a change that is supposed to alter NOTHING — a refactor, a counter, a
type-level fix — the eval is the wrong instrument, and `eval neutrality` is
the right one:**

```bash
cargo dev eval neutrality <baseline-ref> [from:to] --cache /work/neutrality-cache   # ~25min/pair, both legs
```

(Its settings are harness keys, parsed once; it writes the ONE run config it
used, `<workdir>/neutrality-run-config.json`, with a `resolved` block.)

It runs the candidate and the baseline over one pair with a shared warm cache
and proves BYTE IDENTITY: 0 differing files, 0 differing lines, zero cache
writes on the baseline leg, matching exit codes. Rule 11 is why — the eval
cannot resolve an effect of zero and will print a confident number and a sign
anyway, so "the KPIs moved a bit" tells you nothing about a refactor. A byte
diff answers the question exactly, in a third of the time.

Three things it will tell you that are easy to misread:

- a MISMATCHED EXIT CODE fails the verdict on its own, which is correct — but
  if the change was _meant_ to fix a failing run, that mismatch is the point.
  Check `differing lines: 0` before reading "NOT NEUTRAL" as a regression.
- a leg that exits non-zero having WRITTEN a tree is recorded and compared, not
  treated as a crash. The pipeline exits 1 on a rename-invariant failure.
- the baseline leg runs in a detached worktree, and each leg BUILDS its
  own binary from its own commit (`cargo dev build-bin <ref>`, which resolves
  the package and binary name from that commit) — so
  **`crates/` must not be edited while a candidate leg is building**. A
  baseline ref from before the cutover has no binary that is the pipeline.

Using the cache here is the use rule 10 permits: it forbids the cache for a
verdict about LLM-dependent behaviour, and this is a verdict about determinism
with the model held fixed.

**Only the BASELINE leg's zero is load-bearing** — this used to read "verify
BOTH legs wrote zero", which the script itself contradicts and which would
reject most valid runs. The candidate leg runs first and POPULATES the shared
cache, so its count is routinely large (7, 1591 and 3525 in one session, all
valid). What the baseline's zero proves is that leg B asked nothing leg A had
not: the two legs put the SAME questions to the model. The verdict logic fails
on that count alone. Read it before the summary line.

**A COLD neutrality run is INVALID — null-control proven 2026-08-11.** The
verdict is only meaningful when both legs replay a WARM cache. Three facts,
established the expensive way:

- **The workdir argument silently moves the cache**
  (`CACHE=$WORK/neutrality-cache`). Pass a fresh workdir and you get an empty
  cache → a fully cold run → a meaningless verdict. To isolate workdirs, keep
  the standing cache: `--cache /work/neutrality-cache` (flag, 2026-08-12 —
  the env vars are gone; unknown flags fail loud).
- **A null control on a COLD pair — two commits with byte-identical `src/` —
  diverged by 177 files / 2,552 lines** (85→86, 2026-08-11). On every cold
  run observed, the baseline leg wrote entries (2/10/10/12/19): cold legs
  drift into asking slightly different questions, the verdict's precondition
  fails, and naming-draw churn (the same empty arrows drawing different
  names; placement blocks hopping files) makes the diff. Earlier, smaller
  divergences (15 files/212 ln on 118→119; 2/8 on 85→86) were the same
  effect at lower amplitude.
- **A WARM run resolves zero exactly.** The same comparison that returned
  NOT NEUTRAL twice cold (139 and 178 lines) came back 0 files / 0 lines /
  both legs +0 warm. All historical clean NEUTRAL verdicts ran warm.

What to do with a NOT NEUTRAL:

1. **Check the cache-write counts BEFORE the verdict line.** The baseline
   leg's count is the precondition: if it wrote ANYTHING, the two legs asked
   different questions and the verdict is void — re-run WARM
   (`--cache /work/neutrality-cache`), do not interpret the diff.
   (A cold candidate with a zero-write baseline is still valid — the
   candidate populated, the baseline replayed — but empirically cold runs
   almost never achieve that zero.)
2. **Check whether your change can even reach the difference.** The exp059
   ledger was exonerated on 118->119 because its own counter read ZERO there
   while neutrality reported 420 differing lines.
3. A warm NEUTRAL pass is definitive for the pair: byte-identical trees,
   identical prompts (baseline +0), identical exit codes. This bug can only
   manufacture a false FAILURE, never a false pass, so no merge gated on a
   NEUTRAL result is in doubt.

The matcher preflight (the TS fingerprint matcher against real npm packages)
is RETIRED with the TS matcher it tested; the Rust
matcher's replacement is an open follow-up
(`git show 6c4b4d13f:docs/rust-port/19-cutover.md`).

A retired flag is an unknown flag, refused. There is no host-wide lock (exp205): the coordinating agent decides what runs together, from each verb's `footprint` that `cargo dev` prints (memory, LLM use, time, folders written). Two runs never share a folder by accident: a verb claims the folder it writes (`<folder>.live-run`) and a second run on it is refused at once, naming the first. Run an eval ALONE — a parallel heavy job (a gate batch, another
eval) has OOM-killed a rebase before. A failed rebase (or any failed pair)
fails the run: `eval score` never falls back, names every failed pair in a
closing `!! eval score FAILED` verdict and exits 1; a missing input for a
selected pair is refused before anything builds.
`eval score` already runs its five pairs in parallel lanes
(`--parallel-pairs`, default 5) with ONE rebase at a time
(`--parallel-rebases`, default 1): a rebase at 472 requests in flight
saturates the 4-replica LLM server, and five at once made the run slower in
total (exp207). Peak memory is the lanes' sum, ~27-31 GB. Each run's output
is the same as it would be alone; only the schedule changed.

**What the eval scores: the Rust binary, always.** `cargo dev eval score
<label>` BUILDS `target/release/humanify-ag` (`cargo build --release --locked`)
and runs it at all three launch sites: the rebase, the scored leg and the
self-hop. `--bin <path>` names a different binary (in some cargo workspace's
`target/`). There is no TS mode — the cutover removed it, and a run config
without a binary is refused. `eval score` refuses a binary whose build commit
is not the label's, is unknown, or came from a dirty tree; `--force-mixed`
overrides that, and binary cards never join a pre-cutover TS label. The
label's `pipeline.json` and every run manifest record the binary's sha256 and
build commit, and the dev crate's recorder tests (`runners::score_test`) hold
the launch lines byte-identical to `dev/eval/goldens/run-launch.golden.txt`,
the ones the binary-scored
references ran with. Its default work dir is `/work/eval-work`. A TS-era
prior is brought across (ledger re-derived, vendor names carried by content)
or refused with a WARNING line. Details:
`experiments/034-eval-harness/README.md`, "Scoring a Rust binary".

**The measure is the diff labeller (`cargo dev diff label`,
`crates/humanify-ag-dev/src/difflabel/`).** It takes two output trees and the
two inputs they were made from, and gives every changed line of `diff -rN`
exactly one label; the labels must add up to the diff's count or the verb
fails. It compares the inputs unit by unit (every function and top-level
statement, identifiers ignored) to find input code that is certainly
unchanged, and reads each tree's `.humanify/provenance.json` to know which
input bytes each output line came from. Labels fall into four groups:

- **real**: from input code that changed, is new or was removed. A ceiling:
  every changed line inside a changed input function counts.
- **noise**: from unchanged input (renamed, moved, printed differently,
  linker text reordered or renamed).
- **wiring**: require and export lines whose cause is not known yet.
- **unknown**: duplicated input text, unpaired input files, files with no
  provenance.

`eval score` writes one label card per pair (`<v>.json`) and a summary table
of the groups; the walk report shows the same groups per hop. There are no
bands yet: nobody has measured how far the labels move between two cold runs
of one commit, so a small move between two labels proves nothing.

**The self-hop gate has two halves (00-control §3, 2026-09-25).** The COLD
self-hop rarely reads 0, because the LLM re-rolls: on the scratch-base
protocol the three same-commit reference runs read 2, 32 and 184 bundle lines
(the seeded-era range was 92–180). That count must fall inside the range in
`dev/eval/self-hop-reference.json` (2.1.216 only). The
WARM self-hop (always run) replays a scratch copy
of the cache the cold leg filled, and it must be byte-identical with 0 cache
writes. This is determinism with the model held fixed, the cache use rule 10
permits. The summary judges both, and says "not judged" when a leg ran on a
cache or on another version.

```bash
cargo dev eval score <label>    # the six pairs, cold (~95 min names mode, parallel lanes)
```

The summary prints, before the group table, the run-health banner (exit
codes, boot, self-hop), and after it the LLM call diagnostics and the MATCHING
block read from each scored run's stats. Five matching counts (functions
total, close-matched, sent to the LLM, unmatched, still ambiguous) are decided
before any model answer, so they are equal between same-commit repeats; a
difference is a harness or determinism bug, not the LLM.

**Names mode is the DEFAULT since exp171 (2026-10-31).** Library code the index
recognises goes under `vendor/<package>/` with its names restored, and the rest
of vendor code is named like app code (exp169). `--library-matching off` is the
opt-out. Which index a names run needs is the bundle's KIND, its detected packing tool
(exp175): with no `--library-index` the run uses an index of that kind found in
`<cwd>/.humanify/library-index`, and when there is none it prints one
`no library index for <kind> bundles … — skipping library name matching` line and
writes off mode's bytes (recorded as `libraryMatching` `outcome: skipped`, which
the eval refuses to score). An explicit `--library-index` that does not fit the
kind fails. `cargo dev eval score` always hands names launches the prebuilt full index
(pairs.json `libraryIndex`) and never a run-time build. **Every label has a
NAMING MODE** (`pipeline.json` `namingMode`; labels before exp171 read as
`off`). Neutrality across the flip must run in off mode.

**Minifier style is not a change (exp200, 2026-11-08).** The match key
(`match-key/5`, the vendor manifest's `hashVersion 5`) treats a concise arrow
body as its braced `return`, ignores parentheses and matches class-private
names per class. These rules are entries in the identity-transform registry
(`crates/humanify-ag-core/src/hash/identity_transforms.rs`); adding one means
a registry entry, its tests, and a version bump for each form that uses it.
The library index's forms (`units/4`, `vendor-content-key/5`) apply the
same transforms since exp204 (2026-11-09), which rebuilt the full index from
the stored builds with `humanify-ag index build --replay <index-dir>` (same
plan, new forms) as `4736dcfa…`, now pairs.json `libraryIndex`; an index
stamped with other form versions is refused. The first run of a new match-key version on a tree
written by an older binary re-matches vendor modules by content (both sides
formatted; each module's file and `lib_<hash>` identifier carried), so that
hop costs about 1,000 extra vendor noise lines, not about 38,000.

**There is no reference label for the labeller yet.** Labels scored by the
statement-hash scorer (everything before 2026-11-17, `ref-names-*`,
`ref-scratch-*`) hold cards the summary no longer reads.

The eval diffs a freshly-humanified `v` against a base `v-1` that it
rebuilds with the current pipeline and **no prior** (a scratch base), so both
sides of every comparison are pure current-pipeline output. The seeded and
archive base modes are retired (exp237): labels scored on them (everything
before 2026-10-03) cannot be compared with current ones. All harness
configuration is flags parsed upfront (see `cargo dev` with no args);
ambient env vars are gone and a guard test (`env_guard`) keeps them gone.

**Before sizing a lever or believing a decomposition, read
`docs/measurement-pitfalls.md`.** Eleven numbered rules. Seven were each learned by
publishing a wrong number first — a sizing predicate that did not test what its
name implied (38% → 7.2%), a mechanism inferred from the largest single example
and refuted by the population (83× → 1.0×), a syntactic proxy biased opposite to
the predicted direction, and a damage ceiling that measured its own scope
correctly and still cost +3,742 lines through second-order effects.

Rule 8 was learned the hardest way: exp033-045 drove `src/` noise to a measured
floor while `vendor/` — same emitted tree, same reviewer, **2.4x the entire
measured `src` noise** — went unscored for thirteen experiments. Every KPI was
honest; the scope was not. **Enumerate what the harness does not look at before
believing a floor.**

Rule 10 is the one that bites the whole gate: **a determinism aid becomes a lie
the moment it is left on for the verdict.** exp047's first gate ran entirely
through `--llm-cache` — all 24,079 entries pre-dated it, not one new entry was
written, so **not one prompt reached the model** across eight runs. Every KPI
agreed with control because both legs replayed the same answers. Re-running cold,
with a cold control, overturned three settled conclusions. Use the cache for
iteration, never for a verdict; `eval score` defaults to no cache. And when the
candidate goes cold, the CONTROL must go cold with it.

Rule 11 is what rule 10 uncovers once the variance is free to move: **a gate
cannot resolve an effect smaller than its own noise floor, and it will still print
a confident number and a sign.** exp048's cold A/B credited the CALM canary hop
with the biggest win in the arc (−2,864 lines) and charged another hop with a
regression — **both hops had shipped zero renames.** The `src/` per-hop draw band
is **±2,800**, 8× the ±350 previously assumed, and the change's real effect was
−335 lines. What resolved it: the change LOGGED every rename it applied (an empty
trail cannot have moved a KPI), a mechanism-derived ceiling computed before the
run, and finally pinning the draws — legitimate here only because the pass is
deterministic and sits downstream of every prompt, and only because the pinning
was PROVEN (the second leg wrote zero cache entries). **Measure what your gate
reads for two runs that should agree before letting it decide anything.**

Rule 9 is the one that bites a reader rather than a measurer: **every retraction
lives in a NEWER file than the claim it retracts**, so an experiment README can
still state a number its own RESULTS file corrected. `experiments/README.md`
explains how to read that directory — briefs are hypotheses, titles expire, and
anything before exp034 was not gated on four real version pairs. Each README in
the active arc carries a STATUS block; one without has not been audited.

## Development workflow

We use red/green TDD. When fixing a bug or adding a feature:

1. Write the test first
2. Run the test and watch it fail (red)
3. Implement the solution
4. Run the test and watch it pass (green)
5. Refactor if needed

Never skip the red step. If the test passes before implementation, the test is not testing the right thing.

## Code style

- Actively unify duplicated code. When two systems do similar things, extract shared functionality rather than duplicating with minor variations. Before writing new helpers, check if an existing one can be reused or generalized.
- **[`docs/responsibility.md`](./docs/responsibility.md) says who owns which question** — name legality, applying a rename, counting changed lines, walking a tree, reading a ledger, reading a kill switch. Check it before writing a helper, and add a row when you create an owner. The dangerous duplication is not two functions that look alike; it is two that answer the same question DIFFERENTLY with nothing declaring the difference (a guard that read fields one producer never sets was dead for 11,094 accepts and reported as perfect precision).
- **[`docs/pipeline-stages.md`](./docs/pipeline-stages.md) lists the twelve stages that actually run** (Rust modules since the cutover), in order, and which two have a real strategy registry (unpack, library detection). Read it before assuming a plug point exists: the working mental model was four stages, and the eight unwritten ones are where the noise has come from — `vendor/` went unscored for thirteen experiments at 2.4x the measured `src/` noise. Splitting has ONE path (`stableSplitFromCode`; prior → inherit layout, no prior → fresh grouping): the standalone `split` command, its adapter registry, and the legacy clustering splitter that backed them were deleted 2026-08-11/12 after an execution census measured them at zero runs.
- **Configuration: every key is a flag.** Add a setting by adding a key to the
  registry (`crates/humanify-ag-cli/src/config/keys.rs`), never a hand-written
  flag; the flag, the config-file key, the help text and
  `humanify.example.toml` are all generated from it, and a flag and a config
  value that disagree fail.
- **Concurrency: [`docs/concurrency.md`](./docs/concurrency.md) is the approach** —
  twelve sanctioned patterns (CPU work through `crate::par` only, the ONE tokio
  runtime the LLM provider owns, a deadline over every whole wait, drained
  pipes, short locks, completion order never reaching output, one retry
  layer, test-support servers only in the testkit). Clippy bans what it can (`await_holding_*`, and
  `disallowed_methods` for blocking sleeps, extra runtimes, detached tasks,
  bare HTTP clients, `wait_with_output`); each allowed exception carries a
  `// concurrency: <reason>` comment. Every other concurrency construct in
  non-test `crates/` code must be on `dev/census/concurrency-allowlist.json`
  with its pattern and reason — the `census:concurrency` stage and the
  pre-commit hook FAIL an unlisted one. That failure is a question, not a
  verdict: confirm the site really is one of the patterns (add the entry)
  or switch to the sanctioned primitive.
- Clippy enforces the complexity ceiling in every crate, the harness included. Extract helpers to keep functions focused.
- Unit tests sit beside their module as `<module>_test.rs` (or a crate's `tests/`). There are no TS tests: the harness is the `humanify-ag-dev` crate, and its tests run in `rust:unit`.
- The harness's committed data lives in `dev/` and is read only through its typed loaders (`crates/humanify-ag-dev/src/committed/`, paths in `paths.rs`): `dev/eval/` (pairs.json, noise-bands.json, self-hop-reference.json, `goldens/` — every launch golden, `results/` — the labels), `dev/library/` (bands.json, data-roots.json, `corpora/` — every corpus and truth-layer manifest, `results/`), `dev/census/` (both census allowlists). A test fails a file under `dev/` that no loader reads.
- The e2e fixtures live in `test/e2e/fixtures/` (committed builds); `cargo dev e2e` runs the binary on them.
