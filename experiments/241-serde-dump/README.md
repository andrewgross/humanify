# exp241 — serde everywhere in the product; a cheap debug dump

STATUS: BUILT (red/green for the dump sink, the dump coverage test and
every byte pin), GATED (`cargo dev check`, all 12 stages pass on the branch
rebased onto `59c371aea`), NEUTRAL on two pairs (warm, current-binary
priors, 0 cache writes on BOTH legs). Branch `exp241-serde-dump`, not
merged. Plan items 1d and 1e of the simplification campaign.

## Totals

Raw diff lines against origin/main (comments and blank lines included):

| | removed | added | net |
| --- | ---: | ---: | ---: |
| product code | 5,500 | 2,631 | **-2,869** |
| product tests (without the parity file) | 837 | 944 | +107 |
| `test/parity/wpb4-stats-schema.json` (only the deleted shape test read it) | 2,574 | 0 | -2,574 |
| harness code (the eval's JS spelling moved in, see below) | 39 | 577 | +538 |
| harness tests | 23 | 48 | +25 |

Per item (product + harness, code / tests):

| item | code − / + | tests − / + |
| --- | ---: | ---: |
| 1e. debug dump: one sink, stage records serialized | -2,533 / +554 | -297 / +239 |
| 1d. LLM boundary, cache entries, split ledger, package.json | -614 / +592 | -141 / +489 |
| 1d. stats, diagnostics, call ledger, profiling, in-tree records (`js_record!`, `jsshape` gone) | -1,684 / +1,429 | -2,911 / +209 (2,574 of it the parity file) |
| 1d. `JsValue`/`JsObject`/parser/`stringify`/`canonical` deleted from `js.rs`; `OrderedMap` + `JsRecord` in one module | -771 / +158 | -67 / +12 |
| harness: the eval's JSON.stringify spelling moves from the product into `humanify-ag-dev/src/json/js.rs` | -39 / +577 | -23 / +48 |

The stats item added back most of what it removed: `js_record!` blocks
became derive structs field for field. The real gain there is that the
records are plain Rust types (counts are `u64`, not JS `f64`) and the
macro, the strict reader, the schema-as-data and its 2,574-line parity file
are gone.

## What changed

**No JS JSON in the product.** `humanify_ag_model::js` keeps only the
JavaScript runtime rules that decide or print (`String(x)`, `Math.log`,
`Math.round`, `toFixed`, `localeCompare`, UTF-16 order and offsets, the JS
whitespace set, numeric-literal values, `path.resolve`, the array-index key
rule). `JsValue`, `JsObject`, its JSON parser, `stringify`,
`stringify_pretty`, `canonical`, `js_record!` and `jsshape` are gone. Every
writer is `serde` / `serde_json`. Two small helpers keep key order where it
is read (`humanify_ag_model::ordered`): `OrderedMap` (insertion order) and
`JsRecord` (JavaScript record order: array-index keys first, a repeated key
keeps its first slot and its last value) — used for the model's answers,
cache entries and the ledger's `nameToFiles`.

**The debug dump** (`--dump-artifacts`) goes through one sink,
`artifact_dump::DumpSink::section(name, &records)`, which writes
`<name>.json` from any `Serialize` value. `write_artifact_dump` is about 60
lines, one `section` call per section; the stage records derive `Serialize`.
Kept: functions, matches, matches-close, transfers (+ transfers-mechanical),
names, placement, prompts.jsonl (still streamed by `DispatchLog` as
dispatches commit), modules, votes, emit, partitions, plus twin-gates,
private-renames and regions, which came for free. Dropped: meta.json, the
`text/` copies (incl. `text/post-split`), tree-manifest.json,
cache-keys.jsonl, twins.json (it re-derived the unique-twin proposals), the
graph-time classification site and the vendor-signature partition family.
Deleted modules: `matches_dump.rs`, `naming/driver/dump.rs`,
`emit/emit_dump.rs`, `graph.rs functions_dump`, `modules.rs modules_dump`,
`PlacementTrail::placement_json`, most of `model/dump.rs`. The close tier now
returns its candidates (`CloseMatchResult::candidates`, the sorted vector
`assign_greedy` already built) and the dump writes those, so
`close_dump.rs`'s `rebuild_vectors` and second `score_pairs` are gone.
`modules` is the one section still re-derived (the minified text classified
again by the unpack's own classifier).

**Spelling outside the tree** changed where it was free: `--stats-json`,
`--diagnostics`, the profile and trace, the call ledger and the dump are
plain serde JSON. Every field the harness reads keeps its name and value
(checked by grepping the paths `eval summarize`/`leaderboard`/`llm_diag`/
`matching`/`report`/`vendor_precision` read, and by the harness tests).

**The harness** writes its cards, run manifests and launch goldens in
`JSON.stringify` spelling. That code (value type, parser, writer) moved
unchanged from the product into `crates/humanify-ag-dev/src/json/js.rs`, so
the committed launch goldens did not move (they hold launch argv; their
bytes come from the same writer as before). Removing that spelling from the
harness is a harness lane's job.

## In-tree files: proof of identical bytes

| file | writer now | proof |
| --- | --- | --- |
| `.humanify/split-ledger.json` (compact, incl. after the post-split reconcile patch) | typed `place::ledger::SplitLedger`, fields in the written key order, `nameToFiles` a `JsRecord` | byte pins in `stable_split_test` (declined emit, vendor-moved module, glued v1/v2 with every key) and `reconcile_test` (patched ledger), run on the old writer first |
| `.humanify/placement-stats.json`, `.humanify/stage-hashes.json` | derive + `to_string_pretty`; `byTier` an `OrderedMap` in registry order | `writers_test::the_in_tree_records_keep_their_bytes`, run on the old writer first |
| `package.json`, `run.cjs` (runnable tree) | serde | `scaffold_test::package_json_and_runner_bytes_are_pinned`, run on the old writer first |
| `vendor/_bun-modules.json` | already serde | unchanged |
| `.humanify/prior-match-map.json` (debug log level only) | `OrderedMap`, compact | same order as before; not in a default run |

No in-tree file needed a dedicated writer beyond serde: each one's bytes are
what `serde_json` writes for a struct with fields in the right order (or an
ordered map). The only number rule kept is `serialize_js_number`
(integral float printed without a fraction, as JS does), used by the cache
key and the request body for `temperature`.

Both neutrality runs also compared the five files directly: identical.

## LLM cache key

The key hashes the canonical JSON of what the endpoint receives. It is now
built with serde (keys are fixed ASCII, so the sorted order is unchanged;
`temperature` through `serialize_js_number`). Evidence:

- `llm_test::cache_key_material_is_pinned_across_parameters_and_escapes`
  pins 48 parameter/prompt combinations (temperature none/0/-0/0.2/1/0.7/
  1.5/2, max_tokens, reasoning_effort, control characters, U+2028,
  non-ASCII, NUL), written and passing on the old code first; the request
  body is pinned the same way (`client_test::request_body_bytes_are_pinned`).
- Both warm neutrality runs wrote **0** cache entries on the CANDIDATE leg
  too, against a cache filled by earlier binaries: every key the new binary
  computed already existed.

## Neutrality

`cargo dev eval neutrality 59c371aea <pair> --cache /work/neutrality-cache`
(default priors `/work/neutrality-priors`, written by the current binary).
Candidate `f8ce970e3`.

| pair | cache writes candidate / baseline | exits | differing files / lines | verdict |
| --- | ---: | --- | ---: | --- |
| 2.1.118 -> 2.1.119 | **0 / 0** | 0 / 0 | **0 / 0** | **NEUTRAL** |
| 2.1.197 -> 2.1.198 | **0 / 0** | 0 / 0 | **0 / 0** | **NEUTRAL** |

`diff -rq` of the two output trees (diagnostics-only artifacts included)
was empty on both pairs.

## Tests added (red first)

- `artifact_dump_test::a_section_is_written_as_its_records` (the sink).
- `crates/humanify-ag-cli/tests/dump_coverage.rs`: runs `bun-cjs-app`
  v1.0.0, then v1.1.0 with `--prior-version`, split on, stub LLM, once with
  `--dump-artifacts` and once without. Every kept section must hold records,
  the dropped files must be absent, and the two output trees must be
  byte-identical (the dump never changes the output). It failed on the old
  code (meta.json was written). `private-renames` and `regions` are empty on
  this fixture and are not asserted.
- The byte pins listed above; `ordered` tests; `JsRecord` order tests
  (`cache_test::entries_read_in_js_record_order`,
  `client_test::json_answers_keep_js_record_order`,
  `llm_test::renames_keep_js_record_order`).

## Behaviour changes (none reach the tree)

- An LLM answer holding a lone-surrogate escape (`\ud800`) now fails the
  JSON parse and goes to the regex fallback parser (before: U+FFFD).
- A non-2xx error body that is a JSON object prints with sorted keys (log
  text only).
- A ledger `emitIndexes` entry that is not a number fails the ledger read
  (this binary always writes numbers).

## Open items

- The harness still owns a JSON.stringify emulation (`json/js.rs`, ~540
  lines) for its eval files. Moving cards and manifests to serde changes
  committed goldens and readers; a harness lane.
- `test/parity/wpb4-vectors.json` `writers` and `wp41-js-vectors.json`
  `canonical` sections are no longer read (the files are, for other
  sections).
- Nothing stops a new hand-written JSON emulation in the product; a
  `disallowed_types`/grep guard was not added because the deleted type has
  no path left to ban.
