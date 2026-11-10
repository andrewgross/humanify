# exp213 — keep an index unit row only if its source file confirms it

STATUS: BUILT, SCORED, GATED, NOT ADOPTED, NOT MERGED (2026-11-10). Branch
`exp213-index-source-confirm` from main `622279472`. The new index is
`/work/library-index-exp213/77b12699e27500f1da7cef8aef19558704189a8086ebc8fec603854296e074f2`
(a replay of 4736dcfa's plan with the rule on). `pairs.json`'s
`libraryIndex` is untouched: adopting the index is Andrew's call. No eval
was run.

## Totals

- **App bytes claimed stay 0** on every slice of bun-dev and bun-held
  (and on cc, see §3).
- **No corpus slice got worse on precision.** Two slices rose, and two are
  byte-for-byte equal on precision and wrong bytes:
  - bun-held node: 0.997162 -> **0.999631**. It passes the 0.998 bar now.
    Wrong bytes fell from 66,396 to 8,604.
  - bun-dev node: 0.997439 -> **0.997931**. Wrong bytes fell from 241,822
    to 194,671. It still misses the bar, by 0.00007.
  - bun-dev bun and bun-held bun: precision and wrong bytes unchanged.
- **The target is gone.** readable-stream -> string_decoder (-40,177 bytes
  on bun-dev node, -14,262 on bun-held node) and readable-stream ->
  safe-buffer (-5,367 / -1,519) are no longer in the breakdown. So is
  bun-held's iconv-lite -> node-fetch (-42,189): the index rows behind that
  claim were iconv-lite's own code misfiled by Bun's map.
- **Recall cost:** bun-dev node 0.8594 -> 0.8570 (256,617 fewer named
  bytes). bun-held node 0.8315 -> 0.8299. The bun slices lose 0.0000-0.0001.
- **One new wrong pair grew:** encoding-sniffer -> undici (+10,336 bytes on
  bun-dev node, +2,061 on bun-held node), plus whatwg-encoding -> undici
  (+3,269 / +654). The cause: refused undici rows that were right about the
  PACKAGE but named the wrong FILE (§4).
- **The rule removed 189,177 of 312.9M shard rows (0.06%):**
  - bun slice: 21,205 of 151.2M (0.014%).
  - node slice: 167,972 of 161.8M (0.10%).
  - After the merge: unit rows 6,887,913 -> 6,882,993 (bun) and
    6,509,243 -> 6,501,083 (node).
  - Module rows, literals and families are identical. The rule does not
    touch module rows.
- **Unconfirmable rows are kept:** 20,033,100 rows (6.4%) have fewer than 3
  checkable tokens. 0 rows had an unreadable source file.
- **Build drift is not the rule, and it is down to 8 rows.** 202 of the 206
  tasks that first drifted were rebuilt from their exact lock. The other 4
  took the fresh install path. Two of those 4 swap 4 rows each (§2c).
- **Build cost:**
  - Shard phase: 4.22 h, 24 workers, 0 failures, peak chunk RSS 78.6 GB.
  - Drift-task rebuild: about 50 min.
  - Merge: 1.34 h, peak 94.7 GB.
  - Disk: shards 276 GB, index 43 GB.
  - Network: none beyond npm's cache.

## What changed in the code

- `crates/humanify-ag-index/src/confirm.rs` (NEW, tests in
  `confirm_test.rs`) holds the rule.
  - A unit's checkable tokens are its `S:` string / template texts and
    `P:` static property names, taken from the unit print's own content
    tokens. Each token counts by its words of 3 or more identifier
    characters.
  - Build-host paths a bundler inlined for `__dirname` / `__filename` are
    not checked.
  - A token is found when all its words are in the words of one of the
    unit's sources. The sources are the files its map segments name inside
    its owner package.
  - Source words also include the tail behind an escape (`'\0SLASH'`,
    `\x00`, `\u0000`, `\n`), because the token is the cooked string.
  - Fewer than 3 checkable tokens: the row is kept and counted `noTokens`.
  - An unreadable file: the row is kept and counted `unreadable`.
  - Otherwise the row is refused when fewer than half of its tokens are
    found.
  - The check reads the map's own sources, so the same rule applies to
    every bundler. It only removes rows.
- `crates/humanify-ag-core/src/libindex/units.rs`: `BundleUnits.tokens`
  keeps each unit's content-token set. The builder path already computed
  it and threw it away. No print changed.
- `crates/humanify-ag-index/src/build.rs`:
  - `unit_rows` applies the rule after the sponge evidence.
  - `UNIT_ROWS_VERSION` goes `unit-rows/5` -> `unit-rows/6`, which is in
    every module shard's key.
  - No index FORM changed. `units/4`, `vendor-content-key/5`, `literals/1`,
    `edges/2` and `names/1` are the same, because the prints are the same
    and only the choice of rows changed. So no identity-transform version
    bump was needed.
- Shard notes `sourceConfirm` record `confirmed` / `refused` / `noTokens`
  / `unreadable` row counts per shard, plus `refusedFiles` (refused rows by
  the `package@version file` the map named). These counts are taken before
  the existing multi-module drop.
  - Not done: `build.json` does not total these counts. The per-shard
    counts are in the store; the totals above come from a scan of all
    131,678 shards (`/work/exp213-data/full/drift-summary.json`).
- **Install: an exact restore from a kept lock** (`restorable`, `restore`,
  `lock_complete`).
  - The case: a work dir kept `package-lock.json` and `build/` but lost its
    tree. That is exp152's dirs after the 2026-11-10 cleanup.
  - Step 1: `npm ci --before=<publish time>`. Without `--before`, npm
    checks the lock's peer ranges against today's registry and refuses.
  - Step 2: if ci refuses a lock that lacks an optional platform binary
    published later (EUSAGE), run an `npm install --before` guided by the
    lock. It is accepted only if the lock comes back byte-identical.
  - Either way the tree must hold every non-optional package in the lock.
    If not, the ordinary fresh install runs.
  - Each task prints `install <pkg>@<v>: restored|fresh (…)`.
  - There is no `--legacy-peer-deps` ci: it dropped the lock's peers.
  - A unit test covers the tree check. The npm steps themselves cannot be
    unit-tested without npm; they were proved on the real build (§2c).
- Fixture: `runtime-build-esbuild`'s ms@2.1.3 module shard was re-recorded
  as its README says. Its rows are identical; only the key and notes moved.
- Docs: `docs/design/library-index.md` §2 (the install step and the rule),
  and a `docs/responsibility.md` row for `humanify_ag_index::confirm`.
- Census: `npm_ci` was added to `dev/census/concurrency-allowlist.json`.

## 1. The sample, before the full build (435 versions)

These are the 35 contaminated readable-stream dependents (exp210) plus 400
random versions (seed 213). The sample excluded shards over 200 MB. One
process, `--jobs 4`: 603-617 s, peak RSS 5.9-6.7 GB.

Each refused unit was judged by searching EVERY installed file of its tree
for the file that best confirms its tokens:

| set | rows | refused | true home in another package | right package, wrong file | no home found (false refusal) |
| --- | --- | --- | --- | --- | --- |
| 35 contaminated, node | 356,609 | 9,968 | 7,911 | 2,059 | 0 |
| 35 contaminated, bun | 367,201 | 2 | | | |
| 400 random, both slices | 1,790,000 | 554 (0.031%) | 406 | 108 | 40 |

Two false-refusal causes found in the sample were fixed before the build,
tests first:
- escaped strings (brace-expansion's `'\0SLASH'`, echarts' `"\0__…"`);
- inlined `__dirname` build paths (import-fresh, terser-webpack-plugin).

**False refusals still in the rule** (from the sample, after the fixes):
about 40 rows in 400 random versions.
- Bun bun-target whole-bundle wrapper units
  (`(function(exports, require, module, __filename, __dirname) {…`) whose
  tokens are mostly Bun's interop-helper property names: d3-polygon (19
  rows), clear-cut (13). @cucumber/messages and html-parse-stringify (34
  rows) fall in the "wrong file" column.
- sshpk `lib/algs.js` (4): a string whose case the source builds at run
  time.
- @exodus/bytes `fallback/encoding.labels.js` (4): template strings the
  source assembles from parts.

Not fixed. A bundler-helper vocabulary would have to come from detection,
not a list. These are listed here as the coordinator asked.

A spot check of 9 versions from the largest refused families (mongodb,
tedious, mysql2, encoding-japanese, undici, node-taglib-sharp, css-tree,
lru-cache, libmime) found 0 refusals with no home. Of 2,055 refused
statement units there, 725 had their true home in another package and
1,330 were the right package in the wrong file.

## 2. The full build

### 2a. How

`humanify-ag index build --replay /work/library-index/4736dcfa… --sponge-regions origin`:
- 120 chunks x `--jobs 4`, 6 at a time.
- Registry: a hard-linked copy of exp204's (`--offline`).
- Store: exp204's literal shards hard-linked, so all 65,839 literal shards
  were reused and 131,678 module shards were rebuilt.
- Work: shells of exp152's trees. Each shell has `build/` and `entries/`
  hard-linked and the lock copied; exp152's originals were never written.
- A memory guard ran (pause under 20 GB free). It never paused.
- Scripts: `/work/exp213-data/scripts/` (`setup-full.sh`,
  `run-chunks.sh`, `run-merge.sh`, `memguard.sh`, `drift.py`).

| phase | wall | peak RSS | notes |
| --- | --- | --- | --- |
| shard phase | 15,202 s (4.22 h) | 78.6 GB one chunk; build tree 77 GB | 120/120 chunks exit 0, 0 FAILED |
| drift rebuild (206 tasks) | about 50 min (a single run, then 6 chunks) | 82.4 GB (one chunk) | §2c |
| merge (`--merge-only --merge-jobs 24`) | 4,839 s (1.34 h) | 94.7 GB | exit 0 |

### 2b. Rows refused, by package family (top 20 of 701 families, rows)

css-tree 37,784; mongodb 11,152; csso 9,226; encoding-japanese 7,960;
tedious 7,800; undici 6,686; node-taglib-sharp 6,410; mysql2 5,952;
readable-stream 5,766; jsdom 4,832; http-cache-semantics 3,996;
minipass-fetch 3,884; iconv-lite 3,688; micromark 3,530; lru-cache 2,968;
@npmcli/redact 2,912; libmime 2,907; cssstyle 2,818; negotiator 2,436;
bl 1,946.

There are 4,061 shards with at least one refusal: 1,432 bun and 2,629
node.

### 2c. Build drift, kept apart from the rule

Every new shard was compared with 4736's shard, row by row. For a shard
the rule alone touched, the rows lost must be <= the rows refused (the
refused count is taken before the multi-module drop, so lost can be less),
and no row may be gained.

- First full run: 204 tasks (about 740 shards) broke that test. They lost
  80.6k rows and gained 13.9k; only 6 of those rows were refused by the
  rule.
- The cause was the first version of the lock restore:
  - `npm ci` without `--before` refused many locks.
  - Its `--legacy-peer-deps` retry then silently dropped peers.
    @react-three/fiber lost three.js; langchain lost @langchain/core
    (-4,520 rows).
  - Or it fell back to a fresh re-resolve and re-bundle (rspack, the
    claude-agent-sdk and copilot-language-server families: optional
    platform binaries missing from the lock).
- That restore was fixed (above). Those 204 tasks, plus react-icons 5.6.0
  and 5.7.0 (shards too big for the first diff), were rebuilt:

| install path (206 rebuilt tasks) | tasks |
| --- | --- |
| restored exactly (ci --before, or the lock-guided install that left the lock byte-identical; tree check passed) | 202 |
| fresh install (re-resolve + re-bundle: NOT the original tree) | 4 |
| other | 0 |

The 4 fresh-path packages, and why each took that path:
- `@react-three/fiber@7.0.29`: `npm ci` ERESOLVE (a react peer
  conflict). The lock-guided install did not leave the lock byte-identical
  or complete.
- `conventional-changelog@6.0.0`, `conventional-changelog-core@8.0.0`,
  `conventional-recommended-bump@10.0.0`: the restored tree lacked a
  package its lock lists.

After the rebuild, the 412 shards hold 4,676,168 rows against 4,676,170:
6 refused, 18 lost, 16 gained. react-icons is row-for-row identical.
408 of the 412 shards pass the test. The 4 that do not are
conventional-changelog@6.0.0 and conventional-changelog-core@8.0.0, both
slices. Each loses 4 rows and gains 4, which is **8 swapped rows** in
total: the same files (conventional-changelog-writer `dist/template.js`,
conventional-changelog-core `lib/merge-config.js`) printed differently
after the fresh re-bundle, so no coverage was lost (accepted by the
coordinator).

The other 65,633 tasks ran on the first restore code. They pass the test
(lost <= refused, 0 gained, same modules), so their trees reproduced 4736's
rows; which install path each took was not logged then. Every one of the
131,678 shards now passes the test except those 4.

## 3. Library score (structural rule, scorer 14; before = 4736 with this branch's binary)

| corpus, slice | P before | P after | bar | R before | R after | wrong bytes before -> after | app bytes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| bun-dev, bun | 0.998342 | 0.998342 | 0.998 pass / pass | 0.8640 | 0.8640 | 150,273 -> 150,273 | 0 / 0 |
| bun-dev, node | 0.997439 | **0.997931** | 0.998 MISS / MISS | 0.8594 | 0.8570 | 241,822 -> 194,671 | 0 / 0 |
| bun-held, bun | 0.999755 | 0.999755 | pass / pass | 0.8697 | 0.8696 | 5,563 -> 5,563 | 0 / 0 |
| bun-held, node | 0.997162 | **0.999631** | MISS / **pass** | 0.8315 | 0.8299 | 66,396 -> 8,604 | 0 / 0 |
| cc (2.1.216, sample truth) | 1.000 | 1.000 | pass / pass | 0.8190 | 0.8190 | 0 -> 0 (named 1,819,438 both) | 0 / 0 |

The "before" column equals exp210's baseline exactly. On bun-dev's bun
slice, every wrong pair is byte-equal. Only named bytes moved: -2,744 bytes
on bun-dev bun, and -2,544 on bun-held bun.

Largest wrong-pair moves, bun-dev node:
- readable-stream -> string_decoder -40,177
- readable-stream -> safe-buffer -5,367
- css-tree -> lru-cache -4,954
- stylelint -> lru-cache -2,795
- undici -> encoding-sniffer -2,310
- stylelint -> css-tree -1,178
- encoding-sniffer -> undici **+10,336**
- whatwg-encoding -> undici **+3,269**

Largest wrong-pair moves, bun-held node:
- iconv-lite -> node-fetch -42,189
- readable-stream -> string_decoder -14,262
- gaxios -> node-fetch -1,660
- readable-stream -> safe-buffer -1,519
- encoding-sniffer -> undici +2,061
- whatwg-encoding -> undici +654

The kind-esbuild / kind-webpack / kind-rollup corpora are not served by
this index: it holds only the two Bun slices, and those corpora need
indexes of their own bundler kind. No kind label exists for 4736 either,
so they were not scored.

## 4. The trade-off the rule makes (file-level, as decided)

The rule asks whether the row's own FILE holds the code. Bun's maps often
file code under another file of the SAME package, especially in node
builds of readable-stream, css-tree, undici, mongodb and node-taglib-sharp.
Those rows are right about the package and wrong about the file, and the
rule refuses them. That accounts for most of the recall cost and for the
new encoding-sniffer -> undici claims: undici's own rows no longer
out-vote encoding-sniffer's copy of the same helpers.

A package-level variant would keep such rows. It would check the tokens
against every file of the owner package and refuse only rows whose code is
in no file of their package. That would likely keep most of the recall and
avoid the undici swap, but the row's file would stay wrong (a vendor path
problem). It is not built: it is a different rule from Andrew's file-level
decision and would need another full rebuild (about 6 h). It is proposed
here for Andrew.

The coarser "cross-shard agreement" alternative was not used or measured.
exp210 §5b sized it (142 of readable-stream's 1,718 node rows dropped).

## Gate

`cargo dev check`, run bare. Worktree at `138a89ee4` plus the README;
the clean checkout is `30777ae53`, a detached worktree with
`npm ci --ignore-scripts`. The first worktree run failed one test,
`a_webpack_run_with_a_relative_output_dir_writes_only_there`, because the
new worktree had no `node_modules` for the webcrack shim. It passed after
`npm ci --ignore-scripts`.

| stage | worktree | clean checkout |
| --- | --- | --- |
| shims | PASS | PASS |
| no-private-addresses | PASS | PASS |
| rust:fmt | PASS | PASS |
| rust:clippy | PASS | PASS |
| census:clones | REVIEW (no group in this lane's code) | REVIEW (same) |
| census:concurrency | PASS | PASS |
| census:slop | PASS | PASS |
| rust:unit | PASS | PASS |
| rust:build | PASS | PASS |
| format-golden | PASS | PASS |
| e2e | PASS | PASS |

The commit after `30777ae53` changes this README only.

## What this could NOT prove

- The eval was not run. The main agent schedules it.
- How the 65,633 first-run tasks installed (no log then). Their rows match
  4736's, which is the property that matters, but not every tree was
  checked against its lock.
- The false-refusal rate at full scale. It was measured on 400 random
  versions (about 40 of 1.79M rows), not on every refused row.
- Whether the package-level variant (§4) is better. It was not built.
- `build.json` carries no source-confirmation totals (see the code
  section).

## Proposed CLAUDE.md text (not edited)

None needed for the gate. If the index is adopted, the references section
should say that the eval index is `77b12699…` (unit-rows/6, source
confirmed) and that a label scored on 4736 needs `--force-mixed` against
it.
