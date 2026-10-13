# exp095 lane "selection": which npm packages to put in the library index

## STATUS (2026-10-12)

- **Update 2026-10-13 (rule 9):** the builder fixes recommended below (bundled-only literal table, `exports`
  entries, one-file-one-module check) and the `year-all+minor` policy landed in `../indexv2/` (commit 47e3bd6d),
  which also built a real index and re-tested the claim rule. Read that README for current numbers.

- **State.** DONE (lab experiment). One builder fix landed in `crates/humanify-index` (quadratic sourcemap offsets,
  below); nothing in the pipeline changed.
- **Verdict, totals first.**
  - **Corpus:** 338 bundled apps (166 Electron apps, 170 npm CLIs, 2 open-source agent CLIs), 1,577 MB of minified
    library code from 7,980 distinct packages. 67 apps (20%) were held out and never informed a selection.
  - **Choosing packages by what apps bundle beats download rank, once the cost is the index's real cost.** At an
    equal real index size, the app-closure greedy index recognises **20.7% vs ~17.5%** of held-out apps' library
    bytes (98 MB index) and **22.8% vs ~20.4%** (148 MB, download extrapolated from 139 MB), with the same wrong-name rate (0.03–0.06% of named bytes,
    all copies or sourcemap errors on hand check). It also names 2.6× more of Claude Code's vendor factories
    (1.36 MB vs 0.52 MB at ~16 MB gzipped).
  - **Choosing by bundled CODE bytes does not.** The first greedy, with cost = the package's bundled bytes, looked 5–8
    points better on paper (42% vs 35% of held-out library bytes at 20 MB of code) but its real indexes were no
    better than download rank per real MB. The real index is ~97% literal rows from EVERY source file of every
    installed package at up to 4 versions, so an owner's index cost is 0.9–11.7× its bundled code (p10–p90;
    caniuse-lite: 43 MB of index for 0.3 MB of bundled code).
  - **Neither ranking reaches Claude Code's packages at a usable size.** @aws-sdk/client-bedrock-runtime,
    @growthbook/growthbook and @anthropic-ai/bedrock-sdk are bundled by 0 of 338 apps; ink by 3 (greedy position
    2,324, ~415 MB of index); yoga-layout by 2. An app's own unusual dependencies have to come from its declared
    readers (pinned versions), not from any popularity list.
  - **Two builder problems found.** (1) `utf16_to_byte_offsets` was quadratic on one-line minified bundles (a
    24 MB app bundle never finished; fixed, red test 503 s → green). (2) **Bun 1.4.0 sourcemaps misattribute whole
    modules** in large bundles: 5.5% of the held-out bundles' keyed library bytes sit in files the map credits to
    2+ different modules (9 of 67 bundles above 5%). humanify-index labels its own rows with the same attribution.

## How the corpus was chosen (`scripts/corpus.py`, `scripts/agents.py`)

| stratum                | rule                                                                                                                                                                                                                                                                                                                                                                                                                                     | apps tried → bundled |
| ---------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------- |
| Electron               | every app in the electron/apps catalog (614 apps) whose repo is on GitHub; stars from ecosyste.ms; not archived, not a fork; `electron` in its root package.json; ≥ 3 runtime library dependencies. Runtime dependencies = `dependencies` minus tooling, plus `devDependencies` minus tooling when the app has a bundler in devDependencies (bundled Electron apps list renderer libraries there). Top 200 by stars asked; 169 qualified | 169 → 166            |
| npm CLI, download rank | npm-high-impact's top-download list (the list exp095's other lanes used): latest manifest has a `bin` and ≥ 5 dependencies (an app with its own tree); top 105 by rank                                                                                                                                                                                                                                                                   | 105 → 89             |
| npm CLI, product       | a hand list of 129 product/platform CLIs (vercel, netlify-cli, firebase-tools, wrangler, @shopify/cli, prisma, n8n, …) with a bin and ≥ 3 dependencies. 26 are pre-bundled (≤ 2 deps: @google/gemini-cli, opencode, amp, @openai/codex, @github/copilot, aws-cdk, …) and were dropped: their libraries are invisible from npm                                                                                                            | 84 → 81              |
| agent                  | the two pre-bundled agent CLIs with open-source repos: the union of the runtime `dependencies` of their `packages/cli` and `packages/core` package.json (google-gemini/gemini-cli, QwenLM/qwen-code)                                                                                                                                                                                                                                     | 2 → 2                |

All network reads are cached on disk (`$SEL/cache`, one file per URL) and back off on 429/5xx.

## How a package is weighted (`scripts/closure.py`)

Each app is installed (`npm install --ignore-scripts --omit=dev --legacy-peer-deps`; Electron/agent apps at their
declared ranges resolved today, CLIs at their latest version) and bundled with **esbuild 0.28.2 `--bundle --minify
--platform=node --metafile`**, `process.env.NODE_ENV="production"`. The entry requires every runtime dependency
(Electron/agent) or the CLI's own bin file(s) — what a bundler pulls in from the real entry. **Weight = the
metafile's `bytesInOutput`** per input file, summed per owning package (the package under the last
`node_modules/`). The CLI's own files are app code, never library bytes. Unresolvable specifiers are retried as
externals (their bytes are lost); 22 of 360 apps failed to install or bundle, or bundled no library code.

## Results

### 1. The model: coverage curve vs index size (`scripts/greedy.py`, `$SEL/greedy.json`)

Share of the corpus's library bytes that belong to an indexed package, as a function of the summed bundled code
of the chosen packages ("code MB", cost = the median bytes a bundling app pulls in). Greedy = packages sorted by
training-apps bytes covered per code byte. Download = npm-high-impact rank order (the scale lane's skip rule).
Held-out = the 67 apps no selection saw; "per app" = the mean of each app's covered share.

| code MB | greedy, held-out (bytes / per app) | download, held-out | greedy, training apps | download, training |
| ------- | ---------------------------------- | ------------------ | --------------------- | ------------------ |
| 1       | 8.3% / 14.6%                       | 7.8% / 13.9%       | 8.8%                  | 8.6%               |
| 5       | 21.7% / 36.9%                      | 17.3% / 33.0%      | 24.2%                 | 19.9%              |
| 10      | 28.3% / 44.5%                      | 23.2% / 38.0%      | 31.0%                 | 24.4%              |
| 20      | 42.4% / 56.2%                      | 34.6% / 45.7%      | 42.2%                 | 35.6%              |
| 50      | 52.4% / 64.4%                      | 50.2% / 62.0%      | 57.8%                 | 49.5%              |
| 100     | 62.5% / 71.3%                      | 55.8% / 68.3%      | 74.5%                 | 59.7%              |
| 200     | 72.9% / 81.4%                      | 65.8% / 74.4%      | 88.9%                 | 74.1%              |
| 400     | 76.5% / 84.4%                      | 73.9% / 81.3%      | 100%                  | 79.6%              |

The training columns overstate greedy (it was fitted there); the held-out columns are the claim. This curve is a
**ceiling**: it counts a package's bytes as covered if the package is in the index at any version, and it prices a
package at its bundled code. Section 2 measures both assumptions; the second one is wrong.

### 2. Real indexes, built with `humanify-index`, looked up with the design rule (`scripts/heldout.py`)

Each index: `humanify-index build --as-of 2026-10-11 --slices bun` (Bun 1.4.0) over the list's packages as roots,
the as-of policy's versions (latest, previous patch, major-1, major-2), their dependencies included as owners.
Lookup (`scripts/libix.py`): the scale lane's design rule — whole-module content key held by one family AND a
literal claim (specific files, 20 bits, cover 0.9, margin 10) for the same family; stop-lists; families.
Queries: the held-out apps rebuilt with **Bun 1.4.0 `--minify --sourcemap`** from the same entry; each CommonJS
module fingerprinted by the pipeline's own `bundle_content_keys` + `libindex::literals` (`fpq/`); truth = owner by
the build's sourcemap (≥ 90% of the module's segments from one file). Recall denominator = every byte of the bundle
the sourcemap credits to a dependency, so ESM-hoisted and lazy-ESM code (which the index cannot key) counts as
missed; "of keyed" = of the CommonJS-module bytes only. Wrong = named a package outside the true one's family, as a
share of named library bytes. Claude Code = the 1,647 CJS factories of 2.1.216.

| index                        | roots | package versions | owners | raw MB | gzip MB | held-out bytes whose package is in it | held-out recall | of keyed | per-app recall | wrong | CC 2.1.216 named |
| ---------------------------- | ----- | ---------------- | ------ | ------ | ------- | ------------------------------------- | --------------- | -------- | -------------- | ----- | ---------------- |
| download-20                  | 460   | 1,438            | 888    | 85.7   | 15.0    | 38.6%                                 | 16.8%           | 23.1%    | 19.1%          | 0.05% | 523 KB / 246     |
| download-30                  | 822   | 2,458            | 1,576  | 138.9  | 22.9    | 44.0%                                 | 19.9%           | 27.3%    | 20.9%          | 0.06% | 843 KB / 348     |
| greedy-13 (code cost)        | 684   | 2,122            | 1,434  | 107.1  | 19.0    | 48.8%                                 | 16.5%           | 22.7%    | 21.9%          | 0.08% | 799 KB / 352     |
| greedy-20 (code cost)        | 875   | 2,674            | 1,632  | 123.7  | 21.8    | 49.8%                                 | 18.3%           | 25.1%    | 24.1%          | 0.07% | 1,423 KB / 451   |
| **greedyix-60 (index cost)** | 505   | 1,572            | 1,401  | 97.7   | 17.4    | 48.0%                                 | **20.7%**       | 28.4%    | 21.6%          | 0.03% | 1,362 KB / 418   |
| **greedyix-90 (index cost)** | 852   | 2,613            | 1,910  | 148.2  | 25.7    | 51.5%                                 | **22.8%**       | 31.2%    | 24.4%          | 0.06% | 1,521 KB / 523   |

- **Equal-size comparison.** Interpolating the download line at the greedyix sizes: 97.7 MB → 17.5%, 148.2 MB →
  ~20.4% (extrapolated: download-30 is 138.9 MB). greedyix is +3.2 and +2.4 points (+18% / +12% relative); per-app +2 to +3.
- **Index cost** (`scripts/greedy_ix.py`): c(p) = every row owned by p in the greedy-20 and download-30 indexes
  (deduplicated), else bundled code × 3.4 (the measured median ratio). Bundled code predicts it badly
  (p10–p90 0.9–11.7×). Only ~30% of the literal table's bytes are files that appear as a CommonJS module in any of
  the index's own bundles (greedy-20: 35.1 of 116.3 MB; download-20: 26.3 of 79.9 MB) — the rest are files no bundle
  pulled in (other entry points, CLI scripts, data files, tests that ship). **Restricting the literal shard to the
  files a bundle actually contains would cut the index ~2–3×** (not built; a builder change).
- **Wrong names, hand-checked (every lib-class example).** core-js-pure named core-js (same code, 20 KB),
  punycode.js named punycode (same code), rrweb-cssom named @acemir/cssom (fork). Every other "wrong" one was a
  sourcemap error, not an index error: e.g. bdash's module attributed to whatwg-url `lib/url-state-machine.js` is
  tr46's index (`TRANSITIONAL`/`NONTRANSITIONAL`, `NFC`) and the index named it tr46; dopamine's
  "node-taglib-sharp `dist/picture.js`" holds 8 different uuid modules. Mixed modules (no file owns ≥ 90% of the
  module: 32 MB of the held-out bundles' keyed bytes) are reported separately in the results files and not counted.
- **Claude Code 2.1.216**: 0 names contradict a declared identity; 1 disagrees with the scale lane's reference
  (`@img/colour` vs `@so-ric/colorspace`: both are pre-bundled copies of the same color code). Of 172 names neither
  reference covers (greedy-20), the 37 that name a different family than today's LLM name were read by their
  strings: all consistent with the indexed package (e.g. `ms` for today's `date-fns` — "val is not a non-empty
  string or a valid number"; `@xmldom/xmldom` for today's `sax`; ajv's keyword modules for today's
  `ajv-keywords-*`).

### 3. Where the packages Claude Code needs land (`$SEL/greedy.json` "watch")

| package                         | apps (of 338) | download rank | median bundled KB | greedy (code cost): position / code MB | download: position / code MB | greedyix: position / index MB |
| ------------------------------- | ------------- | ------------- | ----------------- | -------------------------------------- | ---------------------------- | ----------------------------- |
| react                           | 78            | 441           | 6.7               | 104 / 1.4                              | 426 / 17.0                   | 2,312 / 414                   |
| react-dom                       | 73            | 476           | 120.7             | 132 / 2.0                              | 461 / 20.0                   | 1,448 / 193                   |
| zod                             | 34            | 423           | 360.3             | 413 / 6.6                              | 409 / 16.9                   | 380 / 42                      |
| @modelcontextprotocol/sdk       | 5             | 1,196         | 51.2              | 2,003 / 85.7                           | 1,135 / 34.1                 | 1,420 / 186                   |
| @anthropic-ai/sdk               | 2             | 1,810         | 77.5              | 2,590 / 121.1                          | 1,724 / 47.1                 | 2,503 / 459                   |
| ink                             | 3             | 4,570         | 49.1              | 2,449 / 110.0                          | 4,234 / 265.4                | 2,324 / 415                   |
| react-reconciler                | 3             | 6,060         | 119.4             | 2,508 / 113.9                          | 5,551 / 398.3                | 2,435 / 434                   |
| yoga-layout                     | 2             | 3,759         | 129.0             | 3,489 / 167.0                          | 3,508 / 189.9                | 3,323 / 616                   |
| @azure/identity                 | 3             | 3,348         | 58.1              | 3,596 / 170.3                          | 3,133 / 153.0                | 3,470 / 627                   |
| @aws-sdk/client-bedrock-runtime | 0             | 4,064         | —                 | not in any app                         | 3,785 / 228.1                | —                             |
| @growthbook/growthbook          | 0             | 10,253        | —                 | not in any app                         | beyond the 8,000 ranked      | —                             |
| @anthropic-ai/bedrock-sdk       | 0             | 12,305        | —                 | not in any app                         | beyond the 8,000 ranked      | —                             |

react is where the corpus helps most (78 apps bundle it; download rank buries it behind build tools). The
Claude-Code-specific packages are rare in the wider app population, so any corpus ranking places them in the
long tail. greedyix moves react back (its package ships 658 KB of indexable source for 6.7 KB a bundle uses).

## Recommendation

1. Build the index **base** from an app-closure greedy over a broad bundled-app corpus, priced by the package's
   **real index cost**, and refresh the cost from the previous index build (it is a by-product).
2. Add **pinned versions from the readers** of the bundles being processed (the design's builder note) for
   everything the base misses; for Claude Code that is most of its `src/` libraries.
3. **Shrink the literal shard to bundled files** (the sources of the shard's own bundles' sourcemaps) before
   growing the package list: it is the cheapest 2–3× in the index.
4. **Make the builder check its attribution** (one source file → one module per bundle; refuse or relabel spans
   whose file is credited to several modules) — Bun 1.4.0's maps are wrong for some modules in large bundles.

## Re-run

```bash
export SEL=/work/exp095-library-fingerprints/selection; S=experiments/095-library-fingerprints/selection/scripts
python3 -I $S/corpus.py && python3 -I $S/agents.py                 # corpus.json (network, cached)
python3 -I $S/closure.py --jobs 12                                    # apps/*/closure.json (npm + esbuild)
python3 -I $S/greedy.py                                               # greedy.json, selection.json (hold-out seed 95)
(cd $S/../fpq && CARGO_TARGET_DIR=$SEL/fpq-target cargo build --release)
python3 -I $S/heldout.py bundles                                      # held-out Bun bundles + fpq
python3 -I $S/heldout.py lists 13 20 30                               # lists/{greedy,download}-<MB>.txt
$S/build-index.sh $SEL/lists/greedy-20.0.txt greedy-20 36-43          # likewise download-20, download-30, greedy-13
python3 -I $S/greedy_ix.py 60 90 && $S/build-index.sh $SEL/lists/greedyix-60.txt greedyix-60 36-43   # (and 90)
V=/work/exp095-library-fingerprints/scale/.venv/bin/python            # numpy + scipy
$V -I $S/heldout.py eval greedy-20 download-20 ... && $V -I $S/heldout.py cc greedy-20 ...
```

`owner-index-bytes.json` (greedy_ix.py's cost) is computed by greedy_ix.py from the greedy-20 and download-30
indexes when missing: each owner's rows, deduplicated across the two, summed as JSONL bytes.

## Files (bulk data under `/work/exp095-library-fingerprints/selection/`, not in git)

- `corpus.json`, `corpus-log.json` (every dropped app and why), `apps/<slug>/closure.json` + `meta.json`.
- `greedy.json` (curves, watch list, greedy top 300), `selection.json` (orders, costs, hold-out),
  `greedy-ix.json`, `owner-index-bytes.json`.
- `ix/<name>/` the six indexes; `results/heldout-<name>.json`, `results/cc-<name>.json`.
- `heldout/<slug>/` the held-out Bun bundles, `q.jsonl` (fpq rows) and `q.jsonl.owners.json` (bytes per owner).

## Unknown / not done

- Index sizes above are uncompressed JSONL and gzip; no other compression was tried.
- Only the Bun slice was built for this lane; an esbuild slice roughly doubles module rows (scale lane).
- The corpus is weighted by bytes; a few large Electron apps (iptvnator 40 MB, movieprint 37 MB) carry a lot of
  weight. The per-app column is the unweighted view.
- 4 CLI closures externalized the CLI's own package after a non-literal require (vitest, ngrok, html-minifier,
  @cloudflare/vite-plugin); fixed in closure.py afterwards but not re-run, to keep the hold-out split fixed.
- Versions: the held-out apps resolve their ranges today, so their versions are mostly the latest — the as-of
  policy's best case. Version density is the density lane's subject.
