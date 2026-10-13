# exp095 lane "misses": why the library index misses real library code, and what would match better

## STATUS (2026-10-13)

- **State.** DONE (lab only). Branch `exp095-match-misses` (from `exp095-index-v2`), not merged. Nothing under
  `crates/` changed; every tool here is a lab script or a throwaway Rust proto outside the workspace
  (`proto/`), linking `humanify-core` / `humanify-index` read-only. No pipeline behaviour changed, so no gate or
  neutrality run is needed.
- **Verdict, totals first.**

  - **Most missed library code is not in the index at all.** The matcher is not the main problem:
    - held-out apps: **74.5%** of library statement bytes belong to packages the index does not hold (CI 51–85%);
    - Claude Code 2.1.216: **33.8%** (MCP SDK, @anthropic-ai/sdk, growthbook, MSAL);
    - minified wrangler: **16.2%** not in, plus **21.2%** "package in, version not".
  - **Where the code IS in the index, the hash usually matches.** On held-out apps, of the library bytes whose
    package is indexed, **72.6%** are named; the rest is a dependency indexed at the wrong versions (8.4%), the
    discovery veto throwing correct claims away (8.3%, almost all highlight.js), units too small to look up (5.5%),
    and real hash differences (3.9%).
  - **Hash differences are a build-recipe problem more than a minifier problem.** The two big ones:
    - **Claude Code is built for `--target bun`; the index is built for `--target node`.** For packages with a
      `module` field and no `exports` map (all of AWS SDK v3, smithy, …) Bun then bundles a different FILE
      (`dist-es` vs `dist-cjs`), and it escapes non-ASCII strings differently. Building the exact versions with
      `--target bun` raises Claude Code index seeds from **52.1% to 71.2%** of library bytes (0 app, 0 wrong
      family); normalising the hash instead gets 52.1% → 59.0%.
    - **minified wrangler: 61% of its library statement bytes are whole CommonJS modules left as ONE statement**,
      because this branch's unpack does not recognise minified esbuild's `__commonJS` (fixed on main, 68b090e3,
      finding #96). The index splits module bodies into statements, so the two can never meet.
  - **Minifier syntax rewrites matter only across minifier pipelines.** `const`↔`let` is the one that pays: a light
    normaliser lifts wrangler (esbuild re-minified) index seeds from 20.9% to 36.3% and Claude Code from 25.8% to
    32.0%; held-out apps (built exactly like the index) do not move.
  - **Better matching ideas, measured on index seeds (no fixpoint), app code claimed 0 in every row:**

    | idea                                                                                  | held-out (struct / agree) | Claude Code (struct / agree) | wrong-family share of named           |
    | ------------------------------------------------------------------------------------- | ------------------------- | ---------------------------- | ------------------------------------- |
    | index built with the TARGET's bundler settings (exact versions, `--target bun`)       | —                         | 52.1% → **71.2%** (struct)   | 0 → 0                                 |
    | light normaliser (template raw spellings, `const`/`let`, function/class decl vs expr) | +0.0 / —                  | **+6.3** / **+6.3**          | flat (wrangler: **+15.4 / +9.8**)     |
    | + one print per binding (`var x=e` ≡ `x=e`, split/joined declarations)                | +0.3 / +0.1               | +0 / +0                      | flat                                  |
    | function prints from 20 nodes (was 30), support rule kept                             | +1.0 / +0.1               | +2.7 / +1.3                  | flat (8.7% / 4.0%)                    |
    | literal agreement over a REGION of same-family claims, not one statement              | — / **+1.4**              | — / +1.3                     | 4.3% → 4.6% (added: vendored copies)  |
    | agreement claims exempt from the isolation veto (from the lane's own reports)         | — / **+3.5**              | — / +2.6 (est.)              | 37 KB vendored copies, 0 app          |
    | one rare long literal, alone                                                          | 4.3% recall alone         | —                            | **20% — unsafe** (pre-bundled copies) |
    | the dependency's exact version in the index (held-out class b, 9 package versions)    | 89% of those bytes found  | —                            | —                                     |

  - **Recommended matching design** (§4): fix coverage and the build recipe first (they hold most of the misses),
    then keep exact unit hashes with a light, versioned normaliser plus binding prints, and name with two kinds of
    evidence — structural plus literal agreement taken over a region, with agreement-backed claims exempt from the
    isolation veto. Retrieve-then-verify (MinHash, alignment) has little left to fix once the code is indexed and
    built the same way: real hash differences are ~4% of indexed bytes on held-out apps.

## 1. What we hash (plain language)

A bundle is cut into **statement units**: each top-level statement (a `var` with several declarators gives one unit
per declarator), each statement inside a lazy ES module's init function. A CommonJS module (`__commonJS`) is one
external unit in the bundle being processed; the index side splits its body into statements too. For each unit the
index and the lookup compute the same thing (`libindex::units`):

- **the whole statement's hash**: the MatchKey canonical form — local names replaced by numbered slots, string
  values blurred to their length, numbers and global names kept, private `#names` blinded;
- **one hash per function inside it**, same form;
- a node count and a content-token count (strings, property names, regexes, globals).

A unit is claimed when its whole hash (≥ 30 nodes, ≥ 3 content tokens) or its largest non-overlapping function
hashes (≥ 30 nodes) covering half its nodes are held by 1–3 packages, in an index module at most 4× the query's
module + 4 KB (**specificity**), and a small claim (< 100 nodes) has a same-package claim within 10 statements
(**support**). The **agreement** rule (the default) also requires the literal rule — IDF-weighted literal
containment against npm source files, ≥ 6 literals, ≥ 20 bits, 90% cover, 10-bit margin — to name the same family
on the SAME statement's literals. Library discovery then drops an index claim with no same-family graph neighbour
(the **isolation veto**) and grows claims along the reference graph.

The index (`b00fa735…`) holds these hashes for 20,847 package versions built by Bun 1.4.0 and esbuild 0.28.2,
`--minify --target node` (Bun) / `--platform=node` (esbuild), through a `require()` entry and an `import()` entry.

## 2. Task 1 — miss decomposition

**Method.** Every statement unit of each test bundle gets the truth's package (held-out and wrangler: the majority
of its mapped bytes in the exact sourcemap, the same labeller `indexv2/scripts/score_claims.py` uses; Claude Code:
the negatives lane's 620-statement labelled random sample) and, where known, the version (the `package.json` beside
the mapped file; wrangler's pnpm path; Claude Code's exact-version specs). Every library unit the lane's reports
(`/work/exp095-index-v2-data/eval/v2*`) do not name correctly (right package or same family, after the fixpoint)
gets ONE cause, first match wins:

| class | meaning                                                                                                                                                          |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| a     | package not in the index                                                                                                                                         |
| b     | package in, this version not, and no indexed version's rows of the family cover half the unit                                                                    |
| c     | cannot be looked up: under the floors (30 nodes; 3 content tokens for a whole statement), even if every eligible print hit                                       |
| e     | the hash IS there under the right family (cover ≥ half, guards off); a guard refused it: specificity, ambiguity (> 3 owners, or a tie), support, agreement, veto |
| f     | the code is in the index under ANOTHER package (vendored or pre-bundled copy, fork), or a claim named another family                                             |
| d     | the version is indexed and the hash differs; sub-classed by diffing the two canonical forms (`scripts/diffd.py`, `scripts/dsub.py`)                              |

The structural part of the rule is re-run in Python (`scripts/mmlib.py`) to attribute e-misses to a guard; it
reproduces the in-run index seeds exactly on the apps checked (59/59, 235/235 claims). Shares are of library
statement bytes; 95% intervals by bootstrap (held-out: resampling the 30 apps that have library statement bytes;
wrangler: units; Claude Code: the stratified sample). `d:` sub-classes are in §2.1.

**Structural rule (`AGREEMENT=0`):**

| class                                          | Claude Code 2.1.216 | held-out apps (all) | held-out, package in index | minified wrangler |
| ---------------------------------------------- | ------------------- | ------------------- | -------------------------- | ----------------- |
| library statement bytes                        | 2.22 MB (est.)      | 39.1 MB             | 9.9 MB                     | 1.92 MB           |
| **named (recall)**                             | **37.1%** (26–50)   | **18.5%** (11–35)   | **72.6%** (61–84)          | **25.6%** (19–33) |
| a — package not in index                       | **33.8%** (22–46)   | **74.5%** (51–85)   | —                          | 16.2% (13–21)     |
| b — version not in, no version has it          | 8.0% (3–14)         | 2.1% (0–7)          | 8.4% (1–21)                | **21.2%** (17–27) |
| c — too small / under floors                   | 1.0% (0–2)          | 1.4% (1–3)          | 5.5% (4–8)                 | 10.7% (2–21)      |
| d — version in, hash differs                   | **17.4%**           | 1.0%                | 3.9%                       | **22.6%**         |
| · d: unit model (whole CJS module)             | —                   | —                   | —                          | 15.9% (11–21)     |
| · d: decl vs assignment / ESM-vs-CJS build     | 8.1% (2–17)         | 0.0%                | 0.1%                       | 0.1%              |
| · d: string escapes (`\uXXXX` vs UTF-8)        | 6.3% (0–16)         | —                   | —                          | —                 |
| · d: other file of the package / not built     | —                   | 0.3%                | 1.4%                       | 2.0%              |
| · d: interop wrapping (`x.default`, `require`) | —                   | 0.2%                | 0.8%                       | 0.0%              |
| · d: lazy vs eager init wrapper                | —                   | —                   | —                          | 1.1%              |
| · d: minifier syntax                           | —                   | —                   | —                          | 0.7%              |
| · d: row missing in the index (builder)        | —                   | 0.1%                | 0.4%                       | —                 |
| · d: generated `__export` table / unclear      | 3.0%                | 0.3%                | 1.0%                       | 2.7% (no build)   |
| e — hash matches, a guard refused it           | 2.7% (veto 2.0)     | 2.2% (veto 2.1)     | 8.6% (veto 8.3)            | 2.7% (veto 1.6)   |
| f — matches / named another package            | 0                   | 0.2%                | 0.9%                       | 0.8%              |

**Agreement rule (the default)** moves bytes from "named" to **e-agree** (the literal rule refused or named
another family): Claude Code named 23.0% (e-agree 8.3%, veto 2.6%); held-out 12.0% (e-agree 3.3%, veto 3.5%; of
indexed packages: named 49.4%, e-agree 13.4%, veto 14.3%); wrangler 16.8% (e-agree 5.8%). Every other class is
the same within a point. Full tables with intervals: `$MM/decomp/table.json` (re-run below).

**Read by hand, per class:**

- **a.** Held-out: concentrated — one app's `@paulirish/trace_engine` is 9.3 MB (24% of all held-out library
  statement bytes), then `@baszalmstra/rattler` 3.9 MB, mermaid, `@renovatebot/pgp`, Sentry, the AI SDK, puppeteer.
  Claude Code: `@modelcontextprotocol/sdk` 1.29.0, `@anthropic-ai/sdk` 0.94.0, growthbook, `@azure/msal-*` (named
  only by the kept banner), mcpb, sandbox-runtime. These need the bundle's own readers (pins), not a better match.
- **b.** Held-out: **99.98% are packages indexed only as some root's DEPENDENCY**, at whatever versions those roots
  pulled in: terser 5.51.2 (index: 4.8–5.6), got 14–16 (index: ≤ 13), `@sindresorhus/is`, ky. Building the exact
  versions of 9 of them (`humanify-index build`, pinned) finds **89%** of their bytes (624 of 705 KB; 59 KB too
  small, 22 KB still different). Wrangler: 71% are ROOT packages at a patch the `year-all+minor` policy dropped
  (yargs 17.7.2 vs 17.7.3, zod 3.22.3, ws 8.18.0, AWS 3.721.0); building those exact versions finds **0 of
  197 KB** — the bundle holds their ESM build or a whole CommonJS module, the index their CJS build (see d).
  Claude Code: smithy packages of unknown version, and lodash-es 4.17.21.
- **c.** Two kinds: data blobs that are big in bytes but tiny in nodes (wrangler: two llhttp wasm base64 strings,
  74 KB each, and the emoji regexes; held-out: `entities` decode tables, `uc.micro` regexes, number tables) — 27%
  of held-out c bytes are units ≥ 1 KB; and the bare `var a,b,c;` lists and one-line statements Bun hoists out of
  lazy modules (59% of held-out c bytes are units < 100 B), which only the graph fixpoint can claim.
- **d.** See §2.1.
- **e.** The isolation veto dominates: highlight.js grammars are independent functions with no same-family graph
  neighbour (714 KB held-out). Under agreement, the vetoed claims are **97% right** (1.37 MB right, 37 KB vendored
  copies, 0 app). Specificity and ambiguity together are < 0.1% everywhere.
- **f.** Vendored copies the truth credits to the shipping package (zod in claude-agent-sdk, js-yaml in mermaid,
  fflate in openpgp), forks (`figures` / `@inquirer/figures`, `chalk` / its vendored ansi-styles), and coincidence
  (SHA-256 constant tables shared by hash.js and @noble/hashes).

### 2.1 What actually differs (class d, canonical-form diffs)

For each class-d unit the index's own build of that version (the indexv2 lane's kept recipe/4 builds) is dumped
with the same unit model, the counterpart found through its sourcemap (same file, nearest original position, best
token similarity; Claude Code: best similarity over the package), and the two canonical forms diffed token by token.
Totals per test set: `$MM/diffd/<set>.sub.json`; every row with its diff: `$MM/diffd/<set>.jsonl`.

- **Claude Code — the index was built for another target.** Bun with `--target bun` (Claude Code) resolves a
  package's `module` field; `--target node` (the index) resolves `main`. Checked directly on
  `@aws-sdk/client-bedrock@3.936.0`: the index's `require` and `import()` builds take 6 `dist-cjs` files; the same
  `import()` built for `--target bun` takes 128 `dist-es` files. The index holds **0 `dist-es` rows of any
  `@aws-sdk` package**. So the bundle's `a=[…],b=[…]` assignment sequences inside lazy ESM inits meet `var a=[…],b=[…]`
  inside a CJS factory, built from different source. The zod locale files differ only by escapes: the target-bun
  output writes `எ`, the target-node output writes UTF-8, and MatchKey keeps a template's RAW length. Both
  are cured by building for the target: 73% of Claude Code's class-d bytes are found by exact `--target bun`
  builds, 0% by exact `--target node` builds.
- **wrangler — the unit model.** 70% of class-d bytes are whole CommonJS modules kept as one statement (the
  minified `__commonJS({"path"(e,t){…}})` helper is not recognised on this branch; main fixed it, 68b090e3). The
  rest: an esbuild lazy init wrapper around code the bundle emits eagerly (`var X=(function(){…X=class…})` vs
  `var X=class…`), and minifier syntax — `const` vs `let` (esbuild `--minify` turns top-level `const` into `let`;
  the bundle was minified from an unminified build), `let a;let b` vs `let a,b`, a `const` inlined as its number.
- **held-out apps (Bun `--target node`, same as the index; 391 KB).**
  - interop (22%): `import_x.default` / `x.AsyncResource` namespace member access vs a direct binding, and a
    `require("node:x")` inside a lazy init vs hoisted;
  - a counterpart in another file of the package (23%): the index's Bun build resolves the **`development` export
    condition** (`micromark*`: `dev/lib/*.js`) where the app's build got `lib/*.js` (Bun picks production files
    only with `--production`, checked on micromark 4.0.3), or the CJS `lib/` vs the ESM `es/` (highlight.js);
  - unclear (21%): mostly the truth — Bun 1.4.0's sourcemap credits a cli-spinners object to `ora/index.js` and to
    `get-east-asian-width/lookup-data.js`, and pre-bundled `index.min.js` files (tar);
  - **row missing (16%)**: the index's own build holds the identical unit, but no row was written. They are
    Bun-generated statements (`__export(ns, {a: () => a, …})` tables, small re-export glue) with no sourcemap
    segment starting inside them: `unit_rows` files them as unowned and `fingerprint_bundle` does not carry the
    unit-level unowned count into the shard notes, so the drop is silent;
  - generated export tables the map credits to some package (6%), declaration vs assignment (2%), a string
    constant of another length (2%; build paths, `import.meta.url` shims).

## 3. Task 2 — better matching ideas, tested

**Setup.** Two indexes:

- the real index `b00fa735…` for rules that only change the lookup;
- an **alternative index built from the indexv2 lane's kept builds** (`selABC/work`: 24,162 package versions,
  44,607 Bun and 44,499 esbuild builds, recipe/4) for ideas that change the fingerprint. Every variant is computed
  over the same builds by one tool (`proto/src/variant_rows.rs`), whose baseline rows reproduce the builder's shard
  rows exactly (zod 4.6.6: 3,768/3,768; node-fetch 3.3.2: 385/385). This index holds more of the held-out apps'
  current versions than the real one, so compare each variant with ITS baseline row.

Claims are the pipeline's own `claim_units` (`proto/src/variant_lookup.rs`); it reproduces the real index's seeds
(structural 9,578 of 9,585 held-out claims, agreement 3,515 of 3,519). Scored as index seeds: no fixpoint, no veto.
Held-out precision: bytes the map gives to app code (`<app>`) and to another family; Claude Code: labelled app
units in the 620-statement sample. "App" is **0 in every row** below (held-out `<self>` — the CLI's own package,
e.g. vite's pre-bundled parse5 — is reported separately in `score.json`).

**Recall of index seeds (share of library statement bytes); wrong = other-family share of named bytes:**

| index / variant                                   | held-out struct | held-out agree | CC struct | CC agree  | wrangler struct | wrangler agree | wrong (held-out struct / agree) |
| ------------------------------------------------- | --------------- | -------------- | --------- | --------- | --------------- | -------------- | ------------------------------- |
| real index, today's prints                        | 17.4%           | 12.8%          | 25.0%     | 13.7%     | 20.7%           | 14.6%          | 12.0% / 4.3%                    |
| alt index, today's prints (**baseline**)          | 24.3%           | 12.8%          | 25.8%     | 14.5%     | 20.9%           | 14.6%          | 8.7% / 4.0%                     |
| alt, normaliser N                                 | 24.3%           | —              | 32.0%     | —         | **36.3%**       | —              | 8.7%                            |
| alt, N + binding prints                           | 24.6%           | 13.0%          | 32.0%     | **20.7%** | 37.2%           | **24.4%**      | 8.8% / 4.0%                     |
| alt, function prints from 20 nodes                | 25.3%           | 13.0%          | 28.5%     | 15.8%     | 22.8%           | 15.3%          | 8.7% / 4.0%                     |
| alt, N + binding prints + 20 nodes                | 25.5%           | —              | 35.9%     | —         | 41.7%           | —              | 8.8%                            |
| exact CC versions only, `--target node`           | —               | —              | 52.1%     | —         | —               | —              | 0                               |
| exact CC versions only, `--target node`, N + bind | —               | —              | 59.0%     | —         | —               | —              | 0                               |
| exact CC versions only, **`--target bun`**        | —               | —              | **71.2%** | —         | —               | —              | 0                               |
| exact CC versions only, `--target bun`, N + bind  | —               | —              | 71.9%     | —         | —               | —              | 0                               |
| real index, agreement + REGION agreement (gap 3)  | —               | **14.2%**      | —         | 14.9%     | —               | 14.6%          | 4.6% (agree)                    |
| real index, rare long literal alone               | 4.3%            | —              | 11.9%     | —         | 29.2%           | —              | **20.3%**                       |

N = template raw spellings blurred, `const`/`let` → `var`, function and class declaration ≡ expression.
Binding prints = one print per declarator init / `x = e` right-hand side / declared function or class, looked up
like function prints. Region agreement = consecutive structural claims of one family (≤ 3 unclaimed statements
between) pool their literals, and the region keeps its claims when the literal rule on the pool names that family.

**What each result means.**

1. **Normaliser (idea 1).** Worth it exactly where the target's build differs from the index's: +6.3 points on
   Claude Code (the zod locale escapes), **+15.4 structural / +9.8 agreement on wrangler** (the class-d diffs on
   wrangler are dominated by the declaration keyword — the re-minified bundle keeps `const` where the index's
   esbuild `--minify` build writes `let`; that the gain is the keyword rather than the other rules of N is read
   from those diffs, not ablated. Most of it sits in function prints inside the whole-module statements Task 1
   files under "unit model", which is why Task 1's own "minifier syntax" row is only 0.7%), ~0 on held-out apps built exactly like the index. Under agreement it
   adds no wrong family on wrangler; structural wrong share 2.8% → 6.0% there (ws inside vitest, cross-spawn and
   which inside tinyexec: vendored copies). The exp095 33-rule Bun normaliser was not ported: Task 1 found no
   Bun-version drift in these targets; the rules that pay are the few above.
2. **Build like the target (idea 2) is the big lever:** +19 points on Claude Code over the same exact versions
   built for node, more than every normalisation combined (+7). It needs a slice per target setting (Bun
   `--target bun` vs `node`, the `production` condition), not more hashing rules.
3. **Retrieve-then-verify (idea 3).** Measured its two halves cheaply instead of building it:
   - retrieval by literals alone is not safe — one rare long literal names the wrong package for 20% of
     held-out bytes (typescript's and prettier's pre-bundled copies, uglify-js for terser, entities inside
     node-html-parser);
   - verification by literals over a REGION instead of a statement recovers a third of what statement-level
     agreement refuses (+1.4 points held-out, +1.3 Claude Code), adding only vendored copies as wrong names.
   - A MinHash/alignment verifier would target class d, which is ~4% of indexed bytes on held-out apps and is
     mostly fixed by the recipe on Claude Code. Not built.
4. **Finer units (idea 4).** Function prints from 20 nodes add +1.0 / +2.7 points structural at the same
   wrong-family share; under agreement +0.1 / +1.3. Binding prints add +0.3 held-out. Small, safe, cheap.
5. **From Task 1:** exempting agreement-backed claims from the isolation veto is worth more than any fingerprint
   change on held-out apps (+3.5 points, 97% right, the wrong 3% vendored copies). On Claude Code under
   agreement the veto removed bidi-js, zod, axios, jose, p-map (36 KB per release on this index; 160 KB with the
   exact-version index C), and no fork-family claim in any of the six runs read (ink, yoga, commander).

Added claims read by hand (`$MM/vl/*/added-samples.json`): right ones are ordinary library statements (got's
`RetryError`, micromark character classes, compress-commons buffers); wrong ones are vendored copies (zod inside
claude-agent-sdk and mcp-proxy, angular's compiler inside prettier), family neighbours the families file does not
list (micromark vs micromark-core-commonmark), and coincidences (SHA-256 constant tables, 1.7 KB of color-convert
on signal-exit).

## 4. Recommended matching design

In order of measured effect:

1. **Coverage: index what bundles ship.** Classes a and b are 77% (held-out), 42% (Claude Code) and 37%
   (wrangler) of library bytes. Readers' pins for the app's own libraries (MCP SDK, @anthropic-ai/sdk); for
   dependencies, the versions apps resolve TODAY, not whatever versions old roots pulled in; and the patch the
   bundle carries, not only the latest patch of a minor (wrangler).
2. **Build the index the way the target was built.** One slice per detected setting: Bun `--target bun`
   (resolves `module`, escapes non-ASCII), Bun `--target node`, esbuild; build with the `production` condition
   (`--production`) as apps do; keep both the ESM and the CJS build of a dual package. This is the
   detection-built-pipeline rule applied to the index: the slice is chosen from the target's detection.
3. **Keep exact unit hashes, add a light versioned normaliser and binding prints.** N (template raw, declaration
   keywords, decl/expr forms) plus one print per binding, function prints from 20 nodes with the support rule.
   Hash-only, a new `units/3` fingerprint version.
4. **Name with two kinds of evidence, at region scale.** Structural hit, then the literal rule over the region of
   consecutive same-family claims; agreement-backed claims exempt from the isolation veto (keep the veto for
   structural-only claims, where the ink fork lives).
5. **Builder fixes found here** (each a red/green test when it lands):
   - count, and decide about, units dropped as unowned (no sourcemap segment inside); today they vanish silently
     (`fingerprint_bundle` ignores `unit_rows`' unowned count);
   - the `development` export condition (Bun without `--production`);
   - the as-of/patch policy for packages a bundle pins.
6. **Not recommended now:** a full MinHash or alignment verifier (class d is small once 1–2 hold), long literals
   as standalone evidence, the 33-rule Bun normaliser (no Bun-version drift in these targets).

## 5. What is not done

- **wrangler on main's unpack.** 61% of its library statement bytes are whole CJS modules only because this branch
  predates 68b090e3; re-run the decomposition on a branch with that fix, where they become factories (whole-module
  keys) and leave the statement scope.
- **Ideas 2–4 on the REAL index.** The fingerprint variants were measured on the alternative index of kept builds;
  the target-bun result on Claude Code's exact versions only (no foreign packages, so no wrong-family test at
  scale). Building a `--target bun` slice of the real index and re-running the claim rule is the next step.
- **Precision on Claude Code is the labelled sample** (≈ 510 app units); the held-out apps are the larger
  precision set. No fixpoint/veto run was made with the variants (seeds only).
- Class d sub-classes on Claude Code rest on 10 sample units (wide intervals).
- `typescript`-sized bundles (> 3 MB, 115 builds) are missing from the alternative index's esbuild slice and 16
  from its Bun slice (the variant dump is slow on them).

## Re-run

```bash
export PATH="$HOME/.cargo/bin:$PATH"; MM=<scratch>; V=/work/exp095-library-fingerprints/scale/.venv/bin/python
S=experiments/095-library-fingerprints/misses/scripts
CARGO_TARGET_DIR=$MM/target cargo build --release --manifest-path experiments/095-library-fingerprints/misses/proto/Cargo.toml
# Task 1
$MM/target/release/exp095-unit-dump <bundle> $MM/dump/<name>.jsonl               # every test bundle
$V -I $S/truth.py heldout <slug> <bundle> <app>; $V -I $S/truth.py wrangler; $V -I $S/truth.py cc
$V -I $S/decompose.py heldout|wrangler|cc        # classes a–f per unit
$V -I $S/diffd.py heldout|wrangler|cc            # class d: the index build's counterpart, canonical diff
$V -I $S/dsub.py <set> [--show <subclass>]       # class d sub-classes
$V -I $S/table.py                                # the tables with bootstrap intervals
$V -I $S/veto.py                                 # vetoed seeds judged
# Task 2
exp095-variant-rows <jobs: index|query, bundle, out, bundler>      # base / norm / bind prints, both sides
exp095-variant-lookup base|norm|normbind <rows dir> <literals|-> <jobs>   # env AGREEMENT, MIN_NODES
exp095-region-agree <literals> <jobs>; exp095-long-literal <literals> <jobs>
$V -I $S/veval.py <claims dir> [<baseline claims dir>]
```

## Files

- `scripts/`: `mmlib.py` (index rows, the structural rule's emulation), `truth.py`, `decompose.py`, `diffd.py`,
  `dsub.py`, `table.py`, `veto.py`, `veval.py`.
- `proto/` (not in the workspace): `unit_dump.rs`, `variant_rows.rs`, `variant_lookup.rs`, `region_agree.rs`,
  `long_literal.rs`.
- Bulk data in the session scratchpad (`$MM`): dumps, truth, decomposition rows, diffs, the alternative index's
  rows (6 GB Bun, esbuild), claim files and scores; the exact-version rebuilds in `$MM/rb`.
