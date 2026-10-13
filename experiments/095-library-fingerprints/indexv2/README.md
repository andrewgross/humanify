# exp095 lane "index-v2": fix the index builder, build one real index, re-test the claim rule

## STATUS (2026-10-13)

- **State.** DONE (lab + builder). Branch `exp095-index-v2` (= `exp095-discovery-core` + `exp095-index-selection`),
  not merged. The pipeline binary does not link `humanify-index`, so no change here can reach a humanify run; the
  in-run lookup is discovery-core's, unchanged.
- **Verdict, totals first.**

  - **One real index built:** id `b00fa73547b5d76d0ff0f62c7ccaaf19878840d8003e95929cf0b9794d959037`. It holds
    20,847 package versions of 1,845 packages (2,960 owners with their dependencies). Size: 430 MB raw, 77 MB as a
    tar.gz. Build time: 48 min for the modules on 24 cores, then 12 min for the literal table (installs reused for
    ~4,700 versions). It covers 55.3% of the held-out apps' library bytes by package.
  - **The structural rule does NOT stay at ~0 wrong names on a big index, and neither does the agreement rule — but
    almost all of what the sourcemap calls wrong is a vendored copy.** That is code of package A shipped inside
    package B, which the map can only credit to B. Index seeds, held-out apps:
    - structural: 12.5% of named bytes in another family;
    - agreement: 4.8%.
      In the samples read by hand, the index named the code's real origin in all but ~6 KB (structural) and ~3 KB
      (agreement).
  - **App code claimed is ~0 under both rules.** Claude Code: 0 sampled app units on 2.1.216, and 0 fork-family
    claims (ink, yoga, commander) on all three releases. Held-out: every "app" byte is vite's own pre-bundled
    copies of parse5 / postcss-value-parser / magic-string inside the vite CLI.
  - **Structural finds 1.6–1.7× what agreement finds:**

    | test set                          | structural | agreement |
    | --------------------------------- | ---------- | --------- |
    | Claude Code 2.1.216 (est. recall) | 37.1%      | 23.0%     |
    | held-out apps (statement bytes)   | 18.1%      | 11.8%     |
    | minified wrangler                 | 25.6%      | 16.8%     |

  - **Builder fixes, each red/green:**
    - the literal table now holds only files a build contains: 3.06× smaller, recall unchanged;
    - entries go through the `exports` map: MCP SDK and nested-clients now produce modules;
    - one source file must map to one module per bundle: 1.05% of Bun module rows are dropped, 0 of esbuild's;
    - plus the version policy (`--policy year-all+minor`), exact pins (`--pinned`), and a symlink fix.
  - **Recommendation:**
    - Keep **agreement as the default** for anything that names code.
    - Use structural only where a wrong family is cheap. Its extra wrong names on a big index are real (vendored
      copies, pre-bundled "sponge" files, a few index-side mislabels), and the isolation veto does not catch them.
    - Build-side, two cheap fixes would make structural much safer:
      - drop unit rows from files the index's own builds show to be pre-bundled copies;
      - read the `/*! name vX */` banner a vendored copy usually keeps.

## Gate and neutrality

| check                                                                                                                              | result                                                         |
| ---------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `npm run check` on 47e3bd6d's tree (12 stages; this branch predates main's concurrency census)                                     | ALL 12 PASSED (e2e: 14 pairs, 1 known gap failing as declared) |
| warm neutrality vs `exp095-library-fingerprints`, 2.1.215→2.1.216, candidate leg with `--diagnostics --library-index <this index>` | see "Neutrality" below                                         |

## 1. Builder fixes (`crates/humanify-index`, commit 47e3bd6d)

Measured on the selection lane's `greedyix-60` list (505 roots, 1,572 versions, Bun slice, as-of policy), looked up
with the selection lane's design rule (`../selection/scripts/libix.py`) on the same 67 held-out apps.

|                                     | before (selection lane) | fix a (literals) | fixes a + b + c |
| ----------------------------------- | ----------------------- | ---------------- | --------------- |
| literal table                       | 94.9 MB                 | 31.0 MB          | 33.8 MB         |
| module table                        | 2.05 MB                 | 2.05 MB          | 2.04 MB         |
| unit table (new since this lane)    | —                       | 34.1 MB          | 37.8 MB         |
| literals + modules, gzip            | 17.5 MB                 | 5.4 MB           | 6.0 MB          |
| held-out recall (all library bytes) | 20.71%                  | 20.75%           | 20.83%          |
| of keyed (CommonJS) bytes           | 28.40%                  | 28.46%           | 28.57%          |
| per-app mean recall                 | 21.55%                  | 21.74%           | 21.83%          |
| wrong names (share of named bytes)  | 0.03% (14.7 KB)         | 0.03% (14.7 KB)  | 0.03% (16.7 KB) |
| wrong names on mixed modules        | 640 KB                  | 211 KB           | 217 KB          |

- **(a) Literal table = files a build contains.** A file is kept only if one of the configured slices' builds of
  that version maps code to it. With no slices, every file is kept (the lab tools' whole-tree table).
  - Nothing was lost: recall rose slightly, and wrong names on mixed modules fell 3×, because fewer stray files
    compete.
  - On the real index it kept 1.73 M files and skipped 4.63 M.
  - Module rows: identical except ~190 keys from modules that embed their own build path (`__dirname`-style code;
    such a key can never match an app's bundle).
- **(b) Entries through the `exports` map.**
  - The entries `require()` and `import()` the package and every subpath a Node app can load (conditions node /
    require / import / default). Wildcards are expanded over the package's files, up to 256 entries.
  - Two kinds of subpath are skipped:
    - one whose file the package does not ship. @modelcontextprotocol/sdk 1.32's `.` → `dist/cjs/index.js` is
      missing; the bundler would retry the failure as an external, and an external package name takes every
      subpath with it.
    - one that only browser or types conditions reach.
  - Results:
    - @modelcontextprotocol/sdk: 1.9.0 (wildcard `./*` only) 29 modules, 1.32.0/1.32.1 81 modules (was 0);
    - @aws-sdk/nested-clients 3.997.47: 5 submodule bundles in Bun (was 0). esbuild picks the `module` condition
      and hoists the ESM build, as real esbuild apps do.
  - The +2 KB of "wrong" names in the last column are truth errors, read by hand. The held-out bundle's own Bun map
    credits readable-stream / bl / ora modules to `ora/index.js`, and css-tree code to lru-cache's `index.min.js`;
    the index named the real package.
- **(c) One source file → one module per bundle.** A file the map credits to 2+ modules of one build labels none
  of them, and none of their units either.
  - greedyix-60: 350 files in 36 of 1,572 builds; 1,424 module rows and 6,059 unit rows dropped.
  - Real index: 3,233 files in 433 of 20,847 Bun builds; 10,757 module rows (1.05%) and 70,179 unit rows (0.28%)
    dropped. **esbuild: 0.**
  - Read by hand: jsdom 30's build credits css-tree's package.json and css-tree code to lru-cache's
    `index.min.js`.
- **Also:**
  - `--policy year-all+minor` (every release of the last 12 months, plus the latest patch of every older minor);
  - `--pinned <file>` (exact versions, no policy picks: what a bundle's readers declare);
  - CLI paths made absolute (a relative `--work` sent every bundle into the wrong directory);
  - a build stamp of recipe + entry text;
  - paths compared canonically (Bun writes map sources relative to the real directory, so a symlinked work dir
    once emptied the whole literal table);
  - recipe version `recipe/4`.

## 2. Package selection, re-priced, and the index built

**Cost model** (`scripts/price.py`, `scripts/fit_growth.py`):

- A package's cost is every row it owns in a 4,501-version **cost index**. That index was built by the fixed
  builder over the selection lane's greedy-20 ∪ download-30 ∪ greedyix-90 roots, under the as-of policy, both
  slices: 232 MB in 351 s.
- The cost is then scaled from as-of to year-all+minor by the release counts humanify-index's own `--dry-run`
  plans. Each extra release adds a share `a` of a release's rows, fitted on the density lane's 101 packages so
  totals match: a = 0.047 (modules, units) and 0.101 (literals); measured ratios 3.9× and 6.1×. Per package the
  model is rough (median error ±43–53%).
- A package not in the cost index costs its bundled code × 4.14, the median measured ratio.
- The unit tables are now the biggest part of an index (63% of the cost index).

**Curve** (greedy by training apps' bytes per cost; held-out = the 67 apps no selection saw; "covered" = held-out
library bytes whose package is a root in the list):

| projected size (roots' own rows, year-all+minor) | packages | held-out covered | gain per 100 MB |
| ------------------------------------------------ | -------- | ---------------- | --------------- |
| 50 MB                                            | 613      | 19.2%            |                 |
| 100 MB                                           | 876      | 22.7%            | +7              |
| 200 MB                                           | 1,450    | 32.7%            | +10             |
| **300 MB (chosen)**                              | 1,818    | 39.9%            | +7.2            |
| 400 MB                                           | 1,847    | 43.7%            | +3.8            |
| 600 MB                                           | 2,393    | 48.1%            | +2.2            |
| 800 MB                                           | 2,580    | 51.6%            | +1.8            |
| 1,200 MB                                         | 2,921    | 57.0%            | +1.4            |
| 1,600 MB                                         | 3,024    | 58.8%            | +0.5            |

**Why 300 MB.** Below it, each 100 MB buys 7–10 points of held-out coverage; above it, 4 points or less. Past
300 MB, half the next 100 MB is a single package. The build also stays at ~22,000 versions, about an hour on 24
cores.

**The index** (`final/ix/v2`, as-of 2026-10-11, Bun 1.4.0 and esbuild 0.28.2 slices, recipe/4):

- Packages:
  - the 1,818 greedy roots under `year-all+minor`;
  - the packages Claude Code's declared readers point to (2.1.216 / 2.1.198 / 2.1.86, via
    `discovery/proto/src/declared_all.rs`: the pipeline's own `scan_app_declarations` over every top-level
    statement). These are AWS clients and credential providers, @smithy/\*, @grpc/grpc-js, gaxios,
    google-auth-library, openid-client, sharp, @opentelemetry/api, highlight.js.
- Versions for the reader packages: every release of the last 12 months plus their declared pins (14, all built).
  - Why not the full policy: **AWS SDK v3 uses the minor as its release counter**, so "the latest patch of every
    older minor" is every release since 2020 (400–650 per package; 9,200 versions for the readers alone).
- 22,356 versions planned; 20,847 indexed. 1,509 failed to install, almost all npm `ETARGET`: AWS publishes a
  release and its dependencies seconds apart, so `--before=<publish time>` cannot resolve them. 2,512 of the 3,505
  reader pins are in.
- Tables:

  | table           | rows    | raw MB  | gzip MB         |
  | --------------- | ------- | ------- | --------------- |
  | literals        | 109,264 | 212.8   | 29.0            |
  | units.bun       | 621,827 | 96.9    | 19.7            |
  | units.esbuild   | 612,065 | 95.6    | 19.7            |
  | modules.bun     | 98,146  | 12.0    | 2.8             |
  | modules.esbuild | 103,378 | 12.6    | 2.9             |
  | manifest        | —       | 9.9     | 3.2             |
  | **total**       |         | **430** | **77 (tar.gz)** |

- Roles: 15,202 `minor`, 3,160 `recent`, 2,485 `pinned`.
- Index load in a run (one slice): 3.6 s with the literal table, 0.9 s for units and modules alone.
- Not in it:
  - @modelcontextprotocol/sdk (greedy position 2,776), @anthropic-ai/sdk, ink: beyond the cut, and no reader
    names them;
  - @aws-sdk/util-endpoint: npm view failed.

## 3. The claim rule on that index (in-run lookup: `exp095-core-discover`, the pipeline's `discover_in_bundle` + `libindex::lookup` with the run's rules)

"Index seeds" = statements the lookup itself claimed; "all claims" adds what the graph fixpoint grew from them and
from the declared readers. Structural = `AGREEMENT=0`; agreement = the run's default.

| test set (truth)                                             | rule       | recall                      | app code claimed                                                   | wrong family (share of named bytes)                                       |
| ------------------------------------------------------------ | ---------- | --------------------------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------- |
| Claude Code 2.1.216 (negatives lab: 620 labelled statements) | structural | **37.1%** (seeds 28.4%)     | 0 sampled units (2 tslib-helper traps before the veto, gone after) | 1 sampled unit: lodash for lodash-es (same code)                          |
|                                                              | agreement  | 23.0% (seeds 13.7%)         | 0                                                                  | 0                                                                         |
| Claude Code 2.1.198 / 2.1.86 (no labels; read by hand)       | structural | 462 / 387 KB of index seeds | 0 in 40 read                                                       | 0 fork-family claims; 1 debatable (confbox for its vendored jsonc-parser) |
|                                                              | agreement  | 265 / 212 KB of index seeds | —                                                                  | 0 fork-family claims                                                      |
| held-out apps, 67 Bun bundles (exact sourcemap)              | structural | **18.1%** (seeds 15.6%)     | vite's own pre-bundled deps only (197 KB of seeds)                 | seeds **12.5%**, all claims 15.5%                                         |
|                                                              | agreement  | 11.8% (seeds 9.3%)          | vite's own pre-bundled deps only (62 KB of seeds)                  | seeds **4.8%**, all claims 12.8%                                          |
| minified wrangler 3.114.0 (esbuild slice; two composed maps) | structural | **25.6%** (seeds 19.6%)     | 0                                                                  | seeds 0.8%, all claims 5.8%                                               |
|                                                              | agreement  | 16.8% (seeds 13.1%)         | 0                                                                  | seeds 0%, all claims 3.0%                                                 |

For comparison: the intermediate cost index (as-of policy, 1,512 roots, none of Claude Code's pinned versions) gave
33.1% / 21.3% on 2.1.216, and 21.4% / 11.2% of seeds on the held-out apps. Discovery-core's index C (Claude Code's
exact versions) gave 81.0% / 54.5%.

**Wrong names, read by hand** (`scripts/copies.py` checks whether the true package's installed files mention the
named package as a path or a string; then every remaining row was read):

- **Vendored copies (the index names the code's origin; the map can only name the package that shipped it):**
  - the biggest: zod inside @anthropic-ai/claude-agent-sdk, js-yaml and lodash inside mermaid, rxjs and
    urlpattern-polyfill inside puppeteer-core, fflate / tweetnacl / noble inside openpgp, esquery / ms /
    supports-color inside @sentry/server-utils, consola and signal-exit inside rolldown, exsolve inside unenv,
    zod-to-json-schema inside @ai-sdk/provider-utils, agent-base inside @sentry/node-core, ansi-styles vendored by
    chalk;
  - share of the sampled index-seed wrong-family bytes: structural 248 of 291 KB, agreement 135 of 142 KB.
- **The same code under another name:**
  - forks: `@inquirer/figures` / `figures`, `@isaacs/balanced-match` / `balanced-match`;
  - a renamed package: `mimic-fn` / `mimic-function`.
  - Structural only.
- **Truth errors.** The held-out bundle's Bun map credits code to the wrong file: follow-redirects and mime-db code
  to webpack-dev-server, marked to highlight.js.
- **Index-side wrong names, structural only, ~6 KB in the samples:**
  - `gtoken` and `postcss-selector-parser` on follow-redirects / mime-db code (index rows mislabelled by a Bun build
    the one-file check cannot see: the file was credited to one module, just the wrong one);
  - `yoctocolors` on ora;
  - `camelcase` on ansi-align;
  - TypeScript's `__values` / `__asyncValues` helpers named `zod` and `web-streams-polyfill`.
  - **Agreement: ~3 KB** (agent-base in Sentry, yoctocolors on ora, marked/highlight.js — the last a truth error).
- **Graph growth, not the index:** the DOWN rule names a dependency after its parent (cli-table3 → @colors/colors
  and emoji-regex, d3-delaunay → delaunator, boxen → camelcase). This is most of the gap between "seeds" and "all
  claims" under both rules.
- **A declared-reader bug:** a license banner read as the package `@license` (21 KB of dompurify).
- **Pre-bundled ("sponge") files, on the intermediate index:** prettier 2's pre-bundled `index.js` gave `prettier`
  for wrangler's resolve / is-core-module / path-parse code. Specificity cannot reject it in a one-module minified
  bundle. On the real index the same statements are named `resolve` (now indexed itself), but the sponge rows are
  still there.

**The earlier scale lab's warning holds.** Structural-only wrong names grow with index size. Here the growth is
mostly vendored copies, which a statement hash cannot tell apart from the original. The agreement rule's literal
check rejects most of them: a vendored copy's statements carry the vendoring package's few literals, and small
literal sets rarely clear the 20-bit / 90% bar.

## 4. Neutrality

WARM, 2.1.215 → 2.1.216, against `exp095-library-fingerprints` (608e9ee5). The candidate is this branch's humanify
(64a54c30) run with `--diagnostics --library-index <this index>`, so discovery and the index lookup are ON:

| differing files | differing lines | cache writes: candidate / baseline | exit codes |
| --------------- | --------------- | ---------------------------------- | ---------- |
| 0               | 0               | 0 / **0**                          | 0 / 0      |

**NEUTRAL.** The run's `libraryDiscovery` section records this index's id and matches the lab tool's agreement run
on 2.1.216 exactly: index seeds 257 (+9 from the queue lookup), 471,858 B seeded, 93,945 B grown, 36,219 B vetoed.

## Re-run

```bash
export PATH="$HOME/.cargo/bin:$PATH"; S=experiments/095-library-fingerprints/indexv2/scripts; D=/work/exp095-index-v2-data
# fix a, a+b+c on greedyix-60 (selection lane's re-run with these binaries): SEL=$D/selA|selABC ../selection/scripts/build-index.sh, heldout.py eval
python3 -I $S/fit_growth.py                                             # growth.json
DATA=$D/selABC $S/build-index.sh $D/selABC/lists/cost-union.txt cost-union 48-63      # the cost index (as-of)
python3 -I $S/packuments.py $D/registry $D/candidates.txt --workers 4
humanify-index build --packages $D/candidates-have.txt --policy as-of|year-all+minor --dry-run ...  # plan-cand-*.txt
python3 -I $S/price.py $D/selABC/ix/cost-union $D/plan-cand-as-of.txt $D/plan-cand-year-all+minor.txt 50 100 ... 1600
DATA=$D/final POLICY=year-all+minor PINNED=$D/final/pins.txt $S/build-index.sh $D/final/base.txt v2 40-63
# claim rule (proto: experiments/095-library-fingerprints/discovery/proto, exp095-core-discover / exp095-declared-all)
INDEX=$D/final/ix/v2 AGREEMENT=0 exp095-core-discover <cc bundle> <report.json>        # and without AGREEMENT=0
python3 ../discovery/index_score.py <edges.tsv> <reports...>                            # 2.1.216 labels
RULES="struct agree" $S/run_heldout.sh $D/final/ix/v2 <out> 40-47 && python3 -I $S/agg_heldout.py <out>/heldout
<scale venv python> -I $S/score_claims.py cli.min.js cli.min.js.map units.tsv out.json - <reports> --inner cli.js.map 1
python3 -I $S/copies.py <out>/heldout/summary.json
```

## Files (bulk data in `/work/exp095-index-v2-data/`, not in git)

`final/ix/v2` (the index), `final/store` (20,847 × 3 shards), `selA`, `selABC` (fix measurements, cost index),
`registry` (3,366 packuments), `price.json`, `growth.json`, `lists/`, `cc/declared-*.jsonl`, `eval/` (reports,
scores, summaries per test set), `gate.log`, `neutrality.log`.

## Unknown / not done

- Claude Code's own `src/` libraries that no reader names (MCP SDK, @anthropic-ai/sdk, ink, ...) are not in the
  index. The index C numbers show what they are worth; a reader for them (or a pinned list) is the next step.
- The sponge filter for unit rows (prettier 2's `index.js` kind) and a vendored-copy rule are not built.
- AWS `ETARGET` installs: 993 reader pins missing. Retrying with `--before` a few minutes later would recover most.
- The cost model's per-package error is large (±43–53%). The real index is 430 MB against 300 MB projected,
  because dependencies, pins and literals are not in the roots' projected cost.
- Held-out "covered" counts a package at any version; the held-out apps resolve today's versions, the policy's
  best case.
- Structural results were taken on the index before the literal table was rebuilt (same unit and module bytes;
  the structural rule does not read literals).
