# exp190 — build shape: cheaper shard routes, and what survives tree-shaking

STATUS (2026-11-06): MEASURED, measurement only — no pipeline or
index-builder code changed. Three results:

1. **A shard does not need its dependencies installed.** Bundling the
   package's OWN tarball with the builder's two entries and every declared
   dependency external ("pkg-bundle") reproduces **98.6%** of the package's
   own unit rows in the existing `bun-bun-cjs-min` shards (**96.2% by
   nodes**), on 51 packages. Per-file minify routes do worse: 92.4%
   (each file its own ESM bundle), 80.0% (its own CJS bundle), 31.9%
   (Bun's transpiler, `--no-bundle`).
2. **Tree-shaking barely breaks a match; the bundler's MINIFIER does.**
   On 33 tiny apps, the Bun bundles' surviving library prints match the
   cb64 index 93.2% of the time. esbuild's match 44.8% (lucide-react left
   out: esbuild pulled its whole CommonJS build), rollup's 13.5% and
   webpack's 19.6% (both terser), 3.8–5.0% by nodes.
3. **A builder defect, found on the way (precision).** When the Bun
   CommonJS output wrapper is not cut into statements (the wrapper gate
   needs ≥ 50 bindings), the whole bundle is ONE unit and every function
   row in it inherits ONE owner and file. In a 1-in-60 sample of the
   `bun-bun-cjs-min` store, 533 of 2,199 shards with rows (24%) have at
   least one uncut build, and in **76 (3.5%) the whole bundle is credited
   to a DEPENDENCY**: `@aws-sdk/middleware-logger@3.183.0`'s shard is 57
   rows, all `tslib/tslib.js`; `@aws-sdk/util-user-agent-browser@3.575.0`
   is 110 rows, all `bowser`. The package's own code is recorded under
   another package's name.

Branch `exp190-build-shape`. Data under `/work/exp190-data` (not committed).

## Gate

| check                                                                          | result                               |
| ------------------------------------------------------------------------------ | ------------------------------------ |
| `npm run check` (13 stages, bare, worktree, at 131e8eed3's tree)               | ALL 13 STAGES PASSED (e2e: 19 pairs) |
| `npm run check` from a clean checkout (`git worktree add --detach`, db649d125) | ALL 13 STAGES PASSED (e2e: 19 pairs) |

Only Python scripts and this README are added (no pipeline, builder or
harness TS change), so no eval or neutrality run applies.

## (b) Bundle route vs direct routes

**Question.** A shard is made today by `npm install`ing a package with its
whole dependency tree (as of its publish date) and bundling two app
entries that `require()` / `import()` it. Would a cheaper route give the
same unit hashes?

**Method.** 51 package@versions from the exp184 store
(`/work/exp184-data/store-full`), by npm-high-impact download rank, 17 per
size class (the package's own distinct unit prints in the shard: tiny
< 15, medium 15–299, large ≥ 300); 30 are ESM (`type: module`), 5 dual, 16
CommonJS; 19 ship a prebuilt `dist/`-style layout (`pick.py`). Ground
truth: the package's OWN rows (owner = the package, ≥ 20 nodes, distinct
hash) in its existing `bun-bun-cjs-min` shard. Each route computes unit
prints with the same `humanify-ag discover --unit-prints --bundler bun`
the run uses (`routes.py`, `compare.py`). Same Bun (1.4.0), same
`--minify`, same `process.env.NODE_ENV` define everywhere.

| route          | what it does                                                                                                      | needs dependency install |
| -------------- | ----------------------------------------------------------------------------------------------------------------- | ------------------------ |
| bundle (today) | builder reproduction: install as of publish date, the two entries, `bun build --target bun --format cjs --minify` | yes                      |
| pkg-bundle     | the package tarball only (`npm pack`), the same two entries, its declared dependencies external                   | no                       |
| single-esm     | every file its own bundle entry, every import external, ESM output                                                | no                       |
| single         | the same, CommonJS output (the slice's own format)                                                                | no                       |
| nobundle       | Bun's transpiler (`--no-bundle --minify`), no bundler                                                             | no                       |

Shard rows that are not package code are set aside first (214 of 16,238,
1.3%): the uncut wrapper's whole-bundle print and its function, Bun's
runtime helpers and the lab app's own function (any print recorded under
≥ 50 distinct owners in a store sample, `shared_prints.py`).

**Totals — share of the package's own shard rows the route reproduces
(same hash anywhere in the route's output):**

| route      | rows   | by count  | by nodes  |
| ---------- | ------ | --------- | --------- |
| pkg-bundle | 16,024 | **98.6%** | **96.2%** |
| single-esm | 16,024 | 92.4%     | 75.7%     |
| single     | 16,024 | 80.0%     | 55.4%     |
| nobundle   | 16,024 | 31.9%     | 8.2%      |

By package class (pkg-bundle / single-esm, by count; by nodes in brackets):

| class         | pkgs | rows   | pkg-bundle    | single-esm    | single | nobundle |
| ------------- | ---- | ------ | ------------- | ------------- | ------ | -------- |
| tiny          | 17   | 103    | 71.8% (62.0%) | 67.0% (40.9%) | 61.2%  | 18.4%    |
| medium        | 17   | 1,269  | 97.9% (66.9%) | 87.4% (40.3%) | 82.9%  | 25.4%    |
| large         | 17   | 14,652 | 98.8% (99.1%) | 93.1% (79.1%) | 79.9%  | 32.6%    |
| ESM           | 30   | 9,180  | 98.4% (98.8%) | 95.2% (78.4%) | 81.4%  | 35.2%    |
| dual          | 5    | 1,046  | 99.8% (99.9%) | 93.4% (59.6%) | 93.2%  | 45.9%    |
| CommonJS      | 16   | 5,798  | 98.7% (88.6%) | 87.9% (72.7%) | 75.4%  | 24.2%    |
| prebuilt dist | 19   | 4,987  | 99.2% (97.9%) | 86.6% (66.3%) | 64.9%  | 31.7%    |
| source layout | 32   | 11,037 | 98.3% (95.7%) | 95.1% (78.3%) | 86.8%  | 32.0%    |

"By nodes" is dominated by a few huge prints (a CommonJS module's whole
factory function: lodash 28,790 nodes, jiti 51,280), so a single miss
there moves it a lot; read the count column first.

**What the misses are.** Each missed row is found in the bundle
reproduction by hash, paired with the most similar print of the route
(blurred names, node count within 30%), and classified from the two texts
(`compare.py`; heuristic, sampled by hand):

| miss kind              | pkg-bundle | single-esm | single | what it is                                                                                                                                                         |
| ---------------------- | ---------- | ---------- | ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| factory granularity    | 0          | 387        | 1      | a CommonJS file that is its own entry prints as one factory (or Bun turns it into ESM declarators); in the bundle its factory body is cut into statements          |
| lazy init              | 52         | 290        | 635    | an ES module reached by `require()`/`import()` sits in Bun's lazy init: its declarations are hoisted and the body ASSIGNS (`Ee=(e,r)=>…` instead of `var Ee=…`)    |
| require link           | 52         | 167        | 319    | a bundled `require("./x")` becomes a call of that module's factory (`g()`); on its own the file keeps `o("./x")`. In pkg-bundle: calls into an external DEPENDENCY |
| import interop         | 20         | 59         | 32     | an import read as `x.default` / a namespace property on one side, a plain binding on the other                                                                     |
| wrapper only           | 3          | 16         | 346    | a whole statement whose functions all matched; only the statement around them differs                                                                              |
| other                  | 78         | 252        | 1,827  | the rest (inlined constants, statement joining, the odd module body)                                                                                               |
| not in my reproduction | 25         | 41         | 47     | cannot be classified (two @babel installs failed without the builder's grace fallback)                                                                             |

`nobundle` misses almost everything for a simpler reason: Bun's
transpiler minifies far less than its bundler (it keeps `const`, does not
join statements into comma expressions, keeps `if`/`return` chains), so
the printed syntax differs.

Tiny packages are hardest for every route because their rows are few and
dominated by statements that touch the package's dependencies or the
lazy-init shape (brace-expansion calling `balanced-match`, find-up's
`path` import).

**Wall time per package** (warm npm cache — every package had been
installed before by exp184, so network time is NOT measured):

| class  | bundle route median (max) | pkg-bundle median (max) | single-esm median (max) |
| ------ | ------------------------- | ----------------------- | ----------------------- |
| tiny   | 0.42 s (1.16)             | 0.44 s (0.48)           | 0.41 s (0.63)           |
| medium | 0.46 s (0.78)             | 0.47 s (0.57)           | 0.51 s (1.35)           |
| large  | 0.80 s (5.63)             | 0.67 s (5.66)           | 2.16 s (16.81)          |

All 51: bundle route 37 s (install 21.5, build 1.6, fingerprint 11.7),
pkg-bundle 33 s (pack 20.3, build 1.5, fingerprint 10.7). Both are
dominated by npm's per-call overhead with a warm cache; Bun builds take
milliseconds. The per-file routes pay one fingerprint process per file
(60 s for single-esm), which an in-process builder would not. The real
difference is what is installed: the 51 bundle-route installs hold **233
packages, 85.3 MB**; the 51 pkg-bundle trees hold **51 packages, 28.5 MB**
(3.0x fewer bytes, 4.6x fewer packages). And the pkg-bundle route cannot
fail on date-limited dependency resolution: two @babel 8 packages whose
install failed in my reproduction (no grace fallback) built fine.

## (c) Tree-shaking survival on 33 tiny apps

**Method** (`apps.py`, `index_rows.py`, `survive.py`). 34 one-line apps,
each importing 1–3 named exports (or the default of a CommonJS package)
from one popular package at the newest version cb64 indexes (lodash-es,
date-fns, zod, rxjs, lucide-react, @tanstack/query-core, uuid, nanoid,
ramda, immer, zustand, yaml, semver, lodash, dayjs, clsx, fast-deep-equal,
deepmerge, ms, change-case, valibot, es-toolkit, remeda, d3-array, marked,
@reduxjs/toolkit, superjson), and exporting them so nothing can drop them.
One app failed in every bundler (@tanstack/table-core 9 no longer exports
`createTable`), leaving 33. Bundled CommonJS, minified, production, with
sourcemaps: Bun (`--target bun`, the index's own recipe), esbuild
(`--platform=node`), rollup and webpack (the index builder's own recipe
scripts: rollup 4 + commonjs + terser; webpack 5 production, terser).
Input sourcemap comments stripped, as the builder does. Each print
(≥ 20 nodes) of `discover --unit-prints --bundler <b>` is attributed
through the bundle's sourcemap (≥ 90% of its segments in one file, the
builder's rule); a print in `node_modules/<package>/` is a SURVIVOR;
it MATCHES when the cb64 index holds its hash under that package (either
Bun slice).

| bundler                      | survivors | matched by hash   | by nodes | survivors / indexed rows at that version |
| ---------------------------- | --------- | ----------------- | -------- | ---------------------------------------- |
| bun                          | 4,953     | **4,617 (93.2%)** | 83.0%    | 16.4%                                    |
| esbuild                      | 8,687     | 5,974 (68.8%)     | 40.2%    | 28.8%                                    |
| esbuild without lucide-react | 4,875     | 2,182 (44.8%)     | 16.3%    | —                                        |
| rollup                       | 5,118     | 689 (13.5%)       | 3.8%     | 17.0%                                    |
| webpack                      | 5,144     | 1,002 (19.5%)     | 5.0%     | 17.1%                                    |

Survival: a 1–3 name import keeps about **one sixth** of what the index
holds for that package version (by count; 748 of 10,555 indexed files had
a survivor in Bun's bundles). Two packages were NOT tree-shaken by
anyone: rxjs (all four bundlers resolve its CommonJS `main`; 822–994
survivors of 961 indexed rows) and, under esbuild, lucide-react (esbuild's
`--platform=node` picks the CommonJS build: 1,906 survivors instead of
10–12). Which file a bundler resolves (CommonJS `main` vs ESM `module`)
decides tree-shaking more than the bundler does.

**What breaks a match.** Every unmatched esbuild/rollup/webpack print is
paired with Bun's print of the same source line; where Bun's print
matched, the pair is classified by the first normalisation under which
the two texts agree (`survive.py transform`):

| why (where Bun's print matched)                                                                        | esbuild | rollup     | webpack |
| ------------------------------------------------------------------------------------------------------ | ------- | ---------- | ------- | --- | --- |
| `if`/`return` chains rewritten as expressions (`?:`, `&&`, `,`)                                        | 1,658   | 998        | 1,171   |
| constant inlined or folded (literal counts differ)                                                     | 252     | 572        | 688     |
| operator / keyword spelling only (`"x"==typeof t`, `==` vs `===`, `const` vs `let`, `void 0`, `(n)=>`) | 75      | 318        | 316     |
| module wrapper or helper (rollup's commonjs `function r(){return s                                     |         | (s=1,…)}`) | —       | 267 | —   |
| other syntax                                                                                           | 332     | 498        | 485     |
| no Bun print on that line                                                                              | 77      | 1,499      | 1,214   |
| Bun's print did not match either                                                                       | 319     | 277        | 268     |
| same text modulo names (scope hoisting / renaming)                                                     | **0**   | **0**      | **0**   |

Renaming and scope hoisting never broke a match, as expected. The
breakers are minifier rewrites, not tree-shaking: esbuild's minifier
turns `if(…)return a;return b` into `return …?a:b` and keeps `new Error`;
terser additionally writes yoda comparisons, `const`, `void 0===x`, and
`for(;;)` for `while`. Bun's own 336 misses are mostly statements whose
SHAPE depends on how the module was reached: a real app's static
`import` hoists an ES module's top level into plain `var` declarations,
while the index's `require()`/`import()` entries put the same module in
Bun's lazy init (assignments) — the lazy-init shape from (b), now as an
index coverage gap — plus the uncut-wrapper artifacts.

## What it implies

- **Index build cost.** The pkg-bundle route builds each package version
  once, from its own tarball, without its dependency tree: the same rows
  to 98.6% (96.2% by nodes), a third of the disk, no date-limited
  resolution failures, and no rows of other packages to attribute (one
  package per bundle, so the uncut-wrapper owner mix-up cannot happen).
  What it loses: statements that reach into a dependency (1.4% of rows),
  and a dependency's rows now come only from that dependency's own build
  — at the versions the plan holds (the `dependents` rule already plans
  the versions dependents resolved to; whether that covers what installs
  pulled in was not measured).
- **Matching.** A Bun-built index cannot see esbuild- or terser-minified
  code at statement level (13–45% of survivors). Per-minifier slices (as
  `step4-wr` / `k2-all` planned) are needed for those bundles; a hash form
  normalising operator spellings would recover only the smallest bucket
  (75–318 prints); the largest (if/return rewrites) is a real syntax
  change.
- **Index entries.** Adding a static `import * as m from "<pkg>"` entry
  would record the hoisted top-level shape real apps have, closing the
  lazy-init gap in both (b) and (c).
- **Builder precision.** Cut the Bun CommonJS wrapper in the builder
  regardless of the 50-binding gate (the builder knows its text is a
  bundle; `recognize_wrapper_function` is the shape-only check), or
  attribute function rows by their own spans. Today 3.5% of sampled
  `bun-bun-cjs-min` shards record a package's code under a dependency's
  name; the `bun-node-cjs-min` slice has no such wrapper (0 of 2,759).

## What I could NOT measure

- Cold wall times: the npm cache was warm, so install vs pack network cost
  is unmeasured; the byte and package counts are the proxy.
- Only the `bun-bun-cjs-min` slice was compared in (b); only rows owned by
  the package itself (not its dependencies' rows in the same shard).
- In (b) the shard's `file` field is the file of the whole statement —
  for an uncut bundle, the whole bundle's majority file (25 of the 51
  shards) — so per-file matching (column "same file") is not reliable and
  the package-wide match is the headline.
- The miss classifications in (b) and (c) are text heuristics checked on
  samples, not proofs; "no Bun print on that line" means the pairing
  failed, not that Bun dropped the code.
- (c) used CommonJS output only, default terser options, cb64 (Bun slices
  only); no esbuild/rollup/webpack slice index was scored.
- No precision measurement of any route (a route that reproduces rows
  could also add rows the shard never had; for pkg-bundle the extra rows
  are the package's own code, but this was not counted).

## Files

- `pick.py` — the 51 package@versions.
- `routes.py` — the bundle-route reproduction and the four direct routes, timed.
- `compare.py` — shard rows vs a route; artifact split; miss classes.
- `summarize.py` — the (b) tables.
- `shared_prints.py` — owner counts per hash over a store sample (runtime-helper detection).
- `uncut_wrappers.py` — the uncut-wrapper census.
- `apps.py`, `index_rows.py`, `survive.py` — (c).

Reproduce: see each script's docstring; scratch data lives in
`/work/exp190-data`.
