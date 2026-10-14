# exp107: app code claimed to zero on every bundler

## STATUS (2026-10-14)

- **State:** DONE on branch `exp107-zero-app-claims`, NOT merged; main `5fe24646` (exp106, scorer 5) is merged in.
  **Scorer 6 = scorer 5 + this lane's three decoder/carry rules** (`patch_sources` first, then `redecode`). Every
  committed label was re-scored once (80: 53 dev, 2 bundlers, 4 apps, 21 test summaries; exp108's 22 labels from
  scorer 4, its two test families with their `--sets test-bun,test-esbuild`); `rescore_diff.py` shows
  only truth-derived fields moved (statement bytes by label, totals, misses, claimed-bytes verdicts, seeds,
  exploratory names). No report sha, index, rule, toolchain or detection field moved, and in the config only the
  chained/families shas of labels that had none. Scorer 6 with its rules off reproduces scorer 5's cards
  (`verify_s5.py`: step4f-structural 70/70, exp101-c2-l12m4-agreement 70/70, exp105-dev-{structural,agreement} 27/27).
  The bare gate passed all 13 stages. Warm neutrality, 2.1.215->2.1.216, cache +0 on both legs: NEUTRAL 0 files /
  0 lines vs main `116090f2` at `1421ebb7` (the crates change is the container rule; dev cards 70/70 identical).
- **Totals first.** App code claimed on the frozen TEST split is **0 on every bundler, under both rules and on all
  three indexes** (step4-wr-v3, c2-l12m4, call2k-l12m4). Scorer 5 alone left test-webpack 3,962 B / 611 B and
  test-rollup 16 B; the 1.31 MB test-esbuild on the exp101 indexes was scorer 5's `chained` artifact (claims right).

  TEST, P / R / F0.5 / F1, index step4-wr-v3, main under scorer 5 (`test-exp105-*`) -> this branch's binary under
  scorer 6 (`test-exp107-cont-*`):

  | set          | structural                                             | agreement                                            |
  | ------------ | ------------------------------------------------------ | ---------------------------------------------------- |
  | test-bun     | 52.8/14.1/34.1/22.3 -> 52.8/14.1/34.1/22.3             | 58.7/12.9/34.4/21.2 -> 58.7/13.0/34.4/21.2           |
  | test-esbuild | 51.4/14.3/33.8/22.3 -> 51.4/14.3/33.8/22.4             | 58.9/11.7/32.7/19.6 -> 58.9/11.8/32.7/19.6           |
  | test-rollup  | 75.8/28.7/57.0/41.6 (app 16) -> 75.9/29.1/57.4/42.1    | 87.1/15.7/45.6/26.6 -> 87.3/16.0/46.1/27.0           |
  | test-webpack | 85.4/34.0/65.6/48.7 (app 3,962) -> 85.8/34.4/66.1/49.1 | 95.5/23.0/58.6/37.1 (app 611) -> 95.8/23.3/59.0/37.4 |

  App claimed after: 0 everywhere. exp108's test labels (k2/k5/c2v3, test-bun + test-esbuild) carried 0.77-1.32 MB
  of test-esbuild app claims under scorer 4 (the chained artifact); under scorer 6, 0 (k2 structural test-esbuild
  80.6/65.5 -> 83.0/66.1 P/R, F0.5 79.0). The container rule alone is +0.01 P on test-webpack; the rest is truth. On the
  bigger indexes scorer 5 -> 6 moves test by at most 0.1 (c2 structural test-esbuild 77.5/46.5/68.4/58.1 ->
  77.4/46.5/68.3/58.1; call2k structural 80.5/54.1/73.3/64.7, agreement 90.6/37.6/70.7/53.2, both unchanged).

  DEV on c2-l12m4 (`exp101-c2-l12m4-*`), scorer 5 -> 6, P / R / F0.5 / F1:

  | set         | structural                                 | agreement                                    |
  | ----------- | ------------------------------------------ | -------------------------------------------- |
  | cc          | 100.0/76.7/94.3/86.8 (unchanged: a sample) | 100.0/63.4/89.6/77.6 (unchanged)             |
  | held-out 67 | 98.7/54.2/84.8/70.0 -> 98.7/54.2/84.8/70.0 | 95.1/38.1/73.2/54.4 -> 95.2/38.1/73.2/54.4   |
  | esbuild-app | 98.3/52.5/83.7/68.5 -> 98.4/52.6/83.8/68.5 | 99.3/34.4/72.1/51.1 -> 99.3/34.4/72.1/51.1   |
  | wrangler    | 95.5/16.5/48.8/28.2 -> 95.5/16.6/48.9/28.3 | 100.0/10.1/36.0/18.4 -> 100.0/10.2/36.1/18.5 |

  App claimed 0 on every dev set before and after.

  DEV-SIDE corpus `apps` (97 bundles, below), this branch, step4-wr-v3, scorer 6, P / R / F0.5 / F1, app claimed 0:

  | set          | structural                | agreement                 |
  | ------------ | ------------------------- | ------------------------- |
  | apps-esbuild | 75.0 / 13.3 / 39.0 / 22.6 | 81.1 / 11.1 / 35.8 / 19.5 |
  | apps-webpack | 97.1 / 47.7 / 80.5 / 64.0 | 98.7 / 28.9 / 66.6 / 44.7 |
  | apps-rollup  | 89.3 / 35.2 / 68.3 / 50.5 | 96.4 / 19.1 / 53.2 / 31.8 |

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
- **Precision is still far below the bar on test** (esbuild ~51%, bun ~53%): app-claimed is now 0, and what
  remains is wrong-family bytes, mostly pre-bundled copies the origin layers cannot see (STATE.md).

## How to reproduce

```bash
python3 -I experiments/107-zero-app-claims/scripts/dev_apps.py plan && python3 -I … build --jobs 8
<exp095 venv python> -I experiments/107-zero-app-claims/scripts/dev_apps.py freeze --jobs 12
npm run eval -- library exp107-main-structural --index /work/exp103-data/ix/step4-wr-v3 --rule structural --corpus apps
<venv python> -I experiments/104-truth-audit/copies.py exp107-main-structural /work/exp107-data/registry \
  experiments/096-library-matching-score/families-derived.json --jobs 1 --corpus apps   # 8 jobs: pickling error
python3 -I experiments/106-matcher-hygiene/scorer5/chained.py build --corpus apps   # scorer 5's chained layer
python3 -I experiments/107-zero-app-claims/verify_decoder.py --jobs 12      # decoder vs every frozen truth
python3 -I experiments/107-zero-app-claims/rescore_diff.py <results before> <results after>  # what a re-score moved
<venv python> -I experiments/107-zero-app-claims/verify_s5.py step4f-structural   # rules off = scorer 5's cards
<venv python> -I experiments/107-zero-app-claims/app_claims.py <label> --corpus apps   # every app-claimed unit
```
