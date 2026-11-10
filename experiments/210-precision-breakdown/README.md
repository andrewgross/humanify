# exp210 — where library naming precision is lost on the node slice

STATUS: DIAGNOSTIC ADDED, MISS EXPLAINED, NO FIX ADOPTED, NOT MERGED
(2026-11-10). Branch `exp210-precision-breakdown` from main `61063385e`.
Index: the adopted `/work/library-index/4736dcfa…`. Rule: structural (the
shipped one). Scorer 14 unchanged: every card and summary is byte for byte
what it was (`cargo dev library proof bun-dev-exp150-main`: cards IDENTICAL,
summary IDENTICAL).

## Totals

- **The node slice misses the 0.998 bar for three different reasons, and
  only about a fifth of its wrong bytes are plainly the matcher's fault.**
  Of bun-dev node's 241,822 wrong-family bytes:
  - **110,695 (46%)**: the truth names a FORK or a re-published COPY that
    the index does not hold (`hyperdht-hugin`, `rambdax`,
    `@vscode/prompt-tsx`, `@babel/core`'s vendored `import-meta-resolve`),
    and the claim names the package the code was copied from (or another
    copy of it). The code is the claimed package's code. Same on the bun
    slice (119,459 there, with `better-sqlite3-multiple-ciphers`).
  - **≥ 22,105 (9%)**: SOURCE-MAP DEFECTS in the truth — the corpus
    bundle's own source map files a module's code (often a JSON module or
    a `package.json`) under a neighbouring source; the claim names the
    right package. bun-held node's biggest single loss is one of these:
    42,189 bytes of iconv-lite's `cp950.json` table filed under
    `node-fetch/index.js`.
  - **45,544 (19%)**: a matcher error from a CONTAMINATED INDEX —
    `string_decoder` (40,177) and `safe-buffer` (5,367) claimed as
    `readable-stream`. In 35 dependents' node-slice shards (all with the
    builder's own `multiModuleFiles` and `mixedSpans` flags raised) the
    index build's own Bun source map files string_decoder's statements
    under readable-stream's `lib/internal/streams/async_iterator.js` /
    `from.js`. An INDEX problem, fixable at build time (§5b).
  - **≈ 27,000 (11%)**: a matcher error from PRE-BUNDLED COPIES —
    `@vue/compiler-sfc`'s CommonJS dist inlines `minimatch`, playwright-core's
    `utilsBundle.js` inlines the same `minimatch`; the app's copy is claimed
    as compiler-sfc (21,734). Same shape: chalk's vendored supports-color
    (1,933), napi-rs generated loaders (`@takumi-rs/core` for
    `@ast-grep/napi`, 3,483).
  - the rest is a long tail of < 2,600 bytes per pair.
- **Why node is worse than bun (91,549 more wrong bytes on bun-dev):**
  readable-stream→string_decoder/safe-buffer (+45,544) exists only on node
  because the index's BUN slice holds no readable-stream 2.x/3.x or
  safe-buffer rows at all (their bun-target shards are empty), so the
  contaminated rows exist only in the node slice; compiler-sfc→playwright
  (+19,425) is seeded only from the node slice's rows; and the source-map
  defects (+22,105) sit in modules only the node builds pull in. It is not
  node builtins and not a polyfill-vs-package confusion.
- **bun-held node (0.99716)**: 42,189 of its 66,396 wrong bytes are the one
  source-map defect above; readable-stream→string_decoder/safe-buffer is
  15,781 more. Without the map defect it would read ≈ 0.99896.
- **No safe matcher fix found.** The one cheap matcher lever, refusing a
  structural tie between two families instead of breaking it by the
  previous unit / alphabetically, was measured (§4). Its strict form (V2)
  removes the readable-stream→string_decoder/safe-buffer bytes and lifts
  BOTH node slices over the bar (bun-dev node 0.997919, bun-held node
  0.997906), but it adds 52,998 wrong bytes on bun-dev's bun slice, which
  then misses (0.997758): it moves wrong claims instead of only dropping
  them. Not adopted. The real fixes are an index-build change (keep a unit row only if
  its attributed source file confirms it, §5b) and two truth-layer / scorer
  decisions (forks / copies as origin; a wider map-defect layer, §5a), all
  Andrew's calls — proposed, not built.

## What changed in the code

- `crates/humanify-ag-dev/src/library/wrong_families.rs` (NEW, tests in
  `wrong_families_test.rs`): the wrong-family breakdown. `cargo dev
  library score` / `rescore` now write `wrong-families.json` beside the
  cards (a private corpus's stays beside its private cards): per set, the
  cards' `wrongFamilyBytes` split by (claimed package, true package) with
  units, distinct truth files, bytes per claim rule and the top truth
  files; by scorer family pair (with how many entries); and by entry. Its
  per-set total is held equal to the summary's `wrongFamilyBytes`
  (`check_total`, the rescore fails otherwise). Sample-truth sets (Claude
  Code) say `notBrokenDown`: their bytes are stratified estimates.
- `crates/humanify-ag-dev/src/library/score.rs`: the scorer collects each
  `wrongFamily` claim on the way (no verdict changed) and writes the file.
  Cards and summary are not touched, so `library proof` still holds.
- `docs/responsibility.md`: a row for the new owner.

## 1. Runs

| label | corpus | binary | what |
| --- | --- | --- | --- |
| `exp210-base-bun-dev` / `exp210-base-bun-held` | bun-dev / bun-held | main `61063385e` + the diagnostic (`84e82efaf`) | fresh discover + score, the baseline |
| `exp210-tiev1-*`, `exp210-tiev2-*` | both | scratch commits (not on the branch) | the tie lever, §4 |

The baseline is byte-equal to exp204's `exp204b-new-*-structural` (same P,
R and wrong bytes on all four slices). bun-dev took ~5 min wall (4 jobs),
bun-held 18 s.

## 2. Results (baseline; nothing adopted, so before = after)

| corpus, slice | P | bar | R | wrong bytes | named bytes | app bytes |
| --- | --- | --- | --- | --- | --- | --- |
| bun-dev, bun | 0.998342 | 0.998 pass | 0.8640 | 150,273 | 90,486,019 | 0 |
| bun-dev, node | 0.997439 | 0.998 MISS | 0.8594 | 241,822 | 94,167,529 | 0 |
| bun-held, bun | 0.999755 | 0.998 pass | 0.8697 | 5,563 | 22,714,993 | 0 |
| bun-held, node | 0.997162 | 0.998 MISS | 0.8315 | 66,396 | 23,324,884 | 0 |

## 3. The top wrong pairs and their causes

bun-dev node unless noted. "Truth" = what the truth layer says the bytes
are; "who is wrong" was decided by reading the bundle text of sampled units
(`entry.js` at the claim's span) and, for the index, the unit rows behind
the claim.

| claimed → truth | bytes node / bun | entries | cause | who is wrong |
| --- | --- | --- | --- | --- |
| hyperdht → hyperdht-hugin | 70,359 / 70,359 | 1 (hugin-desktop) | `hyperdht-hugin@1.0.3` is a renamed fork of hyperdht (own repo, no declared link); the index has no hyperdht-hugin version; 93% of the claim is index seeds on hyperdht's own statements | truth family: the code is hyperdht's |
| readable-stream → string_decoder | 40,177 / 0 | 10 | index contamination: 35 node-slice shards of readable-stream dependents (ora@5/6, shadcn-ui, @larksuite/openclaw-lark-tools, …), every one with `multiModuleFiles>0` and `mixedSpans>0`, file string_decoder's statements under readable-stream's `async_iterator.js` / `from.js`. A print held by readable-stream and string_decoder ties and the tie goes to readable-stream (previous unit, else alphabetical); the rest follows by DOWN / MODULE growth | matcher (index builder) |
| @vue/compiler-sfc → playwright-core | 21,734 / 2,309 | 1 (vscode-github-actions) | the app's playwright-core `lib/utilsBundle.js` inlines minimatch 10 (with `// node_modules/minimatch/dist/esm/…` markers); compiler-sfc 3.5.33+'s `dist/compiler-sfc.cjs.js` inlines the same minimatch with no markers, so the index files it as compiler-sfc's own | matcher (index): neither package wrote this code. The truth is right that it is not compiler-sfc |
| monaco-editor → @vscode/prompt-tsx | 17,799 / 17,799 | 1 | prompt-tsx ships copies of VS Code's `vs/base/common` (path.js, uri.js, platform.js); monaco-editor ships the same copies; prompt-tsx is not indexed | neither: both are copies of VS Code's source; truth family question |
| rambda → rambdax | 15,845 / 15,845 | 1 | `rambdax@10.0.0` `dist/rambdax.js` carries rambda's functions; rambdax is not indexed (one rambda version is) | truth family: the code is rambda's |
| micromark → mdast-util-from-markdown | 8,504 / 0 | 1 (doctoc) | source-map defect: micromark's `tokenize/html-flow.js`, `html-text.js` (index rows: micromark 2.11.x) are mapped to `mdast-util-from-markdown/dist/index.js` | truth |
| @dual-bundle/import-meta-resolve → @babel/core | 6,692 / 6,688 | 1 | @babel/core vendors import-meta-resolve in `lib/vendor/import-meta-resolve.js`; the claim names another re-packaging of it | neither: truth has no origin for the vendored file |
| css-tree → lru-cache | 5,533 / 0 | 2 | source-map defect: css-tree parser scope code mapped to `lru-cache/dist/commonjs/node/index.min.js` | truth |
| readable-stream → safe-buffer | 5,367 / 0 | 7 | as string_decoder (safe-buffer's bun-target shard is empty too) | matcher (index builder) |
| @takumi-rs/core → @ast-grep/napi | 3,483 / 0 | 1 | both ship the napi-rs generated `index.js` loader | matcher: generated-code twin |
| css-tree → csso / csso → css-tree | 3,301 + 1,972 / 0 | 1 | source-map defect: each package's `package.json` (a JSON module) mapped to a file of the other | truth |
| stylelint → lru-cache | 2,795 / 0 | 2 | source-map defect, as css-tree → lru-cache | truth |
| chalk → supports-color | 1,933 / 0 | 1 | chalk 5 vendors supports-color (`source/vendor/supports-color`); the index files the copy as chalk's | matcher (index) |
| better-sqlite3 → better-sqlite3-multiple-ciphers | 0 / 8,768 | 1 | fork; the index holds 11 versions of the fork, not 11.10.0 | truth family |
| **bun-held node:** iconv-lite → node-fetch | 42,189 | 1 (codex.notes) | source-map defect: iconv-lite's `encodings/tables/cp950.json` module (`var P4=R(function(KGt,_We){_We.exports=[["0","\x00",127],["a140",…`) is voted node-fetch 1.7.3 `index.js` / `lib/body.js` by the map; the misattr13 layer caught the NEXT module (`big5-added.json`, `installed` evidence) but not this one, and mapdefect14 flags 0 rows here | truth |
| bun-held: react-is → @jest/react-is-18 / -19 | 3,802 (both slices) | 1 | Jest's re-published react-is copies, not indexed | truth family |

The sums in the totals use the whole pair's bytes for a cause verified on
sampled units; a pair can mix causes (not proven unit by unit).

## 4. The one cheap lever, measured: refuse cross-family structural ties

`LibraryIndex::structural` breaks a tie between packages by the previous
unit's package, else by the alphabetically first package
(`crates/humanify-ag-core/src/libindex/lookup.rs`). That is how
`readable-stream` beats `string_decoder` on a print they share. Two scratch
variants (commits on `exp210-tie-scratch`, NOT on this branch):

- V1: a tie spanning two families that the previous unit does not resolve
  is no pick.
- V2: any tie spanning two families is no pick.

| corpus, slice | base P / wrong | V1 P / wrong | V2 P / wrong | bar |
| --- | --- | --- | --- | --- |
| bun-dev, bun | 0.998342 / 150,273 | 0.997838 / 196,385 (MISS) | 0.997758 / 203,271 (MISS) | 0.998 |
| bun-dev, node | 0.997439 / 241,822 | 0.997485 / 237,249 | **0.997919** / 195,571 | 0.998 |
| bun-held, bun | 0.999755 / 5,563 | 0.999738 / 5,940 | 0.999753 / 5,613 | 0.998 |
| bun-held, node | 0.997162 / 66,396 | 0.997293 / 63,288 | **0.997906** / 48,858 | 0.998 |

App bytes claimed: 0 in every run. Recall: V2 costs 0.0037 on bun-dev node
(named −399,152 bytes), 0.0015 on bun-held node.

- **V2 confirms the mechanism**: readable-stream→string_decoder (−40,177)
  and →safe-buffer (−5,367) vanish on bun-dev node, and both node slices
  pass the bar.
- **But V2 is not safe**: dropping a seed does not just drop its bytes, it
  changes what the growth rules (DOWN / MODULE / GLUE) and the isolation
  veto do next. On bun-dev's BUN slice V2 ADDS 52,998 wrong bytes —
  compiler-sfc→playwright-core +19,827, @mastra/schema-compat→playwright-core
  +11,611, rambda→rambdax +16,118, mermaid→d3-shape +7,485 — and that
  slice falls from a pass to 0.997758. V1 (only ties the previous unit does
  not resolve) is worse on both bun slices and barely moves node.
- Not adopted. The scratch commits are `dd981dcd9` (V1) and `3d6fd6046`
  (V2) on `exp210-tie-scratch` (local, not pushed). A tie rule safe on both
  slices would need the growth rules to stop treating an unseeded unit
  between two tied families as free for DOWN — a separate experiment.

## 5. Truth-layer problems vs index problems, and what would fix each

### 5a. Truth-layer problems (the claim is right or names the copied code)

Bytes on bun-dev node / bun-dev bun / bun-held node.

| problem | pairs | bytes |
| --- | --- | --- |
| the truth names a FORK or re-published COPY the index does not hold; the code is the claimed package's | hyperdht-hugin, rambdax, @vscode/prompt-tsx, @babel/core's vendored import-meta-resolve, better-sqlite3-multiple-ciphers (bun), @jest/react-is-18/19 (bun-held) | 110,695 / 119,459 / 3,802 |
| SOURCE-MAP DEFECTS in the corpus bundle's map (a module filed under a neighbouring source; often a JSON module or `package.json`) | micromark→mdast-util-from-markdown, css-tree/stylelint→lru-cache, css-tree↔csso, iconv-lite→node-fetch (bun-held) | ≥ 22,105 / 0 / 42,189 |

Fixes (scorer decisions, Andrew's call — each is a new scorer version):

1. **Credit a copy to the package it copies.** A truth label of a fork or
   re-published copy the index cannot hold could carry the upstream as its
   origin, as the copies layer already does for marked regions, under an
   evidence rule (e.g. byte-identical source files with an indexed
   version) — not a family-table edit made to pass a bar. Crediting
   hyperdht-hugin and rambdax alone would lift bun-dev node to ≈ 0.99835
   (all four copy pairs: ≈ 0.99861).
2. **A wider map-defect layer.** misattr13's `installed` check (the
   module's text is found in an installed package it is NOT labelled with)
   caught `big5-added.json` but not the `cp950.json` module right before
   it; the csso/css-tree `package.json` modules and the code mapped onto
   `lru-cache/…/index.min.js` are the same shape. bun-held node would read
   ≈ 0.99897 without the one cp950 module.

### 5b. Index problems (the matcher is wrong because the index is)

| problem | pairs | bytes (bun-dev node / bun / bun-held node) |
| --- | --- | --- |
| CONTAMINATED node-slice shards: another module's statements filed under a readable-stream file | readable-stream→string_decoder, readable-stream→safe-buffer | 45,544 / 0 / 15,781 |
| PRE-BUNDLED COPIES: a package's marker-less dist inlines a third package | @vue/compiler-sfc→playwright-core (minimatch), chalk→supports-color | 23,667 / 2,309 / 0 |
| generated-code twins | @takumi-rs/core→@ast-grep/napi (napi-rs loader) | 3,483 / 0 / 0 |

**What the contamination is.** The builder attributes each statement unit
of a shard's bundle to a source file through the INDEX BUILD's own source
map (`humanify-ag-index` `build::unit_rows`, `unit_owner` over
`map.span_segments`). In 35 node-target shards of readable-stream
dependents (ora@5.3.0, ora@6.2.0, shadcn-ui@0.8.0,
@larksuite/openclaw-lark-tools@1.0.40-44, …) Bun's map files
string_decoder's module body under readable-stream 3.6.0/3.6.2's
`lib/internal/streams/async_iterator.js` or `from.js`: 22 merged node rows
of readable-stream carry string_decoder's prints (the `isEncoding`
statement `3986bfce9ad81c15`, `StringDecoder`'s methods, …). Every one of
the 35 shards raises the builder's own `multiModuleFiles` and `mixedSpans`
notes, but the existing guard (a source credited to two MODULES drops that
file's units) does not fire, because at module level each file was still
credited once. On the BUN target the same builds fail the other way —
readable-stream 2.x/3.x and safe-buffer shards attribute to "mixed" and
hold no rows at all — so the bun slice has no wrong rows to tie with.
The same class of defect (a map filing one module under a neighbour) is
what §5a's truth defects are, in the corpus bundles.

**The index-build fix (proposed, not built).** Keep a unit row only when
its attributed source file confirms it:

- *Lexical confirmation (preferred; local to one shard).* The builder
  already reads each attributed source lexically (`sponge_evidence` →
  `lexical(...)`). Require a unit's distinctive tokens — its string
  literals and property names — to occur in the attributed file's text;
  a unit that fails is dropped and counted (`unitLexicalRefused`).
  string_decoder's statements carry `"ucs2"`, `"utf16le"`, `isEncoding`;
  readable-stream 3.6.2's `async_iterator.js` and `from.js` contain none of
  them (checked on an installed copy). Units with no distinctive token
  stay as now.
- *Own-shard confirmation (merge time; coarser).* A row for P@v
  contributed by a DEPENDENT's shard is kept only if P@v's own shard (when
  it exists and is non-empty) holds the same (file, print). Sized on
  readable-stream's node rows: of 1,718, 1,218 are confirmed, 358 cannot
  be checked (no own shard for any of their versions), 142 would be
  dropped — including all 22 contaminated rows whose range includes
  3.6.2 (3.6.0 has no own shard, so 3.6.0-only rows would survive).
  The other 120 dropped rows are unexamined: some may be legitimate code
  only a dependent's build reaches.

Either needs a builder change, unit tests, and an index re-merge
(`index build --replay` over the stored builds, hours of CPU and ~43 GB);
it changes rows for every package these shards carry, so it needs its own
before/after on bun-dev, bun-held and the CC versions. It cannot touch
app bytes (it only removes index rows), so it can only lower claims.

The pre-bundled-copy case (compiler-sfc / playwright-core both inlining
minimatch) has no cheap fix: the index cannot tell a marker-less inlined
copy from the shipper's own code. A content cross-check of each shipper
row against other packages' own rows at build time is the only lever I
see.

## 6. Gate

`cargo dev check` at `56f7d00ae` (the coordinator's go; worktree first,
then a clean checkout, one at a time). The commit after it changes only
this README.

| stage | worktree | clean checkout |
| --- | --- | --- |
| shims | PASS | PASS |
| no-private-addresses | PASS | PASS |
| rust:fmt | PASS | PASS |
| rust:clippy | PASS | PASS |
| census:clones | REVIEW (no group in this lane's code; `score.rs` `read_card` was already in a group) | REVIEW (same) |
| census:concurrency | PASS | PASS |
| census:slop | PASS (humanify-ag-dev erosion −0.2 pp) | PASS |
| rust:unit | PASS | PASS |
| rust:build | PASS | PASS |
| format-golden | PASS | PASS |
| e2e | PASS | PASS |

Also: `cargo dev library proof bun-dev-exp150-main` — cards IDENTICAL,
summary IDENTICAL (the scorer's output is unchanged; only the new file is
added).

## What this could NOT prove

- Why the bun-target shards of readable-stream 2.x/3.x, safe-buffer and
  string_decoder hold no modules at all (`unitMixedSpans: 2`, 0 modules):
  observed, not explained. It is the reason the contamination is node-only.
- Which Bun map segments put string_decoder's body under
  `async_iterator.js` / `from.js` in the 35 shards: the misattribution is
  in the per-unit map tally (`unit_owner`); the map itself was not decoded
  for those builds (their trees are deleted after the shard lands).
- Whether Bun's node target writes worse source maps than its bun target
  in general: both the truth defects and the index contamination are
  node-target map misattributions, and the bun-target builds of the same
  packages fail as "mixed" instead (dropped, harmless); that is these
  cases, not a measured rate.
- The category sums assume every unit of a pair shares the verified cause
  (sampled units were read; not every unit).
- No eval was run: nothing here changes a pipeline output.
