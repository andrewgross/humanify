# exp214 — library scorer 15: credit copied code, exclude map defects

STATUS: BUILT, RESCORED (bun-dev, bun-held, kind-esbuild/webpack/rollup),
GATE: ALL 11 STAGES PASS (worktree and clean checkout, §6), NOT MERGED (2026-11-10). Branch `exp214-libscore-copy-credit`
from main `622279472`. Harness crate only (`crates/humanify-ag-dev`); the
pipeline, the index builder and every pipeline output are untouched, so no
eval applies. Scorer version `exp096-score/14` -> `exp096-score/15`.

## Totals

- **Both node slices now pass the 0.998 bar, with every corrected byte
  shown in its own column.** Of the 241,822 wrong bytes scorer 14 charged on
  bun-dev node: 59,183 are now a **credited copy** (the claim names the
  package the code came from), 6,859 sit on **map defects** (taken out of
  both sides), and **175,780 stay wrong**. bun-dev node reads **0.998138**
  (0.997511 with the credit charged as wrong; scorer 14: 0.997439).
- bun-held node: 66,396 wrong -> 22,599 wrong + 1,146 credited + 42,651 on
  defects (the cp950 table filed under node-fetch is the 42,189 of them).
  **0.999032** (scorer 14: 0.997162).
- bun slices: bun-dev bun 0.998342 -> 0.999079 (all from copy credit;
  0.998342 without it, byte-equal to scorer 14), bun-held bun 0.999755 ->
  0.999820.
- App bytes claimed: 0 everywhere (unchanged).
- The copy credit is strict: of the wrong bytes on bun-dev node it looked
  at, 175,780 were refused, each with a reason (§4). Every negative case
  from exp210 STAYS WRONG on the real data: readable-stream for
  string_decoder / safe-buffer (contaminated index rows), compiler-sfc for
  playwright-core, @takumi-rs/core for @ast-grep/napi, chalk for
  supports-color, mermaid for d3-*.
- kind-esbuild / kind-webpack / kind-rollup: no credit, no defect
  (precision unchanged); their wrong claims are mostly playwright-core's
  inlined bundle (`no-link` / `print-not-in-claimed`), §5.

## 1. What changed

Scorer 15 = scorer 14 plus two corrections. Both only ever move bytes OUT
of `wrongFamily`; neither can turn app bytes into library bytes.

### 1a. Map defect (`library/defect15.rs`, applied in `library/audit15.rs`)

A library truth label is a DEFECT when the label's package cannot contain
the unit. Evidence is the unit's distinctive literals (strings, property
names, regexes of 4+ characters; numbers and the bundler's interop names
`exports` / `__esModule` / `default` are not evidence), read by the library
index's own literal extractor (`humanify_ag_core::libindex::literals`) on
both sides, so escapes and folded concatenations agree. A unit is a defect
only when ALL hold:

1. it has at least 8 distinctive literals;
2. its labelled files (the corpus map's `sourcesContent`) have text, and it
   was not already relabelled by scorer 13/14 (`misattr13`, `mapdefect14`);
3. ALL of the label package's files in the corpus map together hold at
   most 20% of them (a literal counts as held if the file's literal set has
   it OR its value occurs anywhere in the file's raw text — lenient, so an
   unparsed file or a `require("./x.node")` specifier can only keep a
   label);
4. no single published file of the label package in the label's index
   literal table holds more than 20%;
5. the label package as INSTALLED for the corpus app (resolved from the
   map's source paths on this host) is on disk, and none of its `.js/.cjs/
   .mjs/.json` files holds more than 20%. Not installed on this host -> no
   verdict (not a defect).

A defect unit is relabelled `<defect>`: out of `libraryBytes`, and any
claim on it is judged `defect` (not right, not wrong). Rules 3-5 make the
check about the PACKAGE, not the file: a unit filed under the wrong file of
the right package (undici's own modules mapped to a neighbouring undici
file; iconv-lite's tables mapped to iconv-lite's `lib/index.js`) keeps its
label. The first draft judged the labelled file only and flagged 272,127
bytes on bun-held node, most of them that harmless case; rule 5 was added
after i18n-iso-countries' and iconv-lite's JSON tables (never in the map,
never in the literal table, which holds no `.json`) were flagged although
their package label is right.

### 1b. Copy credit (`library/copy_credit.rs`)

A claim scorer 14 judges `wrongFamily` is judged `copyCredit` only when
ALL hold:

1. the unit has a statement print and the label's index holds that print
   in a unit row OWNED by the claimed package;
2. that same file of the claimed package — its PUBLISHED text, through the
   index literal table, not a source map — holds >= 90% of the unit's
   literals, and the unit has >= 3 distinctive ones (a print blurs strings:
   two napi-rs loaders print alike and differ in their package names);
3. the truth's own file(s) (corpus map `sourcesContent`) hold >= 80% of
   them (else it is a defect question, not a copy);
4. the claimed package does not ship the TRUTH's code (no holding file under
   a directory named for the truth package; its cached registry manifest
   does not depend on the truth package);
5. a LINK, in this order: `dependency` (the truth's package@version
   declares the claimed package), `fork` (the truth's name extends the
   claimed one's — stem >= 5 chars, compatible scope — AND the truth's
   manifest is known and does not list the claimed package as a
   devDependency: `micromark-extension-frontmatter` is an extension, not a
   fork), `vendored` (the truth's file sits under `vendor/`-like directory
   named for the claimed package), `same-file` (a non-generic basename
   shared with a holding file of the claimed package, which holds >= 80% of
   the truth file's >= 20 distinctive literals).

Credited bytes count as named in the headline precision and are reported
as `namedAsCopyCreditBytes`; `precisionNoCopyCredit` charges them as wrong.

### 1c. Cards, summary, listing, version

- New card / scope / summary columns: `namedAsCopyCreditBytes`,
  `defectExcludedBytes`, `defectExcludedClaimedBytes`,
  `precisionNoCopyCredit`. `claimedBytes` gains `copyCredit` / `defect`
  verdicts; `statementBytes` gains `<defect>`.
- `truth15.json` beside the cards: per set, every defect (grouped by truth
  package/file with what it was claimed as, and the 400 largest units with
  evidence, held count, absent literals and the best other map source
  holding it), every credit (by pair and link, the 400 largest units with
  the claimed file and both shares), and `wrongStill` — every wrong claim
  left wrong, by refusal reason. The rescore FAILS unless, per set, credits
  = `namedAsCopyCreditBytes`, wrongStill = `wrongFamilyBytes` and defects =
  `defectExcludedBytes` (`audit15::check_totals`).
- `summary.config.copyCredit` records the literal source and the registry
  cache path.
- `SCORER_VERSION` `exp096-score/15`. The leaderboard already refuses mixed
  scorer versions (identity guard, `scorer.version`); it prints a
  `scorer 15:` line per set for a label that has the columns (scorer-14
  goldens unchanged). `library proof` now REFUSES a label of another
  scorer version (a proof compares one version with itself).
- `libscore.registry` (`--npm-registry`, default `/work/exp112-data/registry`)
  is now read by `library score|rescore|proof` too (one key, doc updated).
- `dev/library/bands.json` is not per scorer version (band 0: discovery is
  deterministic) and needs no change; scorer 15 is deterministic too
  (bun-held rescored twice: every file byte-identical).
- `docs/responsibility.md`: one row for the new owner.

Tests (red first, each rule): `defect15_test.rs` (9), `audit15_test.rs`
(8), `copy_credit_test.rs` (16). Negative tests: unrelated look-alike
packages (napi loaders) stay wrong; a byte-identical generated twin with no
link stays wrong; a contaminated index row is no evidence even with a
dependency link; a claimed package that vendors the truth stays wrong; a
truth file without the code is no copy; an extension is no fork; a fork
with no known manifest is no fork; a unit under the wrong file of the right
package, a unit the label package publishes or has installed, a scorer-14
relabel, and a package not installed on this host are never defects. The
three package-level defect rules and the installed check were each verified
red by disabling them.

## 2. Results (scorer 14 = exp210's baseline cards; scorer 15 = the same reports rescored)

Label `exp210-base-*` (index `4736dcfa…`, structural rule). Scorer 15
output: `/work/exp214-rescore/<label>/` (not committed: library labels are
gitignored).

| corpus, slice | P s14 | P s15 | P s15 no credit | bar | R s14 | R s15 | wrong s14 | wrong s15 | credited | defect bytes (claimed) | app |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| bun-dev, bun | 0.998342 | **0.999079** | 0.998342 | pass | 0.8640 | 0.8652 | 150,273 | 83,447 | 66,826 | 72,657 (0) | 0 |
| bun-dev, node | 0.997439 | **0.998138** | 0.997511 | **pass** (was MISS) | 0.8594 | 0.8618 | 241,822 | 175,780 | 59,183 | 242,240 (6,859) | 0 |
| bun-held, bun | 0.999755 | **0.999820** | 0.999755 | pass | 0.8697 | 0.8697 | 5,563 | 4,090 | 1,473 | 0 (0) | 0 |
| bun-held, node | 0.997162 | **0.999032** | 0.998983 | **pass** (was MISS) | 0.8315 | 0.8341 | 66,396 | 22,599 | 1,146 | 87,169 (42,651) | 0 |
| kind-esbuild (`exp181-esbuild`) | 0.998613 | 0.998613 | 0.998613 | pass | — | 0.8585 | 106,902 | 106,902 | 0 | 0 | 0 |
| kind-webpack (`exp181-webpack`) | 0.994681 | 0.994681 | 0.994681 | MISS | — | 0.8979 | 319,267 | 319,267 | 0 | 0 | 0 |
| kind-rollup (`l3-rs-rollup`) | 0.977024 | 0.977024 | 0.977024 | MISS | — | 0.8198 | 1,179,008 | 1,179,008 | 0 | 0 | 0 |

Band: 0 (deterministic). Wrong bytes add back exactly: s14 wrong = s15
wrong + credited + defect-claimed on every slice (e.g. bun-dev node
175,780 + 59,183 + 6,859 = 241,822). Every other corpus in
`data-roots.json` (dev, test, test2, bundlers, apps, pop, remin*, cc) has
no label whose index still exists on this host, so it could not be
rescored; cc is sample truth, on which scorer 15 changes nothing (not
judged, zero columns).

## 3. Credited copies

| claimed (credited) | truth (shipped the copy) | link | bun-dev bun | bun-dev node | bun-held bun / node | why it is a copy |
| --- | --- | --- | --- | --- | --- | --- |
| hyperdht | hyperdht-hugin | fork | 43,351 (67 units) | 43,351 | — | renamed fork (exp210); its manifest does not build against hyperdht; each unit's print + >=90% literals are in hyperdht's own published file |
| rambda | rambdax | fork | 8,167 (38) | 8,167 | — | rambdax's dist carries rambda's functions; same author |
| better-sqlite3 | better-sqlite3-multiple-ciphers | fork | 7,862 (13) | — | — | SQLCipher fork, same file layout (`lib/database.js`, `lib/methods/*`) |
| @dual-bundle/import-meta-resolve | @babel/core | vendored | 4,585 (21) | 4,585 | — | `lib/vendor/import-meta-resolve.js` |
| monaco-editor | @vscode/prompt-tsx | same-file | 2,861 (6) | 2,861 | — | both ship VS Code's `vs/base/common/{path,uri}.js` |
| micromark | mdast-util-from-markdown | dependency | — | 219 (1) | — | mdast-util-from-markdown@0.8.5 depends on micromark; the unit is micromark's `html-flow.js` |
| react-is | @jest/react-is-19 | same-file | — | — | 1,146 / 1,146 | Jest's re-published react-is (`cjs/react-is.production.js`) |
| sudo-prompt | @vscode/sudo-prompt | fork | — | — | 327 / — | Microsoft's fork |

Not credited although exp210 named them: the rest of hyperdht-hugin
(27,008: 24,809 claimed by growth on statements whose print is not in
hyperdht's rows, 2,199 too few literals), rambdax (7,678), prompt-tsx
(14,938: its TS-compiled copy prints differently), @jest/react-is-18
(its registry manifest is a 404 in the cache: no fork can be proved).

## 4. Map defects (excluded from both sides)

| set | truth label (package, file) | bytes | claimed as | what the unit really is (listing: absent literals / holder) |
| --- | --- | --- | --- | --- |
| bun-held node | node-fetch `index.js` | 42,189 | iconv-lite 42,189 | iconv-lite's `cp950.json` table (`a140`, `a1a1`, …): the exp210 case |
| bun-held node | encoding-sniffer `dist/commonjs/sniffer.js` | 25,286 | undici 462, rest unclaimed | iconv-lite code and tables |
| bun-held node, bun-dev node | chalk `source/vendor/ansi-styles/index.js` | 19,494 each | unclaimed | cli-spinners' `spinners.json` (`aesthetic`, `arrow2`, …) |
| bun-held node | undici `lib/core/errors.js` | 200 | unclaimed | encoding-sniffer code |
| bun-dev node | encoding-sniffer `dist/commonjs/sniffer.js` | 126,435 | undici 2,310, rest unclaimed | iconv-lite tables and code |
| bun-dev node | micromark `dist/tokenize/character-escape.js` | 29,679 | unclaimed | the HTML character-entity table (`AElig`, `Aacute`, …) |
| bun-dev node | domhandler `lib/index.js`, dom-serializer `lib/index.js` | 28,734 + 1,152 | unclaimed | entity tables |
| bun-dev node | needle `lib/needle.js` | 17,607 | unclaimed | iconv-lite table |
| bun-dev node | core-js `modules/_fix-re-wks.js`, `es6.array.from.js` | 5,730 + 5,625 | unclaimed | i18n-iso-countries locale JSON |
| bun-dev node | lru-cache `dist/commonjs/node/index.min.js` | 6,432 | css-tree 3,985, stylelint 311 | css-tree's syntax code (exp210's case) |
| bun-dev node | undici `lib/core/errors.js`; micromark-extension-frontmatter `lib/matters.js`; yocto-queue `index.js` | 1,000; 253; 99 | micromark 253 | encoding-sniffer code; micromark's label-end; @swc/core code |
| bun-dev bun | semver `semver.js` | 72,657 | unclaimed | the app's own Electron code (`BrowserWindow`, `ELECTRON_IS_DEV`; 185 of 967 literals in semver) |

Only 6,859 (bun-dev node) and 42,651 (bun-held node) CLAIMED bytes sit on
defects; the rest only leaves recall's denominator.

Not caught (stays wrong, strict): micromark → mdast-util-from-markdown
8,285 (`truth-file-lacks` for the credit; the package-level defect check
finds >20% of the evidence in mdast's installed files), css-tree ↔ csso
`package.json` modules (csso's installed `package.json` shares >20% of the
keys).

## 5. Wrong claims left wrong (bun-dev node, 175,780 bytes)

| reason | bytes |
| --- | --- |
| print-not-in-claimed | 67,669 |
| few-literals (< 3 distinctive) | 41,212 |
| literals-not-in-claimed (contamination; look-alikes) | 41,060 |
| truth-file-lacks | 14,153 |
| no-link | 8,235 |
| claimed-ships-truth | 3,451 |

Largest: readable-stream → string_decoder 40,177 (literals not in
readable-stream's published files: the exp210 index contamination, now
proved by content), hyperdht → hyperdht-hugin 27,008, compiler-sfc →
playwright-core 21,734 (no link), prompt-tsx 14,938, mdast 8,285,
rambdax 7,678, readable-stream → safe-buffer 5,367, takumi → ast-grep
3,483 (no link / literals differ).

## 6. Gate

`cargo dev check` at `c81da2adb` (rebased on main `2df1f28a6`), the
coordinator's go; worktree first, then a clean detached checkout, one at a
time. The commit after it changes only this README.

| stage | worktree | clean checkout |
| --- | --- | --- |
| shims | PASS | PASS |
| no-private-addresses | PASS | PASS |
| rust:fmt | PASS | PASS |
| rust:clippy | PASS | PASS |
| census:clones (advisory) | REVIEW: only twins already on main (none in the new code; scope_block/map_body, which this branch made twins, were unified into `verdict_cols`, output byte-identical) | REVIEW (same) |
| census:concurrency | PASS | PASS |
| census:slop (advisory) | PASS | PASS |
| rust:unit | PASS | PASS |
| rust:build | PASS | PASS |
| format-golden | PASS | PASS |
| e2e | PASS (19 fixture pairs, 152 launches = golden) | PASS |

The first worktree run failed `rust:unit` (`pipeline_stages` webpack
test: `Cannot find package 'webcrack'`): the fresh worktree had no
`npm ci` yet (setup step), not this change; after `npm ci
--ignore-scripts` all 11 stages pass.

## What this could NOT prove

- Rule 5 of the defect check reads the corpus apps' INSTALLED trees on this
  host (`/work/exp150-data/bun/apps/*/node_modules`), which are not part of
  the frozen corpus. The verdict is deterministic on this host, but a host
  without those trees gets no defects at all (the rule then never fires;
  it cannot invent one). The kind-* corpora got 0 defects, probably for
  that reason (not checked further).
- Copy credit reads the npm registry-manifest cache (not frozen either; npm
  manifests of a published version do not change). A missing manifest only
  refuses credits.
- playwright-core's `utilsBundle` inlines zod, commander, minimatch, diff,
  chokidar… (kind-webpack/rollup's largest wrong pairs, and compiler-sfc's
  on bun-dev): claims naming the inlined package may name the code's true
  origin, but there is no declared link (no dependency, name, vendor path
  or shared file), so the rule refuses them. A credit for marker-less
  inlining would need a different kind of evidence.
- Thresholds (8 literals / 20% for a defect; 3 literals, 90% / 80% for a
  credit; fork stem 5) were set before the data run and not swept; the
  refusals listed per reason show what a looser setting would admit.
- A defect EXCLUDES a unit whose package is unknown; it does not say what
  the package is. Where the claim was probably right (css-tree on
  lru-cache's file), the exclusion costs a right claim, not a wrong one.
- The defect check is new truth logic on 2 corpora; its listing was read
  group by group (§4), not unit by unit.

## Proposed CLAUDE.md text

Under "Validating cross-version changes", after the library score verb:

> **The library scorer is scorer 15 (`exp096-score/15`, exp214).** Scorer
> 14's cards plus two truth corrections, each its own column: a
> source-map DEFECT (the label's package — its map files, its published
> files in the index literal table and its installed copy — holds at most
> 20% of the unit's literals) is out of both sides
> (`defectExcludedBytes`); a COPY CREDIT (the claimed package's own
> published file holds the unit's print and >= 90% of its literals, the
> truth's file holds the code, and a dependency / fork / vendored /
> same-file link says the truth package ships a copy) counts as named
> (`namedAsCopyCreditBytes`; `precisionNoCopyCredit` charges it as wrong).
> Every corrected unit is listed in `truth15.json` beside the cards, with
> the wrong claims left wrong by reason. Labels of scorer 14 and 15 do not
> share a leaderboard, and `library proof` refuses a label of another
> version: rescore it with `library rescore <label> --out DIR`. The defect
> check needs the corpus apps' installed trees on this host.
