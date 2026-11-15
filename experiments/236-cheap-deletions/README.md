# exp236 — cheap deletions (simplification campaign, phase B)

STATUS: BUILT (red/green where behaviour was added or merged), GATED
(all stages pass on the rebased branch), NEUTRAL: byte-identical trees on
two pairs, warm, both judged runs with 0 cache writes on both legs.
Branch `exp236-cheap-deletions`, built from main `6c4b4d13f` and rebased
onto `a34c3c6f6` (after exp235 and exp237; no product code changed there,
so the neutrality baseline stays `6c4b4d13f`). Product crates only
(`humanify-ag-{core,cli,index,llm,model,testkit}`); the harness crate is
lane exp237's and was not touched.

## Proposal

Phase B of the simplification campaign (plan `lanes-1-3-4.md`, items 3a,
1a, 1b, 4a): delete code that only kept old spellings, test-only options
and TypeScript-era byte copies alive, and give each small duplicated
helper one owner. None of it may change a written tree.

## Totals

Raw diff lines (comments included), product crates, four commits:

| item | code removed / added | test removed / added |
| --- | ---: | ---: |
| 3a retired flag and key lists | -182 / +15 | -93 / +75 |
| 1a + 1b test-only options, `/3` reader, hand-written serializers | -276 / +62 | -107 / +71 |
| 4a one owner per small helper (and a comment reworded) | -346 / +272 | -263 / +273 |
| **total** | **-800 / +345 (net -455)** | **-463 / +419 (net -44)** |

The 4a "added" code is mostly the new owners (`model::time`, `model::numfmt`,
`testkit::helpers`, `ingest::node_type`, the `babel_view` line helpers) and
the four retired keys kept as data for the harness (below). Test lines are
close to even because each new owner and each generic refusal got a test.

Plus six rows in `docs/responsibility.md` for the new owners and two
lines in `humanify.example.toml` (regenerated).

## Neutrality

`cargo dev eval neutrality 6c4b4d13f <pair> --cache /work/neutrality-cache
--priors /work/exp050-cold --workdir /work/exp236-neutrality`, each pair run
twice; the first run after the recent cache-key change refills the cache,
the second is the one judged.

| pair | run | cache writes candidate / baseline | exits | differing files / lines | verdict |
| --- | --- | ---: | --- | ---: | --- |
| 2.1.118 -> 2.1.119 | 1 (refill) | 11,521 / 0 | 0 / 0 | 0 / 0 | NEUTRAL |
| 2.1.118 -> 2.1.119 | **2 (judged)** | **0 / 0** | 0 / 0 | **0 / 0** | **NEUTRAL** |
| 2.1.197 -> 2.1.198 | 1 (refill) | 10,822 / 0 | 0 / 0 | 0 / 0 | NEUTRAL |
| 2.1.197 -> 2.1.198 | **2 (judged)** | **0 / 0** | 0 / 0 | **0 / 0** | **NEUTRAL** |

The candidate was the pre-rebase commit `04e396db3`; the rebase onto
`a34c3c6f6` applied without conflict and brought no product change.

## What was done

**3a. Retired names are just unknown.** The pipeline's `RENAMED_FLAGS` (32
old spellings) and `RETIRED_FLAGS`, the stage verbs' pre-clap check, and the
config file's `RETIRED_KEYS` are gone. clap's "unexpected argument" and the
registry's "unknown key" (with its did-you-mean) already refused all of them;
new tests hold that generic refusal (`a_removed_flag_gets_the_parsers_unknown_argument_error`,
`a_removed_key_gets_the_unknown_key_error`; both red before the deletion
because the lists answered first with their own text). The launch parity
test still translates commander-era argv, so it keeps that map as its own
test data. `Registry.retired_keys` / `retired_flags` stay: the harness
registry (exp237's crate) still has them; the product's `retired_flags` is
empty. The harness's `--pipeline-set` check still reads the PIPELINE
registry's `retired_keys` (and its test expects "retired" for
`perf.profile`), so those four keys stay as data in `config/keys.rs`, read
by nothing in the product. Handoff: delete that check, its test line, the
four entries and the `Registry.retired_*` fields together.
Lost: the refusals' hints (`--api-key` used to name `HUMANIFY_API_KEY`).

**1a. Test-only options and the old index reader.**
- `MatchOptions.max_cascade_depth` (set only by tests) and
  `exclude_session_ids` (set by nothing) are gone, with the `depthGate` rung,
  the exclusion filter, and three tests of a depth production never used.
  Propagation's rungs keep their own tests (`propagation_test.rs`).
- `TwinGateOutput.dump_stats` reproduced a TS bug fixed 2026-09-20 and has
  equalled `stats` since; the dump reads `stats`.
- The library index reader accepts only `humanify-library-index/4`. The `/3`
  arm, `INDEX_FORMAT_NO_EDGES` and `Manifest::has_edges` are gone; a `/3`
  index is refused like any unknown format (tests turned from "still loads"
  to "is refused"). The in-memory index keeps `edges: Option` because the
  tests' row constructors build indexes without edges.

**1b. Derived serializers.** `ResolutionStats`, `EnclosingStmtAbstainCounts`,
`PropagationRungCounts`, `Occurrence` and `SiteSkip` serialize by `derive`
(field order already was the key order). A new byte pin of the stats bag
was run against the OLD hand-written serializer first (passed), then
against the derive (passes). `to_ts_value` is gone.

**4a. One owner per helper** (each with its own test):

| question | owner | copies merged |
| --- | --- | ---: |
| ESTree JSON node `type` | `ingest::node_type` | 10 functions + 5 inline |
| `\n`-only line starts; 0- and 1-based line of an offset | `babel_view::{line_starts, line_of, line_number}` | 3 builders, 2 lookups |
| oxc node parent | `babel_view::parent` | 2 |
| UTC clock text, civil date | `humanify_ag_model::time` | 5 (`js::iso_now` is a re-export, the harness calls it) |
| thousands separator | `humanify_ag_model::numfmt` | 1 product copy; 3 harness copies left for exp237 |
| token-stream divergence text | `naming::driver::validate::format_divergence` | 2 (the cli copy was test-only; the frozen vectors pass against core's) |
| test: owned string list, scratch dir | `humanify_ag_testkit::{strings, ScratchDir}` | 13 + 2 |

## Skipped, and why

- **Harness halves of 3a and plan item 4d** (`crates/humanify-ag-dev`): exp237's crate.
- **`init_identity_test::hs`**: looked like a string-list copy but sorts.
- **`is_babel_statement` ×3**: the kind lists differ (one counts
  `FunctionBody` and declarations). Same name, different questions — a
  real merge needs a decision, not a mechanical edit.
- **llm tests' tokio `run` ×2**: moving a runtime builder into testkit (a
  non-test crate) needs a concurrency-census entry; not worth 7 lines.
- **`suggest_all_names` ×2**: the trait's two methods default to each
  other; merging means reshaping the provider trait.
- **Other scratch-dir structs** (pipeline_stages, finish driver, temp_dir
  helpers): different shapes (extra methods, two paths, no drop).
- **Not attempted** (plan's 4a table, larger or decision-bearing):
  `fn_row_of_session`, `Interner::lookup`, `build_vector_map` /
  `rebuild_vectors` (1e territory), `spaced_lower_upper`, rename-ledger
  building, declaration index, JSON-AST index walk (1g territory).
- **1b's other hand-written serializers**: the dump's twin-gate `Stats`
  (writes a `Value` by key list, lane 1e), `SwitchState` (computed), and
  profiling's `JsNumber` / `JsObject` (lane 1d).

## Conclusions

Phase B removed about 800 product code lines and added about 345 (net
-455), with no change to any written tree on two pairs. The by-name lists
and test-only options had no reader left; the derived serializers give the
old bytes (pinned); each merged helper now has one owner and one test.
What is left in this area needs a decision rather than a deletion:
`is_babel_statement`'s three different kind lists, the provider trait's
two methods, and the plan's larger 4a rows (`fn_row_of_session`,
vector-map rebuild, JSON-AST walks), which belong with lanes 1e and 1g.
