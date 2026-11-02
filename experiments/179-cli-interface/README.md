# exp179 — the command-line interface: split on, prior folder, flags = keys, no .env

STATUS: BUILT + GATED (2026-11-02), branch `exp179-cli-interface` off main
`5a48564bd`, with main `0e5893f77` (exp175) merged in. NOT merged to main.
Gate 13/13 (on the merge, and from a clean checkout of the final commit).
Warm neutrality against main `0e5893f77` is NEUTRAL in names mode and in off
mode (0 files, 0 lines, baseline cache +0).

Andrew's interface decisions (2026-11-01), all four built:

1. **The split is on by default.** `split.enabled` defaults to `true`, so
   `humanify app.js -o out` writes the `src/` + `vendor/` + runnable
   scaffold tree. `--no-split` (or `split.enabled = false`) writes one file.
2. **`--prior-version` takes the prior output folder.** A directory resolves
   to `<dir>/.humanify/humanified.js` (the split's one constant,
   `place::layout::HUMANIFIED_SOURCE_PATH`). Every other prior-side file (the
   vendor manifest, `.humanify/vendor-humanified.js`, the split ledger) was
   already found next to that file, so nothing else changes. The file path
   still works. A folder without the file is refused before any work:
   `prior version (--prior-version) <dir> is a folder without
.humanify/humanified.js: pass a prior humanify output folder, or the
prior humanified file itself`. The run's `Next release:` line now prints
   the output folder, so the e2e stage's second leg uses the folder form.
3. **Flags and config keys are one-to-one**, both generated from the key
   registry (`crates/humanify-cli/src/config/keys.rs`). `crate::surface`
   builds the clap command from `KEYS`: there are no hand-written flags left
   except `--config`. A test (`surface_test::every_key_is_a_flag_and_every_flag_is_a_key`)
   proves the bijection: one flag per key, no shared spelling, the command
   accepts exactly the registry's flags, and each flag sets its own key and
   nothing else. The rule that a flag disagreeing with the `--config` file is
   refused, naming both, STAYS. It now covers every key, and a boolean names
   the form you typed (`(--no-split)`). Retired and unknown keys are still
   refused.
4. **No `.env` file.** `HUMANIFY_API_KEY` from the process environment is the
   only source of the key. The `dotenvy` dependency, its loader in `env.rs`
   and every `.env` mention in the help, messages and README are gone. A new
   integration test (`tests/api_key_source.rs`) proves that a `.env` file
   holding the key is refused as a missing key.

## Gate

| check                                                                                   | result                                                                                                                                        |
| --------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| typecheck, lint, rust:fmt, rust:clippy, knip, knip:prod                                 | PASS                                                                                                                                          |
| census:clones (advisory)                                                                | PASS                                                                                                                                          |
| census:concurrency                                                                      | PASS. The first run flagged `clap::Command::new` as a subprocess. It is now built with `Command::default().name(..)`, with no allowlist entry |
| unit, rust:unit (incl. the new bijection, prior-folder, split-default and `.env` tests) | PASS                                                                                                                                          |
| rust:build, rust:format-golden                                                          | PASS                                                                                                                                          |
| e2e (19 pairs; the second leg now passes the output FOLDER as `--prior-version`)        | PASS                                                                                                                                          |
| clean checkout of the final commit                                                      | ALL 13 STAGES PASSED                                                                                                                          |

## Neutrality (item 1: the flip changes no eval output)

Both runs were warm on 2.1.85→2.1.86 against main `0e5893f77`:
`npm run eval -- neutrality 0e5893f77 2.1.85:2.1.86 --priors /work/exp050-cold --cache /work/neutrality-cache`
(plus `--pipeline-set library.matching=off` for off mode).

| run                 | differing files | differing lines | exit codes | cache: candidate / baseline | verdict |
| ------------------- | --------------- | --------------- | ---------- | --------------------------- | ------- |
| names (the default) | 0               | 0               | 0 / 0      | +0 / **+0**                 | NEUTRAL |
| off                 | 0               | 0               | 0 / 0      | +0 / **+0**                 | NEUTRAL |

Both legs launched the same argv (`--config <toml> --endpoint ... --model
... -o ... --prior-version <file>`). The harness's files set
`split.enabled = true` explicitly, so the default flip cannot reach the
eval, and the result confirms it did not.

## What this does NOT prove

- Neutrality covers one pair (2.1.85→2.1.86). The launch goldens
  (`run-launch.test.ts`, unchanged and passing) are what show the other
  pairs launch the same configuration.
- The `--prior-version <folder>` form is exercised by the e2e stage and a
  unit test, not by the eval. The eval still passes the file path.
- Help readability is a judgment call: `-h` is 120 lines, `--help` 421.

## The naming scheme (item 3)

- **Derived (the rule):** key `<section>.<name>` is flag
  `--<section>-<name>`, with dots and underscores turned into dashes. When the
  name already starts with the section's last word, that word is not
  repeated: `naming.naming_floor` is `--naming-floor`,
  `diagnostics.diagnostics_file` is `--diagnostics-file`, `perf.perf_trace`
  is `--perf-trace`. Examples: `llm.concurrency` is `--llm-concurrency 64`,
  `library.lookup.targeted.min_nodes` is
  `--library-lookup-targeted-min-nodes`.
- **Overrides (short names for common settings, in the registry's `flag`
  field):** `--output-dir` (`-o`), `--endpoint`, `--model` (`-m`),
  `--prior-version`, `--bundler`, `--minifier`, `--split`, `--verbose`
  (`-v`, counted: `-vv` = 2), and `--library-index-store` for
  `library.index_store_dir` (its exp174 spelling, kept). That last one forces
  a second override: `library.index_store` (where index ids live) would also
  derive to `--library-index-store`, so it is `--library-index-id-store`.
  (These two keys look like one concept held twice. That cleanup belongs to
  the library lanes, not this one.)
- **Value forms:** a boolean is `--x` (on) and `--no-x` (off); giving both is
  refused. A list is comma-separated (`--switches-disable a,b`). Integers and
  numbers are typed at parse time (`--llm-concurrency 12abc` is refused,
  naming the flag). Ranges and choices are checked by the same resolver that
  checks the file, under the flag's name.
- **Renamed spellings** (the config sweep's "moved" flags, and spellings
  exp179 changed) are refused, naming the replacement
  (`surface::RENAMED_FLAGS`). For example, `-c` gives `renamed: use
--llm-concurrency`, and `--sequential` gives `renamed: use
--naming-schedule sequential`. Old spellings that are valid flags again
  (`--split`, `--split-pure`, `--split-ledger`, `--bundler`, `--minifier`,
  `--perf-trace`, `--naming-floor`, `--no-naming-floor`, ...) are simply
  accepted. Their RETIRED_FLAGS / MOVED_FLAGS entries were deleted.
- **Help:** `-h` prints one line per flag, grouped by section
  (`LLM [llm]`, `Split [split]`, ...), and leaves out the 45 library tuning
  flags (`library.lookup*`, `library.discovery*`). `--help` lists every flag
  with its whole doc, its default and its `[config: section.key]`. Both are
  pinned by goldens: `test/golden/help/humanify.txt` and the new
  `humanify-short.txt`.
- **`humanify.example.toml`** now shows each key's flag. Every key line is
  commented out at its default, because every key is now a flag and a line
  copied as-is would make that flag a conflict. A test checks that each
  line, uncommented, gives exactly the default.

## The harness keeps its exact launches (item 1)

| launcher                                                                                          | split before                                 | how it is kept                                                                                                                                           |
| ------------------------------------------------------------------------------------------------- | -------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 034 `run.sh` (rebase, scored leg, self-hop)                                                       | `split.enabled = true` in its `--config`     | unchanged: the launch goldens are byte-identical, `run-launch.test.ts` passes untouched                                                                  |
| `neutrality.sh` legs                                                                              | `split.enabled=true` in its `--config`       | unchanged                                                                                                                                                |
| `npm run eval` run configs                                                                        | carry run.sh's keys                          | unchanged                                                                                                                                                |
| `scripts/e2e.ts`                                                                                  | bundle fixtures `true`, others unset (= off) | now `["split.enabled", bundle]`: off said EXPLICITLY for the non-bundle fixtures                                                                         |
| `scripts/ask-trace.ts`                                                                            | unset (= off) unless `--set`                 | adds `split.enabled = false` unless `--set split.enabled=...`                                                                                            |
| crate integration tests (`pipeline_stages`, `replay_determinism`, `ask_trace`, `provider_errors`) | unset (= off)                                | each launch helper sets `split.enabled = false` unless the test chooses the split                                                                        |
| `launch_parity_test` (captured commander launches)                                                | no `--split` = off                           | the translation adds `--no-split` to a launch without `--split`; the captured fixture is untouched and every row still resolves to its captured settings |

## Before / after `humanify --help`

Before (main `5a48564bd`, 20 lines; `-h` printed the same thing): the input,
`--config`, `-o`, `--prior-version`, `--endpoint` (with "the environment or a
.env file"), `-m`, `-v`, and the four `--library-*` flags. Every other
setting could only be set in the config file.

After: `--help` is 421 lines (every flag, its doc, default and config key)
and `-h` is 120 lines (one line per flag, without the library tuning
sections). Both goldens are committed. An `-h` excerpt:

```
Output [io]:
  -o, --output-dir <dir>  Output directory

LLM [llm]:
      --endpoint <url>
          OpenAI-compatible API endpoint
  -m, --model <model>
          Model identifier
      --llm-concurrency <n>
          Max concurrent function-lane LLM requests (the in-flight cap is this plus module_concurrency)
...
Split [split]:
      --split                Split the output into a multi-file tree (src/ + vendor/ + run scaffold); off writes one...
      --split-pure           Emit the byte-exact review tree instead of the runnable module graph (needs enabled)
      --split-ledger <path>  A prior split ledger, for cross-release file-assignment inheritance (needs enabled)
```

## Every key and its flag (97)

| config key                                 | flag                                                                                       | name     |
| ------------------------------------------ | ------------------------------------------------------------------------------------------ | -------- |
| `run.order`                                | `--run-order`                                                                              | derived  |
| `io.output_dir`                            | `-o, --output-dir`                                                                         | override |
| `llm.endpoint`                             | `--endpoint`                                                                               | override |
| `llm.model`                                | `-m, --model`                                                                              | override |
| `llm.timeout_ms`                           | `--llm-timeout-ms`                                                                         | derived  |
| `llm.retries`                              | `--llm-retries`                                                                            | derived  |
| `llm.concurrency`                          | `--llm-concurrency`                                                                        | derived  |
| `llm.module_concurrency`                   | `--llm-module-concurrency`                                                                 | derived  |
| `llm.max_tokens`                           | `--llm-max-tokens`                                                                         | derived  |
| `llm.context_tokens`                       | `--llm-context-tokens`                                                                     | derived  |
| `llm.reasoning_effort`                     | `--llm-reasoning-effort`                                                                   | derived  |
| `llm.cache_dir`                            | `--llm-cache-dir`                                                                          | derived  |
| `matching.prior_version`                   | `--prior-version`                                                                          | override |
| `matching.reconcile_prior_diff`            | `--matching-reconcile-prior-diff / --no-matching-reconcile-prior-diff`                     | derived  |
| `naming.schedule`                          | `--naming-schedule`                                                                        | derived  |
| `naming.relaxed_levers`                    | `--naming-relaxed-levers`                                                                  | derived  |
| `naming.batch_size`                        | `--naming-batch-size`                                                                      | derived  |
| `naming.max_retries`                       | `--naming-max-retries`                                                                     | derived  |
| `naming.rename_retries`                    | `--naming-rename-retries`                                                                  | derived  |
| `naming.max_free_retries`                  | `--naming-max-free-retries`                                                                | derived  |
| `naming.lane_threshold`                    | `--naming-lane-threshold`                                                                  | derived  |
| `naming.code_window_lines`                 | `--naming-code-window-lines`                                                               | derived  |
| `naming.naming_floor`                      | `--naming-floor / --no-naming-floor`                                                       | derived  |
| `naming.naming_floor_sweep`                | `--naming-floor-sweep / --no-naming-floor-sweep`                                           | derived  |
| `library.matching`                         | `--library-matching`                                                                       | derived  |
| `library.index`                            | `--library-index`                                                                          | derived  |
| `library.index_store`                      | `--library-index-id-store`                                                                 | override |
| `library.index_store_dir`                  | `--library-index-store`                                                                    | override |
| `library.index_from_bundle`                | `--library-index-from-bundle / --no-library-index-from-bundle`                             | derived  |
| `library.index_build_max_packages`         | `--library-index-build-max-packages`                                                       | derived  |
| `library.unnamed`                          | `--library-unnamed`                                                                        | derived  |
| `library.report_units`                     | `--library-report-units / --no-library-report-units`                                       | derived  |
| `library.lookup.min_nodes`                 | `--library-lookup-min-nodes`                                                               | derived  |
| `library.lookup.min_content`               | `--library-lookup-min-content`                                                             | derived  |
| `library.lookup.min_function_content`      | `--library-lookup-min-function-content`                                                    | derived  |
| `library.lookup.cover`                     | `--library-lookup-cover`                                                                   | derived  |
| `library.lookup.max_packages`              | `--library-lookup-max-packages`                                                            | derived  |
| `library.lookup.support`                   | `--library-lookup-support`                                                                 | derived  |
| `library.lookup.strong`                    | `--library-lookup-strong`                                                                  | derived  |
| `library.lookup.specificity`               | `--library-lookup-specificity / --no-library-lookup-specificity`                           | derived  |
| `library.lookup.specificity_factor`        | `--library-lookup-specificity-factor`                                                      | derived  |
| `library.lookup.specificity_slack`         | `--library-lookup-specificity-slack`                                                       | derived  |
| `library.lookup.agreement`                 | `--library-lookup-agreement / --no-library-lookup-agreement`                               | derived  |
| `library.lookup.min_literals`              | `--library-lookup-min-literals`                                                            | derived  |
| `library.lookup.literal_score`             | `--library-lookup-literal-score`                                                           | derived  |
| `library.lookup.literal_cover`             | `--library-lookup-literal-cover`                                                           | derived  |
| `library.lookup.literal_margin`            | `--library-lookup-literal-margin`                                                          | derived  |
| `library.lookup.literal_margin_by_family`  | `--library-lookup-literal-margin-by-family / --no-library-lookup-literal-margin-by-family` | derived  |
| `library.lookup.max_packages_by_family`    | `--library-lookup-max-packages-by-family / --no-library-lookup-max-packages-by-family`     | derived  |
| `library.lookup.terser_slices`             | `--library-lookup-terser-slices`                                                           | derived  |
| `library.lookup.terser_share`              | `--library-lookup-terser-share`                                                            | derived  |
| `library.lookup.call_context`              | `--library-lookup-call-context`                                                            | derived  |
| `library.lookup.call_context_callees`      | `--library-lookup-call-context-callees`                                                    | derived  |
| `library.lookup.targeted.min_nodes`        | `--library-lookup-targeted-min-nodes`                                                      | derived  |
| `library.lookup.targeted.support`          | `--library-lookup-targeted-support`                                                        | derived  |
| `library.discovery.closure_floor`          | `--library-discovery-closure-floor`                                                        | derived  |
| `library.discovery.queue_cap_bytes`        | `--library-discovery-queue-cap-bytes`                                                      | derived  |
| `library.discovery.max_rounds`             | `--library-discovery-max-rounds`                                                           | derived  |
| `library.discovery.veto_support`           | `--library-discovery-veto-support`                                                         | derived  |
| `library.discovery.evidence_conflict`      | `--library-discovery-evidence-conflict / --no-library-discovery-evidence-conflict`         | derived  |
| `library.discovery.literal_conflict`       | `--library-discovery-literal-conflict / --no-library-discovery-literal-conflict`           | derived  |
| `library.discovery.contested_cover`        | `--library-discovery-contested-cover`                                                      | derived  |
| `library.discovery.banner_names`           | `--library-discovery-banner-names`                                                         | derived  |
| `library.discovery.helper_families`        | `--library-discovery-helper-families`                                                      | derived  |
| `library.discovery.helper_prints`          | `--library-discovery-helper-prints`                                                        | derived  |
| `library.discovery.payload_min_bytes`      | `--library-discovery-payload-min-bytes`                                                    | derived  |
| `library.discovery.specific_rivals`        | `--library-discovery-specific-rivals / --no-library-discovery-specific-rivals`             | derived  |
| `library.discovery.derived_families`       | `--library-discovery-derived-families / --no-library-discovery-derived-families`           | derived  |
| `library.discovery.context_presence`       | `--library-discovery-context-presence`                                                     | derived  |
| `library.discovery.context_presence_slack` | `--library-discovery-context-presence-slack`                                               | derived  |
| `library.discovery.shadow_ties`            | `--library-discovery-shadow-ties / --no-library-discovery-shadow-ties`                     | derived  |
| `library.discovery.presence_fence`         | `--library-discovery-presence-fence / --no-library-discovery-presence-fence`               | derived  |
| `library.discovery.declared_max_bytes`     | `--library-discovery-declared-max-bytes`                                                   | derived  |
| `library.discovery.copy_presence`          | `--library-discovery-copy-presence`                                                        | derived  |
| `library.discovery.copy_presence_factor`   | `--library-discovery-copy-presence-factor`                                                 | derived  |
| `library.discovery.copy_presence_origin`   | `--library-discovery-copy-presence-origin / --no-library-discovery-copy-presence-origin`   | derived  |
| `library.discovery.copy_presence_own`      | `--library-discovery-copy-presence-own`                                                    | derived  |
| `library.discovery.factory_bodies`         | `--library-discovery-factory-bodies / --no-library-discovery-factory-bodies`               | derived  |
| `library.discovery.factory_tie_fence`      | `--library-discovery-factory-tie-fence / --no-library-discovery-factory-tie-fence`         | derived  |
| `library.discovery.factory_whole_seed`     | `--library-discovery-factory-whole-seed / --no-library-discovery-factory-whole-seed`       | derived  |
| `library.discovery.factory_banner_index`   | `--library-discovery-factory-banner-index / --no-library-discovery-factory-banner-index`   | derived  |
| `unpack.bundler`                           | `--bundler`                                                                                | override |
| `unpack.minifier`                          | `--minifier`                                                                               | override |
| `split.enabled`                            | `--split / --no-split`                                                                     | override |
| `split.pure`                               | `--split-pure / --no-split-pure`                                                           | derived  |
| `split.ledger`                             | `--split-ledger`                                                                           | derived  |
| `switches.disable`                         | `--switches-disable`                                                                       | derived  |
| `switches.probe`                           | `--switches-probe`                                                                         | derived  |
| `diagnostics.verbosity`                    | `-v, --verbose`                                                                            | override |
| `diagnostics.log_file`                     | `--diagnostics-log-file`                                                                   | derived  |
| `diagnostics.diagnostics_file`             | `--diagnostics-file`                                                                       | derived  |
| `diagnostics.stats_json`                   | `--diagnostics-stats-json`                                                                 | derived  |
| `diagnostics.dump_artifacts`               | `--diagnostics-dump-artifacts`                                                             | derived  |
| `diagnostics.dump_asks`                    | `--diagnostics-dump-asks`                                                                  | derived  |
| `diagnostics.rename_ledger`                | `--diagnostics-rename-ledger`                                                              | derived  |
| `perf.perf_trace`                          | `--perf-trace`                                                                             | derived  |
| `perf.simulate_llm_latency`                | `--perf-simulate-llm-latency`                                                              | derived  |

97 keys

`override` = a spelling written in the registry; `derived` = the
`--<section>-<name>` rule. A boolean's `--no-` form is listed only where
the key is a boolean. The registry also accepts it for every other boolean,
for example `--no-naming-floor` and `--no-library-lookup-agreement`.

## Overlap with exp175 (`exp175-index-by-bundle-kind`) — RESOLVED

exp175 was merged to main (`0e5893f77`) while this lane ran. It is merged
into this branch: conflicts in `surface.rs` (took this branch's generated
surface; exp175's help wording lives in the registry docs it had already
updated), `README.md` (exp175's library wording moved into this README's
library paragraph), `scripts/e2e.ts` (exp175's `bundler` parameter plus
this branch's explicit `split.enabled`), `pipeline_stages.rs` (a doc
comment). The help goldens and `humanify.example.toml` were regenerated.
The notes below were written before the merge.

exp175 edits the library key docs in `keys.rs`, the four `--library-*`
help strings in `surface.rs`, `config.rs`, the README library paragraph, and
`test/golden/help/humanify.txt`. My library-section changes are mechanical:
the `library.index_store` entry gets the `--library-index-id-store` flag
(the unset_text call became a Key literal), and nothing else in the library
keys changed. Merge notes:

- `surface.rs`: the hand-written `CommandOptions` fields are gone, so
  exp175's help-string edits have nowhere to land. Take this branch's
  `surface.rs`. The help text now comes from the registry docs, which
  exp175 already updates in `keys.rs`.
- `keys.rs`: the `doc:` lines are untouched here, so exp175's doc edits
  should apply cleanly.
- `README.md`: the library paragraph moved into its own paragraph under
  Usage. Re-apply exp175's wording there.
- Help goldens and `humanify.example.toml`: regenerate after the merge
  (`humanify --help`, `humanify -h`,
  `cargo test -p humanify-cli write_the_example_config -- --ignored`).

## Proposed CLAUDE.md text (not edited, per the lane rules)

Nothing in CLAUDE.md names the flag set or `.env`. Optional: under "Code
style", "**Flags = config keys**: every key in
`crates/humanify-cli/src/config/keys.rs` is also a flag, generated from the
same table (`--<section>-<name>`, short overrides in the `flag` field); add a
setting by adding a key, never a hand-written flag. `surface_test.rs` proves
the bijection."
