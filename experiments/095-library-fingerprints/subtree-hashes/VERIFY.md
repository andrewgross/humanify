# VERIFY: adversarial re-check of the subtree-hash claims (exp095)

Verifier, 2026-10-11. Everything I built and every number below is in `verify/` (`verify/README.md` has the
commands; `verify/results/` has the outputs). I used the original `canon.mjs`, `extract.mjs` and `lib/` unchanged,
and the same index the claims used: lab + rwlab, Bun 1.4.0, minified, 35,696 hashes of 20+ nodes.

## Verdict in one paragraph

The **stability** claims hold up on data the normaliser never saw. Unit hashes survive Bun versions and esbuild
versions that were not used to write the rules. The **precision** claims do not hold. Every lab precision number
("100%, 0 wrong of 20,481") measures confusion _between packages that are in the index_. No lab scenario queries
library code that is missing from the index, and real bundles are full of such code. When I ran 82 popular
packages that are not in the index, the recommended rule (k=1, 20+ nodes, same bundler family) named **193 of
4,562 spans (4.2%, 14.9% of bytes) as a package they are not**. On Claude Code itself, the same rule names at
least **two app-code groups** as libraries (5189 → marked, 4570 → @anthropic-ai/sdk). Neither group holds a
product string, which is why the negative set missed them. It also names at least four modules of unindexed
libraries wrongly (@xmldom ×2 → iconv-lite, the grpc-js server → tslib, js-sdsl → asynckit). The "exact
version" claim is wrong for @anthropic-ai/sdk: Claude Code declares `"0.94.0"`, which was never built, and the
method answered 0.93.0. Unit hashing is a good signal. The voting rule and its precision claim need to be redone,
using foreign code as the negative set.

## 1. What was re-measured and how

| test                                   | why                                                                                                                                                    | data                                                                                                                                                                                                                                                                |
| -------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Foreign code**                       | Every lab scenario indexes every queried package, so precision there cannot show false positives on unknown code                                       | 82 popular packages in neither lab nor rwlab (openai, groq-sdk, @google/genai, lodash, rxjs, fp-ts, jsdom, @aws-sdk/client-s3, msal-browser, express, ...), Bun 1.4.0 `--minify`, `require` + `import()` entries, labelled by sourcemap with the lab's own labeller |
| **Unseen Bun / esbuild versions**      | The normalisation rules were derived by diffing Bun 1.3.14 / 1.4.0 / 1.4.3 / esbuild 0.28.2, which are the same builds the stability numbers come from | The 18 rwlab packages at their latest release, built with Bun 1.2.23, 1.3.5, 1.4.1 and esbuild 0.19.12 (none used for the rules), with 1.4.0 and esbuild 0.28.2 as controls                                                                                         |
| **CC app code on unseen versions**     | The 20-node floor was picked where the CC negatives read 0 (they read 17/630 FP at 10 nodes), so the floor was tuned on the test set                   | CC 2.1.86 and 2.1.119, same product-string rule, new layouts                                                                                                                                                                                                        |
| **CC named modules with weak support** | The negative set only covers groups that contain a product string                                                                                      | Every CC 2.1.216 module named at k=1 where fewer than 25% of its units hit the index (22 modules), judged by hand                                                                                                                                                   |
| **Version guess**                      | rwlab sampled at most 40 releases per package, so neighbouring releases were never built                                                               | Every release next to the claimed version, built and compared                                                                                                                                                                                                       |

Leakage checks on the original setup:

- No unminified twins are used as queries; identify.mjs queries only `bun-1.4.3/min`, `esbuild/min` and `rollup/min`.
- `same-versions` indexes Bun 1.4.0 and queries Bun 1.4.3 builds of the _identical_ sources. That tests bundler
  drift, not precision.
- `unseen-version` holds out one release but keeps its neighbours, and adjacent patches share 100% of units (drift.txt).
  In practice it is in-sample.
- The lab's 682 "app spans" are each a one-line `rwAppMain` stub, so "0 lab-app FP" carries no information.

## 2. Upheld

**Unit stability across Bun versions: upheld, on versions never used for the rules** (`results/unseen-bun.txt`).
Units of 20+ nodes found in the Bun 1.4.0 index:

| build of the same sources                           | units in index          | spans named k=1 | precision | whole-module hash in index |
| --------------------------------------------------- | ----------------------- | --------------- | --------- | -------------------------- |
| Bun 1.4.0 (control: the index's own build)          | 100.0%                  | 88.6%           | 100%      | 100%                       |
| Bun 1.4.1 (unseen)                                  | 99.9% (req+dyn: 99.4%)  | 88.6%           | 100%      | 98.1%                      |
| Bun 1.3.5 (unseen)                                  | 100.0% (req+dyn: 99.9%) | 88.6%           | 100%      | 100%                       |
| Bun 1.2.23 (unseen; byte-identical output to 1.3.5) | 100.0% (req+dyn: 99.9%) | 88.6%           | 100%      | 100%                       |
| esbuild 0.28.2 (rules derived from it)              | 84.6%                   | 82.4%           | 100%      | 13.2%                      |
| esbuild 0.19.12 (unseen, about 2 years older)       | 84.6%                   | 82.3%           | 100%      | 13.1%                      |

The claimed 99.9% / 98.7% (Bun) and 87% (esbuild) reproduce within a few points on a different package sample.
esbuild 0.19 behaves exactly like 0.28. "Hoisted functions match by themselves" also holds: CC lazy-ESM groups
are named from hoisted functions.

**CC product-string app code: 0 hits at 20+ nodes reproduces on unseen versions** (`results/cc-app-hits-*`):
2.1.86 0/6,708 units, 2.1.119 0/8,984, 2.1.198 0/14,107, 2.1.216 0/16,257. At 15 nodes: 5, 9, 9 and 9 hits.
These four sets are mostly the _same app code_ across versions, so they are not four independent tests.

**Index size** (35,696 hashes for the corpus) matches the index the scripts build. The top-N extrapolation remains
an estimate.

**Version guess**: right for msal-node 3.8.1 (also the banner), axios 1.15.2 and highlight.js 11.11.1, which
beat their unsampled neighbours (`results/version-check-2.1.216.txt`).

## 3. Refuted or weakened

### 3a. Precision on code the index does not hold (refutes "precision 100%" as a statement about real bundles)

`results/foreign.txt`, `results/foreign-classes-{20,30,50}.txt`. The foreign set has 4,562 library spans
(14.1 MB) whose package is in no index entry, so any name given to one of them is wrong.

| unit floor             | foreign spans named k=1 | bytes     | k≥2 | k≥3 |
| ---------------------- | ----------------------- | --------- | --- | --- |
| 10 nodes               | 371 (8.1%)              | 24.1%     | 174 | 116 |
| 20 nodes (recommended) | **193 (4.2%)**          | **14.9%** | 149 | 93  |
| 30 nodes               | 156 (3.4%)              | 12.7%     | 118 | 78  |
| 50 nodes               | 137 (3.0%)              | 12.7%     | 76  | 52  |

At the unit level, 1,327 of 66,357 foreign units of 20+ nodes (2.0%) hit a single-package hash. Inside the same
builds, indexed dependencies were named right 568 of 569 times. The index is good at recognising what it holds.
It is bad at saying "I do not know this".

The 193 false names at 20 nodes, sorted by cause (each pair checked by hand):

| cause                                                                                                                                                                                                                                                                                                                                  | spans (KB) at k=1 | k≥2 | k≥2 or one 50+ unit |
| -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- | --- | ------------------- |
| shared code generator: openai 46 and groq-sdk 38 spans → **@anthropic-ai/sdk** (all Stainless SDKs)                                                                                                                                                                                                                                    | 84 (358)          | 74  | 74                  |
| compiler helper inlined into a module: TypeScript `__extends` / `__spreadArray` / `__generator` / `__rest` / `__esDecorate`, Babel `_toPrimitive` / `_defineProperties`, rollup interop → named tslib, diff, @opentelemetry/api, zod, js-yaml, marked, axios (rxjs 46, fp-ts 5, cheerio family 3, markdown-it / linkify / pako 4, ...) | 68 (1,227)        | 54  | 65                  |
| copied, bundled or forked code: @aws-sdk → @smithy/core, lodash → lodash-es, tar → glob, @inquirer/figures → figures, punycode → uri-js, jsdom → whatwg-url / undici, @img/colour → color-convert, ...                                                                                                                                 | 25 (411)          | 20  | 22                  |
| **unrelated code, one coincidentally identical 20–29-node unit**                                                                                                                                                                                                                                                                       | 16 (99)           | 1   | 1                   |

Examples of the last class, all named at k=1 from a single unit:

- fp-ts `Ord.js` / `number.js` / `string.js` and ramda `median.js` → **asynckit**, from `(e,r)=>e<r?-1:e>r?1:0` (21 nodes)
- jsdom `dates-and-times.js` → **date-fns**, from `function(e){return e%400===0||e%4===0&&e%100!==0}` (21 nodes)
- mobx → **formdata-node**, from `(e,t){for(let[n,s]of this)e.call(t,s,n,this)}` (20 nodes)
- run-async → is-promise; @colors/colors → escape-string-regexp; css-tree → whatwg-url; enquirer → ink

So the claim "the only wrong names anywhere were library-vs-library cases (vendored copies)" is refuted.
Coincidental collisions do happen at 20–29 nodes. They vanish at 30 nodes or with k≥2. Compiler helpers and
shared generators survive every rule tried, including k≥3 and a 50-node floor. The rule "a hash claimed by 2+
packages does not vote" does not protect against them, because only one _indexed_ package happens to inline that
helper.

### 3b. Claude Code: app code and unindexed libraries are named (refutes "0 app-code false positives")

`results/cc-suspects-2.1.216.txt`, `results/cc-named-helpers-2.1.216.txt`. The cc-scan result "modules
containing a product string named: 0" reproduces, but the negative set is narrow. Under the recommended rule
(k=1, 20 nodes):

| CC 2.1.216 module                  | what it really is                                                                                                       | named as              | evidence                                                                                                                                            |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------- | --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| 5189, lazy-ESM, 9.4 KB             | **CC app code**: OSC-8 hyperlinks, a git-host set (`"bitbucket.org","codeberg.org",...`), markdown-table cell splitting | **marked**            | 1 vote: `(n,o,i)=>{let s=!1,a=o;while(--a>=0&&i[a]==="\\")s=!s;return s?"\|":" \|"}` (36 nodes; marked's splitCells callback, copied into app code) |
| 4570, lazy-ESM, 1.6 KB             | **CC app code** (`"MCP tool returned an error"`)                                                                        | **@anthropic-ai/sdk** | 1 vote: ``(m)=>m.type==="text"?m.text:`[${m.type}]` `` (22 nodes)                                                                                   |
| 4832 (29 KB) and 4834 (12 KB), CJS | @xmldom/xmldom (`"prefix is non-null and namespace is null"`)                                                           | **iconv-lite**        | 1 vote: `function nhr(e,t){for(var r in e)if(ehr(e,r))t[r]=e[r]}` (23 nodes). These are _both_ of cc-scan's "iconv-lite 2 (41 KB)"                  |
| 4234, CJS, 29 KB                   | @grpc/grpc-js server (`"grpc.max_connection_age_ms"`)                                                                   | **tslib**             | 3 votes from inlined `__runInitializers` / `__esDecorate`, so it survives k=3                                                                       |
| 4161, CJS, 7.7 KB                  | js-sdsl tree (`"Iterator access denied!"`)                                                                              | **asynckit**          | 1 vote: `function(r,n){if(r<n)return-1;if(r>n)return 1;return 0}` (21 nodes); 7.7 of cc-scan's 10 KB "asynckit"                                     |
| 857, 861, 615, 616, 835, CJS       | @aws-sdk code                                                                                                           | @smithy/core          | the copy class from 3a                                                                                                                              |
| 3180, 3194, 3201, lazy-ESM         | CC's in-house fork of ink                                                                                               | ink                   | arguably right, but changes there are real app changes                                                                                              |
| 6309, lazy-ESM                     | a base64url helper, owner uncertain                                                                                     | zod                   | 1 vote, 42 nodes                                                                                                                                    |

A 2 KB app group named as a vendor library hides its changes from the cross-version diff. That is the failure
precision was meant to rule out. The stricter rule "k≥2, or one unit of 50+ nodes" would drop 5189, 4570, 4832,
4834, 4161 and 6309, but not 4234 or the @aws-sdk → @smithy/core copies.

### 3c. "Names the exact version CC ships" is refuted for two of seven packages

rwlab sampled only some releases. Building the unsampled neighbours (`results/version-check-2.1.216.txt`, units of
each release found in CC / not found):

- **@anthropic-ai/sdk**: claimed 0.93.0. CC declares `var hhe="0.94.0"` next to the SDK's `detect-platform` code.
  0.94.0 was never built. It scores 422 in / 320 out, better than 0.93.0 at 418 / 324.
- **zod**: claimed 3.25.75. Unsampled 3.25.74 (1,059 / 832) and 3.25.73 (1,058 / 833) score at least as well as
  3.25.75 (1,057 / 834).

The method gives a version _neighbourhood_. It is exact only if every release is indexed, and adjacent releases
differ by a handful of units.

### 3d. Thresholds tuned on the test data

- The 20-node floor was picked on the CC negatives: at 10 nodes they gave 17/630 FP, at 20 nodes 0.
- The gap ≤3 rule was picked after a 10-statement gap claimed app code.
- The normaliser rules were derived from the same Bun / esbuild builds the stability table reports.

Only the last of these survived a held-out test (2 above). On foreign code the 20-node floor leaves 16
coincidental names. 30 nodes or k≥2 is what removes them.

## 4. Method flaws in the original

1. **No foreign-code negatives.** Every lab query span's package is in the index, so "precision" only measures
   package-vs-package confusion. That class of error is the easy one. Unknown code (app code _and_ unindexed
   libraries, which make up most of a real bundle's `src/`) is never tested.
2. **The CC negative set is defined by product strings.** It excludes exactly the app code that looks most like
   library code (generic utilities, copied snippets). Two wrong app names were outside it.
3. **Tuning on the test set:** the node floor, gap filling and normaliser rules (see 3d).
4. **"Exact version" from a sparse sample**, reported without margin against unbuilt neighbours.
5. **"Held-out version" keeps the neighbouring patches**, which share 100% of units, so it is not held out in practice.
6. The lab app spans are stubs, so lab-app FP counts carry no information.

## 5. Corrected verdict

Hashing small units (identifiers masked, with the loose normaliser) **is** a robust way to recognise _known_
library code without module boundaries. It survives Bun 1.2–1.4 at ~99.5–100% and esbuild versions at ~85%,
and finds hoisted library functions inside `src/`.

It is **not** precise enough, with k=1 at 20 nodes, to say "this is library X" about code of unknown origin. On
82 unindexed packages it mislabels 4.2% of spans and 14.9% of bytes. On Claude Code it labels at least two
app-code groups and four unindexed-library modules as libraries.

Before it ships, it needs:

- a vote rule of at least k≥2 or a 30+-node floor (this removes coincidences);
- a stop-list of compiler-helper units (tslib / Babel / rollup helper shapes, matched by shape, not by package);
- package families for shared generators and copies (Stainless SDKs, @smithy / @aws-sdk, msal, lodash / lodash-es);
- precision re-measured against foreign code, scaled to the size of index that would ship.

Collisions should grow with index size, and this was not measured. Version answers should be reported as a range
unless every release is indexed.
