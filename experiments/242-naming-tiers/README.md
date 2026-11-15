# exp242 — one naming schedule: the turn driver, the sequential tier, the levers and the ordered mode go

STATUS: BUILT (red/green for the removed flags and keys), GATED
(`cargo dev check`, all 12 stages), NEUTRAL on two pairs (see
Neutrality). Branch `exp242-naming-tiers`, not merged. Plan item 1c of the
simplification campaign.

## Totals

Raw diff lines against origin/main `340b37ea7` (comments and blank lines
included):

| | removed | added | net |
| --- | ---: | ---: | ---: |
| product code | 602 | 51 | **-551** |
| product tests (incl. the launch-parity fixture, 111 of it) | 536 | 89 | -447 |
| harness code (the e2e sequential leg) | 79 | 10 | -69 |
| harness tests | 19 | 17 | -2 |
| `test/golden/legacy-default/` (6 files) | 343 | 0 | -343 |
| launch goldens, help goldens, example config, census allowlist | 1,038 | 111 | -927 |
| docs (concurrency.md rule 9, one flow doc, five fixture READMEs) | 57 | 38 | -19 |

Per item (code / tests):

| item | code − / + | tests − / + |
| --- | ---: | ---: |
| the tier enum and levers (`fast.rs`, `FastTier`, `Lever`, `Levers`) and the `fast` field threaded through naming, era and prior match | -115 / 0 (fast.rs) plus the plumbing below | -1 each in 7 struct literals |
| the turn driver (`drive_round_turns`, `run_retries`, `dispatch`, `commit_prepared_immediate`) and the two lever switches in the processor | -170 / +8 | processor_test +6 / -3 |
| the non-pipelined branches in `naming/driver.rs`, `era.rs` (close contexts), `prior.rs` (both sides parsed in sequence; `program_jsons`) | -109 / +29 | — |
| the ordered mode in `par.rs` (the `ORDERED` static and the sequential branch of every helper) | -44 / 0 | par_test -76 / +4 |
| config keys `run.order`, `naming.schedule`, `naming.relaxed_levers` (registry, resolver, validation, record) | -152 / +6 | config/surface/config_file tests -91 / +10 |
| e2e: the sequential leg and its comparison with the legacy goldens | -79 / +10 | run_test -1 |
| tier-comparison tests: `fast_test.rs` (turn driver vs pipelined, lever sizing), the ordered replay pin | — | fast_test -141 / +60, replay_determinism -90 |

## What was there

The naming waves had three schedules (`FastTier`):

- `Off`, the turn driver: every lane's next call dispatched together, each
  turn waiting for its slowest call. Reached only through the ordered test
  mode (`run.order = "ordered"`), which also made the `par` helpers run
  sequentially and forced a one-call prompt window.
- `Exact` (`naming.schedule = "sequential"`): pipelined, both relaxed
  levers off. It reproduced the bytes of the default before the relaxed
  flip; the e2e stage ran it twice per fixture pair (batch size 10) and
  compared it with six committed trees in `test/golden/legacy-default/`.
- `Relaxed(levers)`: the default, both levers on (`window-lanes`,
  `defer-shadowed`); `naming.relaxed_levers` picked a subset for sizing.

## What changed

There is one schedule, the old default: pipelined rounds, one lane per
batch-sized window for a function larger than one batch, and the shadowed
pass of wave N riding with wave N+1. Nothing switches on a tier any more.

- `crates/humanify-ag-core/src/fast.rs` is deleted. The `fast` field is
  gone from `NamingConfig`, `EraOptions`, `WaveInputs` and
  `PriorMatchInput`, and every `if fast { parallel } else { sequential }`
  keeps only the branch the default took: the prior match builds the prior
  side on its own thread, the close contexts map on the pool, the semantic
  baseline and the family-permute index are built beside the naming era,
  and the verdict runs beside a speculative reconcile.
- The processor's `drive_round` is the pipelined driver (renamed from
  `drive_round_pipelined`); the turn driver and the helpers only it used
  are deleted. `compute_lane_count` stays: it still decides the lanes of a
  function that fits in one batch but has more bindings than
  `lane_threshold`.
- `par.rs` has no mode: each helper is its parallel body only.
- The config keys `run.order`, `naming.schedule` and
  `naming.relaxed_levers` and their flags (`--run-order`,
  `--naming-schedule`, `--naming-relaxed-levers`) are gone. A config file
  that sets one is refused with the one unknown-key error (`[run]` as a
  whole is now an unknown table); a flag is refused by the parser as an
  unexpected argument. The run record (`--stats-json` `config.naming`)
  loses its `schedule` block; no harness reader read it.
- e2e runs fresh once and prior twice per pair, as the default always did;
  the sequential leg and the legacy goldens are gone. 88 launches left the
  e2e launch golden.
- The launch-parity fixture (the commander-era captures) loses its
  `fastTier` field, `--sequential` from four captured argvs and the one
  `--relaxed-levers` row. Its `split` exception for the e2e golden existed
  only for the sequential legs and is removed; the ask-trace golden loses
  its `schedule = "sequential"` line.
- The census allowlist loses the `static ORDERED` entry and the census
  loses the `order-mode` pattern. `docs/concurrency.md` rule 9 now says
  there is no ordered mode and how determinism is tested instead.

## Tests

- Red first: `surface_test::a_removed_flag_gets_the_parsers_unknown_argument_error`
  now lists `--run-order`, `--naming-schedule`, `--naming-relaxed-levers`,
  and `config_file_test::a_removed_key_gets_the_unknown_key_error` lists
  `naming.schedule`, `naming.relaxed_levers` and `[run] order`. Both failed
  on the old code and pass on the new.
- "Completion order never reaches output" (docs/concurrency.md rule 6) is
  still pinned, without a turn driver to compare against:
  `fast_test::pipelined_dispatch_ships_the_same_bytes_under_any_completion_order`
  runs the naming stage under a newest-first provider (asserting the run
  really overlapped) and under three seeded shuffled orders and requires
  the same code, dispatch records and processor report;
  `with_a_prior_any_completion_order_ships_the_same_bytes` does the same
  with a prior (close match, speculative reconcile).
  `a_run_is_byte_identical_to_itself` stays.
- Deleted with what they compared: turn driver vs pipelined, the lever
  sizing tests (`window_lanes_shorten_the_longest_chain`,
  `defer_shadowed_saves_rounds`: there is no lever-off run to compare
  with), the par ordered-mode tests, the config tests of the three keys and
  the ordered replay pin in `tests/replay_determinism.rs` (the live/replay
  test above it stays).
- `processor_test::a_cross_lane_collision_gets_exactly_one_disclosed_reask`
  ran under the turn driver with four lanes; under the one schedule its 37
  bindings are two windows of 25 and the colliding pair shares a lane. It
  now sets `batch_size = 10`, which gives the same four lanes and keeps it
  a cross-lane collision.
- Harness tests that used `naming.schedule=sequential` as an example
  pipeline setting use `naming.lane_threshold=30`.

## Kept, and why

- `compute_lane_count` and `naming.lane_threshold`: still live on the
  default path (see above).
- `NameProvider::run_wave`: the sweep, the split namer and the vendor namer
  still dispatch through it; only the turn driver's use went.
- The registry's `Kind::List(Option<choices>)`: no product key uses a
  choice list now (`relaxed_levers` was the only one), but the type is
  shared with the harness registry; making it a plain `List` touches about
  ten harness lines. Left for a harness lane.
- `docs/config-consolidation.md` and `docs/dev-harness.md` mention
  `--sequential` as history; not edited.

## Neutrality

(filled in below)
