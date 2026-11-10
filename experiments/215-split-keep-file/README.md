# exp215 — an unchanged module keeps its prior file when upstream splits a module

STATUS: BUILT, STUB-MEASURED, GATED, NOT EVALUATED, NOT MERGED (2026-11-10).
Branch `exp215-split-keep-file` from main `73d01430b`. No live-LLM run:
two version walks held the LLM server, so `eval score` waits until they
finish. Every number below is from the stub LLM (`cargo dev run stub`).

## Totals

On Claude Code 2.1.90 -> 2.1.91, stub LLM, same 2.1.90 prior tree for both
legs:

| measure                                         | main 73d01430b | exp215 |  change |
| ----------------------------------------------- | -------------: | -----: | ------: |
| GNU diff lines, whole tree (`eval diff` ground truth) |      7,142 |  7,031 |    -111 |
| REAL column (`eval diff`)                       |          5,655 |  5,546 |    -109 |
| of it, one-sided add/remove                     |          1,920 |  1,815 |    -105 |
| of it, edited                                   |          3,481 |  3,477 |      -4 |
| classified naming noise                         |          1,580 |  1,568 |     -12 |
| `diff -r` lines, whole tree                     |         20,682 | 20,565 |    -117 |
| files in the tree                               |          3,332 |  3,331 |      -1 |
| LineDecoder's file: diff lines this hop         | 120 + 111 (new `-2.js`) | 22 | -209 |
| fossil modules kept by content (new step)       |              — |      1 |         |

- The 111-line `…-2.js` file is gone. LineDecoder, its helpers and its init
  stay in the file they were in. The 22 lines left in that file are the real
  change: the SDK version string, `stringifyQuery`, and the new init.
- Three importers no longer change: their `require` of the `-2.js` file and
  every `-2` alias are gone (13, 5 and 1 lines).
- The step fired once on this hop (one file now holds two modules). Every
  other file in the tree is byte-identical between the two legs.
- Both trees run: `node index.js --version` prints `2.1.91 (Claude Code)`,
  and LineDecoder loads and decodes from its file in each.

## The problem

The marker split (`emit::stable_split`, regime Fossil) reads each `__esm`
init as the end of one module: every statement after the previous init is
that init's module. In 2.1.90, the Anthropic SDK's runtime helpers had no
init of their own, so they were glued to LineDecoder's module. Both were one
module and one file.

In 2.1.91 the SDK (0.74 -> 0.80) gave that group its own init (it gained
`stringifyQuery`). The bundle now has two modules where the prior had one.
The matcher (`place::assign::fossil_match`) pairs modules one to one. It
gave the prior file to one half. The other half, LineDecoder, which had not
changed, was named fresh and moved to `runtime-utils-2.js` (in the walk; in
the stub run, `lagoon-orchid-orchid-fjord-2.js`). That cost about 100 lines
of move noise plus the importers' `require` changes.

## The fix

The split already had a carry-by-content reading: the placement trail's
`priorFile` (statement hash -> the ONE prior module file holding it). Only
the trail read it. It did not cover this case because the module matcher is
one to one, and nothing placed a module by its statements.

Now, after the matcher's pairs inherit their files and before fresh naming:

- `prior_hash_files` (moved out of `record_fossil_trail`, which now calls
  it) builds the hash -> prior-file map.
- `kept_prior_file`: an UNMATCHED module keeps a prior file when every
  carried statement it has (a hash the prior holds in exactly one file)
  names that same file, and those statements are more than half the
  module. New statements and ambiguous ones do not vote.
- The trail records such a module as `fossil:kept-by-content`. The split's
  summary counts it (`N kept by content`). `FossilStats.kept_files` holds
  the count.
- Two modules may now share one file. The ledger records both at that file,
  so the next hop matches each to itself (tested).

The owner is the existing one, `place::assign::fossil` (row added to
`docs/responsibility.md`, next to the matcher's row).

Files:

- `crates/humanify-ag-core/src/place/assign/fossil.rs`: the step, the shared
  map, the stats field.
- `crates/humanify-ag-core/src/place/placement_dump.rs`: the summary line.
- `crates/humanify-ag-core/src/place/assign/fossil/fossil_test.rs`: four
  tests on synthetic code.
- `crates/humanify-ag-core/src/emit/stable_split/stable_split_test.rs`: one
  end-to-end test through the runnable emit.

## Tests (red, then green)

| test                                                                                      | before the fix   | after |
| ----------------------------------------------------------------------------------------- | ---------------- | ----- |
| `a_module_split_out_of_a_prior_module_keeps_the_prior_file`                                | FAIL (`src/group-read.js`) | pass |
| `an_unchanged_module_split_out_of_its_prior_module_keeps_the_file_in_the_runnable_tree`    | FAIL (`src/group.js` vs `src/line.js`) | pass |
| `the_kept_file_holds_on_the_next_hop`                                                      | pass (guard)     | pass  |
| `a_module_whose_carried_statements_came_from_two_prior_files_is_named_fresh`               | pass (guard)     | pass  |
| `a_mostly_new_module_is_named_fresh`                                                       | pass (guard)     | pass  |

The fixture runs in the other direction from the real case: in the fixture
the matcher gives the prior file to the line module, and the group (with its
new function and init) would move. The rule covers both directions. The real
case (the larger group matched, LineDecoder unmatched) is the 2.1.91 stub
run above.

## Gate

| run                                              | result |
| ------------------------------------------------ | ------ |
| `cargo dev check`, worktree `/work/exp215-split-keep-file` | see below |
| `cargo dev check`, clean checkout of the branch  | see below |

## Not fixed, noted

- **The export key differs from the local name.** In the walk, 2.1.91's
  `runtime-utils.js` writes
  `Object.defineProperty(module.exports, "initRuntimeEnvironment", { get: () => initRuntimeUtils })`.
  The runnable emit writes the BUNDLE name as the export key
  (`emit::cjs` `accessor_line`). The post-split reconcile
  (`finish::reconcile`) then renames the file's local binding back to the
  prior file's name, and rewrites identifiers only, never the key string.
  So the key keeps this run's LLM name and the local keeps the prior's.
  The same happens with `doNothing14` (key) -> `emptyFunctionVal` (local):
  `doNothing14` is the model's numbered junk name for an empty arrow, and
  reconcile restored only the local. A fix would let reconcile rewrite the
  export key of a binding it renames, and every importer's member access
  with it. That touches the module's API between files, so it needs its own
  test and a neutrality run.
- This fix makes reconcile see more of these files against their prior
  file, so it may restore more locals. Not measured under a live LLM.

## What this cannot prove

- The stub names every identifier the same mechanical way. It shows where
  code is placed, not what the model would name it. The eval (`eval score`
  against `ref-names-40921b9f-r1`) is still to run, after the walks.
- One hop was measured. How often the step fires across a walk is unknown;
  on 90 -> 91 it fired once in 3,331 modules.
- The rule can also keep code in an old file when upstream really moved it
  into a new source file. A minified bundle does not show upstream file
  names, so the split cannot tell the two apart. Keeping the file is the
  smaller diff.
