# exp198 — harness migration lane L3: every runner in Rust

STATUS: DONE (2026-11-08), branch `harness-l3-runners`, rebased on main
`05d399832` (L2a, L2b and L4 landed). Scope is `docs/dev-harness.md` §5
lane L3 (the runners), on top of lane L1's crate (`crates/humanify-ag-dev`),
with Andrew's 2026-11-07 decisions: the walk is ported, and the eval KPI
scorer is a new scorer version (lane L2a), which the runners now call.

## Totals

- **Every runner the harness launches is a `cargo dev` verb**: `eval score`,
  `eval neutrality`, `eval walk` + `eval walk-report`, `library score`,
  `boot-gate`, `index build`, `run stub`, `build-bin <ref>`. About 8,600
  lines of Rust (1,900 of them tests), in `src/runners/` and `src/launch.rs`.
- **Launch goldens: 10 of 10 that cover a runner are held byte for byte**
  by recorder tests (run-launch x2, neutrality x5, library x2, boot gate);
  a planted one-word change turns them red. One stated normalisation (the
  boot gate's `OLDPWD`/`SHLVL`, which only a shell creates).
- **Real runs, each against the TS instrument on the same inputs:**
  - `eval score --pairs 85->86` (cold): same files, same record shapes,
    same verdicts; **novel 787 / realLn 78,791 byte-equal** between the TS
    and the Rust run (both under scorer v1 and v2); the Rust run's card is
    byte-identical to the TS v1 scorer re-run on the Rust run's trees.
  - `eval neutrality` (warm, `2e0ef033f` vs this branch, 85->86): TS and Rust
    both **NEUTRAL, 0 files / 0 lines, candidate +0 / baseline +0**; the two
    candidate trees are byte-identical to each other.
  - `eval walk 2.1.214..2.1.216` (names mode, 3 hops, all exit 0, all boot
    OK): the Rust report equals the TS report on the same trees except the
    walk dir's path and the new section; both reports' cards are
    byte-identical.
  - `eval walk-report` on the full 124-hop walk `/work/walk-9ce9dee2-full`:
    equal to the TS report there except the new section; **2.1.179->2.1.181
    is flagged** (noise 53.8x median, vendor hash turnover 20.6%).
  - `library score` on `kind-rollup` (34 bundles): **34 / 34 cards
    byte-identical** to the TS runner's (the label name aside); pipeline.json
    and summary.json equal but for the binary's build commit and sha.
- **Gate**: §3.

## 1. What was ported (old path -> verb)

L5 deletes exactly these:

| Old (TS / sh / Python)                                                                      | New (`cargo dev ...`, module)                                                   | Notes                                                                                                                            |
| ------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| `scripts/eval.ts` verbs `score`, `neutrality`, `library`                                    | `eval score`, `eval neutrality`, `library score` (`runners::verbs`)             | flags are L1's `HARNESS_KEYS`                                                                                                    |
| `experiments/034-eval-harness/run.sh`                                                       | `eval score` (`runners::score`)                                                 | rebase (scratch / seeded / archive), scored leg, run status + refusals, card, evidence page, boot, cold + warm self-hop, summary |
| `experiments/lib/pipeline-bin.ts`                                                           | `runners::bin` (`build_and_describe`, `bin_commit_refusal`) on L1's `bin_build` |                                                                                                                                  |
| `experiments/lib/pipeline-config.ts`                                                        | `launch` (`render_config`, `write_config`, `HumanifyArgs`)                      |                                                                                                                                  |
| `experiments/lib/run-pipeline.ts`                                                           | `runners::leg` (`run_leg`)                                                      | peak RSS sampled from /proc while the child lives (`proc::run_to_file`)                                                          |
| `experiments/lib/run-manifest.ts` (writer, `manifestWarnings`, `priorKindOf`, `baseModeOf`) | `runners::records`                                                              | comparison readers: L1 identity / L2a                                                                                            |
| `experiments/lib/invariants.ts` (writer: `writeRunStatus`, both refusals)                   | `runners::records`                                                              | reader side (banners, `coldRangeVerdict`): L2a's summarize                                                                       |
| `experiments/lib/llm-endpoint.sh`                                                           | L1 `committed::local::resolve_endpoint`                                         |                                                                                                                                  |
| `experiments/lib/neutrality.sh`, `experiments/lib/build-bin.sh`                             | `eval neutrality` (`runners::neutrality`), `build-bin <ref>`                    | legacy no-`--config` leg kept                                                                                                    |
| `experiments/lib/boot-gate.sh`                                                              | `boot-gate <tree> <version>` (`runners::boot`)                                  |                                                                                                                                  |
| `experiments/096-library-matching-score/run.sh`, `run.py`                                   | `library score` (`runners::library::score_label`)                               | corpus verify and scoring call L2b's `library::corpus::verify` and `library::score::rescore`                                     |
| `experiments/166-restore-or-ask/stub-run.mts`                                               | `run stub <bin> <bundle> <out> [<prior>]` (`runners::stub`, testkit stub)       |                                                                                                                                  |
| `scripts/build-index.sh` (branch `exp184-index-spec`, never on main)                        | `index build <spec>` (`runners::index`)                                         | on today's `humanify-ag index build`                                                                                             |
| `experiments/walk/walk.sh`                                                                  | `eval walk <from>..<to> --walk-dir D` (`runners::walk`)                         | rust only                                                                                                                        |
| `experiments/walk/report.ts`, `hunks.ts`, `history.sh`, `hunks.test.ts`                     | `eval walk-report <name>=<dir>...` (`runners::walk::{report,hunks,history}`)    | + "Hops to look at"                                                                                                              |
| `experiments/034-eval-harness/run-launch.test.ts`                                           | `runners::score_test`                                                           |                                                                                                                                  |
| `test/launch-goldens.test.ts` (neutrality, library, boot parts)                             | `runners::{neutrality,library}_test`, `tests/boot_gate.rs`                      |                                                                                                                                  |
| `test/eval-score-args.test.ts` (score, neutrality parts)                                    | `runners::config_test`, `runners::score_test`                                   |                                                                                                                                  |
| `test/boot-gate-env.test.ts`                                                                | `tests/boot_gate.rs`                                                            |                                                                                                                                  |

NOT ported (experiment-specific index builds with pinned tool paths and
non-spec flags, no living caller; retire with their experiments):
`experiments/152-full-bun-index/scripts/*` (`build-full.sh`, `make_shims.sh`,
`timed.sh`, `step_times.py`, `root_roles.py`) and
`experiments/181-index-all-bundlers/scripts/ix.sh`.

## 2. Proof

| Check                                                                    | How                                                                                                                                                                 | Result                                                                                                                                                                                                                     |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| run-launch.golden.txt, run-launch.seeded-base.golden.txt                 | `eval score` with the run.sh recorder (byte-identical script, its sha is in the golden) at all three sites; the scored legs' launches checked against their runcfgs | byte-identical (both)                                                                                                                                                                                                      |
| neutrality-{names, off, baseline-pre-rename, legacy-refused, legacy-leg} | sandbox repo, cargo + binary recorders; run config compared by value                                                                                                | launches byte-identical, exit codes equal (5/5)                                                                                                                                                                            |
| library-structural, library-agreement-sets                               | discover under a taskset recorder (the `python ::` lines are L2b's corpus.py / score.py)                                                                            | byte-identical (2/2)                                                                                                                                                                                                       |
| boot-gate.golden.txt                                                     | the real `humanify-ag-dev boot-gate` under `env -i` + seeded session vars                                                                                           | byte-identical except `OLDPWD`/`SHLVL` (bash-created)                                                                                                                                                                      |
| red seen                                                                 | `split.enabled` planted false: 7 golden tests fail; `ANTHROPIC_SMALL_FAST_MODEL` dropped from the scrub: boot test fails                                            | red, restored                                                                                                                                                                                                              |
| eval score 85->86, cold                                                  | TS label `l3-ts-8586` (frozen `2e0ef033f`) vs Rust `l3-rs-8586` (`e15f54d4c`), each under the lock, alone                                                           | same file set (Rust adds `2.1.86.identity.json`); every record's shape equal except the manifest's `provenance.node` -> `provenance.harness`; both exit 0, boot OK, cold self-hop 0 ln, warm self-hop identical + 0 writes |
| the card                                                                 | TS `analyze.ts` re-run on the Rust run's trees                                                                                                                      | `cmp` identical to the Rust run's card (v1 seam)                                                                                                                                                                           |
| band-0 KPIs                                                              | TS vs Rust card; and `eval rescore` (scorer v2) of both labels                                                                                                      | novel 787 / realLn 78,791 equal in all four; noise 1,490 vs 1,484 (inside the band)                                                                                                                                        |
| neutrality, warm                                                         | `npm run eval -- neutrality 2e0ef033f 2.1.85:2.1.86` vs `cargo dev eval neutrality ...`, same priors, same cache                                                    | both NEUTRAL 0/0, candidate +0, baseline +0, exit 0/0; TS vs Rust candidate trees 0 files differ, same TOML                                                                                                                |
| walk 2.1.214..2.1.216                                                    | `cargo dev eval walk`, then `eval walk-report`, and TS `report.ts` on a hardlinked copy with no cards (it scored its own)                                           | report.md equal except dir path and the new section; cards identical; hops 2-3 record `priorKind: walk-previous-hop`, no ARCHIVE warning                                                                                   |
| full walk report (124 hops)                                              | `eval walk-report rust=/work/walk-9ce9dee2-full` vs the committed TS `report/report.md` there                                                                       | equal except the new section; report.json equal minus `vendorHashTurnover`; 179->181 flagged                                                                                                                               |
| library score, kind-rollup                                               | `npm run eval -- library` vs `cargo dev library score`, same index (`ix-rollup`), rule structural                                                                   | 34/34 cards identical but the label                                                                                                                                                                                        |

## 3. Gate

| Check                                                                                    | Result                                                                                                                                                      |
| ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo test -p humanify-ag-dev`                                                          | 296 + 8 + 1 + 2 passed                                                                                                                                      |
| concurrency census (TS and `cargo dev census concurrency`)                               | GREEN, 114 sites / 114 entries                                                                                                                              |
| `npm run check` in the worktree (`4afa1696f`)                                            | 13/13 PASS (unit 113.7 s, rust:unit 306.5 s, rust:build 126.6 s, e2e 77.3 s)                                                                                |
| `npm run check` on a clean checkout of the merge (`6a31828c3`, README-only change since) | 13/13 PASS (unit 114.2 s, rust:unit 300.9 s, rust:build 136.9 s, e2e 77.1 s)                                                                                |
| `cargo dev check` (L4's gate) on the same clean checkout                                 | 11/11 PASS; census:clones REVIEW (advisory): `provider_error_refusal` / `library_refusal` twins, `build_bin_ref` vs main.rs `build_bin` output — follow-ups |

## 4. What changed from the design, and why

1. **Proof (c) for the scored leg reads its runcfg.** run.sh launched the
   scored leg through `run-pipeline.ts`, so its golden holds the runcfg, not a
   launch line. The Rust runner launches it directly and still writes the
   runcfg; the test compares every non-scored launch to the golden, the
   runcfgs to the golden, and each scored launch to its own runcfg.
2. **The run config is the harness's one** (`settings::run_config`, plus a
   `resolved` block in neutrality's), not eval.ts's shape; the neutrality
   goldens' `run-config|` lines are checked by value.
3. **Retired: `--no-layout`, `--no-vendor-score` (and `--no-vendor`).**
   Scorer v2 scores the layout block and vendor whenever both trees have them
   (rule 8); a switch it would ignore is refused, naming why.
4. **`eval score`'s default work dir is `/work/eval-work`**, not `/tmp/eval-work`
   (an overlay that does not survive a rebuild, §8 risk 6). `run.workdir`'s
   default is per verb (neutrality keeps `/work`).
5. **The label guard also checks the naming mode and the library index**
   (the gap §3.2 names), and a legacy label's base mode is read from its
   run manifests.
6. **The run manifest's `provenance.node` is gone** (no Node runs);
   `provenance.harness: "humanify-ag-dev"` says who wrote it.
7. **Walk:** priors are `walk-previous-hop` (the false ARCHIVE warning fixed);
   `--resume` extending a walk keeps versions in semver order (walk.sh's
   `unique_by` sorted them as strings, which would have mis-paired hops in
   the report); off mode is `--pipeline-set library.matching=off`, the same
   spelling `eval score` uses, instead of `--library-matching-off`; each hop's
   pipeline run takes `/work/heavy.lock` (or none under `HELD_LOCK`) instead
   of `/work/heavy-run.sh` slots; the report is part of `eval walk`
   (`--no-report` to skip) and `eval walk-report` for finished walks.
8. **Hops to look at** (new): a hop is flagged when its noiseLines is >= 10x
   the walk's median, or when >= 5% of its hash-named vendor files
   (`module-<8 hex>.js`, `lib_<8 hex>.js`) appear in only one tree and that
   count is >= 10x the median. On the full walk it flags 69->70, 70->71,
   148->149, 179->181 (both rules) and 193->195.
9. **One subprocess fan-out** (`runners::each`, results in input order)
   serves the discover pool, the index chunks and the walk's scoring:
   allowlisted once. File-logged children (`proc::run_to_file`) and the
   frozen walk binary are written by child processes (no ETXTBSY).
10. **`index build` takes the heavy lock itself** (unless `HELD_LOCK`) plus
    `/work/index-build.lock`; the dry run's retry is bounded (24 x 5 min).
    Its flags: `--registry --store --index-work --index-out --chunks
--index-jobs --merge-jobs` (registry keys, no env reads).

## 5. What I could not prove

- `index build` has no TS golden: `scripts/build-index.sh` never landed on
  main and targets the pre-exp186 binary. Its launch sequence (dry run, every
  chunk, one merge; a failed chunk stops the merge) is held by recorder tests,
  not by a recorded live run, and no real index was built in this lane.
- `run stub` is tested against a fake binary that asks the stub; no real
  bundle was stub-run here.
- The boot gate's `OLDPWD`/`SHLVL` cannot be reproduced without a shell (§2).
- No identity record is written for walk hops or neutrality legs (§3.1 lists
  them); only eval cards get one.
- `launch_parity_test` (humanify-ag-cli) still reads only the run-launch
  goldens for its settings parity; the runner goldens are held by the
  dev-crate tests above.

## Proposed CLAUDE.md text (for L5)

In "Validating cross-version changes": replace `npm run eval -- score
<label>` / `neutrality <ref>` with `cargo dev eval score <label>` /
`cargo dev eval neutrality <ref> [from:to] --cache /work/neutrality-cache`;
add `cargo dev eval walk <from>..<to> --walk-dir D` ("a real version walk;
its report flags the hops to read") and `cargo dev library score <label>`.
Note `--no-layout` / `--no-vendor-score` are gone (scorer v2 always scores
both), and `eval score`'s default work dir is `/work/eval-work`.
