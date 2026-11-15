# exp237: harness cleanup (phase C of the simplification campaign)

STATUS: done on branch `exp237-harness-cleanup` (rebased on 37662192c), not merged.
Harness only (`crates/humanify-ag-dev`, `dev/eval/`), plus three lines of
one product TEST (`humanify-ag-cli/src/launch_parity_test.rs` read the deleted
seeded launch golden); no product code changed, so the pipeline binary is
byte-identical.

## Totals

Against origin/main: **-4,354 code lines, -1,864 test lines, -41,143 data
lines** (net). The data is the 43 TS-era eval labels (578 files) and the
seeded launch golden.

| Item | Code (net) | Test (net) | Data |
|---|---:|---:|---:|
| Dead verbs out (match-truth, ask-trace, diff-asks, env-check, eval cjs-placement, library tokens14) and the test-only prettier spelling, SUCCESSORS, RETIRED_FLAGS, the verb table's lane/today fields | about -3,300 | about -1,150 | 0 |
| eval score fails loud; seeded/archive base modes out; neutrality and walk fallbacks | about -400 | about -800 | seeded golden, pairs.json `priorsBase` |
| identity.json is the identity source; TS-era labels and their readers out | -19 | +43 | -40,835 (578 files) |
| Harness modules crate-private; what rustc then called dead, deleted | about -630 (plus the verb table moved from main.rs to cli.rs) | +50 | 0 |

## What changed, in plain words

**eval score never falls back.** Before, a failed rebase quietly scored the
pair against the July-era archive tree, and a missing input skipped the pair
with exit 0. Now a missing input for a selected pair is refused before
anything builds (exit 2). Any pair that fails (rebase, scored leg, analysis,
a refused card, the boot gate, the self-hop) says so in its own block, the
other pairs still run, and the run ends with one `!! eval score FAILED`
verdict listing every failure, exit 1. An analysis failure no longer writes an
empty card file.

**One base mode.** `--base-mode` and `--priors-base` are gone; every base is
rebuilt from scratch. The seeded launch golden, the archive priors path in
pairs.json, the "mixed base modes" guard and every base-mode guess from prior
paths are deleted. The scratch launch golden (`run-launch.golden.txt`) is
byte-identical: the binary's launches did not change.

**Identities come from identity.json.** `eval score` already wrote one per
card; now the leaderboard and bands read those records instead of rebuilding
identities from old file shapes. `eval rescore` writes one beside every card
it re-scores. A label without them is refused with the fix named
(`eval rescore`). The 43 committed TS-era labels and the code that read only
them (scorer-1 cards, the TS pipeline kind, the TS matcher preflight notes,
the date rule for the naming mode) are deleted.

**The compiler finds dead harness code.** Only the verb table (`cli`) is
public; everything else is crate-private, so rustc's dead-code lint covers
the harness and reported about 50 more dead items, now deleted or moved into the tests that use them.

## Every silent fallback found, and what it does now

| Where | Before | Now |
|---|---|---|
| score: rebase writes no tree | scored against the archive prior | pair fails, run exits 1 |
| score: rebase config cannot be written | archive prior | pair fails |
| score: rebase input missing | archive prior | refused before the build (exit 2) |
| score: scored input missing | `SKIP`, exit 0 | refused before the build (exit 2) |
| score: archive prior missing | `SKIP` | the archive is gone; not needed |
| score: gated base never arrives | said, exit 0 | pair fails |
| score: scored leg config / runcfg write fails, leg does not run, artifacts missing | said, exit 0 | pair fails |
| score: run status cannot be recorded | warning, scored anyway | pair fails |
| score: card refused (provider errors, library skipped) | card removed, exit 0 | pair fails |
| score: analysis fails | wrote an EMPTY card | no card, pair fails |
| score: card or boot verdict write fails | said, exit 0 | pair fails |
| score: boot gate fails | said, exit 0 | pair fails |
| score: self-hop cache copy / dir creation | error ignored (`let _`) | self-hop fails |
| score: self-hop skipped because its tree is missing | silent return | self-hop failure listed |
| score: self-hop verdict write | error ignored | self-hop fails |
| score: warm self-hop fails | in the verdict file only | listed, exit 1 |
| score: summary fails / identities not written or cards disagree | summary exit only / ignored | listed, exit 1 |
| score: commit unreadable | commit.txt silently not written | refused |
| neutrality: no `<from>-rebased` prior | used `<priors>/<from>` instead | refused, naming the path |
| neutrality: a leg's `--help` does not run | empty help read as a legacy binary (flags) | refused |
| neutrality: build log write | ignored | message carries both errors |
| walk `--resume`: manifest.json unreadable | replaced by a fresh manifest | refused, left as it was |
| walk: preflight log / boot record write | ignored | error / loud line |
| library layers `--jobs` | accepted and ignored | not a setting of the verb |
| cards without `scorerVersion` | read as scorer 1 | refused by summarize and the identity derivation |

Kept on purpose: a missing diagnostics trail or a failed report page is said
but does not fail the pair (every KPI is unaffected); a pipeline that exits
non-zero having written its tree is scored and stamped, as before; the walk's
range form still skips versions without an input (its tested design).

## Kept, and why

- `eval neutrality`'s legacy-baseline support (pre-`--config` binaries, the
  pre-rename crate): small, tested, and the product lanes still rely on
  neutrality; left for the runner lane.
- `noise-bands.json`'s per-scorer layout (`byScorer`): changing it is a data
  migration with the bands writer; left for a later lane.
- The library scorer, its builders and the Python JSON spelling (H4, later).
- `dev/eval/goldens/ask-trace.golden.txt` and `match-truth.golden.txt`:
  their verbs are gone, but the product's launch-parity test reads every
  golden as a spec of the binary's launch shapes, so they stay as data.
- `records.rs` still writes run manifests and runcfgs with `baseMode:
  "scratch"`: the runcfg is part of the launch golden, which must stay
  byte-identical.

## Consequence for readers

The current reference labels on this host that were re-scored before this
change (`*-s3`) have no identity records, so `eval leaderboard` refuses them
until they are re-scored once: `cargo dev eval rescore <label>-s3 --cards
<the original label> --work <its trees>` (47 s for `ref-names-40921b9f-r1`).
Done here for `ref-names-40921b9f-r1-s3` (cards byte-identical to the host's
copy). An off-mode label from before the naming-mode record (2026-10-03) gets
an UNKNOWN naming mode from its records, which the leaderboard refuses unless
`--force-mixed`.

## How it was checked

- red/green tests for every behaviour change (score, neutrality, walk,
  identity, rescore, summarize);
- `cargo dev eval leaderboard night-3cb20421-r1 night-6c4b4d13-r1`, `eval
  summarize night-6c4b4d13-r1` and `eval leaderboard ref-names-40921b9f-r1-s3
  night-6c4b4d13-r1` print the same text as main's harness except the removed
  base-mode lines;
- `cargo dev check`, all stages.
