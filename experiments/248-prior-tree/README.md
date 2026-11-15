# exp248 — the prior release read once, into one `PriorTree` value

STATUS: BUILT, GATED (`cargo dev check`, all 13 stages), NEUTRAL on two
pairs (warm, standing priors, 0 cache writes on both legs). Branch `exp248-prior-tree`, not merged. Step D of the
functional-core migration (experiments/243-effects-ratchet/README.md,
"Refined step order").

## Totals

- **The core opens no prior file.** Before, the prior release's folder
  was found again from the `--prior-version` path in five places
  (`find_prior_tree_root` in the unpack, the finish and the vendor-body
  naming; `split_tree_root_of` in the split and the post-split reconcile).
  Its files were read lazily, at different moments of the run, and a
  failed read silently counted as "absent". Now the cli reads the prior
  once, at the start, in one function, `humanify_ag_cli::prior_tree::load_prior`:
  the prior file, every file of its tree (`humanify_ag_io::tree::read_tree`,
  skipping `node_modules` and hidden folders other than `.humanify`), and
  the split ledger in force (`--split-ledger` wins over the one beside the
  prior). The core gets values: `core::prior_tree::PriorTree` (the prior
  text, the tree's files as `PriorFiles`, the parsed vendor record, the
  named vendor bodies' text) and the parsed ledger.
- **Fail loud.** A prior file, tree folder, vendor manifest or ledger that
  is there but cannot be read or parsed stops the run at load, with its
  path. The only "no prior" is a run without `--prior-version`. A file the
  tree does not hold is still absent (data: the prior run did not write
  it), as before.
- **The self-hop question is settled.** No harness run uses the same
  folder for `--prior-version` and `-o` (the eval's self-hop writes
  `<v>-selfhop`, the walk writes each version to its own folder). But a
  user can, and before this change two readers then read THIS run's own
  output as the "prior": the vendor-body inherit (it read prior vendor
  files after the unpack had overwritten them) and the post-split
  reconcile (it read prior app files after the split wrote the tree). Now
  every prior file is in memory before the first write, so that case reads
  the real prior.
- **census:effects: 413 entries before, 399 after** (origin/main
  `a78ab2ef3`). 16 entries deleted: 14 core `legacy` effect sites (fs ×13:
  `find_prior_tree_root`, `load_prior_vendor`, `carry_by_content`,
  `stable_split`, `VendorBodyInheritor::bytes_for`, `prior_bodies_named`,
  `prior_vendor_naming`, `find_split_ledger_in`, `read_ledger`, and
  placement_dump's `assign_regime`, `read_carry`, `read_json`; plus the
  `PRIOR_PARSES` static), 2 core `legacy` `Result<_, String>` signatures
  (`load_prior_vendor`, `read_ledger`), and 1 cli `shell` site
  (`load_prior_split_ledger`). 2 `shell` entries added: cli
  `prior_tree.rs` `load_prior` and io `tree.rs` `read_into`. No `legacy`
  entry added. Core `legacy` effect sites outside profiling: 47 → 33.
- **census:concurrency: 124 entries before, 120 after** (the four
  `PRIOR_PARSES` entries).
- **No new `Result<_, String>`.** New errors are typed: `PriorTreeError`
  and `LedgerParseError` (core), `TreeError` (io), `PriorLoadError` (cli).
  The cli's `Display` keeps the old texts: `ENOENT: … open '<prior>'`,
  `prior vendor manifest <path> is not one this binary reads (…); rebuild
  the prior release with this binary`, `<ledger>: <why>`,
  `Unsupported split ledger version in <ledger>`.

Raw diff lines against origin/main `a78ab2ef3` (comments included):

| | added | removed |
| --- | ---: | ---: |
| product code (core, cli, io) | 710 | 395 |
| tests | 479 | 134 |
| census allowlists | 12 | 124 |
| docs (responsibility, plugin-spec, concurrency) | 8 | 6 |

The product code grew: the loader (cli, 190 lines with its error type),
`PriorTree`/`PriorFiles` (core, 160) and the folder reader (io, 75) are
new, and most of what they replace was short path lookups. What went:
the five root lookups, the two ledger reads, the split stage's second
read of the prior text, the dead placement-dump file inputs (three
`PlacementGate` fields that every caller set to None, `read_carry`,
`read_json` and an `out_dir` write nobody asked for) and the
`PRIOR_PARSES` counter.

## Gate

`cargo dev check` on the final tree:

| stage | result |
| --- | --- |
| shims | PASS |
| no-private-addresses | PASS |
| rust:fmt | PASS |
| rust:clippy | PASS |
| census:clones | REVIEW (advisory; nothing from this lane) |
| census:concurrency | PASS (120 entries) |
| census:effects | PASS (399 entries) |
| census:slop | PASS |
| census:comments | PASS (no new findings) |
| rust:unit | PASS |
| rust:build | PASS |
| format-golden | PASS |
| e2e | PASS (22 pairs, fresh and `--prior-version`, 88 launches equal to the golden) |

Red seen before green: the io folder reader's tests failed to compile
before `read_tree` existed; the core's "named-bodies source needs a vendor
manifest" test failed before the gate was added; the cli's
"broken ledger beside the prior fails" test fails on a mutant that turns
the ledger error back into "absent"; the match verb's parse-count pin
fails (2 parses, expected 1) on a mutant that parses the prior twice.

## What changed, reader by reader

| reader (core) | before | after |
| --- | --- | --- |
| unpack vendor record | `load_prior_vendor(path)`: finds the root, reads the manifest | `PriorVendor::parse(bytes, files, bodies_named)` inside `PriorTree::parse` |
| content carry, stale-era re-key | read each prior module file from the root, `.ok()` | the module texts read at parse (`PriorVendorFiles::texts`) |
| "prior bodies were named" | `root/.humanify/vendor-humanified.js` `is_file()` | `PriorVendorFiles::bodies_named` |
| vendor-body naming's prior | `prior_vendor_naming(path)`, `.ok()` | `PriorTree::vendor_naming` |
| split: prior entry order, import aliases | `read_to_string(root/f).ok()` | `PriorFiles::text(f)` (a non-UTF-8 file still reads as absent, tagged `// default:`) |
| finish: vendor-body inherit | `fs::read(root/f)`, lossy | `PriorFiles::text_lossy(f)` |
| finish: post-split reconcile | `read_opt(root, f)`, lossy | `PriorFiles::text_lossy(f)` |
| split ledger | read twice: the module carry (`let Ok(..) else` → nothing) and the split stage (`?`) | parsed once at load; the split clones it before settling the hashes |
| split stage's prior text | read again, `.ok()` | `PriorTree::code` |

Two small behaviour notes, neither reachable on a prior this pipeline
wrote: the vendor root and the split root were found by two different
rules and are now one (`split_tree_root_of`); they differed only when the
prior file sat in a sub-folder of its tree other than `.humanify`. And a
ledger beside the prior that is broken now fails the run even when the
split is off (before, that run ignored it).

The parse-count pin of the `match` verb (one prior parse per run) used a
global atomic in production code. It now counts `prior:parse` phase
spans from an installed profiler, so the pin stays and the static is gone.

## Neutrality

`cargo dev eval neutrality a78ab2ef3 <pair> --priors /work/neutrality-priors
--cache /work/neutrality-cache` on the committed candidate `53f973309`,
one at a time under `/work/neutrality.lock`:

| pair | differing files | differing lines | exit codes | cache writes (candidate / baseline) | verdict |
| --- | ---: | ---: | --- | --- | --- |
| 2.1.118 → 2.1.119 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |
| 2.1.197 → 2.1.198 | 0 | 0 | 0 / 0 | +0 / +0 | NEUTRAL |

Both pairs run with `--prior-version` on a standing prior tree, so every
reader above ran on the in-memory tree, and the fail-loud paths did not
fire. Logs: `/work/exp248-neutrality-118.log`, `/work/exp248-neutrality-197.log`.

## Open items

- `bodies_of_bridge` and `write_vendor_naming` still read and write the
  OUTPUT tree; that is step F (in-memory `OutputTree`).
- `PriorVendorFiles::texts` holds the prior vendor module texts a second
  time beside `PriorFiles` (about 18 MB on a Claude Code prior). A borrow
  would avoid it, but `PriorVendor` is taken by value and mutated by the
  re-key; left as is.
- The `PriorTree` loads `src/` and `vendor/` of the prior (about 62 MB plus
  the metadata files it does not hold twice on the eval priors) for the
  whole run.
