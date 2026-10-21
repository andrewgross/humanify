# exp132: precision for cut CommonJS modules (`factory_bodies`)

## STATUS (2026-10-18)

DONE on branch `exp132-factory-precision` (off main `bd24cb3c`; main `35355039` — exp129, scorer 12 — merged in at
`23be7ec8`). Gate 13/13 bare (on `9f59c546` and on the merge tree). Warm neutrality of `23be7ec8` vs main `35355039`
on 2.1.215→2.1.216: NEUTRAL 0 files / 0 lines, both legs +0 cache writes, in default AND report mode (k2-all-r5-origin).
**Not merged** (the coordinator sequences merges); nothing flipped.

- **Bar met.** `factory_bodies` + this lane's three new keys (`factory_tie_fence`, `factory_banner_index`,
  `factory_whole_seed`, all `[library.discovery]`, default off) vs today's shipped defaults on k2-all-r5-origin,
  scorer 13, both scopes: precision **at least today's on every set** (up on all 12 Bun / esbuild sets; the 9 rollup / webpack /
  cc sets are byte-identical), F0.5 **up 29-62 points on every Bun / esbuild set**, app claimed 0.
  Held-out 99.31 / 32.06 → **99.41 / 90.90** (P / F0.5); test-bun 99.33 → 99.51, test2-bun 98.54 → 99.15,
  test2-esbuild 99.44 → 99.63; remin-terser 99.82 → 99.90.
- **Recommendation: flip** `factory_bodies`, `factory_tie_fence`, `factory_banner_index` and `factory_whole_seed`
  into `DiscoveryOptions::recommended()` together. Without `factory_whole_seed` (the higher-F0.5 option, 0.2-2.8
  points more on 8 sets) precision misses today's on three esbuild sets: pop-esbuild -0.004, test2-esbuild -0.07,
  remin-terser -0.08 (section 4.2). That one is Andrew's call.
- **Scorer 13** (rule `misattributed`, frozen layer `misattr13`): Bun 1.4's maps credit whole modules of large
  bundles to an unrelated file; the truth now relabels such a module to the one file that holds it. Relabels Bun
  sets only (held-out 348 modules / 6.0 MB of module bytes, test2-bun 350, pop-bun 179; esbuild sets 0-11 modules).
  Every committed label (411) re-scored once under scorer 13. Hand-check 20/20 relabelled dev modules right (seed 132,
  one debatable), ledger in section 3.
- Matching inside cut modules: exp114's context presence / shadow ties / fence, exp121's copy presence, exp102's
  conflict guards, exp113's payload guard and the isolation veto all already read factory bodies (checked in
  the code). They miss two module-level classes, which the new keys add: whole-module cross-family ties (forks,
  sponges carrying a file verbatim, generated parsers) and `/*!` banner seeds a module's own code contradicts.

## 1. What was wrong (dev diagnosis: apps-esbuild, pop, remin; held-out by aggregate class only)

exp128's `factory_bodies` (each bundled CommonJS module cut into its body statements) lifted every Bun / esbuild
set by 30-60 F0.5 points, but its factory scope read P 96.3-99.98. The wrong bytes in cut modules split into
four classes:

- **Cross-family ties on a whole module.** Every statement of the module is held by two or more families'
  index rows: forks (`node-ipc` / `@achrinza/node-ipc`, `sqlite3` / `@vscode/sqlite3`, `expr-eval` /
  `expr-eval-fork`, `@sideway/formula` / `@hapi/formula`), a sponge carrying the file verbatim (rollup's dist
  holds yargs-parser; nodemailer holds punycode), generated code many packages ship (the jison CSS-calc parser
  in postcss-calc, reduce-css-calc, jsonpath, mermaid, prettier). The structural rule breaks the tie by the
  previous claim's package, else by name order. On apps-esbuild this was most of the wrong bytes.
- **Banner seeds.** A kept `/*!` banner seeds the statement after it, and a banner beats an index seed. In a cut
  Bun module the banner is often another file's (kept from a file Bun merged, or one ahead of the next module's
  wrapper): on held-out this class was ~670 KB of wrong bytes (aggregate decomposition, no package names read),
  on statements another family's prints cover and the banner's family's do not.
- **Truth: Bun 1.4's source maps.** Large Bun bundles credit whole modules to an unrelated file: iconv-lite's
  big5 / cp950 / shiftjis tables to `unpipe`, `file-type`, `text-encoding`; `ms` to vue-router.common.js;
  css-tree's node modules to lru-cache's `index.min.js`; whatwg-url's files to whatwg-mimetype; mime-db's
  `db.json` to `combined-stream`. The index builder already drops every file credited to two modules
  (`humanify_index::build`, `multi_module_files`); the truth did not.
- **Truth: inlined copies** (prettier's js-yaml, @rrweb/replay's postcss, @img/colour's color-convert ...): scorer
  12's copies layer (exp129) is the fix; this lane previewed its effect (section 4) but does not change it.

Rules that did NOT pay (offline sizing on dev diag tables, `diag.py`, then the real run where noted):

- a module's claims split between families: no module had a minority family worth naming (majority rule
  moved 4.6 KB of 362 KB on apps-esbuild);
- tie-break by module size (the family whose index module is closest to the module's bytes, or the smallest
  context): turned 0.23-0.85 MB right into wrong on apps-esbuild — the previous-claim tie-break is right ~94% of
  the time;
- dropping every tied seed (unit level): -2.5 R points for less than the module-level fence buys.

## 2. The matcher keys (all `[library.discovery]`, default off, generic, test-first)

Code: `crates/humanify-core/src/libdetect/discovery.rs` (`presence` → `factory_banners`, `factory_ties`,
`factory_whole`; `Containment::whole_held`), config `crates/humanify-cli/src/config{,/keys}.rs`, documented in
`humanify.example.toml` and `docs/library-matching-config.md`. Tests (each watched red, then green):
`discovery_test.rs` `factory_tie_fence_names_no_one_for_a_module_every_seed_of_which_two_families_tie`,
`factory_whole_seed_needs_one_whole_statement_of_the_module_in_the_family_s_index`,
`factory_banner_index_refuses_a_banner_the_module_s_own_code_contradicts`. All three read only cut factories
(`FactoryBody` units), so with `factory_bodies` off they are no-ops; with every key off the binary claims what
main claims (default-mode reports byte-identical: the code paths are behind the keys).

Which of the existing precision rules reach cut modules (checked in the code): the isolation veto, the
evidence / literal / contested guards, the payload guard (judged by a body statement's own bytes), copy presence,
context presence and shadow ties all read `FactoryBody` units as they read lazy bodies (`is_statement`,
`module_groups` put a factory's bodies in its group). None of them sees a module-level tie or a banner, which is
what the new keys add.

- **`factory_tie_fence`** — a bundled CommonJS module is ONE file of one package. When every index seed on its body
  statements is a tie (the structural rule accepts two or more families for it: `Containment::families_accepting`)
  and none is evidence only one family gives (an untied index seed, or a declared / banner seed), the module's
  package is unknowable from its code: its seeds are dropped and every body statement is fenced (claimed by no one,
  so no rival's propagation takes it either). One untied seed keeps the whole module as it was (a fork's own
  patched statement decides it). Sizing on apps-esbuild: wrong 362 → 120 KB for -0.77 R points.
- **`factory_banner_index`** — a `/*!` banner seed on a body statement is refused when another family's index prints
  cover more of that statement than the banner family's (`Containment::outweighed`, exp102's evidence-conflict
  test applied to banners). First tried as "the structural rule accepts some family and not the banner's": 8 KB on
  held-out — the contradicting evidence is usually function prints under the accept bar. With `outweighed`:
  held-out wrong 2.09 → 1.42 MB (scorer 11 truth), recall unchanged.
- **`factory_whole_seed`** — a family names a module's statements only when one of its index seeds there is a WHOLE
  statement its index holds (or a declared / banner seed); function-print seeds alone are dropped and the module's
  other statements fenced. Measured and NOT recommended: apps-esbuild P 99.858 → 99.884 for R -3.6, esbuild-app
  R -9.6, held-out wrong -0.28 MB for R -2.0 (lab labels `*lab132-ftf2/4*`). Kept, default off, for the trade-off
  table.

## 3. Scorer 13: rule `misattributed`

`truth_audit.Audit.misattributed` over the frozen layer `misattr13` (`<DATA>/truth/<id>.misattr13.json.gz`, shas in
`misattr13*.json`), built by `scorer13/misattr13.py build --corpus C --units <reference label>` (the module spans
of exp128's `factory_bodies` run on that corpus; a factory the scored run cuts differently is left alone). Scorer 13
= scorer 12 + this rule; off, scorer 12's cards exactly (the rule touches no unit).

A factory F whose majority-mapped source S (package P)

1. is the majority source of at least one OTHER factory of the bundle (one file, two modules: the builder's
   `multi_module_files` signal),
2. whose OTHER module(s) credited to S do hold S's tokens (S's own module is elsewhere in the bundle), and
3. holds none of F's code: less than half of F's RARE tokens (`TOKEN`, held by 1..max(5, 2% of) the map's sources;
   at least 5) are in any of P's sources, and P's installed files do not hold half of F's long string literals;

takes the SMALLEST file of ANOTHER package, no bigger than 8 x F + 8 KB (one file is one module: a whole bundled
dist holding F's code is a copy, not F's file), holding at least max(80%, own + 50%) of them: first among the map's
sources, else among the installed files of the packages the map names (Bun maps no JSON: iconv-lite's tables,
mime-db's `db.json`, tr46's `mappingTable.json`). No such file: F keeps its label. At score time a unit of F
whose majority-mapped source is still S takes the new package, version and file (its copy origin then read for
the new shipper); a body statement mapped to another file of P keeps its label.

**Ledger, both directions** (`scorer13/ledger13.py`, on each corpus's `factory_bodies` label: claimed bytes by
verdict, rule off → on; test / test2 aggregates):

| set | modules relabelled | wrong → right | origin → right | right → wrong | other moves |
|---|--:|--:|--:|--:|--:|
| heldout | 348 | 33,086 | 18,738 | 6,693 | 8,035 |
| pop-bun | 179 | 143,372 | 18,745 | 3,184 | 7,955 |
| pop-esbuild | 3 | 0 | 0 | 0 | 1,254 (family → right) |
| test-bun | 257 | 306,874 | 79,720 | 6,434 | 15,705 |
| test-esbuild | 6 | 768 | 0 | 0 | 633 |
| test2-bun | 350 | 166,192 | 42,189 | 6,661 | 20,272 |
| test2-esbuild | 11 | 0 | 0 | 0 | 5,066 |
| remin-esbuild | 4 | 0 | 0 | 0 | 0 |
| apps, bundlers, esbuild-app, wrangler, remin-terser, rollup / webpack sets | 0 | | | | |

The dev right → wrong bytes (pop-bun 3.2 KB) are one module whose Bun map names files of two packages
(cssstyle's `CSSStyleDeclaration.js` and @acemir/cssom's `CSSStyleRule.js`); the module is cssstyle's by its own
code, a few statements still index as cssom — debatable, kept. **Hand-check** (pop, the only dev corpus with
relabels outside held-out; `--draw 20 --seed 132`): 20/20 right (whatwg-url files credited to whatwg-mimetype,
iconv-lite's tables and streams to `unpipe`, mime-db's `db.json` to `http-signature`, @html-eslint rules to
`@rviscomi/capo.js`, sharp's and esrecurse's `package.json`, core-js-compat's `modules.json` to
`@babel/preset-env`, ...), one debatable (the cssstyle module above).

Each condition came from a dev false positive: (1)+(2) — esbuild's compiled-TypeScript modules (`__esModule`,
`__importDefault` absent from the `.ts` source) relabelled to a big dist; the size bound — the shiftjis table
credited to lucide-react's bundle, a radix module to headlessui's `prod.cjs`; P's installed files — babel-core's
inlined `package.json` credited to ajv's regenerator bundle; the per-unit source check — a cssstyle module whose
cssom statements are genuinely cssom's.

## 4. Results

Index k2-all-r5-origin, structural rule, scorer 13, binary `23be7ec8`. Labels: base = today's shipped defaults
(`*exp130-origin*`, factory_bodies off), factory_bodies = exp128's (`*exp128-fbo*`), candidate = factory_bodies +
`factory_tie_fence` + `factory_banner_index` (`*exp132-ftf*`), + `factory_whole_seed` (`*exp132-ftfw*`). test / test2:
aggregates only (never diagnosed).

### 4.1 Both scopes, recommended configuration (`*exp132-ftfw*`; bundlers: `exp132-ftf-bundlers`, no factories)

P / R / F0.5 per scope. "Statement" is scorers 1-10's scope; "factory" the bundled CommonJS modules (scorer 11).

| set | statement scope, base | statement scope, cand | factory scope, base | factory scope, cand | both scopes, base | both scopes, cand | app B |
|---|--:|--:|--:|--:|--:|--:|--:|
| cc | 100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 | - | - | 100.00 / 81.90 / 95.77 | 100.00 / 81.90 / 95.77 | 0 |
| esbuild-app | 96.33 / 69.40 / 89.39 | 96.33 / 69.42 / 89.40 | 0.00 / 0.00 / 0.00 | 99.99 / 76.66 / 94.25 | 96.33 / 18.20 / 51.83 | 99.07 / 74.74 / 93.01 | 0 |
| heldout | 99.31 / 67.82 / 90.87 | 99.28 / 66.34 / 90.31 | 0.00 / 0.00 / 0.00 | 99.43 / 67.91 / 90.99 | 99.31 / 8.65 / 32.06 | 99.41 / 67.71 / 90.90 | 0 |
| wrangler | 96.45 / 21.00 / 56.12 | 96.45 / 21.00 / 56.12 | 0.00 / 0.00 / 0.00 | 99.97 / 41.04 / 77.66 | 96.45 / 8.10 / 30.31 | 99.07 / 33.14 / 70.87 | 0 |
| apps-esbuild | 99.79 / 92.45 / 98.23 | 99.79 / 92.45 / 98.23 | 0.00 / 0.00 / 0.00 | 99.91 / 74.67 / 93.59 | 99.79 / 15.26 / 47.35 | 99.89 / 77.65 / 94.48 | 0 |
| apps-rollup | 99.09 / 79.31 / 94.38 | 99.09 / 79.31 / 94.38 | - | - | 99.09 / 79.31 / 94.38 | 99.09 / 79.31 / 94.38 | 0 |
| apps-webpack | 99.58 / 73.50 / 92.98 | 99.58 / 73.50 / 92.98 | - | - | 99.58 / 73.50 / 92.98 | 99.58 / 73.50 / 92.98 | 0 |
| dev-rollup | 99.87 / 77.53 / 94.43 | 99.87 / 77.53 / 94.43 | - | - | 99.87 / 77.53 / 94.43 | 99.87 / 77.53 / 94.43 | 0 |
| dev-webpack | 99.64 / 62.89 / 89.22 | 99.64 / 62.89 / 89.22 | - | - | 99.64 / 62.89 / 89.22 | 99.64 / 62.89 / 89.22 | 0 |
| pop-bun | 98.91 / 85.90 / 96.00 | 98.91 / 85.99 / 96.03 | 0.00 / 0.00 / 0.00 | 99.73 / 72.39 / 92.72 | 98.91 / 26.48 / 63.93 | 99.44 / 76.61 / 93.85 | 0 |
| pop-esbuild | 99.02 / 89.32 / 96.92 | 99.02 / 89.41 / 96.93 | 0.00 / 0.00 / 0.00 | 99.46 / 73.87 / 93.02 | 99.02 / 27.44 / 65.07 | 99.30 / 78.71 / 94.37 | 0 |
| remin-esbuild | 99.04 / 78.17 / 94.02 | 99.04 / 78.17 / 94.02 | 0.00 / 0.00 / 0.00 | 99.77 / 49.70 / 83.04 | 99.04 / 5.51 / 22.53 | 99.69 / 51.76 / 84.11 | 0 |
| remin-terser | 99.82 / 60.64 / 88.40 | 99.83 / 61.02 / 88.56 | 0.00 / 0.00 / 0.00 | 99.93 / 16.85 / 50.31 | 99.82 / 4.65 / 19.59 | 99.90 / 20.33 / 56.04 | 0 |
| test-bun | 99.33 / 73.80 / 92.90 | 99.33 / 73.82 / 92.91 | 0.00 / 0.00 / 0.00 | 99.56 / 67.65 / 90.98 | 99.33 / 15.81 / 48.29 | 99.51 / 68.99 / 91.42 | 0 |
| test-esbuild | 99.12 / 82.83 / 95.37 | 99.12 / 82.84 / 95.37 | 0.00 / 0.00 / 0.00 | 99.31 / 70.75 / 91.89 | 99.12 / 17.53 / 51.33 | 99.26 / 73.37 / 92.72 | 0 |
| test-rollup | 98.96 / 72.43 / 92.21 | 98.96 / 72.43 / 92.21 | - | - | 98.96 / 72.43 / 92.21 | 98.96 / 72.43 / 92.21 | 0 |
| test-webpack | 99.48 / 72.19 / 92.49 | 99.48 / 72.19 / 92.49 | - | - | 99.48 / 72.19 / 92.49 | 99.48 / 72.19 / 92.49 | 0 |
| test2-bun | 98.54 / 79.31 / 93.99 | 98.52 / 79.34 / 93.97 | 0.00 / 0.00 / 0.00 | 99.30 / 65.84 / 90.14 | 98.54 / 13.22 / 43.02 | 99.15 / 68.12 / 90.87 | 0 |
| test2-esbuild | 99.44 / 75.75 / 93.59 | 99.45 / 75.78 / 93.60 | 0.00 / 0.00 / 0.00 | 99.66 / 70.15 / 91.93 | 99.44 / 11.85 / 40.13 | 99.63 / 71.04 / 92.21 | 0 |
| test2-rollup | 96.35 / 75.65 / 91.35 | 96.35 / 75.65 / 91.35 | - | - | 96.35 / 75.65 / 91.35 | 96.35 / 75.65 / 91.35 | 0 |
| test2-webpack | 99.27 / 70.09 / 91.64 | 99.27 / 70.09 / 91.64 | - | - | 99.27 / 70.09 / 91.64 | 99.27 / 70.09 / 91.64 | 0 |

Statement-scope numbers move by hundredths except held-out (P 99.31 → 99.28, F0.5 90.87 → 90.31) and test2-bun
(P 98.54 → 98.52): cutting the modules re-forms the module groups the presence rules read (exp128 saw the same
on held-out). The bar is read on the headline (both scopes).

### 4.2 The trade-off (headline P / F0.5; sets where any column differs)

| set | base P / F0.5 | factory_bodies | candidate | + factory_whole_seed | bar (cand / +whole) |
|---|--:|--:|--:|--:|---|
| esbuild-app | 96.328 / 51.83 | 99.159 / 95.89 | 99.169 / 95.80 | 99.066 / 93.01 | PASS / PASS |
| heldout | 99.312 / 32.06 | 98.855 / 91.38 | 99.394 / 91.63 | 99.414 / 90.90 | PASS / PASS |
| wrangler | 96.446 / 30.31 | 99.148 / 73.61 | 99.133 / 73.05 | 99.069 / 70.87 | PASS / PASS |
| apps-esbuild | 99.786 / 47.35 | 99.701 / 95.52 | 99.860 / 95.48 | 99.886 / 94.48 | PASS / PASS |
| pop-bun | 98.915 / 63.93 | 99.327 / 94.56 | 99.412 / 94.56 | 99.442 / 93.85 | PASS / PASS |
| pop-esbuild | 99.022 / 65.07 | 98.960 / 94.88 | 99.018 / 94.82 | 99.304 / 94.37 | FAIL / PASS |
| remin-esbuild | 99.044 / 22.53 | 99.377 / 85.16 | 99.679 / 84.91 | 99.691 / 84.11 | PASS / PASS |
| remin-terser | 99.822 / 19.59 | 99.493 / 57.18 | 99.743 / 57.10 | 99.902 / 56.04 | FAIL / PASS |
| test-bun | 99.329 / 48.29 | 98.462 / 91.02 | 99.450 / 91.69 | 99.507 / 91.42 | PASS / PASS |
| test-esbuild | 99.116 / 51.33 | 99.138 / 93.02 | 99.186 / 92.94 | 99.260 / 92.72 | PASS / PASS |
| test2-bun | 98.544 / 43.02 | 96.859 / 89.63 | 99.108 / 91.30 | 99.145 / 90.87 | PASS / PASS |
| test2-esbuild | 99.442 / 40.13 | 98.106 / 91.71 | 99.368 / 92.42 | 99.626 / 92.21 | FAIL / PASS |

`factory_whole_seed` buys the last precision on esbuild (pop-esbuild +0.29, test2-esbuild +0.26, remin-terser +0.16
P) for F0.5 0.2-2.8 points (esbuild-app -2.8, wrangler -2.2, apps-esbuild -1.0, held-out -0.7): its recall cost is
modules whose statements match their package only through function prints (version drift).

### 4.3 What each key bought (lab, scorer 11 truth, same index; `*lab132-ftf1/5/6*`, not committed)

| set | factory_bodies | + tie fence | + banner (accepting) | + banner (outweighed) |
|---|--:|--:|--:|--:|
| held-out P / R | 98.60 / 69.98 | 98.73 / 69.60 | 98.74 / 69.61 | 99.14 / 69.65 |
| apps-esbuild P / R | 99.67 / 81.76 | 99.86 / 81.22 | 99.86 / 81.22 | — |
| pop-bun P / R | 99.22 / 79.26 | 99.29 / 78.85 | 99.36 / 79.05 | — |
| remin-esbuild P / R | 99.30 / 54.11 | 99.68 / 53.32 | 99.68 / 53.32 | — |

## 5. Reproduce

```bash
S=experiments/132-factory-precision; PY=/work/exp095-library-fingerprints/scale/.venv/bin/python
# scorer 13's layer, per corpus (units = exp128's factory_bodies run on that corpus)
$PY -I $S/scorer13/misattr13.py build --corpus pop --units pop-exp128-fbo      # dev: exp128-fbo, apps: exp128-fbo-apps, ...
$PY -I $S/scorer13/ledger13.py pop-exp128-fbo --corpus pop --draw 20 --seed 132   # ledger + hand-check draw (dev only)
# the labels (k2-all-r5-origin; prefix per corpus as usual)
npm run eval -- library exp132-ftfw --index /work/exp130-data/full/ix/k2-all-r5-origin --rule structural --corpus dev \
  --pipeline-set library.discovery.factory_bodies=true --pipeline-set library.discovery.factory_tie_fence=true \
  --pipeline-set library.discovery.factory_banner_index=true --pipeline-set library.discovery.factory_whole_seed=true
# re-score every committed label (rescore_many per corpus; ~31 GB per process, at most ~6 at once)
$PY -I experiments/120-truth-families/rescore_many.py --corpus C --cache <scratch> <labels...>
python3 -I $S/table.py --tradeoff exp130-origin:exp128-fbo:exp132-ftf:exp132-ftfw ...
# lab (uncommitted trees only): $S/lab_run.py <label> --bin <binary> [run.py flags]; $S/score_rules.py; $S/diag.py
```
