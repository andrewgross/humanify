# exp095 lane "library discovery": graph-closure candidates + leaf-inward fixpoint (lab, then core)

## STATUS (2026-10-11)

- **Phase 1 is in core, report-only**, on branch `exp095-discovery-core` (not merged):
  - the statement reference graph is `humanify_core::graph::references`;
  - discovery is `humanify_core::libdetect::discovery`;
  - a run with `--diagnostics` writes it as `libraryDiscovery`.
    Nothing is named, moved or renamed by it. Design: `docs/design/library-discovery.md` (§6 has the lab numbers).
- **No index lookup runs inside a pipeline run yet.** In-run seeds are the declared readers and kept `/*!` banners.
  The candidate queue is emitted, not looked up. The lab below feeds the negatives lane's Step B claims in as index
  seeds.
- Scored against the negatives lane's `labels.jsonl` (620 random `src/` statements) and `traps.jsonl` on Claude Code
  2.1.216. 2.1.198 and 2.1.86 have no labels: their vetoes were read by hand.

## Totals first

- **The core reproduces the prototype.** Same seeds on 2.1.216 (declared + Step B index C + the prototype's veto):
  - 12,255 of the core's 12,256 claimed units are claimed by the prototype too;
  - estimated library recall 93.65% on both, 0 sampled app units claimed, the same two tslib traps;
  - the fixpoint adds 64.2 KB over 8 rounds (prototype: 63.8 KB);
  - the isolation veto drops 155 seeds (prototype: 155);
  - library class 116 KB with 9/9 sampled units library (prototype: 117 KB, 9/9);
  - candidate queue 390 KB, ≈113 KB library / 232 KB app (prototype: 399 KB, the same estimates).
- **Every difference is explained** (below). Most are a bug in the prototype's declared-seed tool, which the core does
  not have.
- **The isolation veto, as the prototype ran it, fails on 2.1.86.**
  - MODULE fills 4.5 KB of Claude Code's own keypress parser as `ink` around the fork's two kept tables. That fill
    then counts as the tables' support, so the veto keeps the fork.
  - **Fix (now the default):** judge a seed's support without MODULE's fills.
  - It removes every ink-fork claim on all three releases.
  - It costs about 2.5 KB of real library claims per release. Recall on 2.1.216 is unchanged at 93.65%, still with 0
    app units.
- **The veto was never tested on the other forks.** The index made no claim on the help formatter or the yoga port in
  any of the three releases.
- **esbuild (wrangler 3.114.0), scored exactly against its source map**, with declared seeds only:
  - claims are 75 KB, 100% library (4.8% of the bundle's 1.58 MB of top-level library code);
  - library class is 431 KB of library against 79 B of esbuild's generated `import_meta_url` shim;
  - the queue is 30 KB, 98% library.
  - pnpm has nothing to discover: 99.7% of it is CommonJS modules, which already go to `vendor/`.
- **Cost:** 0.6–1.0 s on the 20 MB bundle (graph + fixpoint), only when `--diagnostics` is given.

## Results

### 1. Core vs prototype, Claude Code 2.1.216

The prototype's own `discover.py` against the core's report (`core_compare.py`). Seeds: declared readers + banners,
plus Step B index C. Rules DOWN, MODULE, GLUE. The prototype's veto is `VETO_SUPPORT=all-rules` in the lab binary.

|                                             | prototype                        | core (prototype's veto)                      | core (default veto) |
| ------------------------------------------- | -------------------------------- | -------------------------------------------- | ------------------- |
| declared + banner seeds                     | 293 units, 214.4 KB              | 282 units, 206.8 KB                          | 282 units, 206.8 KB |
| declared only: grown, rounds (KB per round) | 36.0 KB, 4 (26, 8.6, 1.4, 0.2)   | 39.3 KB, 4 (28.4, 9.5, 1.2, 0.2)             | same                |
| declared only: library recall (est.)        | 9.39%                            | 9.39%                                        | 9.39%               |
| + index C: claimed units, `src/` KB         | 12,270, 1,956.8 KB               | 12,256, 1,956.1 KB                           | 12,240, 1,954.1 KB  |
| + index C: grown by the fixpoint            | 63.8 KB                          | 64.2 KB                                      | 64.7 KB             |
| rounds (KB per round)                       | 8                                | 8 (42.8, 8.5, 1.2, 2.2, 2.0, 3.8, 3.7, <0.1) | 8 (43.3, 8.5, …)    |
| library recall / right family (est.)        | 93.65% / 93.65%                  | 93.65% / 93.65%                              | 93.65% / 93.65%     |
| app claimed (sample)                        | 0                                | 0                                            | 0                   |
| traps claimed                               | tslib ×2                         | tslib ×2                                     | tslib ×2            |
| seeds vetoed                                | 155, 15.5 KB                     | 155, 16.3 KB (150 shared)                    | 167, 18.8 KB        |
| library class (floor 30 B)                  | 117.0 KB, 9/9 sampled library    | 116.1 KB, 9/9                                | 117.2 KB, 9/9       |
| candidate queue (≤ 256 KB)                  | 399 KB: 113 lib / 232 app (est.) | 390 KB: 113 / 232                            | 390 KB: 113 / 232   |

The graph is the prototype's: 79,911 units, identical. The core has 225,573 edges, against the prototype's 222,172
plus its wrapper-to-body edges.

**Every difference, explained:**

- **11 declared seeds the prototype has and the core does not.**
  - 10 are an artifact of the prototype's `exp095-declared`. It extends each span back to the previous unit it keeps,
    and it keeps no CommonJS module. So an app statement's span swallows the factory before it, and the factory's
    declaration lands on the next app statement.
  - One of the 10 is `var mLl={}`, an app namespace, which the prototype seeds as `@smithy/node-http-handler` (the
    content-free claim §6.3 describes).
  - The 11th is the chokidar banner. The pipeline's banner rule (`parse_banner`) rejects a one-word name with no
    version.
  - These 11 seeds account for all 15 prototype-only claims: 5 seeds plus 10 units grown from them.
- **5/5 vetoed seeds differ: family data.**
  - The core reads the shipped `families.json` (same npm scope = one family).
  - The prototype hard-coded `@typespec/*` as Azure, so the core vetoes 5 `@typespec/ts-http-runtime` seeds. DOWN then
    reclaims them as `@azure/identity`, so no bytes are lost.
  - The prototype split `@inquirer/*` packages apart, so it vetoed 5 `@inquirer/core` seeds that the core keeps.
- **13 package differences** follow from the above: 6 are the artifact declarations overriding a correct index
  package, 5 are `@typespec` → `@azure/identity`, 2 are `@inquirer/core` → `@inquirer/select`.
- The AWS client claim name differs too. The prototype's `STSClient` → `@aws-sdk/client-s-t-s` is now `client-sts`.
  Within one run it is the same equality key, so no claim set changes.

### 2. The isolation veto on other releases and forks

Step B index C (`other/stepb_other.py`: `stepb_cc.py`'s exact builds and config) on each release's units. The ink-fork
column is the bytes still claimed as `ink`.

| release | index seeds (before the veto) | veto: prototype's rule | ink fork claimed                                  | veto: default (no MODULE support) | ink fork claimed |
| ------- | ----------------------------- | ---------------------- | ------------------------------------------------- | --------------------------------- | ---------------- |
| 2.1.216 | 10,022                        | 155 units, 16.3 KB     | 0                                                 | 167 units, 18.8 KB                | 0                |
| 2.1.198 | 9,889                         | 150 units, 15.4 KB     | 0                                                 | 160 units, 17.8 KB                | 0                |
| 2.1.86  | 7,856                         | 174 units, 20.6 KB     | **4,540 B** (25 units: 2 seeds + 23 MODULE fills) | 190 units, 24.1 KB                | **0**            |

- **2.1.86, the failure.**
  - The fork's keypress module holds two index claims, `PZ4`/`XZ4`, the kept `ink` key tables.
  - MODULE fills the parser around them: the module's claims are all `ink`, the parser's closure stays inside, and its
    only caller is the module's own entry.
  - The fill makes each table's neighbour `ink`, so the prototype's veto finds the tables supported.
  - On 2.1.216 and 2.1.198 the closure guard happens to block the fill, and the veto works.
- **The extra vetoes under the default rule (10–16 per release) were read by hand.**
  - Apart from the two 2.1.86 ink tables, they are real library code that only a MODULE fill had supported: the smithy
    `Client.send` class, msal-common time helpers, is-unicode-supported, fflate's adler32, chalk's color support,
    wrap-ansi, `@alcalzone/ansi-tokenize` and the mcpb requires.
  - That is 2.4–3.4 KB per release. Some of it is still marked library class.
- **Recall, measured on 2.1.216, is unchanged:** 93.65%, 0 app units. Every vetoed unit in the random sample is
  library code, and there is 1 such unit under either rule.
- **A third rule was measured and rejected.** Seeds-only support (another seed of the family must be a direct
  neighbour) vetoes 2,389 seeds (43 KB) and lowers recall to 93.04%.
- **The other forks were not tested.** The index made no claim on the commander-style help formatter
  (`commander-fork` traps) or the yoga port (`yoga-port`, 3 sampled `yoga-layout` units), in any release. The lodash
  helper trap (real `lodash-es`) is vetoed under both rules, and DOWN-CLASS still marks it library class.

### 3. An esbuild bundle

| bundle                                                                                       | top-level statements | library in them (source map) | seeds                    | claims                                       | library class                            | queue                         |
| -------------------------------------------------------------------------------------------- | -------------------- | ---------------------------- | ------------------------ | -------------------------------------------- | ---------------------------------------- | ----------------------------- |
| wrangler 3.114.0 `wrangler-dist/cli.js` (5.8 MB, unminified, 549 lazy modules, 368 CommonJS) | 10,954, 3.2 MB       | 1.58 MB (49%)                | 40 declared (AWS), 59 KB | 121 units, 75 KB: 100% library (recall 4.8%) | 4,206 units, 431 KB: library except 79 B | 100 units, 30 KB: 98% library |
| pnpm 9.15.9 `dist/pnpm.cjs` (8.8 MB, unminified)                                             | 159, 30 KB           | —                            | 0                        | 0                                            | 0                                        | 0                             |

- **What works.**
  - esbuild's helpers are recognised by shape: `__esm` and its object-method form, `__export`, `__reExport`,
    `__commonJS`, `__toESM`.
  - Lazy modules split into body units.
  - Every claim is library code.
  - Package names are right or family-right: `@aws-sdk/client-s3` claims are `@aws-sdk/middleware-sdk-s3` code.
  - The library class is clean except esbuild's generated `import_meta_url` shim, which belongs to the bundler, not the
    app.
- **What doesn't.**
  - **Recall without an index is low (4.8%).** Only the AWS SDK declares itself in its own text.
  - **unminified esbuild keeps a path comment for every module, and nothing reads it yet.** It writes
    `// ../../node_modules/.pnpm/<pkg>@<ver>/node_modules/<pkg>/<file>` before each module's code (1,957 in wrangler).
    That is a declaration for every hoisted library module. Reading it would be a new `ModuleIdentitySource` (P17)
    fact for app code. Minified builds drop it.
  - **esbuild's other helpers are not recognised as runtime.** These are `__name`, `__publicField` and
    `__require`. Library code calls `__name`, so a library unit whose closure reaches it fails MODULE's closure guard.
  - **Nothing to test the veto on.** No index claims exist for these bundles.
  - **pnpm**: its own code is CommonJS too, so it all goes to `vendor/`, and the 30 KB of top-level statements hold
    nothing to find.

### 4. Neutrality

Pending (filled in after the warm runs).

## Files

- `proto/`: a throwaway Rust crate, detached from the repo workspace (its own `[workspace]`; not built by
  `npm run check`).
  - `exp095-refgraph` writes the prototype's statement-level reference graph: oxc semantic, resolved references
    between units, writes linked both ways, and definitional member mutations, including the `rt`/`Oi` namespace
    fill. It takes 1.5 s on the 20 MB bundle and is byte-deterministic.
  - `exp095-declared` runs the pipeline's own `identity::scan_app_declarations` over every unit (with the span
    extension described in Results §1).
  - `exp095-core-discover` runs the PIPELINE's discovery (`libdetect::discovery::discover_in_bundle`) on a raw bundle.
    It takes optional Step B claims (lab unit ids mapped to span starts through `raw.jsonl`). `VETO_SUPPORT=all-rules`
    selects the prototype's veto. `DUMP_UNITS=<tsv>` writes every unit.
- `graphlib.py`: loads the units, the graph, the labels and the Step B claims; iterative Tarjan; the stratified
  estimator.
- `discover.py`: closure candidates, seeds, the fixpoint (GLUE / DOWN / MODULE), the isolation veto, DOWN-CLASS, the
  UP queue. Writes `results.json`.
- `structure.py`: the SCC structure (the giant app core is 1.2 MB; most app code lies below it).
- `core_compare.py`: the core's report against `discover.py`, unit by unit, scored the same way (Results §1).
- `other/scan_units.mjs`: the lab's unit model for a release without realworld labels (2.1.86).
- `other/stepb_other.py`: Step B, index C, on another release's units.
- `other/score_sourcemap.py`: scores a report exactly against an esbuild bundle's source map (Results §3).

## Re-run

```
export PATH="$HOME/.cargo/bin:$PATH"; T=<scratch>
B=/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js
RAW=/work/exp095-library-fingerprints/bundler-variation-and-boundaries/data/units/cc/min/cc-2.1.216/raw.jsonl
NEG=/work/exp095-library-fingerprints/negatives/data
(cd proto && CARGO_TARGET_DIR=$T/target cargo build --release --offline)
# prototype
$T/target/release/exp095-refgraph $B $RAW $T/edges.tsv rt,Oi        # rt = __export, Oi = __reExport in 2.1.216
$T/target/release/exp095-declared $B $RAW > $T/declared.jsonl
python3 discover.py $T/edges.tsv $T/declared.jsonl results.json     # ~35 s
python3 structure.py $T/edges.tsv
# core
$T/target/release/exp095-core-discover $B $T/core-declared.json
VETO_SUPPORT=all-rules $T/target/release/exp095-core-discover $B $T/core-C.json $RAW $NEG/stepb-pred-C.json
python3 core_compare.py $T/edges.tsv $T/declared.jsonl $T/core-declared.json $T/core-C.json $T/compare.json
# another release (2.1.86 has no lab units: scan first)
node other/scan_units.mjs <bundle-2.1.86> $T/cc-2.1.86
PYTHONHASHSEED=0 python3 other/stepb_other.py $T/cc-2.1.86 $T/stepb-C-2.1.86.json
$T/target/release/exp095-core-discover <bundle-2.1.86> $T/core-2.1.86.json $T/cc-2.1.86/raw.jsonl $T/stepb-C-2.1.86.json
# esbuild with a source map (npm pack wrangler@3.114.0)
W=<package>/wrangler-dist/cli.js
DUMP_UNITS=$T/w.tsv $T/target/release/exp095-core-discover $W $T/core-w.json
python3 -I other/score_sourcemap.py $W $W.map $T/w.tsv $T/core-w.json
```

The prototype's helper names are passed by hand. The core recognises them by shape from the toolchain's pieces
(`graph::references::BundlerFacts::recognise`). Seeds "stepB-A" and "stepB-C" are the negatives lane's
`data/stepb-pred-{A,C}.json`.
