# exp215 — an unchanged module keeps its prior file when upstream splits a module

STATUS: BUILT, STUB-MEASURED, GATED (all 11 stages, twice), EVALUATED (one
cold names-mode run, `exp215-5f09fad2-r1`, 2026-11-11), NOT MERGED.
Branch `exp215-split-keep-file` from main `73d01430b`. The eval ran alone on
the host after the walks finished. The 2.1.90 -> 2.1.91 numbers are from
the stub LLM (`cargo dev run stub`); that hop is not one of the eval's pairs.

## Totals

### Eval (cold, names mode, scratch bases, five pairs, index 4736dcfa)

`cargo dev eval score exp215-5f09fad2-r1` against the reference
`ref-names-40921b9f-r1`. The results live on this host only (they quote
bundle code): `dev/eval/results/exp215-5f09fad2-r1/` in the worktree,
trees in `/work/eval-work/exp215-5f09fad2-r1/`.

- **Run validity:** all 5 rebases and 5 scored legs exit 0, no provider
  errors, no rebase failures, all 5 boots OK. Cold self-hop 8 lines (the
  reference range is 2-8). Warm self-hop byte-identical with 0 cache writes.
- **MATCHING:** every exact column is EQUAL to the reference: fnTotal
  254,797, fnClose 3,595, fnAsked 5,928, fnUnmatched 4,749, fnAmbiguous
  4,756. fnCarried 220,762 and mbCarried 95,920 are equal too. Only the
  LLM's draws moved (idAsked -29, mbLLMRenamed +1). The change acts after
  naming, so this is what it should look like.
- **Real change is exact:** novel 5,336 (=), realLn 556,875 (=). app:novel,
  app:realLn, vendor:novel, vendor:realLn, cross:treeLn and cross:stmts are
  all equal too. cleanReal is inside its band (-330, band +-848).
- **The step fired on ONE pair, once:** 2.1.85 -> 2.1.86, one module of 6
  statements (an OAuth port finder: `buildCallbackUrl`,
  `findAvailableOAuthPort`, ...). It kept `src/oauth-client.js`. In the
  reference the same module moved to a new 74-line file
  (`src/oauth-flow-manager/oauth-port-finder.js`). It did not fire on the
  other four pairs (0 trail rows, 0 shared files in their ledgers).
- **Noise and relocation, per surface:**

| column          | ref r1 | exp215 | change  | band  |
| --------------- | -----: | -----: | ------: | ----: |
| treeLn          | 175,268 | 175,054 | -214 | +-1,183 |
| noise           | 3,034  | 3,033  | -1      | +-20  |
| noiseLn         | 68,179 | 67,124 | -1,055  | +-350 |
| reloc           | 1,653  | 1,627  | -26     | +-83  |
| relocSt         | 436    | 428    | -8      | none  |
| newName         | 4,029  | 4,051  | +22     | +-42  |
| app:noiseLn     | 67,670 | 66,621 | -1,049  |       |
| app:relocSt     | 382    | 378    | -4      |       |
| vendor:treeLn   | 24,169 | 24,044 | -125    |       |
| vendor:vendorLn | 10,027 | 9,923  | -104    | +-84  |
| vendor:reloc    | 113    | 101    | -12     |       |
| cross:reloc     | 58     | 60     | +2      | +-7   |

Per pair (ref / exp215):

| pair            | noiseLn         | treeLn          | relocSt | files added | files removed |
| --------------- | --------------- | --------------- | ------- | ----------- | ------------- |
| 2.1.85->2.1.86  | 33,070 / 33,136 | 32,424 / 32,281 | 181 / 173 | 29 / 21 | 17 / 10 |
| 2.1.118->2.1.119 | 4,373 / 4,464  | 36,759 / 36,726 | 5 / 5   | 93 / 93     | 22 / 22       |
| 2.1.179->2.1.181 | 17,874 / 17,604 | 30,875 / 30,861 | 75 / 75 | 46 / 48    | 22 / 24       |
| 2.1.197->2.1.198 | 7,949 / 7,028  | 49,589 / 49,699 | 66 / 66 | 51 / 52     | 36 / 37       |
| 2.1.215->2.1.216 | 4,913 / 4,892  | 25,621 / 25,487 | 109 / 109 | 32 / 32   | 2 / 2         |

How to read it:

- **noiseLn -1,055 (outside the +-350 band) is not this change.** noiseLn
  scores the BUNDLE, and a split-only change cannot move it (the
  leaderboard's own note). 921 of the 1,055 lines come from 2.1.197 ->
  2.1.198, where the step never fired. It is the model's draws (one cold
  run against one cold reference; the band was measured at 40921b9f, not
  here). vendor:vendorLn -104 (band +-84) is the same: no vendor module
  was kept.
- **The one pair where the step fired reads the right way:** on 2.1.85 ->
  2.1.86, relocSt 181 -> 173, files added 29 -> 21 and files removed
  17 -> 10. Part of that is the kept module, part is the cold bases
  differing. One run cannot separate the two.
- The kept file shows a small cost: its `http` binding prints as
  `httpModule` (3 lines) now that the moved module's `require("http")`
  sits in the same file. That is the per-file library-import naming
  (`finish::library_names`) choosing a name in a file with more code.
- **LLM diagnostics:** 20 connection retries (0 on the reference), all
  recovered, 0 failed asks. Server-side; no answers lost.

### Stub, 2.1.90 -> 2.1.91

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

`cargo dev check`, run twice, never at the same time: in the worktree
(`/work/exp215-split-keep-file`, before the commit), then in a fresh clone of
the branch at `117291e46` (`/work/exp215-clean`).

| stage                | worktree | clean clone |
| -------------------- | -------- | ----------- |
| shims                | PASS     | PASS        |
| no-private-addresses | PASS     | PASS        |
| rust:fmt             | PASS     | PASS        |
| rust:clippy          | PASS     | PASS        |
| census:clones        | REVIEW   | REVIEW      |
| census:concurrency   | PASS     | PASS        |
| census:slop          | PASS     | REVIEW      |
| rust:unit            | PASS     | PASS        |
| rust:build           | PASS     | PASS        |
| format-golden        | PASS     | PASS        |
| e2e                  | PASS (19 pairs, 152 launches equal to the golden) | PASS (same) |

- census:clones (advisory): two twins, both in `humanify-ag-dev` and both
  already on main (`js_of`, the library card `read`). None is in this
  change.
- census:slop (advisory) in the clean clone: the clone has no `main`
  branch, so the merge-base lookup failed and no measure was taken. In the
  worktree it passed.
- e2e goldens did not change: no e2e fixture splits a module across a hop.

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

- One cold run. The step fired on one of the five eval pairs, so the eval
  cannot size the effect: its move is inside the pair-to-pair draw. The
  sized effect is the stub hop (-111 GNU diff lines on 90 -> 91) and the
  one kept module on 85 -> 86 (the reference moved it to a 74-line new
  file).
- How often the step fires across a whole walk is unknown: once in about
  3,300 modules on 85 -> 86 and on 90 -> 91, never on the other four eval
  pairs.
- The rule can also keep code in an old file when upstream really moved it
  into a new source file. A minified bundle does not show upstream file
  names, so the split cannot tell the two apart. Keeping the file is the
  smaller diff.
