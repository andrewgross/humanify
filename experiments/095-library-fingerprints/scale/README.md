# exp095 lane "scale": fingerprint precision at shipping index size (SYNTHESIS §5 items 1, 3, 5)

## STATUS (2026-10-11)

- **State.** DONE (lab experiment). Nothing in the humanify repo was touched or committed.
- **Verdict.**

  - **Every single evidence family gets worse with index size, and two of the §4 rules break outright at top-2,000.**
    The literal rule (any best file) names 6.3% of foreign spans and its recall falls from 64% to 30%. Unit votes
    under §4 (30 nodes, 3 content tokens, k ≥ 2) name 5.7% of them. The cause is the same in both: packages that
    pre-bundle other packages (playwright-core, vite, next, prettier, rolldown, webpack…). They only appear at scale.
    The fix that works is a **specificity rule**: the matching index file or module may be at most about 3–4× the
    query. It cuts literal false names 854 → 162 and unit-vote false names 673 → 86, and literal recall comes back
    to 53%.
  - **Two-family agreement (structural AND literal) is the precision bet, and it pays.** At top-2,000 roots (2,521
    packages in the Bun slice) it puts a wrong name, outside the true package's family, on 29 of 13,515 foreign spans
    (0.21%), against 129 for structural alone and 162 for literal alone. Another 35 names are family-correct. 27 of the 29 are real copies, forks or generated code. Recall is 51% vs 72.5% for
    structural alone, about the same as literal alone. It named 0 Claude Code app groups or functions, and 0 of the
    negatives lane's 511 randomly sampled app statements.
  - **On Claude Code 2.1.216 vendor files** the combined detector names 710 of 1,647 factories:

    - 272 agree with today's name and 67 are in the same family;
    - 251 differ from today's names, all of them LLM-chosen;
    - 120 name a file that today has a `lib_<hash>` fallback name;
    - 0 contradict the 174 declared identities.

    By hand, 39 of a random 40 renames are the right package. The 40th is node-forge's vendored jsbn.

- **Read first:** "Results" below, then `results/combine.json` (the two-family table) and
  `results/cc-vendor-design-R2000.tsv` (the Claude Code rename list).
- **Scale reached.** All 2,000 roots planned (npm top-download ranks 1–2,542 after hold-outs), at up to 4 versions
  each: 6,191 versions, 24,618 builds, 1.29 M labelled spans, 2,853 packages including dependencies.

## What was built

|                   |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| roots             | npm top-download list (`npm-high-impact`, the list exp095's fresh corpus used). Dropped: 146 type-only / platform-binary packages, and **396 held-out roots**, the union of the three foreign corpora (493 names: flab 83, neg 75, fresh 400), so those corpora stay foreign. Result: 2,000 roots, last one at rank 2,542                                                                                                                                                                                                                                                      |
| versions per root | latest; the previous patch of latest; the latest of each of the 2 previous majors (for 0.x the minor plays the major). 6,191 versions in total                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| builds            | each version is installed alone (`npm install --ignore-scripts`, retried with `--legacy-peer-deps`). Input sourcemap comments are stripped. Each version is built 4 ways: Bun 1.4.0 `--minify` and esbuild 0.28.2 `--minify`, each through a `require` entry and an `import()` entry (lab `entries.mjs` app stub), CJS output, node target. An unresolvable specifier is retried as `--external`; top-level await is retried with ESM output                                                                                                                                   |
| build failures    | installs 2 / 6,191. Builds: Bun 44 / 12,378, esbuild 94 / 12,378. All 4 builds failed for 8 versions (`.node` / `.cs` loaders, tsx's own sources). 1,165 builds needed an external. 1 labelling error                                                                                                                                                                                                                                                                                                                                                                          |
| labels            | the lab's sourcemap labeller (`scripts/label.mjs`, a copy generalised to other corpus roots)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| index content     | every pure library span whose package is not held out. Scale R = the spans of roots ranked ≤ R (with their dependencies)                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| kinds             | (a) whole-module normalised hash: humanify MatchKey via `exact-module-hash/fphash`, `normM`/`normV`/`normLen`. (b) sub-tree units: `subtree-hashes/scripts/canon.mjs`, loose mode, names masked, plus a count of content tokens. (c) B5 5-token shingles: `ngram-minhash/lex.py`. (d) literals from npm **source**: `immutable-literals/scripts/lits.mjs`, source mode, every package@version in the installs, sponge regex applied                                                                                                                                            |
| foreign corpora   | flab (subtree-hashes/verify, 83 roots), neg (immutable-literals/verify, 75 roots; relabelled here as spans, `q/neg`), fresh (exact-module-hash/verify, 400 roots), all Bun 1.4.0 `--minify`. **Foreign set:** their CJS-factory and lazy-ESM library spans whose package is in NO index at the largest scale (either bundler). That is 13,515 spans from 597 packages (flab 3,759 / neg 4,163 / fresh 5,593), the same at every scale. Spans of packages that ARE indexed, 7,707 from 1,279 package instances, were **moved out of the foreign set** and are used as positives |
| positives         | lab Bun 1.4.0 / esbuild min spans (7,480 Bun), the moved-out dependency spans, and Claude Code's declared positives (212), restricted to packages in the index at that scale. "exactVer" = that package@version is indexed                                                                                                                                                                                                                                                                                                                                                     |
| app negatives     | lab/realworld cc-2.1.216 negatives (100 groups) and negative-functions (222), plus the negatives lane's `labels.jsonl` + `traps.jsonl` (903 units)                                                                                                                                                                                                                                                                                                                                                                                                                             |

## Results

### 1. Index size per bundler family and kind (measured: compressed files under `data/shipped/R<R>/`)

Rows/MB per kind. Distinct minified library code is counted as the largest instance per (package, version, file).

| roots R | pkgs  | code MB bun/esb | WM rows / MB (bun) | units ≥30n ≥3c rows / MB (bun) | units ≥20n (bun)             | B5 entries / raw MB (bun) | literals (shared by both)                | total without B5 / with raw B5 |
| ------- | ----- | --------------- | ------------------ | ------------------------------ | ---------------------------- | ------------------------- | ---------------------------------------- | ------------------------------ |
| 125     | 248   | 7.8 / 9.1       | 1,559 / 0.03       | 11,360 / 0.10 (1.42 per KB)    | 14,962 / 0.13 (1.87 per KB)  | 2,000 / 6.9               | 4,967 files, 113 k literals / 1.0 MB     | 1.3 / 16.6 MB                  |
| 250     | 489   | 48.8 / 48.4     | 6,122 / 0.12       | 61,658 / 0.50 (1.23 per KB)    | 84,136 / 0.67                | 8,137 / 30.1              | 11,937 files / 3.1 MB                    | 4.4 / 65.6 MB                  |
| 500     | 763   | 70.4 / 71.3     | 11,834 / 0.23      | 88,806 / 0.73 (1.23 per KB)    | 119,682 / 0.97               | 20,277 / 40.9             | 26,923 files / 4.8 MB                    | 6.7 / 90.7 MB                  |
| 1000    | 1,396 | 112.9 / 114.6   | 21,956 / 0.43      | 148,741 / 1.24 (1.29 per KB)   | 195,667 / 1.60               | 35,371 / 59.9             | 49,846 files, 481 k literals / 7.3 MB    | 10.7 / 133.5 MB                |
| 2000    | 2,561 | 246.7 / 259.3   | 44,041 / 0.85      | 289,217 / 2.36 (1.15 per KB)   | 376,140 / 3.00 (1.49 per KB) | 68,323 / 118.8            | 101,705 files, 1.08 M literals / 17.7 MB | 24.1 / 270.2 MB                |

- **esbuild slice:** about the same size as Bun. At R = 2000: WM 0.85 MB, units 2.39 MB, B5 127 MB.
- **Whole-module rows per KB:** 0.12–0.19.
- **Without B5, the index is small.** At top-2,000 roots and 4 versions per root, WM + units + literals for both
  bundler families come to **24 MB**.
- **Raw B5 shingle sets are the size problem.** They total 246 MB for both families. Shipping B5 needs MinHash or
  sub-sampling, which was not measured here: 68 k entries × 128 × 4 B ≈ 35 MB per family.
- These are measurements of what was built. The top-1,000 point is the measured row R = 1000; the SYNTHESIS's
  "5–30 MB" estimate holds if B5 is excluded.

### 2. False names on the fixed foreign set as the index grows (Bun slice, 13,515 foreign spans)

Each cell: false names that are not even family-correct (rate), with recall on that scale's positives in brackets.
Family-correct copies are excluded from the count (all names in `results/growth.json`).

| detector                                  | R=125 (246 pkgs)  | R=250 (486)       | R=500 (753)       | R=1000 (1,383)    | R=2000 (2,521)           |
| ----------------------------------------- | ----------------- | ----------------- | ----------------- | ----------------- | ------------------------ |
| UV k=1, ≥20 nodes (original rule)         | 716 (5.30%) [61%] | 521 (3.85%) [80%] | 533 (3.94%) [78%] | 608 (4.50%) [72%] | **1,726 (12.77%)** [69%] |
| UV §4: k=2, ≥30 nodes, ≥3 content         | 474 (3.51%) [45%] | 155 (1.15%) [46%] | 160 (1.18%) [45%] | 221 (1.64%) [38%] | 769 (5.69%) [38%]        |
| UV §4 + stop-lists + families             | 327 (2.42%)       | 149 (1.10%)       | 157 (1.16%)       | 216 (1.60%)       | 673 (4.98%) [38%]        |
| UV §4 + stop + families + **specificity** | 127 (0.94%) [43%] | 43 (0.32%) [45%]  | 56 (0.41%) [44%]  | 65 (0.48%) [38%]  | 86 (0.64%) [38%]         |
| WM exact module, no guards                | 6 (0.04%)         | 9 (0.07%)         | 18 (0.13%)        | 27 (0.20%)        | 74 (0.55%) [65%]         |
| WM exact module + guards (§4)             | 6 (0.04%) [55%]   | 9 (0.07%) [73%]   | 18 (0.13%) [63%]  | 23 (0.17%) [60%]  | 39 (0.29%) [64%]         |
| JC J≥0.7, no filter                       | 32 (0.24%) [68%]  | 93 (0.69%) [88%]  | 72 (0.53%) [92%]  | 273 (2.02%) [91%] | 239 (1.77%) [91%]        |
| JC J≥0.7, filter d=2 (§4)                 | 12 (0.09%) [53%]  | 13 (0.10%) [70%]  | 17 (0.13%) [68%]  | 31 (0.23%) [57%]  | 34 (0.25%) [57%]         |
| LT §4 rule (any best file)                | 35 (0.26%) [64%]  | 215 (1.59%) [52%] | 311 (2.30%) [51%] | 453 (3.35%) [39%] | **854 (6.32%)** [30%]    |
| LT §4 + stop-lists + **specific files**   | 25 (0.18%) [66%]  | 43 (0.32%) [69%]  | 41 (0.30%) [62%]  | 114 (0.84%) [53%] | 162 (1.20%) [53%]        |
| S = structural consensus (WM/JC/UV)       | 134 (0.99%) [63%] | 54 (0.40%) [83%]  | 76 (0.56%) [78%]  | 102 (0.75%) [70%] | 129 (0.95%) [73%]        |
| **S AND L (two-family agreement)**        | 11 (0.08%) [45%]  | 22 (0.16%) [67%]  | 27 (0.20%) [61%]  | 34 (0.25%) [52%]  | **29 (0.21%) [51%]**     |
| S OR L                                    | 148 (1.10%) [84%] | 75 (0.55%) [85%]  | 90 (0.67%) [79%]  | 182 (1.35%) [71%] | 256 (1.89%) [74%]        |
| WM alone OR (S AND L)                     | 13 (0.10%) [59%]  | 25 (0.18%) [81%]  | 37 (0.27%) [72%]  | 50 (0.37%) [66%]  | 59 (0.44%) [68%]         |

- **What drives growth with size.**
  - Unfiltered or unguarded rules grow 3–25× from 246 to 2,521 packages. The jump between 1,383 and 2,521 packages
    is driven by pre-bundling packages entering the index.
  - The guarded single families grow 2–4×.
  - The agreement rule stays flat at 0.2% from 753 packages up.
- **Comparison with the original 176 / 347-package results** (82-package corpus only):
  - **UV k=1, ≥20 nodes.** Then: 193 of 4,562 spans named, against the lab + rwlab index. Here, against the
    3,759 spans still foreign in that corpus: 119 at 246 packages, 136 at 486, 140 at 753, 200 at 1,383 and
    **780 at 2,521**.
  - **JC with d=2:** 5 → 25.
  - **Literal rule:** 14 → 329.
- **By corpus at R=2000** (named, with non-family in brackets, out of the corpus's foreign spans).
  - Two-family agreement: flab 28 (13) / 3,759; neg 14 (7) / 4,163; fresh 22 (9) / 5,593.
  - Structural consensus: 64 (41), 50 (35), 86 (53).
  - Literal: 105 (91), 64 (39), 57 (32).

### 3. Whole-module hash collisions (Bun slice, distinct normM hashes held by 2+ packages)

| normLen band | R=250 | R=1000 | R=2000 (hashes) | R=2000, 2+ families |
| ------------ | ----- | ------ | --------------- | ------------------- |
| 400–600      | 0.00% | 4.01%  | 4.20% (809)     | 3.96%               |
| 600–800      | 0.00% | 2.20%  | 4.35% (919)     | 3.48%               |
| 800–1,600    | 1.36% | 2.58%  | 3.06% (3,753)   | 2.42%               |
| 1,600+       | 0.25% | 0.72%  | 0.95% (35,099)  | 0.76%               |

- In 32 / 34 / 106 / 323 of these hashes (by band) the strings also agree, so the exact-string check (`normV`)
  rejects almost nothing.
- These rates are lower than the 1,221-package pool's 5–12%: that pool counted core-js and core-js-pure, which are
  two packages; here they count once per family.
- See `results/wm-collisions.json`.

### 4. Thresholds re-derived at 2,000 roots (Bun slice) vs SYNTHESIS §4

| §4 starting threshold                                                       | survives?                              | evidence at R=2000 (`results/single-*.json`)                                                                                                                                                                                                                                                                                                                                                                                                                    |
| --------------------------------------------------------------------------- | -------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| WM: normLen ≥ 400, one owner, normV agrees, string-bearing, stop-lists      | **yes**                                | 200 / 400 / 800 floors give 78 / 78 / 76 named (39 non-family at 400), recall 64.4 / 64.0 / 61.8%. Stop-lists halve non-family false names (74 → 39) for −0.8 points of recall                                                                                                                                                                                                                                                                                  |
| units ≥ 30 nodes, ≥ 3 content tokens                                        | **necessary, not enough**              | 769 non-family false names (5.7%) at k=2. They become safe only with the new **specificity rule**: a voting hash must sit in an index module no larger than 4× the query + 4 KB. That gives 86 (0.64%)                                                                                                                                                                                                                                                          |
| units k ≥ 2                                                                 | **yes**                                | k1 / k2 / k3 (all guards on): 253 / 125 / 92 named, recall 59 / 38 / 29%. k=2 is where false names halve                                                                                                                                                                                                                                                                                                                                                        |
| hits < 100 tokens need a same-package neighbour (±10 statements)            | **not tested**                         | Step B only. This lane measured module naming                                                                                                                                                                                                                                                                                                                                                                                                                   |
| Jaccard ≥ 0.7, margin 0.1, gate 20                                          | **yes; 0.8 slightly better**           | J 0.5 / 0.6 / 0.7 / 0.8 / 0.9: 110 / 94 / 64 / 50 / 42 named, recall 57.6 / 57.0 / 56.6 / 56.2 / 55.3%. Gate 10: 84 named, recall 66%; gate 50: 50 named, 41%                                                                                                                                                                                                                                                                                                   |
| Jaccard filter: drop runs in > 2 packages                                   | **yes, still needed**                  | d = 0 / 2 / 5 / 10: 315 / 64 / 98 / 104 named, recall 91 / 57 / 64 / 66%. Cost: d=2 recall falls with scale (70% at R=250 → 57% at R=2000), because more packages share code. A family-aware count is the next thing to try                                                                                                                                                                                                                                     |
| Literals: ≥ 20 bits, cover ≥ 0.9, margin ≥ 10, word cap 2 bits, abstain ≤ 5 | **only with the new specificity rule** | Any-file rule: 854 non-family false names (6.3%), recall 30%. **Specific files**: a file may claim only if it holds ≤ 3× the query's literals + 20. That gives 162 (1.2%) at recall 53%. Under it: T 10 / 20 / 40 / 60 → 230 / 226 / 187 / 159 named; cover 0.7 / 0.8 / 0.9 / 0.95 → 325 / 274 / 226 / 168; margin 5 / 10 / 20 → 247 / 226 / 184. So 20 / 0.9 / 10 survive, and margin 20 or cover 0.95 buy about 20% fewer names for −1 to −6 points of recall |
| Literal IDF                                                                 | changed scale                          | With N = 2,875 packages, one unique literal weighs 11.5 bits (12.5 if unseen), against 7.5–8 at the lab's 176–251. So "20 bits" means about 2 rare literals instead of about 3. Cover, not T, is what binds                                                                                                                                                                                                                                                     |
| near-ties refused (no smallest-file tie-break)                              | **yes**                                | used throughout. The specificity rule replaces the tie-break's intent without inventing claims                                                                                                                                                                                                                                                                                                                                                                  |

**Where the specificity guard comes from (measured, `scripts/lt.py`).** For right literal claims, the best file
holds a median of 1.0× the query's literals (90th percentile 1.36×). For foreign false claims the median is 37×
(75th percentile 102×). At ≤ 3× the guard keeps 96.5% of right claims and drops 81% of the false ones.

### 5. Two-family agreement: precision vs recall (R=2000, Bun slice, `results/combine.json`)

| rule                                      | foreign named (non-family false) | recall (strict) | exact-version recall / other-version | wrong package on positives | CC app groups / functions named |
| ----------------------------------------- | -------------------------------- | --------------- | ------------------------------------ | -------------------------- | ------------------------------- |
| WM alone                                  | 78 (39)                          | 64.0%           | 69.3 / 35.6%                         | 4                          | 0/100, 0/222                    |
| JC alone                                  | 64 (34)                          | 56.6%           | 59.3 / 42.2%                         | 4                          | 0, 0                            |
| UV alone (k=2, all guards)                | 125 (86)                         | 38.1%           | 41.0 / 21.2%                         | 1                          | 0, 0                            |
| S = structural consensus                  | 200 (129)                        | 72.5%           | 75.4 / 56.6%                         | 9                          | 0, 0                            |
| L = literal (specific, stop)              | 226 (162)                        | 52.6%           | 55.7 / 35.8%                         | 44                         | 0, 0                            |
| S OR L                                    | 356 (256)                        | 74.0%           | 76.2 / 61.9%                         | 30                         | 0, 0                            |
| **S AND L**                               | **58 (29)**                      | **51.0%**       | 54.8 / 30.5%                         | **0**                      | 0, 0                            |
| S AND reader / L AND reader               | 5 (0) / 1 (0)                    | 2.5% / 2.0%     | —                                    | 0 / 1                      | 0, 0                            |
| **design = S∧L ∨ S∧Rd ∨ L∧Rd**            | **64 (29)**                      | **51.5%**       | 55.3 / 30.9%                         | 1                          | **0, 0**                        |
| design-loose (UV k1, JC 0.5, L cover 0.7) | 135 (95)                         | 53.1%           | 56.3 / 35.6%                         | 1                          | 0, 0                            |

- **Agreement cuts non-family false names 4.4× compared with structural alone** (129 → 29). The price is 21 recall
  points.
- **Against literal alone, agreement is 5.6× cleaner for about 1 recall point.** Literal is the recall bottleneck.
- **Agreement does not license looser thresholds.** design-loose has 3.3× the false names for +1.6 recall points.
- **Readers add little at module level** (+0.5 points of recall). The foreign "named" figure for readers counts true
  names of foreign packages; their non-family false count is 0–1.
- **All 29 non-family false names of the design, by hand** (`results/combine.json` examples):

| class         | spans | what                                                                                                                                                                                                                                                                                                                                                            |
| ------------- | ----- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| real copy     | 20    | acorn and graphql parsers vendored by prettier (7); ora's published index.js carries string-width, get-east-asian-width and yoctocolors (5); jsbn in node-forge; is-network-error in ky (2); unpipe in mcp-proxy; detect-indent in confbox; cluster-key-slot in @redis/client; cssstyle and whatwg-url utilities in jsdom/domexception (webidl2js-generated, 4) |
| fork / rename | 5     | @hapi/formula vs @sideway/formula (3); global-dirs vs global-directory; node-gyp-build vs node-gyp-build-optional-packages                                                                                                                                                                                                                                      |
| look-alike    | **2** | crypto-js `blowfish.js` named bcrypt-pbkdf: two independent Blowfish implementations that share the cipher's constant tables                                                                                                                                                                                                                                    |

**Caveat on the copies.** Holding out popular roots makes them look worse than real use would. acorn and graphql are
held out, but prettier, which vendors them, is indexed. A shipping index would hold the original, and that would
win the margin (not measured).

### 6. Stop-lists and package families (R=2000, `results/stop-effect.json`, `results/family-effect.json`)

Stop-lists, built by `scripts/stoplists.py`:

- **helpers:** tslib, @babel/runtime\*, @babel/helpers, @swc/helpers, regenerator-runtime, @oxc-project/runtime;
- **runtime:** both bundlers' runtime, unmapped namespace glue and app stubs;
- **Stainless:** the template files of every indexed package carrying `X-Stainless-Lang`. Only @anthropic-ai/sdk is
  indexed; openai and groq-sdk are held out;
- **shim:** the ljharb es-shims template (implementation / polyfill / shim / auto.js) of the 32 packages that ship
  both polyfill.js and shim.js.

| detector           | without stop-lists: named (non-family) / recall | with stop-lists   | removed                                                                |
| ------------------ | ----------------------------------------------- | ----------------- | ---------------------------------------------------------------------- |
| WM                 | 124 (74) / 64.8%                                | 78 (39) / 64.0%   | −35 false, −0.8 points                                                 |
| JC d=2             | 153 (112) / 57.2%                               | 64 (34) / 56.6%   | −78 false (at least 56 were openai / groq-sdk named @anthropic-ai/sdk) |
| UV k=2             | 822 (770) / 38.9%                               | 713 (673) / 38.1% | −97, −0.7 points; then specificity: 125 (86) / 38.1%                   |
| LT, specific files | 319 (244) / 53.0%                               | 226 (162) / 52.6% | −82, −0.4 points                                                       |
| LT, any file       | 885 (854) / 29.9%                               | 805 (782) / 29.6% | −72 (sponges dominate)                                                 |

Families, judged three ways. The claims are the same; only what counts as wrong changes:

| family rule                                                                                                                                                                                                     | S non-family false | L   | S∧L    | design | recall effect |
| --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ | --- | ------ | ------ | ------------- |
| none (exact package)                                                                                                                                                                                            | 200                | 196 | 58     | 58     | —             |
| §4's five families (msal, @smithy + @aws-sdk, glob + minimatch + lru-cache + path-scurry, lodash + lodash-es + lodash.\* + es-toolkit, uri-js + fast-uri)                                                       | 182                | 192 | 56     | 56     | +0.04 points  |
| + data-driven: same npm scope; an unscoped package and its scoped fork (colors / @colors/colors, busboy / @fastify/busboy, sax / @trysound/sax, quick-lru / @alloc/quick-lru, proxy-addr / @fastify/proxy-addr) | 129                | 162 | **29** | **29** | +0.07 points  |

- **What the data shows beyond §4.**
  - Scope-mates: @aws-crypto/_, @opentelemetry/_, @graphql-tools/\*.
  - Scoped forks of unscoped packages.
  - **Pre-bundling packages ("sponges")**: playwright-core, vite, next, prettier, rolldown, webpack, @vercel/cli-config.
    They are not a family; the specificity rule handles them.
- **Brief vs data-driven families.** The brief's five families remove about 2–18 false names per rule. The
  data-driven scope and fork rules remove half of what is left.

### 7. Claude Code 2.1.216 vendor files (`results/cc-vendor-design-R2000.{json,tsv}`)

The rule is the design rule at R=2000, run on the 1,647 raw CJS factories. Manifest order equals raw order, so each
claim maps to `/tmp/eval-work/candidate-e214db7a-scratch/2.1.216/vendor/_bun-modules.json`.

| verdict vs today's name                 | factories        | today's nameSource    |
| --------------------------------------- | ---------------- | --------------------- |
| named by the detector                   | **710** of 1,647 |                       |
| agree (same package)                    | 272              | declared 172, llm 100 |
| same family                             | 67               | llm 65, declared 2    |
| **DIFFERENT**                           | **251**          | all llm               |
| **NEW** (today a `lib_<hash>` fallback) | **120**          | fallback              |
| contradicts a declared identity         | **0**            |                       |

- **The biggest rename clusters.**
  - opentelemetry-sdk-metrics-base → @opentelemetry/sdk-metrics (20);
  - @protobufjs/cli → protobufjs (15);
  - xmldom → xmlbuilder (14) and domutils → xmlbuilder (10);
  - ajv-runtime-\* / ajv-keywords / ajv-formats / escodegen → ajv (33);
  - @aws-sdk/types → @opentelemetry/api (6);
  - lodash-isobject / semver-\* → semver;
  - fallback `lib_*` → @opentelemetry/sdk-metrics (66), resources (29) or api (16).
- **Hand check** (`results/cc-vendor-handcheck.tsv`). A random 40 of the 371 DIFFERENT/NEW (seed 95), plus one
  example of every distinct (old, new) pair outside the three biggest clusters (67 more):
  - **Random 40: 39 right package** (one of them "likely"). The 40th is **1 wrong package**: node-forge's vendored
    `lib/jsbn.js` named "jsbn" (a copy).
  - **Pair review: all right except 3 at @smithy package granularity.** @smithy/config-resolver and smithy-client
    serde code were claimed as @smithy/core: right family, wrong package, partly version-dependent because serde
    moved into core.
  - Typical fixes: detect-libc (was "minimatch"); picomatch (was "color-convert" / "sharp"); pngjs (was "zlib");
    xmlbuilder (was "domutils"); google-auth-library crypto (was "crypto-browserify"); proper-lockfile (was
    "fs-extra"); @opentelemetry/api (was "@aws-sdk/types").

### 8. The negatives lane's hand-labelled src/ statements (`results/ccsrc-R2000.json`)

`negatives/labels.jsonl` and `traps.jsonl` exist and were used: 903 statement units. Two configurations ran on them:
**vendor** (exactly the module-level design) and **stepb** (literal specificity off, unit votes at k=1 without
module specificity).

| set                                                                | vendor design: claims | stepb design: claims                       | stepb literal alone                      |
| ------------------------------------------------------------------ | --------------------- | ------------------------------------------ | ---------------------------------------- |
| random sample, app (511 units, 2.56 MB)                            | **0**                 | **0**                                      | 0                                        |
| review queue, app (19, 11 KB)                                      | 0                     | 1 (q215, 202 B: Claude Code's marked fork) | 2 (also q99, 445 B of marked setup code) |
| traps, app (20)                                                    | 0                     | 0                                          | 1 (48302, marked-setup, 445 B)           |
| random sample, library (108, 225 KB): right / claimed, byte recall | 13 / 13, 33.6%        | 13 / 13, 27.1%                             | 19 / 19                                  |

- **Wrong-package names under stepb** come from sponges with specificity off: zod named @vercel/cli-config, and the
  MCP SDK named playwright-core. Specificity is needed at statement level too, but for statements it has to be
  defined differently: a statement is a fraction of its file.
- **Library recall is low** (34% of bytes). The top-2,000 index lacks Claude Code's exact versions: @anthropic-ai/sdk
  is indexed at 0.131–0.133, while Claude Code ships 0.94.

## What remains unknown

- **Step B under agreement.** Statement-level agreement was not designed; only "stepb", a looser variant, was tried
  on the 903 units. The neighbour rule (§4 structural, < 100 tokens) was not tested at scale.
- **Copies when the original IS indexed.** The hold-out puts popular originals (acorn, graphql, lodash) out of the
  index while the packages that vendor them stay in. Whether the original wins the margin when indexed was not
  measured.
- **esbuild-slice precision.** No foreign esbuild corpus exists; esbuild results are recall on lab spans only.
- **B5 at shipping size.** The shipped form needs MinHash; 128 × 4 B per entry, about 35 MB per family, is an
  estimate and was not built.
- **Beyond 2,000 roots.** The agreement rule's false names were flat from 753 to 2,521 packages (27 / 34 / 29). The
  single families were not. Growth past 2,521 is extrapolation.
- **Exact-version coverage.** Recall on other versions of an indexed package is 31% for the design (55% for exact
  versions). Choosing versions from what readers find in real bundles (§4 builder note) was not done.
- **Gaps in the data.**
  - 167 spans over 1 MB have no whole-module hash in these results. They were hashed afterwards into
    `data/fphash.big.jsonl`, which is not merged.
  - 8 versions have no build.
  - Purity-None spans (e.g. ora's pre-bundled index.js) count as their package.

## Re-run (exact commands; about 3 h wall clock on 24 cores, network for steps 1–2)

```bash
cd /work/exp095-library-fingerprints/scale
python3 -m venv .venv && .venv/bin/pip install numpy scipy orjson
node scripts/pick.mjs 2000                                     # data/roots.json, data/versions.jsonl (live registry: re-picking later picks newer versions)
node scripts/ixbuild.mjs --maxpos 1000 --jobs 20               # install + 4 builds per version (resumable)
node scripts/ixbuild.mjs --minpos 1001 --maxpos 2000 --jobs 20
find ix/builds -name out.js.map | sed 's|/out.js.map||' | sed "s|^|$PWD/|" | sort > ix/logs/dirs.txt
LABROOT=$PWD/ix node scripts/label.mjs --dirs ix/logs/dirs.txt --jobs 24 && find ix/builds -name labels.jsonl | sort | xargs cat > ix/all.jsonl
# the 75-package corpus relabelled as spans
N=/work/exp095-library-fingerprints/immutable-literals/verify/neg
for d in $N/builds/*/*; do m=$(basename $d); p=$(basename $(dirname $d)); js=$(ls $d/*.js | head -1); o=q/neg/builds/bun-1.4.0/min/$p/$m
  mkdir -p $o && cp $js $o/out.js && cp $js.map $o/out.js.map; done
find $PWD/q/neg/builds -name out.js.map | sed 's|/out.js.map||' > q/neg/dirs.txt
LABROOT=$PWD/q/neg SRCROOT=/work/exp095-library-fingerprints/immutable-literals/verify/neg node scripts/label.mjs --dirs q/neg/dirs.txt --jobs 24
find q/neg/builds -name labels.jsonl | sort | xargs cat > q/neg/all.jsonl
python3 scripts/collect.py                                     # data/spans.pkl, tasks.jsonl, fphash-jobs.jsonl
node --max-old-space-size=8000 scripts/features.mjs data/tasks.jsonl data/features.jsonl --jobs 24   # units + literals
PYTHONHASHSEED=0 .venv/bin/python scripts/b5.py data/tasks.jsonl data/b5.npz 24                      # B5 shingles
# whole-module hashes in memory-bounded chunks (one unchunked run reached 74 GB RSS)
python3 scripts/split_fphash.py && scripts/fphash-chunks.sh && .venv/bin/python scripts/fphpack.py
node --max-old-space-size=16000 scripts/srclits.mjs --jobs 8   # literal index input from npm source
.venv/bin/python scripts/pack.py
cd scripts && export PYTHONHASHSEED=0 && P=../.venv/bin/python
$P jcidx.py && $P structidx.py && $P stoplists.py              # index tables + stop-lists
$P jc.py && $P uv.py && $P lt.py && $P lt.py --nostop && $P wm.py && $P readers.py   # per-query scores, every scale
$P report_single.py all && $P combine.py && $P growth.py && $P stop_effect.py && $P fam_effect.py
$P index_size.py && $P ccsrc_eval.py 2000 && $P cc_vendor.py 2000 design
```

Order note (rule 9): `jc_claims` reads the package-id table `data/pkgs-jidx.json` that `jcidx.py` writes. The run
reported here built jidx before the negatives-lane units joined `spans.pkl`, so that table predates
`data/pkgs.json`. `claims.py` handles both.

## Files

- `scripts/`: every step above.
- `data/`: raw data, about 13 GB.
  - `spans.pkl`: 1.42 M span rows.
  - `units.npz`: 16.5 M units.
  - `b5.npz`: 716 M shingles.
  - `fphash.jsonl`, `srcfiles.jsonl`, `srclits-content.jsonl`.
  - per-scale score caches: `jc-*`, `uv-votes-*`, `lt-*`, `wm-claims.pkl`, `combine-*`.
  - `shipped/R<R>/`: the measured index files.
- `ix/`: index installs and builds, 60 GB. `ix/logs/status.jsonl` has the per-version install and build status.
- `results/`:
  - `growth.json` (section 2);
  - `wm-collisions.json`;
  - `single-{wm,uv,jc,lt}.json` (sweeps, with example false names);
  - `combine*.json`;
  - `stop-effect.json`, `family-effect.json`;
  - `index-size.json`;
  - `ccsrc-R2000.json`;
  - `cc-vendor-*.{json,tsv}`, `cc-vendor-handcheck.tsv`.
