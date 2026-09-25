# Parity fixtures

Committed TS-side artifact dumps (the 07 §2 catalog as implemented) cut at
the oracle commit from the e2e fixture corpus, one per fixture — the
`rust:parity` check stage's corpus (scripts/rust-parity.ts compares a
`ts/` side against a `rust/` side when one exists; today only `ts/`).

Cut 2026-09-19 at oracle-0294b28 from the fixture pairs' to-version runs
(prior = the from-version's humanified output). The five fixtures whose
full-pipeline run exited 0: mitt, nanoid, preact, r1b-synthetic,
disambiguation. zustand's dump is from a run whose rename-invariant
FAILED (a pre-existing export-const bug class, recorded in the WP0.4
hand-back) — its dump is still a valid decision record for the hash and
matching sections, which is what phases 1-2 compare.

The cache-key vectors (R4) live beside this: cache-key-vectors.jsonl +
its generator, and babel-counts.mjs (the WP1.2 counts table's Babel side).

## WP3.1 (validated rename) probes — 2026-09-24

Each probe runs the REAL TS function on a fixture set and freezes its
verdicts; the Rust test named beside it replays the same inputs.

| probe                                 | frozen at              | pins                                                                                                                                            | Rust test                                                                      |
| ------------------------------------- | ---------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| `wp31-scope-probe.mjs` (+ snippets)   | `wp31-scope-view.json` | Babel's scope model: scopes, binding maps in `Object.keys` order, kinds, owners, reference / violation paths with `path.scope`, globals         | `scope_view_matches_the_babel_probe`                                           |
| `wp31-scope-bundle-probe.mjs`         | (bundle scale, /tmp)   | the same rows for a whole oracle text; compare with `humanify scope-view <text> <out>` byte for byte                                            | gate log `/work/rust-port/gates/wp3.1/`                                        |
| `wp31-name-probe.mjs`                 | `wp31-names.json`      | RESERVED_WORDS, GLOBAL_BUILTINS, isValidIdentifier, isValidRenameTarget, isBunToken, isDecoratedDescriptive, isBelowFloorName, createIsEligible | `target_sets_match_the_ts_exactly`, `name_predicates_match_the_ts_truth_table` |
| `wp31-rename-probe.mjs` (+ scenarios) | `wp31-rename.json`     | every validated-rename.test.ts / scope-era.test.ts case + predicate probes: each verdict and every binding's final name                         | `rename_scenarios_match_the_ts_probe`                                          |
| `wp31-soundness-probe.mjs`            | `wp31-soundness.json`  | `isBindingEvalTaintFrozen` for every binding                                                                                                    | `eval_taint_freeze_matches_the_ts_probe`                                       |
| `wp31-ledger-probe.mjs`               | `wp31-ledger.json`     | rename-ledger.test.ts cases: the TS ledger and Babel's generated output                                                                         | `ledgers_match_the_ts_probe_and_replay_to_its_output`                          |
| `wp31-catch-var-capture-repro.mjs`    | —                      | a REAL TS capture (catch param renamed to a `var` in its own body is applied; runtime 5 → undefined)                                            | `the_catch_var_capture_is_reproduced_not_fixed`                                |

Regenerate with `npx tsx test/parity/<probe> > test/parity/<frozen>` (the
scope probe runs under plain `node`).

## Library-freeze carry (#32) — 2026-09-25

`library-carry-probe.ts` runs the REAL TS beautify with the function carry
armed over synthetic snippets plus the gate regimes' raw texts
(`library-carry-inputs.json`, copied from /work/lf/cases) and freezes the regions,
the carry, the walk over the re-parsed text, the resolved library functions
and the raw-tree walk into `library-carry.json`; the Rust tests are
`libdetect::function_carry::function_carry_test`. Regenerate with
`npx tsx test/parity/library-carry-probe.ts > test/parity/library-carry.json`.
