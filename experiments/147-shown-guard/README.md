# exp147: a guard for `veto_support = "shown"`'s dev errors — none found

## STATUS (2026-10-24)

DONE on branch `exp147-shown-guard` (off `origin/exp144-veto-flip` `0c4431839`, main `d84196a24` merged in for the
remin4 corpus and exp145/146's bar). **Analysis only: no crates change, no key, no default flipped, nothing merged,
no score run, no judge read.** Dev corpora only; test / test2 / remin2 were not scored, diagnosed or opened.

**Result: no generic guard removes `shown`'s dev errors while keeping >= 90% of its gain on every dev set. Nothing
was built.** Seven guards were simulated (table §3); the best one (E) still keeps only 88.3% on remin-esbuild,
leaves pop-bun's 2.45 KB event and the remin4 `ag-psd` app claim in place, and needs an index-path rule with a
one-half share.

- **What `shown` adds that is wrong** (unit level, scorer 14, every dev set, `*-exp144-shown2` vs `*-exp142-asof`):
  **29.9 KB new wrong against 55.1 MB newly right** (0 on every terser set). 18.9 KB of the 29.9 (63%) belongs to a
  (bundle, family) that **main already claims there with no right bytes at all**; another 5.6 KB is mixed, 4.4 KB
  is small boundary bytes next to a right family, and 1.0 KB is a family absent from main's claims in that bundle
  (`basefam.py`).
- **The mechanism: `shown`'s presence condition trusts main's own kept seeds, so it extends families main already
  names wrongly.** "The bundle shows family F" is read from F's seeds that the default veto keeps. When those seeds
  are themselves a copy, or a coincidence in app code, `shown` gives the copy's remaining files fill support.
  It adds no new wrong family; it adds more of an old one.
- **No local signal separates those rescues from right ones.** Exact-duplicate prints in the bundle: 0 events
  (inert). Rescued seed's module, before and after fill, and the family's kept seeds' module coverage: the copy
  rescues look the same as right ones (whole modules, interleaved with the family's kept modules). Guards based on
  these features either miss the errors or cost 10-100% of the gain on some sets (§3).
- **The one signal that does see the largest event comes from the index's file paths, not the bundle**: every seed
  of the copied family maps to ONE child directory of that package's indexed files, a directory holding a minority
  of them (a generated SDK's shared runtime directory). As a guard (D/E) it removes the 8.22 KB event but costs
  6-43% on wrangler / remin3-esbuild / remin-esbuild (packages with dual cjs/esm build dirs, a runtime directory
  next to icon files). It is a layout heuristic with a threshold.
- **The bar is unchanged.** exp145's terser check 2 still fails on pop-esbuild's 8.35 KB single-bundle event (and
  pop-bun's 2.45 KB, which is over the pooled T36 of 2.21 KB). The remin4-esbuild app bar still fails on the
  `ag-psd` 592 B. Whether exp144 goes to Andrew as is, with these documented as main's errors amplified, is his
  call.

## 1. The errors, unit by unit (dev only)

`newwrong.py` (exp144) per corpus, then `feat.py` / `kepthash.py` / `ixfiles.py` / `layout.py` (this directory;
the rows name bundles and stay in `/work/exp147-data`).

| event                                                                                                                             | new wrong                          | why `shown` admits it                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| --------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| pop-esbuild, `@elevenlabs/elevenlabs-js` on `agentmail`                                                                           | 8.22 KB (bundle total 8.35)        | `agentmail` ships a verbatim copy of the generated SDK runtime (fetcher, logging, schema builders) that `@elevenlabs/elevenlabs-js` also ships. `agentmail` is not in the index (no version), so the prints are unambiguous. Main already claims that copy as `@elevenlabs/elevenlabs-js` (66 kept seeds, 34 whole modules): all 35.4 KB of main's claims of the package in this bundle are wrong, none right. `shown` sees the family as shown and rescues the copy's other files (64 seeds), each a whole module, interleaved with the kept ones. |
| pop-bun, same app, same copy                                                                                                      | 2.45 KB                            | same (102 kept seeds; main's 43.3 KB of the package here all wrong).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| remin4-esbuild, `@elevenlabs/elevenlabs-js` on `cohere-ai`                                                                        | 0.21 KB                            | same runtime copy, shipped by `cohere-ai` (not indexed either). One rescued seed.                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| remin4-esbuild, `ag-psd` on the app's own code (app claimed)                                                                      | 0.59 KB                            | Main already claims 4.2 KB of this app's own modules as `ag-psd` (12 kept seeds from the package's reader/writer/helper files, covering 19% of their modules, no whole module), all of it app code. `shown` rescues a 95 B seed in a 6.7 KB unclaimed app module; the fill reaches 592 B (9% of the module).                                                                                                                                                                                                                                        |
| heldout / pop-bun (7 bundles), `stylelint` on `lru-cache`                                                                         | 0.87-0.89 KB each, 6.2 KB in total | Main keeps two small `stylelint` seeds (636 B, one rule file's helpers) in each bundle, all on other code; the rescued seed (311 B, the same rule file) fills an 888 B `lru-cache` module.                                                                                                                                                                                                                                                                                                                                                          |
| heldout, `@ahooksjs/use-request` on `@umijs/use-request`                                                                          | 0.81 KB                            | a fork; the family is shown through a scope sibling.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| remin4-esbuild, `@aws-amplify/*` (0.24 KB), `cockatiel` (74 B x 9 bundles over remin/remin3/remin4), `@inversifyjs/common` (80 B) | small                              | boundary bytes of a module fill next to a RIGHT rescue (`@aws-amplify/datastore` +66.6 KB right with 160 B wrong; `cockatiel` +8.3 KB right with 74 B wrong). Not separable from the right fill; a guard that drops them drops 100x as much right.                                                                                                                                                                                                                                                                                                  |

The shown -> right rescues they have to be told apart from (3-10 MB per set): the same shape, a whole small module
whose seed the default veto drops for lack of a claimed neighbour, in a family the bundle shows elsewhere.

## 2. Signals checked

| signal (generic, from the report or the index)                                          | what it shows                                                                                                      |
| --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| the rescued seed's print appears elsewhere in the bundle (a second copy)                | 0 of 46,278 rescued seeds: inert.                                                                                  |
| the rescued seed's module is claimed before the fill                                    | 0% for errors and for right rescues alike.                                                                         |
| the rescued seed's module after the fill (wholly / half the family's)                   | catches `ag-psd` (9%); the copies are whole modules. Terser groups are rarely whole: costs 15-71% there.           |
| the family's kept seeds' module coverage / a whole module held                          | `ag-psd` holds none; the runtime copy holds 34. Remin sets lose 25-60%.                                            |
| exact package has a kept seed (not a scope sibling)                                     | catches the fork (0.81 KB), costs apps-esbuild 23% (an icon package shown by its scope sibling).                   |
| the family's seeds all from one child directory of its indexed files (index file paths) | catches the runtime copy (all its seeds are in `core/`); also every package whose code is under `lib/` or `dist/`. |

## 3. Simulation (`scripts/simall.py`; % of `shown`'s gained-right bytes kept, `(-x)` = new wrong KB removed)

Family-level guards (A-E) drop every changed byte of the (bundle, family); seed-level guards (F, G) drop the
refused seed's module's changes. No propagation re-modelled. cc and dev-webpack: `shown` changes nothing.

| guard                                                     |       heldout | esbuild-app | wrangler |  apps-esbuild | apps-rollup | apps-webpack | dev-rollup |       pop-bun |   pop-esbuild | remin-esbuild | remin-terser | remin3-esbuild | remin3-terser | remin4-esbuild | remin4-terser |  min kept |   wrong removed |
| --------------------------------------------------------- | ------------: | ----------: | -------: | ------------: | ----------: | -----------: | ---------: | ------------: | ------------: | ------------: | -----------: | -------------: | ------------: | -------------: | ------------: | --------: | --------------: |
| `shown`: gained right KB / new wrong KB                   |  8,162 / 4.33 |  884 / 0.00 |   25 / 0 |  3,371 / 1.16 |   76 / 0.05 |      102 / 0 |      3 / 0 | 7,345 / 11.10 |  8,100 / 9.77 |  2,703 / 0.19 |      706 / 0 |   8,360 / 1.47 |     1,443 / 0 |  10,664 / 1.88 |     3,140 / 0 |         - |               - |
| A shown needs a kept seed of the exact package            | 98.0% (-0.81) |       99.7% |   100.0% | 77.1% (-0.05) |       98.3% |       100.0% |     100.0% |         98.6% |         98.6% |         87.5% |        96.5% |          82.4% |         87.2% |  89.6% (-0.10) |         95.8% |     77.1% |  1.0 of 29.9 KB |
| B shown needs a whole module the family holds             | 99.5% (-0.81) |       99.9% |     0.0% |         99.4% |       91.7% |        99.1% |     100.0% |         99.5% |         99.5% | 75.2% (-0.07) |        65.4% |  39.6% (-0.52) |         75.0% |  64.5% (-0.99) |         78.0% |      0.0% |  2.4 of 29.9 KB |
| C kept seeds all in one child dir of the code root        | 95.6% (-2.08) |       98.5% |    57.4% | 86.3% (-0.28) |      100.0% |       100.0% |     100.0% | 96.2% (-6.87) | 86.8% (-8.28) |         88.8% |        70.2% |  52.9% (-0.49) |         78.5% |  73.0% (-0.39) |         75.8% |     52.9% | 18.4 of 29.9 KB |
| D C, and that dir holds < half the family's indexed files | 99.8% (-0.05) |       99.8% |    57.4% |         99.4% |      100.0% |       100.0% |     100.0% | 99.8% (-2.45) | 99.9% (-8.22) |         98.7% |        96.5% |  60.3% (-0.15) |         93.7% |  97.1% (-0.29) |         93.5% |     57.4% | 11.2 of 29.9 KB |
| E D on kept + rescued seeds                               | 99.8% (-0.05) |      100.0% |   100.0% |         99.8% |      100.0% |       100.0% |     100.0% |         99.9% | 99.9% (-8.22) |         88.3% |        98.2% |  90.1% (-0.07) |         91.5% |  90.3% (-0.31) |         96.0% | **88.3%** |  8.6 of 29.9 KB |
| F rescued seed's module wholly the family's after fill    | 96.1% (-0.28) |       97.3% |     0.0% | 99.6% (-0.13) |       50.5% |        51.3% |      82.5% | 96.2% (-0.92) | 98.5% (-0.60) |         71.6% |        35.0% |  78.4% (-0.15) |         28.8% |  59.0% (-0.66) |         30.2% |      0.0% |  2.8 of 29.9 KB |
| G rescued seed's module >= half the family's after fill   |         99.5% |       99.6% |    92.4% |         99.9% |       97.6% |        99.8% |     100.0% | 99.3% (-0.32) |         99.9% |         96.0% |        84.0% |  97.3% (-0.07) |         85.3% |  96.2% (-0.59) |         84.8% |     84.0% |  1.0 of 29.9 KB |

Keep rule: removes the errors (the pop 8.35 / 2.45 KB events and the remin4 app claim) AND keeps >= 90% on every
set. **None qualifies.** E is the closest: misses the 90% on remin-esbuild (88.3%), leaves pop-bun's 2.45 KB
(over the pooled T36 of 2.21 KB) and the `ag-psd` app claim; G removes the app claim but costs 15-16% on every
terser set; no single guard removes both kinds, and combining them compounds the losses. The `stylelint` /
`lru-cache` events (0.9 KB each) are under the pooled T36, and only C removes them, at 47% of remin3-esbuild's
gain.

## 4. What was not done, and why

- **No crates change, no score run, no neutrality**: nothing passed the simulation.
- **No judge commands for a new variant**: there is none. exp144's own judge commands are unchanged (exp144
  README §4).
- **What would need to come first to fix the root cause**, in main as much as in `shown` (main already claims
  35.4 KB / 43.3 KB of the runtime copy, all wrong): index coverage of the shipper (`agentmail` and `cohere-ai` are not in the
  index at any version; with them, the prints would be ambiguous and exp144's ambiguity condition would refuse the
  rescue).

## Files

- `scripts/rescued.py` (every rescued seed with local features), `feat.py` (per bundle and family: changed bytes,
  kept seeds, module coverage), `kepthash.py` + `ixfiles.py` (index file paths behind each seed), `basefam.py`
  (main's verdict on each family that gained wrong), `layout.py` (module layout dump, prints bundle rows: run
  locally, never commit its output), `simall.py` (the §3 table).
- Inputs: exp144's `newwrong.py` output for each dev corpus (`/work/exp147-data/nw-*.json`). All row-level data in
  `/work/exp147-data`.
