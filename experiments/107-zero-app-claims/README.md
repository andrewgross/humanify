# exp107: app code claimed to zero on every bundler

## STATUS (2026-10-14)

- **State:** DONE on branch `exp107-zero-app-claims`, NOT merged. The gate passed all 13 stages (bare) at every commit.
  Warm neutrality vs main `116090f2` on 2.1.215->2.1.216: NEUTRAL, 0 files / 0 lines, cache candidate +0 / baseline
  +0, at `1421ebb7`. The crates change is the container rule below; dev cards are byte-identical (70/70, structural).
- **Totals first.** App code claimed on the frozen TEST split falls to **0 on every bundler, under both rules and
  on all three indexes** (step4-wr, exp101 c2-l12m4, call2k-l12m4). It takes scorer 6 (this lane) plus exp106's
  scorer-5 source-table rules (`virtual`, `chained`); the merged scorer was measured on a scratch merge of exp106 @
  `7f47aacb` (not committed: whoever lands second merges and re-scores). App claimed, bytes:

  | test label (index)              | scorer 4                       | scorer 6 alone    | scorer 5 + 6 |
  | ------------------------------- | ------------------------------ | ----------------- | ------------ |
  | main, structural (step4-wr)     | wp 3,962 / ru 248 / esb 13,806 | esb 13,727        | **0**        |
  | main, agreement (step4-wr)      | wp 611 / ru 242                | ru 76             | **0**        |
  | exp101 c2-l12m4, structural     | esb 1,311,851 / ru 148         | (esb is exp106's) | **0**        |
  | exp101 call2k-l12m4, structural | esb 1,320,308 / ru 72          |                   | **0**        |
  | exp101 call2k-l12m4, agreement  | esb 765,142 / ru 98            |                   | **0**        |

  (wp = test-webpack, ru = test-rollup, esb = test-esbuild.) Scorer 6 alone takes webpack to 0 and rollup to 0
  structural / 76 B agreement (the rest is `?commonjs-*`, exp106's `virtual`); every esbuild byte is exp106's
  `chained`. Aggregates only; no per-app test card or report was read.

  TEST P / R / F0.5 / F1 under scorer 5 + 6, this branch's binary (container rule), index step4-wr, vs main under
  scorer 4 (`test-exp105-*`):

  | set          | structural (scorer 4 -> 5+6)                                  | agreement (scorer 4 -> 5+6)                               |
  | ------------ | ------------------------------------------------------------- | --------------------------------------------------------- |
  | test-bun     | 52.8/14.1/34.1/22.3 -> 52.8/14.1/34.1/22.3                    | 58.7/13.0/34.4/21.2 -> 58.7/13.0/34.4/21.2                |
  | test-esbuild | 51.4/14.6/34.1/22.7 -> 51.4/14.3/33.8/22.4 (app 13.8 KB -> 0) | 58.9/12.0/33.0/19.9 -> 58.9/11.8/32.7/19.6                |
  | test-rollup  | 75.7/28.7/57.0/41.6 -> 75.9/29.1/57.4/42.1 (app 248 -> 0)     | 86.9/15.7/45.5/26.5 -> 87.3/16.0/46.1/27.0 (app 242 -> 0) |
  | test-webpack | 85.4/34.0/65.6/48.6 -> 85.9/34.4/66.1/49.1 (app 3,962 -> 0)   | 95.4/23.0/58.6/37.1 -> 95.8/23.3/59.0/37.4 (app 611 -> 0) |

  The container rule alone moves test-webpack by +0.01 P (structural and agreement); everything else is truth.
  On the bigger indexes under 5 + 6: c2 structural test-esbuild 74.3/45.5/65.9/56.4 -> 77.5/46.5/68.4/58.1; call2k
  structural 77.7/53.2/71.1/63.1 -> 80.5/54.1/73.3/64.7; call2k agreement 88.0/37.2/69.1/52.3 -> 90.6/37.6/70.7/53.2
  (the chained bytes the matcher named move from app-claimed to right).

  DEV-SIDE corpus `apps` (97 bundles, below), this branch, step4-wr, scorer 5 + 6, P / R / F0.5 / F1:

  | set          | structural                | agreement                 |
  | ------------ | ------------------------- | ------------------------- |
  | apps-esbuild | 75.0 / 13.3 / 39.0 / 22.7 | 81.1 / 11.1 / 35.8 / 19.5 |
  | apps-webpack | 97.2 / 47.7 / 80.5 / 64.0 | 98.7 / 28.9 / 66.6 / 44.7 |
  | apps-rollup  | 89.3 / 35.2 / 68.3 / 50.5 | 96.4 / 19.1 / 53.2 / 31.9 |

  App claimed 0 on every set. (F0.5 here = 1.25PR / (0.25P + R), exp106's definition; this branch's cards do not
  carry the field until exp106 merges.)

- **Every app-claimed byte was a TRUTH artifact except one class** (webpack's module table, a matcher error, fixed).
  The claims themselves were right: @babel/code-frame's bytes, chromium-bidi's, @prisma/\*'s.

## The dev-side reproduction: corpus `apps`

`scripts/dev_apps.py`: 39 apps from exp095's TRAINING corpus (never the 67 held-out apps, never exp100's test apps),
built with exp100's own esbuild / webpack / rollup recipes (its `build.py`, unchanged), frozen with exp096's truth
and origin layers: **97 bundles** (esbuild 39, webpack 28, rollup 30).

- stratum 1: 16 CLIs + 16 Electron apps drawn at random (seed 107) from the training apps with a library closure of
  at most 15 MB;
- stratum 2 (`chained`): every other training app whose install ships a package source map with >= 20 sources
  resolving OUTSIDE node_modules, one per set of such packages (7 apps: element-plus, @daytona/sdk..., @angular/
  common, typeorm, chromium-bidi, @prisma/orm-\*, @snyk/error-catalog-nodejs-public). Chosen by mechanism, on dev
  installs only.

Main's configuration on it (index step4-wr) claimed app code on webpack (1,209 B structural / 697 B agreement) and
rollup (138 / 234 B); on the exp101 c2-l12m4 index its esbuild builds claimed **349,784 B** structural / 312,723 B
agreement (chromium-bidi 189 KB, @prisma/orm-\* 160 KB). Each byte classified:

| mechanism                                                                        | where                                             | kind    | fix                       |
| -------------------------------------------------------------------------------- | ------------------------------------------------- | ------- | ------------------------- |
| UTF-16 columns read as code points                                               | webpack, rollup (any minified line with an emoji) | truth   | scorer 6 `utf16`          |
| one-field segments dropped (glue carried the previous mapping)                   | webpack wrappers, `var i;`                        | truth   | scorer 6 `unmapped`       |
| carried unit with nothing to check (rollup guard glue `if(ju)return Hu;ju=1;`)   | rollup                                            | truth   | scorer 6 `unverified`     |
| rollup `?commonjs-entry` virtual ids                                             | rollup                                            | truth   | exp106 scorer 5 `virtual` |
| a package's own source map chained by esbuild, paths outside node_modules        | esbuild                                           | truth   | exp106 scorer 5 `chained` |
| webpack's module table joined its FIRST module's group; MODULE claimed the table | webpack                                           | matcher | container rule (crates)   |

### 1. The decoder (scorer 6, `decoder.py`, `truth_audit.redecode`)

The source map v3 generated column counts UTF-16 code units. The truth's decoder (exp095 `truth.py`, exp096
`origins.segments`) read it as a code-point index, so every astral character (an emoji in a string) before a
mapping on its line moved that mapping one byte right, cumulatively. Bun and esbuild rarely put many on one line;
terser-minified webpack and rollup output is a few lines of megabytes. `verify_decoder.py`, every source-mapped
entry:

| corpus   | entries | segments    | moved      | in entries | one-field segments now kept |
| -------- | ------- | ----------- | ---------- | ---------- | --------------------------- |
| dev      | 69      | 41,801,451  | 124,470    | 21         | 5,762                       |
| bundlers | 27      | 670,175     | 11,890     | 2          | 851                         |
| apps     | 79      | 41,316,267  | 6,310,643  | 21         | 22,274                      |
| test     | 191     | 142,862,499 | 13,587,130 | 53         | 42,800                      |

(apps at its first freeze, 79 entries.) One webpack dev bundle: 2,087,563 of 2,441,932 segments moved; its
@babel/code-frame unit was credited to the app's `entry.cjs`. A one-field segment ("this code maps to nothing")
was dropped, so the bundler's glue took the previous mapping's source; it is now kept as `<unmapped>`. Both rules
off reproduces every frozen truth's segments exactly (366 entries); the origin layer's runs are re-indexed onto the
new segments (the mapped segments are scorer 4's, in order: checked, refused otherwise).

### 2. `unverified` (scorer 6, `truth_audit.carried_verdict`)

Scorer 4's `carried` rule: a unit with no mapping of its own is `<unmapped>` when at least 3 of its tokens can be
checked and most are absent from the carried-from source. With fewer than 3 it kept the carried label: adjacency
as attribution. Now it is `<unmapped>`: nothing confirms it. rollup's guarded-require glue (`Hu={}`,
`if(ju)return Hu;`, `ju=1;`) carried the require call's mapping into the app entry.

### 3. The container rule (crates, `module_groups`)

Discovery groups the statements before a module with it (Bun hoists a module's declarations before its wrapper).
webpack's module table is ONE statement holding every module, so it joined the first module's group, and MODULE
claimed the whole table as that package (a dev build: the table claimed as bin-wrapper). A statement holding two or
more module heads is now their container and in no group. Dev corpus (step4, structural): 70/70 cards
byte-identical. apps corpus (step4-wr): webpack wrong-family bytes -5,645 structural / -4,415 agreement, named
-199 B; esbuild and rollup byte-identical.

### 4. esbuild on the bigger indexes (the orchestrator's 1.31 MB question)

Reproduced on dev: esbuild chains a package's own `//# sourceMappingURL` map; chromium-bidi's and @prisma/orm-\*'s
maps list `../../../third_party/...`-style sources that resolve outside node_modules into the app's tree, so the
truth said `<app>`. The claims are right (the index now holds those packages, so it names them). exp106's
`chained` rule (that package keeps the bytes) takes dev from 349,784 B to 0 and test-esbuild from 1,311,851 B
(c2) / 764,678 B (call2k agreement) to 0. It is a truth artifact, not a matcher error.

## What remains (open)

- **rollup's compressed guard** `function requireX(){return h||(h=1,…,m.exports)}` (and `return h?m:(h=1,…)`): ~30%
  of the guarded requires in the dev rollup builds; the rollup grammar recognises only `if(h)return m;h=1`. So
  those modules are no modules, and an app statement before the next module joins that module's group (dev: 16 B of
  `entry$4.exports;` claimed by MODULE as execa, agreement; `unverified` makes it unmapped, but the claim is
  wrong). The fix changes the unit model: it needs a `LIBRARY_UNITS` bump, i.e. rebuilding every module shard.
- **Scorer numbering / merge order:** 5 is exp106's (truth fixes + F0.5). This branch implements 6 on top of 4; both
  are needed for 0 on test. If exp106 lands first, merging it here means: keep both scorers' sections (the scratch
  merge's resolution: `patch_sources` first, then `redecode`; lib096 gains `chained-apps.json`, built with exp106's
  `chained.py build --corpus apps`), then re-score every committed label once. The committed labels of this branch
  carry scorer 6 alone.
- **Precision is still far below the bar on test** (esbuild ~51%, bun ~53%): app-claimed is now 0, and what
  remains is wrong-family bytes, mostly pre-bundled copies the origin layers cannot see (STATE.md).

## How to reproduce

```bash
python3 -I experiments/107-zero-app-claims/scripts/dev_apps.py plan && python3 -I … build --jobs 8
<exp095 venv python> -I experiments/107-zero-app-claims/scripts/dev_apps.py freeze --jobs 12
npm run eval -- library exp107-main-structural --index /work/exp103-data/ix/step4-wr-v3 --rule structural --corpus apps
<venv python> -I experiments/104-truth-audit/copies.py exp107-main-structural /work/exp107-data/registry \
  experiments/096-library-matching-score/families-derived.json --jobs 1 --corpus apps   # 8 jobs: pickling error
python3 -I experiments/107-zero-app-claims/verify_decoder.py --jobs 12      # decoder vs every frozen truth
<venv python> -I experiments/107-zero-app-claims/verify_s4.py step4f-structural   # rules off = scorer 4's cards
<venv python> -I experiments/107-zero-app-claims/app_claims.py <label> --corpus apps   # every app-claimed unit
```
