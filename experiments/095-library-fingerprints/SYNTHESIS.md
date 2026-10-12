# exp095: can we ship a fingerprint index of npm libraries?

Synthesis of five approaches, each checked by a second, adversarial agent.
Written 2026-10-11. Every number below is one the verifier upheld, or the
verifier's corrected figure where it disagreed. When a number is only the
original agent's claim and was not re-checked, it says so.

## Totals first

- **Data behind this.**
  - **The shared lab** (`lab/`): 176 packages at 315 versions, built by 5 bundler setups (Bun 1.3.14 / 1.4.0 / 1.4.3, esbuild 0.28.2, rollup+terser), minified and not. That is 6,184 builds and 321,619 spans, each labelled with its package, version and file.
  - **Claude Code 2.1.198 and 2.1.216**: 214 positives with a declared identity (188 are highlight.js grammars), plus 100 app module groups and about 220 app functions as negatives.
  - **Corpora the verifiers added**, never seen during tuning: 1,221 packages built from the npm top 150–3000; 82 popular packages the index did not hold; 75 more out-of-index packages, including protocol look-alikes (openai, groq-sdk, fastmcp); 7.3 MB of held-out Claude Code app code; and CC versions 2.1.86 and 2.1.119.
- **The short answer: yes for naming vendor files, not yet for hiding code in `src/`.**
  - **Vendor files.** We can recognise library code we have indexed, across Bun versions, with very few wrong names, as long as each method's guards are on.
  - **Library code inside `src/` (Step B).** It can be found. Every method still put a library name on some real Claude Code app code once tested on app code it was not tuned on: 0 to 2.6% of its claims. That is fine for listing candidates, but not yet safe for hiding code from the app diff.
- **Index size is not the problem.** A useful index for the top 1,000 npm packages is roughly 5–30 MB. The binding constraints are:
  1. **Indexing the exact versions apps ship.** One major version off cuts recall from 94% to 14% on Claude Code's MSAL code.
  2. **False matches grow faster than the index.** Four times more packages gave about 10× more false calls without a filter. Every threshold here was set on a lab of 176–347 packages and has to be re-derived at shipping size.

| Approach (folder)                                                                             | What it fingerprints                                           | Best use                                                    | Verified precision problem                                                                                                                                                                                                       |
| --------------------------------------------------------------------------------------------- | -------------------------------------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Whole-module exact hash (`exact-module-hash/`)                                                | normalised module body → humanify's MatchKey hash              | naming CJS factories and lazy ES modules that have a marker | At 1,221 packages, 5–12% of hashes in the normLen 400–1,600 bands are shared by 2+ packages. At least 8 groups are DIFFERENT modules that also have identical strings.                                                           |
| Sub-tree unit hashes (`subtree-hashes/`)                                                      | each statement, function and class of 20+ nodes, names masked  | finding hoisted library functions                           | With k=1 vote it wrongly names 4.2% of spans (14.9% of bytes) from packages that are not indexed, and 2 Claude Code app groups.                                                                                                  |
| n-gram / MinHash (`ngram-minhash/`)                                                           | 5-token runs, names masked; Jaccard and containment            | vendor naming (Jaccard ≥ 0.7)                               | Whole modules: 0 false calls out of 60,126 under an honest leave-one-out test. Sliding windows: real CC app code was called a library.                                                                                           |
| Immutable literals (`immutable-literals/`)                                                    | strings, property names, numbers and regexes, IDF-weighted     | the only bundler-independent index (built from npm source)  | 18 of 686 CC `src/` claims (2.6%) are app code or Claude Code's own forks; 8 of the 15 app ones go away when pre-bundled files are dropped from the index.                                                                       |
| Normalised function + statement hashes, Step B detector (`bundler-variation-and-boundaries/`) | 33-rule canonical form, functions and statements of 30+ tokens | Step B, finding library code with no boundaries             | Held-out CC app code: 0 real app bytes flagged (690 bytes of compiled library helpers mis-named). But wrapper-group growth absorbs app code (5.8%) with an index from another bundler, and the bundler runtime at every setting. |

---

## 1. Direct answers

### Exact matching: what is it good for?

**Exact matching is good for whole modules that have a clear boundary and are long enough, inside one bundler family, with guards. It is also the right primitive at function and statement level.**

- **Raw text is useless.** The same file built by Bun 1.3.14 and by Bun 1.4.0 has identical minified text 0.1% of the time. humanify's structural hash on its own gives 90.4%.
- **Strip the wrapper and normalise, and Bun versions agree completely.**
  - Bun 1.3.14 vs 1.4.0: 100.0% of 40,219 fresh file pairs the normaliser had never seen (CJS 100.0%, lazy ESM 99.8%, eager 99.9%).
  - Bun 1.4.0 vs 1.4.3: 96.5%. Bun 1.4.3 started rewriting `(0,x.f)()` and inlining single-use variables.
- **What the pipeline can already do today.** The existing vendor `structuralHash`, looked up in an index of highlight.js builds, finds all 188/188 of Claude Code's grammars. The intersection of their version ranges pins **11.11.1**; that still holds when 11.11.0 and 11.11.2 are added, but now on the evidence of one grammar.
- **What breaks at scale.** The original claim, "nothing above normLen 400 (about 60 minified bytes) collides across packages", came from a 175-package index. At 1,221 packages:
  - the share of hashes held by 2+ packages is 5.1% at normLen 400–600, 12.3% at 600–800, 11.3% at 800–1,600 and 3.5% at 1,600+. Most are true copies, such as core-js vs core-js-pure;
  - at least 8 groups are different modules that also have identical strings, so the exact-string check (normV) cannot reject them:
    - `module.exports=X;module.exports.default=X` (camelcase vs is-fullwidth-code-point);
    - `module.exports=function(e){return A(B(e))}`;
    - import-only headers;
    - polyfill shim templates;
    - the rolldown runtime chunk;
    - TypeScript `__exportStar` index files, up to normLen 4,838;
  - 0.07% of unseen package code hits the lab index, mostly naming the wrong package. Every `@opentelemetry/*/version.js` hashes as `@opentelemetry/api/version.js`, and Claude Code's own hit list has 4 of these.
- **Exact-string agreement is weaker evidence than it looks.** 237 of the 642 CC hits contain no string at all, so for them "the strings agree" is automatic.
- **Without boundaries, exact whole-module hashing loses a third of library bytes.** With label-free segmentation of the combo apps, recall is 67.4% for Bun and 46.4% for esbuild. (The first report said 82% / 64% because two failed bundles were dropped from the total.)

### Probabilistic matching: which family, which thresholds, what precision?

Three families were measured. All are probabilistic in the sense of "score plus threshold plus margin".

1. **n-gram Jaccard, for whole modules.** Uses the B5 stream: all local names masked, `${…}` in templates emptied, numbers normalised.
   - **Rule:**
     - Jaccard ≥ 0.7;
     - margin ≥ 0.1 over the next package;
     - drop 5-token runs seen in more than 2 packages;
     - refuse modules with fewer than 20 informative runs.
   - **Precision:** survives an honest leave-one-package-out test, with the filter recomputed without the query's package: **0 false calls among 60,126 gated lab modules**. All 65 hits are the real ansi-styles copy inside chalk. It does not sit on a cliff: still 0 with the size gate at 10, 20, 30 or 50 and the filter at 2 or 5 packages.
   - **Without the filter it is unsafe:** 53 false calls at ≥ 0.7, and false calls grew about 10× when the index grew 4×.
   - **Recall, from the verifier:** file-level recall at J ≥ 0.7 across all cross-bundler modules is 0.39 (Bun 1.3.14 0.57, esbuild 0.28, rollup 0.07). 38% of library modules are below the size gate.
2. **Literal containment.** Fingerprints only what a minifier cannot rename.
   - **Rule:**
     - score ≥ 20 bits;
     - cover ≥ 0.9, with literals the index has never seen counted at full weight;
     - margin ≥ 10 bits;
     - identifier-like literals (`role`, `tool_use`) capped at 2 bits;
     - abstain at 5 literals or fewer.
   - **Recall:** about 93% of library bytes when the exact version is indexed; **81.9%** (45.6% of statements) when it is not.
   - **Abstention:** 97.8% of code from out-of-index packages gets no claim; 0.3% is claimed by look-alikes and 1.9% gets a vendored copy's name.
   - **Wrong-package errors between indexed libraries:** ≤ 0.1%.
   - **The "smallest file wins" tie-break is unsafe.** The verifier showed it inventing claims, so near-ties should be refused instead.
3. **Unit-hash voting.** Count matching sub-tree hashes per package.
   - **k = 1 vote at 20+ nodes is not safe:** 193 of 4,562 foreign spans are wrongly named. Requiring k ≥ 2 leaves 149, k ≥ 3 leaves 93, and a 50-node floor leaves 137.
   - **Four causes, none of which a threshold alone fixes:**
     - inlined compiler helpers: tslib, TypeScript `__esDecorate`, Babel `_toPrimitive`;
     - shared code generators: every Stainless SDK reads as @anthropic-ai/sdk;
     - copies and forks: @aws-sdk read as @smithy/core, lodash as lodash-es;
     - coincidental 20–29-node units.

**Exact or probabilistic?** Both, in layers.

- **Exact hashes** of normalised units are the cheap, deterministic lookup.
- **Probabilistic scores** (Jaccard, literal containment) do the naming across library versions and bundlers.
- Either way, a claim should need **two independent kinds of evidence**, one structural and one literal. Nothing measured here shows that one kind alone is safe for hiding code.

### Do we need sub-tree (not-from-root) hashing?

**Yes.** Root hashes break in all three places Bun's layout creates (verified):

- **Lazy vs eager emission of the same file** (reached by `require` vs `import`): module hashes agree on 2.4%; functions agree on 97.5%.
- **Tree-shaking:** about 10% of eager module hashes change. Functions are 100% contained.
- **A minor library release:** axios 1.19 → 1.20 shares 0 of 1 module hashes but 82% of its units.
- **Step B:** library code in `src/` has no module root to hash at all. Hoisted library functions are found only by their own units, e.g. `nml` ("profile name is empty"), an @anthropic-ai/sdk function hoisted into an app group.

**The price is precision on small units.** A floor of 20 nodes was tuned on the test data and still lets coincidences through. Use about 30 tokens/nodes, plus at least 3 distinct strings, property names or globals in a statement-level hit, plus k ≥ 2 or a second evidence family.

### How do bundlers differ, and does normalisation fix it?

Numbers below are code-weighted where the verifier corrected count-weighted figures.

| Pair                           | Module hash (normalised) | Units / functions (normalised)                         | Literal set identical |
| ------------------------------ | ------------------------ | ------------------------------------------------------ | --------------------- |
| Bun 1.3.14 vs 1.4.0            | 100%                     | 100% (also 1.2.23 / 1.3.5 / 1.4.1: 99.4–100%)          | 27,930 / 27,933       |
| Bun 1.4.0 vs 1.4.3             | 96.5%                    | 98.7% of units                                         | 98.1%                 |
| Bun vs esbuild                 | 60.1%                    | ~85% of units; **62–70% of function code by tokens**   | 95.1%                 |
| Bun vs rollup+terser           | 5.1%                     | 45% of units; **~9% of outermost functions by tokens** | 58%                   |
| Bun minified vs Bun unminified | 71.7%                    | —                                                      | —                     |

- **Within the Bun family, normalisation fixes it.** The rules come from real diffs:

  - arrow vs `function` factories;
  - class-private `#names` renumbered;
  - `(0,x.f)()`;
  - destructuring splits;
  - `!0`, `void 0`;
  - comma sequences and `a&&b;` turned into `if`;
  - early-return inversion;
  - `new Error` vs `Error`.

  The rule list is a spec to port: `exact-module-hash/fphash/src/norm.rs` (about 25 rules, Rust), `subtree-hashes/scripts/canon.mjs` and `bundler-variation-and-boundaries/scripts/canon.mjs` (33 rules, with per-rule ablation counts).

- **Across bundler families it only partly fixes it.** esbuild needs its own index entries (+17–25% size). terser is not worth chasing structurally; only literals reach it.
- **Literals are the one bundler-independent signal.** 95% of lab spans keep 99–100% of their literals relative to the npm **source** file, for every Bun version and esbuild. So a literal index can be built from npm tarballs with no bundling at all.
- **What normalisation cannot fix:**
  - bundle-time defines: Bun inlines `process.env.NODE_ENV`, so react's index.js is `Ne.exports=Pe()` in Bun;
  - build-machine constants: `cpu:"arm64",os:"darwin"` inlined into CC's MSAL code;
  - Bun moving tiny functions across modules;
  - Bun dropping the `__esm` end marker on 10.8% of lazy modules (2.6% of lazy bytes). esbuild drops none.
- **Index from minified builds, not source.** Structural fingerprints must be built from minified bundler output, because minified vs unminified only agree 64–72%.

**Library versions** (Bun, same file, by tokens): patch 96%, minor 64%, major 44%.

- Versions are cheap to store: 40 releases cost 2.2–3.2× one release, and one package's versions share about half their fingerprints.
- **Coverage of the shipped version is what decides recall.** On Claude Code's MSAL, which has banner ground truth (108 modules, 208.5 KB in lazy ESM in `src/`), recall is 94.3% with the exact versions (msal-node 3.8.1, msal-common 15.13.1) and 13.7% with the lab's next major. 0 bytes were labelled MSAL outside MSAL.
- **Version guesses from fingerprints give a neighbourhood, not an exact version.** CC declares @anthropic-ai/sdk `0.94.0`; the unit method said 0.93.0 because 0.94.0 was never built. zod 3.25.73–76 tie. The declared constant is the oracle.

### How many hashes / MB to ship?

| Fingerprint                                            | Measured                                                                                                                                            | Top 1,000 npm                                                                        | Top 10,000 npm                              |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------ | ------------------------------------------- |
| Whole-module exact (normalised)                        | lab: 4,569 usable hashes for 314 package versions, < 120 KB gzipped; fresh corpus: 7.6 usable hashes per package version (median 1)                 | ~0.27 M entries / ~4 MB (latest only, top 15,916); ~0.42 M / ~7 MB (last 5 versions) | 4.8 M / ~77 MB (all versions, top 15,916)   |
| Sub-tree units (functions + statements, 20–30+ tokens) | ~2–3 hashes per KB of minified library code; 64 packages × ~800 versions = 35,696 hashes ≈ 0.43 MB; lab Bun family 28.7k ≈ 0.85 MB with version ids | ~0.3–0.4 M ≈ 3–5 MB (estimate)                                                       | ~4 M ≈ 50 MB (estimate)                     |
| MinHash-128 + LSH per module                           | lab 7,722 modules = 4.0 MB                                                                                                                          | ~10 MB latest only; ~30 MB one per major                                             | ~300 MB one per major                       |
| Literal sets (from source)                             | 251 packages: 165k package-level entries ≈ 1.3 MB; 950k file-level ≈ 7.6 MB                                                                         | —                                                                                    | 2,000 packages ≈ 10–60 MB (linear estimate) |

- Adding esbuild-built entries costs +17–25% (sub-tree) or ×1.6 (MinHash). The literal index needs **no** per-bundler copies.
- Every extrapolation is an order-of-magnitude model, not a measurement.
- Collision growth with size was measured only up to 1,221 packages.

---

## 2. Part A: other immutable strings and keys that work as package readers

Measured on 24,358 npm source files, where the owner is known so precision is measurable, and on every Claude Code layout unit.

**Reliable readers:**

| Reader                                                                          | Precision measured               | Found in Claude Code                                                                                                  |
| ------------------------------------------------------------------------------- | -------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ------------ |
| `/*! name vX */` banners                                                        | 673 / 696 name their own package | 125 banner units (108 MSAL); 83 / 85 agree with literal-index claims                                                  |
| Inlined `package.json` objects                                                  | —                                | 11, 5 of them in `src/` (`@aws-sdk/client-sso`, `-bedrock`, `-bedrock-runtime`, `-sts`, `-cognito-identity` @3.936.0) |
| `VERSION`-style constants                                                       | 56 / 56 equal the true version   | e.g. axios `var Nvt="1.15.2"`, @anthropic-ai/sdk `var hhe="0.94.0"`                                                   |
| Scoped error prefixes (`"[pkg] …"`, `"pkg: …"`)                                 | name the origin package          | 17                                                                                                                    |
| AWS SDK `.n("BedrockClient","ListTagsForResourceCommand")` Client/Command pairs | —                                | 145 units, 72 KB, all in `src/` (BedrockClient 94, CognitoIdentity 23, STS 13, BedrockRuntime 10)                     |
| highlight.js grammar shape `{name, keywords                                     | contains}` (strict)              | only highlight.js among 347 packages                                                                                  | 188 grammars |

**Medium:** debug namespaces (112 / 139).

**Not reliable inside a bundle:**

- `Symbol.for(...)`: React Compiler emits `Symbol.for("react.memo_cache_sentinel")` into app code.
- github URLs: in Claude Code, 86 of 116 are the app's own repo.
- User-Agent strings, `process.env` names, `x-` headers: app code is full of them.

**Is the highlight.js `name:` field a highlight.js-only convention?**

- **The strict form is highlight.js-only.** `{name, keywords}` or `{name, contains}` matched nothing else in 347 packages.
- **The loose form is not.** `{name, aliases}` also matches Claude Code's own slash-command table (`{name:"teleport",aliases:["tp"],description:…}`), so a reader on the loose form would call app code a library.
- **Many packages have registries of objects with a fixed key set, but each is its own convention.** These were found in the census and are not yet validated as readers:

  - human-signals `{name:"SIGPWR",number,action,description,standard}` ×70;
  - date-fns locales `{code:"en-GB",formatDistance,…}` ×1,722;
  - marked tokens `{type:"strong",raw,text,tokens}`;
  - ajv `{keyword:"$ref",schemaType,code}`;
  - @smithy middleware `{name:"serializerMiddleware",step,tags,override}`.

  So there is no universal "name:" convention to read. Each package-shaped registry needs its own reader and its own negative test.

- **The general version of Part A is the literal index.** Every rare string or key is a weighted, partial reader. The declared readers above are the high-confidence ones, and they are the version oracle that fingerprints cannot be.

**A side result, from the original agent and not re-checked by the verifier.** The literal index disagrees with current LLM-chosen vendor names, and in a sample of 25 disagreements it looked right 22 times. For example, `vendor/date-fns.js` is really `ms` (literal `"val is not a non-empty string or a valid number. val="`, `31556952`), and `vendor/escodegen/lib_3fbd658a.js` is ajv's codegen.

---

## 3. Step B: library code in `src/` with no boundaries

**Can we recover it? Yes, as candidates. Not yet as a verdict that hides code.**

What was found in Claude Code 2.1.216 `src/`: 0.8–1.16 MB of library code per method. The packages are zod, msal-common and msal-node, @anthropic-ai/sdk, the MCP SDK, jose, bedrock-runtime, axios, marked and lru-cache. This is a lower bound against the ~4 MB estimate from `declared-info.md`, because the index lacked Claude Code's exact versions and many of its packages.

**Precision on app code, measured on code the method was NOT tuned on:**

| Method                                                                                             | False library claims on real CC app code                                                                                                                                                                  |
| -------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Normalised function/statement hashes + S3 detector (content rule, runtime veto, neighbour support) | **0 real app bytes** in 7.3 MB of held-out app groups. The 690 bytes flagged are compiled TypeScript helpers and lodash listCacheClear, one of them under the wrong name ("zod").                         |
| Literal containment, per statement                                                                 | 18 of 686 claims (2.6%, ~11 KB), e.g. a 5 KB React spinner component named bowser. 8 of the 15 app claims go away when pre-bundled, minified and CDN files are dropped from the index, at no recall cost. |
| Unit hashes, k = 1                                                                                 | at least 2 app groups (one named marked, one named @anthropic-ai/sdk) plus 4 mis-named unindexed libraries (@xmldom named iconv-lite, the grpc-js server named tslib)                                     |
| 128-token sliding windows                                                                          | at least 2 found in 27 hand-checked hits: CC's own edit-distance function named commander, and an app zod schema named the MCP SDK. Windows also count app bytes at every edge.                           |
| Unbounded neighbour fill (any method)                                                              | unsafe: claimed 211 KB of app negatives and 646 KB of product-string code                                                                                                                                 |

**Known gaps in the best detector:**

- **Wrapper-group growth absorbs app code** when the index comes from another bundler: 553 of 9,525 lab app bytes (5.8%) went to lodash-es.
- **It labels the bundler runtime as a library** at every setting: 35–79% of runtime bytes.
- **Recall depends on version coverage:**
  - exact versions, same bundler family: 98%;
  - other versions only: 86%;
  - esbuild-only index: 87–89%;
  - one major version off: 14% (MSAL).

**Why "0 app false positives" kept failing.** The original negative sets were picked by product strings (tengu\_, CLAUDE.md, …), which excludes exactly the app code that looks like a library:

- generic helpers;
- re-implemented standard algorithms;
- app zod schemas;
- code speaking a library's protocol (`role`, `tool_use`);
- Claude Code's own forks of ink and commander.

The verifiers' held-out and hand-checked sets are the better bar.

---

## 4. Recommended design

**Pieces, in the plugin system's terms (`crates/humanify-core/src/toolchain.rs`).**

**Bundler-specific pieces**, chosen once per detected bundler, like the existing P17 `ModuleIdentitySource`:

- **Fingerprint normaliser:** Bun family vs esbuild. These are hash-only rewrite rules, with a hash-version stamp.
- **Wrapper stripper / boundary piece.** This is mostly there already (`ModuleWrapperGrammar`, `BundleLayout`):
  - CJS factory: exact boundary;
  - lazy ESM: end marker plus hoisted functions, with the namespace block and bare `var a,b;` dropped;
  - eager: no boundary.
- **Bundler runtime veto list.** It is a sibling of the never-rename lists (P7).
- **Which index slice to query:** Bun-built or esbuild-built.

**Bundler-independent pieces**, which never see which bundler this is:

- **Package readers**, extending `modules::identity::readers`.
- **The literal index**, built from npm source.
- **The structural index lookup** (data keyed by bundler family).
- **The claim combiner** that decides when evidence is enough.
- **Stop-lists:**
  - tslib, tsc and Babel helpers;
  - rolldown and other bundler runtimes;
  - the ljharb shim template;
  - Stainless-generated SDK code.
- **Package families:**
  - @azure/msal-\*;
  - @smithy/_ with @aws-sdk/_;
  - glob with its bundled minimatch, lru-cache and path-scurry;
  - lodash, lodash-es and es-toolkit;
  - uri-js and fast-uri.

**Build order and the gate for each step.** For any step that renames vendor files, the eval's `novel`/`realLn` must not move (band zero) and `vendorReal`/`vendorLn` must stay inside their bands or fall.

| #   | Step                                                                                                                                                                                                                                                                                                                                                                                                                                                                              | Gate before it lands                                                                                                                                                         |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0   | **Fix the known-global bug in `vendor_content_key`:** mask a minified name like `gc` when the bundle itself declares it. Seen in the lab: one Bun-minified module's `gc` broke 714 comparisons.                                                                                                                                                                                                                                                                                   | Red/green unit test with the real case; neutrality on one pair (expect it NOT neutral only where `gc`-style names occur; inspect the diff)                                   |
| 1   | **More declared readers:** VERSION constants, inlined package.json, scoped error prefixes, AWS Client/Command pairs, the strict hljs grammar shape.                                                                                                                                                                                                                                                                                                                               | Owner-known source census (precision per reader, as measured above); 0 hits on CC app negatives AND on the hard negatives (slash-command table, product code); e2e           |
| 2   | **Vendor naming for CJS factories and marker-bounded lazy modules** from (a) the existing `structuralHash` / normalised module hash, with exact-string agreement required, plus (b) the literal index without pre-bundled "sponge" files and refusing near-ties. Name only when both agree, or when one agrees with a declared reader. Floors: refuse empty or stringless cores and pure re-export/require glue; drop hashes held by more than one package; apply the stop-lists. | Precision on the three foreign corpora (82 / 75 / 1,221 packages) at an index size close to the one we will ship; CC audit of every name that changes; eval vendor columns   |
| 3   | **Port the normaliser rules to Rust**, next to `crates/humanify-core/src/hash/`. Hash-only, versioned.                                                                                                                                                                                                                                                                                                                                                                            | One unit test per rule from its real diff; Bun 1.2–1.4 stability ≥ 99% of units on the fresh corpus; rules may not change any existing pipeline hash unless versioned        |
| 4   | **Step B in report-only mode:** the sub-tree index plus the S3-style detector writes a diagnostics file of claimed `src/` spans, with no effect on output. Growth capped, runtime vetoed, gaps filled only when bounded (≤ 3–6 statements) and consistent.                                                                                                                                                                                                                        | Held-out CC app sets (0 real app bytes), a hand-review of a random sample of claims, MSAL banner recall, the foreign corpora                                                 |
| 5   | **Step B affects output** (e.g. moves a claimed span into `vendor/`) only when a structural AND a literal claim agree on the same package.                                                                                                                                                                                                                                                                                                                                        | A new scorer column for reclassified bytes; every reclassified app byte reviewed; eval `novel`/`realLn` explained line by line (hiding app change is the regression to fear) |
| —   | **The offline index builder**: a script outside the pipeline that installs each package version, builds it with Bun `--minify` through `require` AND `import()` entries, with the target's defines, and writes the index data file. It picks versions from what readers find in real bundles.                                                                                                                                                                                     | Reproducible from a version list; index stats vs this experiment's per-KB numbers                                                                                            |

**Thresholds to start from**, all to be re-derived at shipping index size:

- **Structural:** units of 30+ tokens; statement hits need ≥ 3 distinct content tokens; hits under 100 tokens need a same-package neighbour within ±10 statements.
- **Jaccard:** J ≥ 0.7, margin 0.1, filter > 2 packages, gate 20.
- **Literals:** ≥ 20 bits, cover ≥ 0.9, margin 10, vocabulary cap 2 bits, abstain at ≤ 5 literals.

---

## 5. Still unknown, and the next experiments

1. **Precision at shipping scale (the main open question).** Build a top-1,000 (or 2,000) index at a few versions per major. Re-measure on the three foreign corpora:

   - whole-module collision rates;
   - unit-vote false names;
   - the Jaccard filter level;
   - literal IDF and thresholds.

   Every verifier flagged that the thresholds were tuned at 176–347 packages.

2. **A hard, unselected app-negative set.** Hand-label a random sample of CC `src/` statements, not selected by product strings. Seed it with the known traps:

   - the edit-distance function;
   - app zod schemas;
   - the slash-command table;
   - transcript normalisers using `tool_use`;
   - CC's ink and commander forks.

   `immutable-literals/verify/cc-uncorroborated-2.1.216.txt` is a ready-made review queue.

3. **Two-family agreement (structural ∧ literal).** Measure recall and precision when both must agree. This is the design's main precision bet and is not yet measured.
4. **Exact-version index for CC's `src/` libraries:** MCP SDK, AWS SDK clients plus smithy, marked, execa, ink, lru-cache, chokidar, zod at the declared patch. This turns the ~4 MB estimate into a measured number. MSAL (94.3%, 0 false bytes) is the template.
5. **Stop-lists and package families.** Build them and measure how many of the 193 foreign mis-names and 195 out-of-index literal claims they remove.
6. **Recovering boundaries.** Recover the 10.8% of Bun lazy modules whose `__esm` end marker is dropped, and cap wrapper-group growth so it cannot absorb app code or the runtime.
7. **Untested inputs:** webpack, rollup's CJS wrapper, Bun `--compile` / `--bytecode` / `--target bun`, `--mangle-props`, and other define sets.
8. **Version pinning.** Report a version neighbourhood from fingerprints. Take the exact version only from declared readers; the hljs 11.11.1 pin rests on one grammar.

## Where the evidence is

- **Per-approach READMEs** (exact commands): `*/README.md`.
- **Verifier write-ups:** `subtree-hashes/VERIFY.md`, `immutable-literals/VERIFY.md` and `bundler-variation-and-boundaries/VERIFY.md`, plus each folder's `verify/`. The `exact-module-hash` and `ngram-minhash` verifiers could not write a VERIFY.md; their findings are in this synthesis, and their scripts are in `verify/`.

---

## 6. Follow-up run (2026-10-11): steps 0–1 landed, scale and negatives measured

Retractions and updates to the sections above live here (rule 9).

- **Step 0 (bug fix, branch `exp095-step0-bundle-globals`, 330f55d0).** A free name is a real global only if the program does not declare it. Reachable only with a TS-era prior (seeded/archive bases); fires on 2.1.85 only (protobufjs minified to `gc`, 9 modules). Warm neutrality on 2.1.85: 31 vendor files differ, all the 9 modules' `lib_` ids; `src/` identical.
- **Step 1 (readers, branch `exp095-step1-readers`, 6a5feeb2, finding #96).** package.json in any key order (518/518), strict highlight.js (1,123/1,123; off for app code because of Cedar), VERSION constants (53/53, version only), AWS Client pairs (23/23, hint only). 0 hits on CC app code. 2.1.216 stub run: 1 vendor rename (sharp `package.json.js`), 0 `src/` diffs; 175 report-only `src/` rows (`appDeclaredLibraries` in diagnostics).
- **Eval `exp095-steps01-d9e990e3` (cold, scratch):** novel/realLn byte-equal; treeLn 143,776 (−821 vs e214db7a); mints 236; self-hop 14 ln; masked whole-tree check 13 lines missing (refs differ 0–6/pair). `layoutReal` −1,063 vs e214db7a is `src/`-only on 2.1.86 (cold naming draws; steps 0–1 cannot reach `src/` on scratch bases); vendorLn +174 is manifest line-scoring of more identity blocks (vendorReal unchanged).
- **Scale (`scale/README.md`).** Top-2,000 roots (2,561 pkgs, 6,191 versions, 24,618 builds). Index without shingles: 10.7 MB at 1,000, 24.1 MB at 2,000 (both families). Every single evidence family's wrong names grow with scale (unit votes 12.8%, literals 6.3% at 2,521 pkgs); **structural AND literal agreement stays flat at ~0.2%** from 753 pkgs up, recall 51%. New decisive rule: **specificity** (matching index file at most ~3–4× the query). Scope + scoped-fork families halve the rest. On CC 2.1.216 vendor: 710/1,647 named, 371 change today's name; 39/40 random changes correct; 0 contradict a declared identity.
- **Negatives (`negatives/README.md`).** 620-statement random sample of CC 2.1.216 `src/` (seed 95), blind re-label 124/124 agree: **library ≈ 14.5% of src bytes (CI 11.2–18.1%)**, plus 3.5% app forks of libraries (ink, yoga port, commander-style help). Step B with exact-version, target-bun builds: 81.5% recall, 0 false claims on sampled app bytes; outside the sample it claimed 383 B of CC's ink fork as `ink` — forks are the open precision risk for Step B.
- **Coverage gap.** Download rank misses the packages big apps are made of: ink 4,570, client-bedrock-runtime 4,064, @azure/identity 3,348, growthbook 10,253, @anthropic-ai/bedrock-sdk 12,305. Next: pick packages by bundled-app dependency closure weighted by bytes, and measure version density (every release of ~100 packages) with `humanify-index`.
- **Index builder (`crates/humanify-index`, e74928a2, finding #97; `docs/design/library-index.md`).** Separate binary; fingerprints via humanify-core; `--as-of` (publish time ≤ date, deps installed with `npm --before`); shards per package version; versions as ranges; index id = manifest sha256.
- **Library discovery (`discovery/`, `docs/design/library-discovery.md`; lab, CC 2.1.216).**
  - **Closure alone is not evidence.** Every statement's downward closure is closed and one-directional. At a 64 KB
    cap, closed regions hold 82% of library bytes but also 5.5 MB of app code (25% library vs 14.5% base).
  - **Leaf-inward fixpoint.** Seeded with the declared readers plus Step B index C, and using private-code / glue /
    closure-guarded module fill, it adds 64 KB over 8 rounds with 0 sampled app bytes: estimated recall 85.7% →
    93.7%, about +3% in exact bytes.
  - **Isolation veto.** Dropping an index claim that has no same-family graph neighbour removes every ink-fork claim
    and costs ≈0 recall.
  - **Content floor on closure starts.** It is required: one 11-byte mis-claim otherwise drags in a whole app module.

---

## 7. Which packages and which versions to index (2026-10-12; `selection/`, `density/`)

- **Packages: rank by what bundled apps contain, priced at the index's real cost.** A corpus of 338 bundled apps
  (166 Electron, 170 npm CLIs, 2 agent CLIs; 1,577 MB of library code, 67 held out). At an equal real index size the
  app-closure greedy recognises 20.7% vs ~17.5% (98 MB) and 22.8% vs ~20.4% (148 MB) of held-out apps' library
  bytes, against download rank, wrong names 0.03–0.06% (all copies or sourcemap errors by hand). Pricing packages by
  their bundled code instead gives no gain: ~97% of an index is literal rows from every source file installed, so an
  owner's index cost is 0.9–11.7× its bundled code. Neither ranking reaches Claude Code's rare packages (Bedrock
  runtime, growthbook, bedrock-sdk: 0 of 338 apps; ink: 3) — those come from the bundle's own readers (pins).
- **Versions: the exact version carries about a third of recall.** Every release of 101 packages (11,303 versions,
  Bun and esbuild): all releases 86.4% (download-weighted); without the bundle's version, nearest patch 54.8%, same
  minor 57.5%, release before 43.8%, other majors 11.9%. Single pre-built files break on any change. Claude Code's
  packages are kinder (nearest patch keeps 85% of all-releases recall).
- **Recommended density: every release of the last 12 months + the latest patch of every older minor** — 83.0%
  (vs 86.7% for all releases; 54.8% for today's as-of policy), 96.5% of the all-releases recall on Claude Code, at
  60% of all-releases' literal bytes. Identical rows across releases collapse 16× (literal) / 9.6× (module).
- **Builder fixes and findings.** `utf16_to_byte_offsets` was quadratic on one-line minified bundles (fixed with a
  red/green test). Bun 1.4.0 sourcemaps credit whole modules to the wrong file in large bundles (5.5% of held-out
  keyed library bytes; the builder labels rows the same way — add a one-file-one-module check). Only ~30% of the
  literal table is files a bundle contains (restrict the shard: ~2–3× smaller). Subpath-only packages
  (@modelcontextprotocol/sdk, @aws-sdk/nested-clients) produce no modules: the recipe needs `exports` entries.
