# exp095 lab: labelled library modules in minified bundles

This is the shared lab for exp095 (could we ship an index of fingerprints of npm library modules?). It holds real
bundles of real npm packages, built with several bundlers, minified and not. **Every module span in every output is
labelled** with its package, version and source file. It also holds a labelled real-world set taken from Claude Code.

Built 2026-10-11. Nothing in the humanify repo was touched.

## Totals

| what                                                                   | count                                                                                                                                              |
| ---------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| catalog packages (direct)                                              | 46 (`scripts/catalog.json`)                                                                                                                        |
| package@versions installed (direct)                                    | 164 (`versions.json`; latest, earlier patch, previous minor, previous major, plus a CJS-era "extra" for 10 packages)                               |
| distinct package@versions that appear in outputs (direct + transitive) | 315 (176 packages)                                                                                                                                 |
| entry apps                                                             | 164 single-package apps × up to 4 modes, plus 6 multi-package "combo" apps = 617 entries                                                           |
| bundler configs                                                        | 5 bundlers × {min, unmin}: bun 1.3.14, bun 1.4.0 (the version Claude Code uses), bun 1.4.3, esbuild 0.28.2, rollup 4.64.5 (+ terser when minified) |
| builds                                                                 | 6,184 (36 failed; see Gaps)                                                                                                                        |
| labelled spans (`labels/all.jsonl`)                                    | 321,619                                                                                                                                            |
| minified LIBRARY spans                                                 | 140,967: 56,849 cjs (166 MB), 37,159 lazy-esm (35 MB), 46,959 eager-esm (52 MB)                                                                    |
| distinct (package, version, file) in minified builds                   | 8,211                                                                                                                                              |
| Claude Code positives                                                  | 214 per version (2.1.198, 2.1.216), all mapped to the exact raw factory                                                                            |
| Claude Code negatives                                                  | 100 module groups + 208 / 222 single functions per version                                                                                         |

Disk: builds 4.8 GB, pkgs 718 MB, labels 169 MB, realworld 18 MB.

## Layout

```
lab/
  README.md              this file
  versions.json          package -> {latest, patch, minor, major, extra} versions used
  scripts/               everything that made the lab (all re-runnable)
  tools/                 esbuild 0.28.2, rollup 4.64.5 + plugins, terser, acorn; bun-1.4.0/ and bun-1.4.3/ (npm `bun`)
  pkgs/<pkg>@<ver>/      one npm project per package version (`/` in a scoped name -> `__`)
      node_modules/      installed with --ignore-scripts; `//# sourceMappingURL` lines stripped (see "Normalisation")
      exports.json       export names per specifier (node import()), used to pick the `some` imports
      entries/           req.cjs  imp.mjs  some.mjs  dyn.mjs   (the entry apps)
  pkgs/combo-<n>@0.0.0/  6 multi-package apps: app/*.mjs|cjs (fake app code) + entries/main.mjs
  builds/<bundler>/<min|unmin>/<app>/<mode>/
      out.js             the bundle (Bun adds a trailing `//# debugId=` line, rollup a `//# sourceMappingURL=` line)
      out.js.map         its sourcemap (the labeler's input)
      labels.jsonl       one row per span (format below)
      modules/<idx>.js   the exact text of each span (5-digit idx, same as the row's idx)
  labels/
      all.jsonl          every labels.jsonl concatenated (321,619 rows)
      builds.jsonl       one row per build: dir, bundler, minified, app, mode, outBytes, module counts, packages
      summary.json       counts by bundler / kind / purity
      validation.json    the two independent label checks (below)
  realworld/
      layout-<v>.json    Claude Code raw-bundle layout: every CJS factory (exact span) and lazy-ESM group
      cc-<v>/positives.jsonl, positives/<n>.min.js (raw), positives/<n>.tree.js (humanified vendor file)
      cc-<v>/negatives.jsonl, negatives/<n>.min.js (raw), negatives/<n>.tree.js (humanified src file)
      cc-<v>/negative-functions.jsonl, negative-functions/<n>.min.js
  logs/                  install logs, build-fail.jsonl, label-errors.jsonl (empty), nomap-check.json
```

All paths in the label rows are **relative to `lab/`**.

## Entry modes: how an app reaches the library

| mode   | entry                                                         | what it produces (Bun / esbuild)                                                  |
| ------ | ------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| `req`  | `const m0 = require(spec)` (CJS entry)                        | CJS packages: CJS factory. ESM packages: lazy ESM (`__esm` init + `__toCommonJS`) |
| `imp`  | `import * as m0 from spec` (whole namespace)                  | ESM packages: eager, scope-hoisted ESM. CJS packages: CJS factory                 |
| `some` | `import { a, b, c } from spec` (3 named exports, tree-shaken) | the same as `imp`, but only the code those 3 exports need                         |
| `dyn`  | `() => import(spec)`                                          | lazy ESM / CJS factory (no code splitting, like Claude Code)                      |
| `main` | combo apps only                                               | a mix: each library reached one way, with app files in between                    |

Every entry also holds a little app function with a unique string, so `<app>` spans exist in every build.
`some` is written only for single-specifier packages with at least 2 named exports (124 apps).
All builds use `--target node` / `--platform=node` and **CJS output** (as Claude Code does), except `ink` (below).

Combo apps (`pkgs/combo-1..6@0.0.0`, latest versions) put 8 or 9 libraries in one bundle with app modules
(`app/feature<i>.*`, `app/util.mjs`) between them. These are the closest to a real app, with eager libraries
interleaved with app code.

## Label format (`labels.jsonl`, `labels/all.jsonl`)

One JSON object per span, in output order:

| field                            | meaning                                                                                                                                                                                                                                                                                                                      |
| -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `build`                          | `<bundler>-<ver>/<min\|unmin>/<app>/<mode>` (the build dir under `builds/`)                                                                                                                                                                                                                                                  |
| `bundler`, `bundlerVersion`      | `bun` / `esbuild` / `rollup`, and the version                                                                                                                                                                                                                                                                                |
| `minified`                       | true for `--minify` (rollup: terser)                                                                                                                                                                                                                                                                                         |
| `app`, `mode`                    | entry app dir and mode                                                                                                                                                                                                                                                                                                       |
| `idx`                            | span index in the output, also the `modules/<idx>.js` file name                                                                                                                                                                                                                                                              |
| `pkg`                            | npm package name (the `node_modules/<pkg>` the source file lives in; transitive deps included). Special values: `<app>` (entry/app code), `<runtime>` (the bundler's helpers at the top of the file), `<unmapped>` (bundler glue with no sourcemap mapping, see below), `<bundler-helper>` (rollup's `commonjsHelpers` etc.) |
| `pkgVersion`                     | the `version` in that package's own `package.json`                                                                                                                                                                                                                                                                           |
| `file`                           | path of the source file inside the package, e.g. `lib/helpers/speedometer.js`                                                                                                                                                                                                                                                |
| `srcFormat`                      | `cjs` / `esm` / `json` (guessed from the source file text)                                                                                                                                                                                                                                                                   |
| `kind`                           | how it was emitted: `cjs` (CJS factory; rollup: a CJS source file), `lazy-esm` (wrapped ESM), `eager-esm` (scope-hoisted ESM), `other` (runtime / glue)                                                                                                                                                                      |
| `marker`                         | lazy-esm only: true when the module has an `__esm` init marker, false when it has none (Bun drops it when nothing is left to run)                                                                                                                                                                                            |
| `part`, `parts`                  | a file's statements can be split by other files' statements (eager ESM). Each contiguous piece is its own row: `part` 0..`parts`-1                                                                                                                                                                                           |
| `start`, `end`                   | **UTF-8 byte** offsets in `out.js` (end exclusive)                                                                                                                                                                                                                                                                           |
| `startChar`, `endChar`           | the same as JS string (UTF-16) offsets                                                                                                                                                                                                                                                                                       |
| `bytes`                          | `end - start`                                                                                                                                                                                                                                                                                                                |
| `textPath`                       | the extracted span text, relative to `lab/` (`builds/.../modules/00042.js`)                                                                                                                                                                                                                                                  |
| `sha256`                         | first 16 hex chars of sha256 of the span text                                                                                                                                                                                                                                                                                |
| `segAgree`, `segTotal`, `purity` | sourcemap segments in the span that name this file / all mapped segments; purity = agree/total (null if none)                                                                                                                                                                                                                |
| `declaratorUnits`                | how many units were single declarators split out of a `var a=…,b=…` that mixed files                                                                                                                                                                                                                                         |
| `attachedGlue`                   | how many unmapped namespace-glue statements were attached to this span (below)                                                                                                                                                                                                                                               |

**Span extent.** A span is the module's top-level statements, from right after the previous span (leading
whitespace skipped) to the end of its last statement. So **comments in front of a module, including `/*! … */` license
banners, travel with the module that follows them**. A CJS span is the whole `var X=H(function(e,t){…})` statement. A
lazy-ESM span runs from its namespace block/hoisted functions to the end of its `var I=H(()=>{…})` marker.

Quick load in Python:

```python
import json
rows = [json.loads(l) for l in open('/work/exp095-library-fingerprints/lab/labels/all.jsonl')]
lib_min = [r for r in rows if r['minified'] and not r['pkg'].startswith('<')]
text = open('/work/exp095-library-fingerprints/lab/' + lib_min[0]['textPath']).read()
```

## How the labels are made, and how far to trust them

`scripts/label.mjs`, per build:

1. Parse `out.js` with acorn and decode `out.js.map`.
2. Each top-level statement is a unit. A `var` statement whose declarators come from different files is split
   into one unit per declarator.
3. Each unit goes to the source file that owns the **majority of the sourcemap segments** inside it.
4. Glue that has no mapping (Bun's `var N={};H(N,{name:()=>local,…})` namespace blocks) is attached to the next
   span **only if most getter targets are declared in that span**. Otherwise it stays `<unmapped>`.
   - What is left as `<unmapped>` is mostly the namespace objects of re-export-only barrel files (e.g. zod's
     `v4/locales/index.js`). The file has no code of its own, so no mapping names it.
5. Consecutive units of one file form one span. A CJS factory always starts a new span.
6. Kind comes from the bundler's wrapper helpers. These are found by their definition text: `exports:{}` means
   `__commonJS`, `(x=0)` means `__esm`.
   - Calls to those helpers are `cjs` / `lazy-esm` markers.
   - A Bun/esbuild ESM span that sits before the last wrapper in the file is a lazy module with no marker. This is
     correct because wrapped modules are printed before eager code.

Independent checks (`scripts/validate.py` → `labels/validation.json`, `scripts/check-nomap.mjs` → `logs/nomap-check.json`):

- **Path comments agree 100%.** The unminified Bun and esbuild builds print `// node_modules/<pkg>/<file>`
  before each module. For every library span whose text holds such a comment, it names the labelled file:
  bun-1.3.14 27,853/27,853, bun-1.4.0 27,847/27,847, bun-1.4.3 27,883/27,883, esbuild 33,043/33,043.
  88–124 spans per Bun config had no comment, and 0 disagreed.
- **Asking for a sourcemap does not change the code.** 30 random builds per bundler config were rebuilt WITHOUT
  `--sourcemap`. All 180 are byte-identical to `out.js` once the trailing debugId line is removed.
- **Min and unmin twins label the same set of (package, version, file):**
  - Bun 608/617. The 9 that differ are tiny modules the minifier dropped or inlined (react `index.js` re-export
    stubs, `cli-boxes`).
  - esbuild 582/619. The 37 that differ are one 293-byte `@smithy/core` constants file.
  - rollup only 283/622, because terser merges and drops modules.
- **Purity:** Bun and esbuild spans are pure (purity 1.0). The exception is 21 Bun-min spans, all ≥0.5.
  Unminified rollup is also pure. **Rollup + terser is not:** of 24,884 minified rollup spans that have mappings,
  21,431 are pure, 1,020 are ≥0.9, 2,012 are ≥0.5 and 421 are <0.5. Terser hoists and merges statements across modules (e.g. `const a=…,b=…` and
  `Object.freeze` namespaces from several files in one statement). **Filter rollup-min rows on `purity >= 0.9`**
  (or ignore them) when you need exact ground truth.

## Normalisation and gotchas (read before comparing bundlers)

- **Input sourcemaps stripped.** esbuild silently CHAINS a library's own `//# sourceMappingURL`. Its labels then
  named TS sources (`src/account/ClientInfo.ts`), split single dist files into several "modules", and created 441
  fake unmarked lazy modules. `scripts/strip-input-maps.sh` deletes those comment lines from every `.js/.mjs/.cjs`
  under `pkgs/*/node_modules`, and everything was rebuilt.
  - esbuild, Bun 1.4.0 and rollup-min code was byte-identical before and after.
  - **Bun 1.3.14 minified code was NOT** (151 builds): the same length, but different minified identifiers. Its
    name assignment depends on the input text including comments, so deleting a comment re-letters the bundle.
- **Byte-identical modules across bundler versions are rare.** The same app, mode and file, minified:

  - bun-1.4.0 vs bun-1.4.3: 10,081 / 27,410 spans identical;
  - bun-1.3.14 vs bun-1.4.0: 20 / 27,410 (1.4.0 writes CJS factories as `function(e,t){}` instead of `(e,t)=>{}`, and letters differ);
  - bun-1.4.0 vs esbuild: 30 / 26,340.

  Raw-text hashes are not a fingerprint.

- **Bun 1.4.x CJS factory** is `var X=H(function(e,t){…})`, exactly as in Claude Code. Bun 1.3.14 uses
  `H((e,t)=>{…})`. Bun 1.4.3 has the new `__esm` helper (`(e,t,r)=>()=>{if(e)try{…}`).
- **Minified Bun drops lazy markers.** 528 modules that have an `__esm` marker unminified have none minified (853
  vs 322 unmarked). An example is axios `helpers/speedometer.js`: the minifier folds `speedometer_default =
speedometer` away, so only `function …{}var sc;` is left. esbuild keeps every marker (0 unmarked).
- `ink` uses top-level await, so ink apps are built with **ESM output** (`--format esm`, catalog field `format`),
  with `react-devtools-core` external.
- Bundle-level metadata the labels do not cover: Bun `debugId` lines, rollup `sourceMappingURL` lines, and esbuild's
  end-of-file `/*! Bundled license information: …*/` block. These sit outside every span.

## Claude Code real-world set (`realworld/`)

Raw bundles: `/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-<v>/binary-decompiled/src/entrypoints/index.js`
(Bun 1.4.0 era). Humanified trees: `/tmp/eval-work/candidate-e214db7a-scratch/<v>/`.

`realworld/layout-<v>.json` (`scripts/cc-layout.mjs`) lists every top-level wrapper in the raw bundle:

- 2.1.198: 1,623 CJS factories, with exact `start`/`end` bytes, and 4,471 lazy-ESM groups;
- 2.1.216: 1,647 CJS factories and 4,850 lazy-ESM groups.

A lazy-ESM group's `end` is exact. Its `start` is the end of the previous wrapper statement, so the group can also
hold unmarked modules.

**Positives** (`cc-<v>/positives.jsonl`, 214 per version) are every vendor module whose manifest entry carries an
`identity` record:

- 188 highlight.js grammars (with `member` = the grammar name);
- 18 scoped-error-prefix (`@smithy/*`, `@aws-sdk/*`, `@opentelemetry/api`);
- 5 inlined package.json (with `pkgVersion`);
- 2 `@grpc/grpc-js` source paths;
- 1 banner (safe-buffer).

Each one is mapped to its **raw minified factory** by comparing string-literal sets (Jaccard ≥ 0.5 with a 0.1 margin).
Ties are broken by the declared literal (5), or by taking identical copies in raw order (4: the five byte-identical
`@smithy/util-base64` copies).

- **Check:** in 2.1.216 the manifest order equals the raw order, and **all 214 matches equal the raw factory at the
  same index**.
- In 2.1.198 the manifest is not in raw order (only 40 agree by index), so the literal match is the only mapping. It
  uses the same method that scored 214/214 on 2.1.216.

Row fields:

- `pkg`, `pkgVersion` (only from package.json identities), `member`, `aliases`, `identitySource`, `evidence`;
- `vendorFile`, `structuralHash`, `manifestIndex`;
- `rawFactoryIndex`, `start`/`end` (bytes in the raw bundle), `matchedBy`, `matchScore`, `matchMargin`, `indexAgrees`;
- `textPath` (raw minified factory) and `treeTextPath` (the humanified vendor file).

**Limits:** the highlight.js version per grammar is not declared (Claude Code moved from 10.x to 11.x at 2.1.198). All
positives are CJS factories. Claude Code's library code in lazy ESM (MCP SDK, AWS, MSAL, marked, axios, execa…) has no
identity record, so it is NOT in the positive set.

**Negatives** (`cc-<v>/negatives.jsonl`, 100 per version) are raw lazy-ESM groups that are clearly Claude Code's own
code. A group is chosen only when ALL of the following hold:

- 400 B – 60 KB;
- contains at least 2 kinds of product string (`tengu_…` telemetry names, `Claude Code`, `claude.ai`, `CLAUDE.md`,
  `ANTHROPIC_…`, `.claude/`, `claude-code`, `Anthropic`);
- contains no library signal (`/*!`, `@license`, `node_modules/`, an inlined `name:…,version:…`, a scoped-package
  error, a non-Anthropic github URL);
- shares no literal of 12+ characters with ANY humanified `vendor/` file;
- at least 60% of its 12+-character literals are found in one humanified `src/` file (`treeFile`, `treeShare`).

Candidates numbered 132 in 2.1.198 and 182 in 2.1.216. 100 were taken, evenly spread by size: median ~6 KB, total
~0.9 MB per version. `startExact` is false, because the group start is inferred.

**Negative functions** (`negative-functions.jsonl`, 208 / 222) are single top-level `function` declarations
of 120 or more characters, taken from those groups, that themselves contain a product string. This is the strictest
app-only set, for per-function fingerprints.

Since the positives are mostly highlight.js, use the lab for the general question and the Claude Code set as the
reality check. "Zero matches on the negatives" is the precision bar.

## Gaps

- **36 builds failed** (`logs/build-fail.jsonl`). `lodash-es@3.10.1` has no root entry (24). `ink` cannot be
  `require()`d because of top-level await (12).
- Rollup + terser labels are approximate (see purity). Rollup's CJS emission is a different shape
  (`function requireX(){…}`), and its kind comes from the source format, not from a wrapper.
- No `--compile` / `--bytecode` builds and no `--target bun` builds. Claude Code is `--compile` with the `@bun-cjs`
  function wrapper. `scripts/label.mjs` and `cc-layout.mjs` descend into that wrapper if present.
- Library versions are the latest few per package, not the exact versions Claude Code bundles. Claude Code's
  versions are only partly known (package.json identities, banners such as msal-common 15.13.1 and msal-node 3.8.1).
- The `<unmapped>` spans are mostly barrel-namespace glue for re-export files and have no file label. Counts per
  minified config: Bun 1,134–1,204, esbuild 458, rollup-min 4,011 (terser's hoisted `var` lists).

## Rebuild from scratch (exact commands)

```bash
cd /work/exp095-library-fingerprints/lab
(cd tools && npm install --no-audit --no-fund --ignore-scripts esbuild@0.28.2 terser@5.51.2 rollup@4.64.5 \
   @rollup/plugin-commonjs @rollup/plugin-node-resolve @rollup/plugin-json @rollup/plugin-terser acorn@8 @jridgewell/sourcemap-codec)
(mkdir -p tools/bun-1.4.0 && cd tools/bun-1.4.0 && echo '{"private":true}' > package.json && npm install bun@1.4.0)
(mkdir -p tools/bun-1.4.3 && cd tools/bun-1.4.3 && echo '{"private":true}' > package.json && npm install bun@1.4.3)
node scripts/pick-versions.mjs        # versions.json (then the 10 "extra" CJS-era versions were added by hand; see summary)
scripts/install.sh                    # pkgs/<pkg>@<ver>/ (16 parallel npm installs)
node scripts/exports.mjs              # pkgs/*/exports.json
node scripts/entries.mjs              # pkgs/*/entries/
node scripts/combos.mjs && for d in pkgs/combo-*; do (cd $d && npm install --ignore-scripts --no-audit --no-fund); done
scripts/strip-input-maps.sh           # BEFORE building (see Normalisation)
node scripts/build.mjs --jobs 56      # builds/ (skips finished builds; --bundlers a,b and --apps <substring> to narrow)
node scripts/label.mjs --jobs 56      # labels.jsonl + modules/ per build (--force to redo, --only <substring>)
python3 scripts/aggregate.py          # labels/all.jsonl, builds.jsonl, summary.json
python3 scripts/validate.py           # labels/validation.json
node scripts/check-nomap.mjs 30       # logs/nomap-check.json
for v in 2.1.198 2.1.216; do
  node scripts/cc-layout.mjs /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-$v/binary-decompiled/src/entrypoints/index.js realworld/layout-$v.json
  node --max-old-space-size=16000 scripts/cc-realworld.mjs $v /tmp/eval-work/candidate-e214db7a-scratch/$v \
    /Users/andrewgross/Development/claude-code-versions/inputs/claude-code-$v/binary-decompiled/src/entrypoints/index.js
done
```

The extra versions, added to `versions.json` under `"extra"`: chalk 4.1.2, execa 5.1.1, nanoid 3.3.11, p-limit
3.1.0, ansi-styles 4.3.0, uuid 9.0.1, marked 4.3.0, commander 11.1.0, zod 3.23.8, highlight.js 10.4.1.

A full build takes about 40 s and labelling about 10 s on 64 cores.
