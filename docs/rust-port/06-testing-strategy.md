# Testing strategy for the Rust codebase

The testing culture is not being ported — it is being kept, and given better
enforcement. Red/green TDD, colocated tests, planted-defect verification of
every guard, doc-drift guards, and the principle that a test that cannot fail
is not a test (`docs/measurement-pitfalls.md` rule 3: "a zero from an
instrument that has never been shown to produce a one is not a measurement")
all carry over verbatim. What Rust adds
is layers TypeScript could not give us: compile-fail proofs that the type-level
promises in `02-rust-target-architecture.md` actually hold, property tests over
generated programs, and mutation testing that automates the planted-defect
check across every guard at once.

Two constraints frame every choice below, matching the goals in `README.md`:

- **Confidence during migration.** While phases 1–5 of `03-migration-plan.md`
  run, the definition of "green" for ported code is the oracle parity gate, not
  the unit suite. Unit tests are written red/green as always, but when a unit
  test and the TS oracle disagree, the oracle wins until phase 5b — the TS
  behavior is the spec. Tests in this document are the standing suite that
  outlives the oracle.
- **Speed after migration.** The per-save loop must stay in seconds and the
  full gate well under today's ~25 s `npm run check`. Anything slower (mutation,
  fuzzing, deep property runs) is explicitly on-demand and never in the gate.

## 1. Test taxonomy

**TOTAL: twelve layers.** Nine run in the commit gate or faster; three are
on-demand instruments. Runtimes are estimates until measured.

| #   | layer                         | tool                                                   | what it covers                                                                                                                                         | runtime (est.)          | when it runs          |
| --- | ----------------------------- | ------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------- | --------------------- |
| 1   | colocated unit                | `#[cfg(test)] mod tests`, cargo-nextest                | every module's logic, next to the code — the `*.test.ts` discipline                                                                                    | < 10 s suite            | per-save + gate       |
| 2   | compile-fail                  | trybuild                                               | the type-level promises of 02 §2: `MatchKey` into a correctness gate must not compile; symbol-name writes outside `rename::validated` must not compile | ~30 s                   | gate                  |
| 3   | property                      | proptest                                               | the five invariants in §4 over generated inputs                                                                                                        | 30–60 s at gate depth   | gate + deep on-demand |
| 4   | integration, in-process       | `tests/` in `humanify-core`, mock LLM provider         | pipeline segments end-to-end on synthetic fixtures — the `src/test/rename.e2etest.ts` analog (6 cases today, all with mocked `suggestAllNames`)        | seconds                 | gate                  |
| 5   | integration, binary           | `tests/` in `humanify-cli`, assert_cmd                 | the built binary on committed fixtures with a committed warm cache: flags, exit codes, tree layout, ledgers                                            | 30–60 s                 | gate                  |
| 6   | prompt snapshots              | insta                                                  | rendered prompts — an external contract with the LLM cache (02 §7), byte-stable                                                                        | < 5 s                   | per-save + gate       |
| 7   | fingerprint snapshots         | insta (migration era: the committed TS snapshot files) | match metrics per fixture × pair × minifier — the `*.fptest.ts` analog                                                                                 | seconds per fixture     | gate                  |
| 8   | JSON contract shapes          | insta JSON snapshots + a Node-byte corpus              | serde structs vs the frozen pipeline contract (03 phase 0); byte parity with Node `JSON.stringify`                                                     | < 5 s                   | gate                  |
| 9   | doc-drift and registry guards | plain `#[test]` over `std::fs`                         | registry names present in their doc; `std::env` confined to one module                                                                                 | < 1 s                   | gate                  |
| 10  | mutation                      | cargo-mutants                                          | every guard module proven able to go red (§5)                                                                                                          | minutes–hours per scope | on-demand / weekly    |
| 11  | fuzzing                       | cargo-fuzz (nightly, libFuzzer)                        | canonical serializer panic-freedom, ledger parser robustness (§7)                                                                                      | unbounded               | on-demand             |
| 12  | coverage                      | cargo-llvm-cov                                         | report only, no threshold (§8)                                                                                                                         | minutes                 | on-demand             |

**Gate wiring.** There is one gate and one list, and that does not change.
`scripts/check.ts:56-98` holds the `STAGES` array today ("if it is not in that
list it does not run" — project `CLAUDE.md`); during migration the Rust stages
are added to that same array under the names `05-rust-toolchain.md` §8
defines — `rust:fmt` (`cargo fmt --all --check`), `rust:clippy`
(`cargo clippy --workspace --all-targets -- -D warnings`), `rust:unit` (`cargo nextest run`,
which executes layers 1–9; trybuild and insta tests are ordinary `#[test]`s),
and `rust:parity` (§3). The `--only` subset stays labelled PARTIAL
(scripts/check.ts:191-200). After phase 6 deletes `src/`, the TS stages that
guarded it retire from the list; `check.ts` itself stays, because the
measurement harness keeps its TS tests (§10).

**CI**: none exists today — no `.github/`, `.circleci`, or equivalent
(verified; the only automated hook is husky pre-commit running lint-staged,
`.husky/pre-commit:1`). "Gate" above means the check command run before commit,
exactly as now. Rows are CI-ready if CI is ever added; nothing in this plan
depends on it.

**Naming convention.** `<unit>_<condition>_<expected>`, mirroring the house
convention, minus the `test_` prefix pytest requires and `#[test]` does not:

```rust
#[test]
fn attempt_validated_rename_when_child_scope_shadows_target_rejects() { ... }

#[test]
fn singleton_guard_when_only_bucket_key_evidence_abstains() { ... }
```

**What REMAINS uncovered by these twelve layers** — deliberately:
LLM-dependent quality, cross-version noise KPIs, and should-change-nothing
proofs belong to the eval harness and `neutrality.sh`, which stay TypeScript
and judge the binary from outside (02 §8); the one-time migration parity gates
are 03's, not this document's; and the TS harness's own tests
(`test/measurement-owners.test.ts` and friends) remain on the TS side (§10).
No Rust-side test may claim a verdict those instruments own — rule 10 and
rule 11 of `docs/measurement-pitfalls.md` bind here exactly as they bind the
eval.

## 2. Fixture strategy: one corpus, two consumers

The existing corpus is the right data and is reused as-is. Six fixtures under
`test/e2e/fixtures/`, each with a `fixture.config.json` (package, source
strategy, entry points, build command, version pairs): `mitt`, `nanoid`,
`preact`, `zustand` (git-tag strategy, fetched and built by
`npm run e2e -- setup <fixture>`), and `r1b-synthetic`, `disambiguation`
(local strategy, source checked in, 76K each). Per fixture and version the
harness materializes plain minified inputs per minifier —
`minified/<version>/<minifier>.js` plus source map — for `terser-default`,
`esbuild-default`, `swc-default`, and (outside the default set) `bun-bundle`
(`test/e2e/harness/minify.ts:24-74`). These are exactly the small, real inputs
the Rust suite needs: `mitt` terser output is 302 bytes; `preact` is ~13–15 KB
per minifier.

**The caveat that shapes the design: almost none of this is committed.**
`git ls-files test/e2e/fixtures` returns 12 files — the six configs, the
`build/` JS of the two synthetic fixtures, and two fixture-local `.gitignore`
files; `.gitignore:5-11` ignores `source/`,
`build/`, `minified/`, and `.tmp-clone/` for everything else, and minified
files are regenerated on every validate/fptest run with the live minifier
versions (`test/e2e/harness/minify.ts:257-285`). A fresh worktree without
`setup` has already produced false "REGRESSED" preflight verdicts, now
distinguished as UNBUILT (`test/measurement-owners.test.ts:164-182`). The
`node:vm` runtime test depending on a gitignored `.tmp-clone`
(`test/e2e/functional.test.ts`) is the same fragility in another spot. Two
suites doubling down on implicit on-disk state would double the failure modes,
so:

- **A shared fixture manifest becomes the single driver.** One committed JSON
  (proposed: `test/e2e/fixtures/manifest.json`) listing every fixture ×
  version × minifier with the sha256 of each minified input, written by the TS
  harness after setup/minify. Both suites enumerate their fixture-driven tests
  FROM the manifest and verify input hashes before asserting. A minifier bump
  then fails both suites loudly on the same cause ("input hash mismatch —
  re-run setup and review"), instead of surfacing as mysterious snapshot churn
  in whichever suite ran first.
- **Fixture-scale parity is a check stage** (`rust:parity` in `STAGES`): it
  fails if any manifest entry lacks a committed snapshot on either suite, and
  if either suite covers combinations the other does not. This is the guard
  against silent under-coverage — the audit that found 64 tests no script ran
  (project `CLAUDE.md`, unit-stage note) and measurement rule 8 (enumerate what
  the harness does not look at) both argue it must be mechanical, not
  remembered.
- Rust tests locate the corpus relative to
  `env!("CARGO_MANIFEST_DIR")` (the crates live in the same repo, 02 §2), and
  skip-with-loud-message when a git-tag fixture is unbuilt — mirroring the
  UNBUILT/REGRESSED distinction rather than reporting failure.

The two synthetic fixtures are fully committed and become the always-available
substrate for layers 4 and 5; the four real-package fixtures require one
`setup` per clone, as today.

## 3. Snapshot testing: insta mechanics

Snapshots live in a `snapshots/` directory beside the test module (insta's
default), as committed `.snap` files; a mismatch writes a pending `.snap.new`,
and `cargo insta review` presents each diff for accept/reject
(`cargo insta accept` for bulk). Pending files are never committed. In the
gate, insta runs in no-update mode (`INSTA_UPDATE=no`) so a mismatch fails the
stage rather than writing a pending file — the equivalent of today's
`assertSnapshotMatch`, which fails on any diff and on a missing snapshot with
a message telling you to run `--update-snapshot`
(`test/e2e/harness/test-helpers.ts:11-25`). The review workflow is a strict
upgrade over `--update-snapshot`: diffs are reviewed one by one instead of
regenerated wholesale.

Three snapshot families:

- **Prompts** (layer 6). Prompt rendering is a pure function of frozen wave
  context (02 §7) and its bytes are a contract with the LLM disk cache — a
  one-byte drift silently invalidates every cache entry and breaks phase 4's
  prompt byte-parity gate. Snapshot per prompt template over fixed context.
- **Fingerprint metrics** (layer 7, the `*.fptest.ts` analog). Today each
  fptest generates one `it()` per version pair per minifier and asserts a
  metrics-only JSON snapshot — match buckets, accuracy/precision/recall,
  `resolutionStats`, `failureSummary`; no code text
  (`test/e2e/mitt.fptest.ts:1-23`, format in `test/e2e/harness/snapshot.ts:9-38`,
  compared fields in `snapshot.ts:174-321`; timestamp stored but not
  compared — insta redactions handle the same). The Rust analog runs the
  ported matcher over the same manifest entries. **During migration it does
  not keep its own snapshots: it asserts equality against the committed TS
  snapshot files in `test/e2e/snapshots/<fixture>/`, field-for-field over the
  same compared-field list.** That is the phase-1/2 decision-parity gate
  reproduced at fixture scale, offline and in seconds — any field diff is a
  real divergence to chase, per 03's no-tolerance stance. After cutover,
  ownership moves to insta files and the TS snapshots retire with the suite.
  The `humanify/` baselines (whole-pipeline runs with `outputHash`,
  `test/e2e/snapshots/humanify/`) only become comparable after phase 4's
  warm-replay gate and are handled there, not here.
- **JSON contract shapes** (layer 8). Every serde struct in the pipeline
  contract — run status, stats, ledgers, trails (02 §3: "same JSON shapes") —
  gets an insta JSON snapshot of a fully-populated value, so shape drift is a
  reviewed diff. Byte parity is the second half: `stageFingerprint` hashes the
  ledger's serialization (`src/commands/unified.ts:912`), the self-hop gate
  byte-compares `split-ledger.json` with `cmp -s`
  (`experiments/034-eval-harness/run.sh:400`), and LLM cache keys are
  sensitive to number formatting (`src/llm/cached-provider.ts:50-69,105-110`)
  — so a committed corpus of value→bytes pairs, generated once by a TS script
  running Node's `JSON.stringify`, is asserted byte-equal against the Rust
  serializer: insertion-keyed order, compact form, and Node's number
  formatting (the `1e21` vs `1e+21` exponent class is a known serde_json/Node
  divergence to test explicitly).

## 4. Property-based tests (proptest)

Property tests are the planted-defect idea run over thousands of generated
inputs, and proptest's shrinking hands back a minimal counterexample. Found
counterexamples persist in committed `proptest-regressions/` files and replay
first on every subsequent run — an automatically-growing regression corpus.
Gate runs use modest case counts (fast); deep runs raise `PROPTEST_CASES`
on-demand. Five properties, each tied to the incident that motivates it, each
with its control built in so the probe is demonstrably able to fail (rule 3:
"a zero from an instrument that has never been shown to produce a one is not a
measurement").

1. **Hash rename-invariance.** For a generated program and any legal
   alpha-renaming of bound identifiers, `MatchKey`, `IdentityKey`, and
   `VendorSignature` are unchanged. This is the core identity claim
   (01 §3) — today guaranteed by construction and review of the binding-keyed
   placeholder scheme (`src/analysis/structural-hash.ts:639-673`). **Control:**
   renaming a free identifier or a property key MUST change the hash — they
   are hash content (`structural-hash.ts:554-590`); a generator that never
   produces that separation is vacuous.
2. **The literal-policy pair.** Swapping a string literal for a different
   same-length string leaves the blurred `MatchKey` equal AND makes the
   verbatim `IdentityKey` unequal — one property, both assertions, always
   together. Motivated by exp046's near-miss: keying vendor byte-reuse on the
   blurred hash would have shipped the prior release's endpoints (01 §3;
   rule 8's corollary "probe with same-length inputs — a probe using `'alpha'`
   vs `'beta'` passes for the wrong reason"). The verbatim-unequal half IS the
   control proving the swap happened, following the class-literal hash test
   precedent that ships a control asserting the non-class case does separate
   (rule 3). The generator additionally asserts the two literals differ.
3. **Validated-rename capture-freedom.** For a generated program and any
   rename accepted by `attempt_validated_rename`, re-resolving all references
   afterward shows every `ReferenceId` still resolving to the same `SymbolId`,
   and no reference changed free/bound status. This is the invariant the seven
   legality rules exist to protect; capture bugs from unvalidated applies are
   how the "never call `scope.rename` directly" rule was earned (01 §5).
   **Control:** the harness also constructs renames known to capture and
   asserts they are REJECTED, and fails if the generator never exercises a
   rejection path — a property that only ever sees accepts has not tested the
   guard.
4. **Decision determinism under input-order permutation.** For the pure
   decision stages — the matching cascade over a candidate set, placement
   tiers, vote tallies — permuting the iteration order of inputs yields
   byte-identical decisions and trails. This is the dynamic half of 02 §5's
   `HashMap`-iteration hazard; the static half is the clippy restriction lint
   `iter_over_hash_type` (allow by default, set to deny in `humanify-core`'s
   decision modules). The lint cannot see order-dependence smuggled through an
   ordered-but-wrong sort key; the property can. The end-to-end versions of
   this check remain self-hop = 0 and warm-rerun byte identity, as today.
5. **Span-applier properties.** The rename ledger persists half-open
   `[start, end)` spans taken from parser offsets and applied by string
   slicing (`src/rename/rename-ledger.ts:29-45,65-66,93`;
   `src/commands/unified.ts:255-281`). For generated text plus a
   non-overlapping span set with recorded original slices: (a) every span
   slices to its recorded original — the stale-ledger guard; (b) applying
   replacements in descending start order equals a reference rebuild from
   original slices; (c) index arithmetic is tested with non-BMP characters,
   because today's spans are UTF-16 code-unit indices despite a doc comment
   calling them a "byte range" (rename-ledger.ts:29) — the unit is DECIDED:
   all persisted/dumped spans are UTF-8 byte offsets, with the TS dumper
   converting (`07-differential-validation.md` §1); this property holds that
   decision under test, or phase 5a's byte-identical tree gate will fail on
   the first astral-plane string.

## 5. Guards: the planted-defect rule, then its automation

**The rule, institutionalized: every new guard lands with the red test that
plants the defect it catches.** This repo has already shipped a structurally
dead guard that read as perfect precision — the `singletonContradicts` incident
(`docs/responsibility.md`): on the binding path the guard examined nothing, for
11,094 unguarded accepts on 215→216 (`01-current-architecture.md` §4). The
existing enforcement precedent is exact: `src/kill-switches.test.ts` and
`src/analysis/fingerprint-index.test.ts` were verified by planting the defect
and watching them go red — and the binding half of the sort check **passed
against a planted break on its first draft**, because the fixture gave no
binding two callees to compare; it now asserts it examined something
(`docs/responsibility.md:139-153`). `src/rename/scope-era.test.ts:126-131`
records the same discipline: planting a break at two of three consultation
points left the first test green, so two more tests were added, each confirmed
red with its consultation point disabled.

Two concrete obligations in Rust:

- The red step for a guard is watching its test fail with the guard inverted
  or emptied (locally, then restored) — not merely watching it fail before the
  guard exists.
- **The examined-something rule:** any test asserting absence (zero
  rejections, no collisions, empty diff) must also assert the examined
  population is non-empty — `assert!(examined > 0, "fixture gave the guard
nothing to examine")` — so a vacuous fixture cannot certify anything.

**Mutation testing (cargo-mutants) automates the planted-defect check at
scale.** cargo-mutants rewrites function bodies and conditions (return a
default, delete an arm, flip an operator) and reports every mutant the suite
fails to kill — which is precisely "plant the defect, demand red", applied to
every guard mechanically instead of once at authoring time. Scope it to the
guard modules, where a silent no-op costs the most: singleton rejection,
contradiction, injectivity demotion (01 §4), the seven rename legality rules
(01 §5), and the placement abstain gates (01 §6).

- Targeted run after touching a guard:
  `cargo mutants --file crates/humanify-core/src/matching/guards.rs`
  (path illustrative until the crate exists).
- Changed-code run: `cargo mutants --in-diff <diff-file>`.
- Cadence: on-demand after guard changes, plus a periodic (roughly weekly)
  sweep of the guard scope. **Never in the commit gate** — each mutant
  rebuilds and runs the suite, so runs are minutes to hours (estimate)
  against a gate budget of seconds.
- Triage protocol, every surviving mutant gets exactly one disposition:
  (1) write the killing test — the default; (2) the mutated code is dead —
  delete it (knip's job today, mutation catches the semantic variant);
  (3) allowlist with a written justification in a committed exclusions file —
  mirroring the clone-census allowlist discipline, where an exception exists
  only with its reason attached (`scripts/check.ts:77-82`). An unexplained
  survivor is an open finding, not background noise.

## 6. Doc-drift guards, ported

`src/rename/transfer-pipeline.test.ts:35-46` reads `docs/naming-pipeline.md`
and asserts every registered step name appears in it — plain substring, no
markdown parsing — with a failure message naming the fix ("regenerate the
phase-1 table"). Companion tests pin the registry itself: the exact ordered
name list (`statement-twin, exact-match, close-match, binding-cascade,
vote-propagation, close-match-suggestions, retry`) and per-step description
and run-function presence (transfer-pipeline.test.ts:7-33). The mechanism is
trivially portable and applies to every Rust registry:

```rust
#[test]
fn naming_pipeline_doc_mentions_every_registered_step() {
    let doc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../../docs/naming-pipeline.md"
    )).unwrap();
    for step in TRANSFER_PIPELINE.steps() {
        assert!(doc.contains(step.name), "doc drift: regenerate the table for {}", step.name);
    }
}
```

In scope: the transfer pipeline, the matching cascade tiers, the placement
tiers, and the pipeline-contract doc from 03 phase 0 (serde field names
asserted present, so contract drift fails a test on the side that changed).
Each registry also keeps the pinned ordered-name-list test, so reordering or
renaming a tier is a conscious two-file change. The registry-confinement
guards port the same way, by the same source-as-text technique
`test/measurement-owners.test.ts` uses throughout: a test walks
`crates/humanify-core/src` and fails on `std::env` outside the one `env`
module (the `src/kill-switches.test.ts` analog, promised in 02 §3), with
clippy's `disallowed-methods` configuration in `clippy.toml` as the redundant
static layer.

## 7. Fuzzing (cargo-fuzz)

Low priority, two targets, run on-demand on nightly (libFuzzer). First, the
canonical hash serializer (02 §4a): fuzz over parseable JS drawn from a corpus
seeded with the fixture minified files, asserting no panics and stable output
across repeated serialization — the serializer becomes the substrate of every
persisted identity, so panic-freedom on weird-but-legal input is worth a cheap
standing harness. Second, the ledger readers (`split-ledger.json`,
`rename-ledger.json`): corrupt or truncated input must produce a loud typed
error, never a partially-applied ledger — a mis-read here writes wrong spans
into real output. Inputs are our own artifacts plus minifier output, not
adversarial content, which is why this is a background instrument and not a
gate.

## 8. Coverage (cargo-llvm-cov)

Report-only, no threshold gate — matching current culture: the TS repo ships
no coverage tooling at all, and a percentage floor is exactly the kind of
number that gets gamed while the dead-guard class sails through at 100%
executed. `cargo llvm-cov --html` on-demand when choosing what to test next or
scoping a mutation run; `cargo llvm-cov --summary-only` for a quick number.
Coverage informs; mutation and planted defects verify.

## 9. Red/green in Rust, mechanically

The cycle is unchanged from the house rules; the commands change:

1. **Red.** Write the test as if the code existed. Run exactly it and watch it
   fail: `cargo nextest run -p humanify-core -E 'test(rename_when_child_scope_shadows_target_rejects)'`
   (or `cargo test -p humanify-core <substring>`). For a bug: reproduce it as
   this failing test first. For a guard: additionally plant the defect (§5)
   and watch the test catch it.
2. **Green.** Implement the minimum; re-run the same filter.
3. **Full loop.** `bacon test` (or `cargo watch -x 'nextest run'`) keeps the
   per-save loop hot; the check command runs the whole gate before commit.

A test that passes on first run is treated as broken until shown otherwise —
the compile step gives Rust a false comfort here, because "it finally
compiles" feels like red/green and is not: a test that only ever failed to
COMPILE has never been observed failing on behavior. Make it compile against
a stub returning the wrong answer, see the assertion fail, then implement.

During phases 1–5 there is a second green above the unit suite: the phase's
oracle parity gate (03). Both are required; neither substitutes for the other.

## 10. What is NOT tested in Rust

The measurement stack stays TypeScript and keeps its own tests, unchanged
(02 §8, 03 phase 6). That includes `test/measurement-owners.test.ts` guarding
the living instrument set, the eval registry's proves/cannotProve discipline
(`scripts/eval.ts`, enforced at measurement-owners.test.ts:184-197), the
fingerprint preflight, and the harness-side e2e machinery under
`test/e2e/harness/`. `npm run check` continues to run those TS suites after
the port; the Rust binary is their subject, never their replacement.
Consequently three verdict classes are permanently out of scope for the Rust
suite: LLM-dependent quality (eval only, cold, rule 10), cross-version noise
KPIs (eval only, judged against measured bands, rule 11), and
should-change-nothing refactor proofs (`experiments/lib/neutrality.sh`, warm).
The binary-integration layer (taxonomy row 5) runs against a committed warm
cache and therefore proves plumbing and determinism with the model held fixed
— the one use rule 10 permits — and must state, in its own test names and
docs, that it cannot prove naming quality.

## Open questions

- **Manifest ownership and name.** `test/e2e/fixtures/manifest.json` is
  proposed here; whether the TS harness writes it during `setup` or a separate
  script owns it (and whether it lists the four real-package fixtures'
  expected minifier-version pins) needs deciding before the `rust:parity`
  stage can exist.
- **Mock-provider seam for layer 5.** In-process tests mock the provider
  trait; the binary-level tests currently plan on a committed warm cache. Is a
  `--llm-endpoint` stub server worth building instead, so binary tests can
  exercise retry/timeout paths the cache short-circuits?
- **Span unit follow-through** (§4 property 5): the unit is decided — UTF-8
  byte offsets per `07-differential-validation.md` §1 — but the internal
  rename-ledger spans are UTF-16 code units today; the open piece is where
  the conversion lives in the TS dumper and that property 5's non-BMP cases
  cover it.
- **Snapshot-format field list.** Which of `snapshot.ts:9-38`'s fields carry
  over into the Rust-era insta snapshots verbatim, and whether
  `cacheSavingsRate`/`hashCollisionRate` (internal-hash-shaped) stay
  meaningful once the canonical serialization replaces beautified-text
  hashing (02 §4a).
- **Nextest as the default runner.** Assumed above for filter expressions and
  speed; plain `cargo test` remains sufficient if adding a tool to the
  toolchain is unwanted.
