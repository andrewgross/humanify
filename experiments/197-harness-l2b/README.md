# exp197 — harness migration lane L2b: the library scorer in Rust

STATUS: DONE for the scorer and its instruments; PARTIAL for the layer
builders (2026-11-07), branch `harness-l2b-library-scorer`. Scope is
`docs/dev-harness.md` §5 lane L2b. The old Python tools stay in place (they
are the oracles); lane L5 deletes the ones marked "ported" below.

## Totals

- **Scorer 14, byte for byte.** `cargo dev library rescore` reproduces
  score.py's cards and summary on **every scorer-14 label whose reports and
  index are still on this host: 24 labels, 24 identical** (the 9 exp184
  labels and 15 committed labels; table in §3). Python's float repr,
  `sort_keys`, `ensure_ascii`, banker's rounding, summation order,
  `Counter` semantics (insertion order, `+` dropping zeros) and Python's
  `re` character classes are reproduced, not approximated. No new scorer
  version.
- **Layer builders: 7 of 11 kinds ported, all reproduce.** The origin
  layers (scorers 4, 7, 10, 12) and the chained layers (scorers 5, 9, 11)
  re-freeze into a scratch dir with **every committed per-entry sha and
  every manifest identical on all six corpora whose data is on this host**
  (bun-dev, bun-held, kind-esbuild, kind-webpack, kind-rollup, cc: 42 of 42
  layer manifests, gzip bytes included). copies2, misattr13 and
  mapdefect14 (with its acorn tokenizer) are NOT ported (§6).
- **The small instruments, byte for byte.** vendor-precision `check`: 400 /
  400 recorded cards identical (5 labels x 80 bundles) and the aggregate
  identical; cjs-placement: 5 / 5 run labels, text and JSON identical;
  the library leaderboard's text identical to leaderboard.py's (two
  committed pairs, kept as goldens).
- **Speed:** a bun-dev label scores in 44-56 s (score.py: about 6 min).
- Two new external dependencies, both for the harness only: `flate2`
  (with the SYSTEM zlib 1.2.13, the one Python links, so a re-frozen
  `.json.gz` is the same bytes) and `memchr` (already locked).

## 1. What was ported (old path -> verb)

| Old (deleted by L5 once this lands)                                                                                                              | New                                                                                                  |
| ------------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------- |
| `experiments/096-library-matching-score/score.py`, `lib096.py`, `truth_audit.py`, `decoder.py`, `origins.py`, `families_lib.py`                  | `cargo dev library rescore <label>` (`src/library/{score,truth,corpus,decoder,origins,families}.rs`) |
| exp095 `misses/scripts/{mmlib,decompose,truth}.py`, `selection/scripts/libix.py` (`family` only: the numpy/scipy part is not read by the scorer) | `src/library/{index,families}.rs`                                                                    |
| `096/leaderboard.py`                                                                                                                             | `cargo dev library leaderboard <label>...`                                                           |
| `096/determinism.py`                                                                                                                             | `cargo dev library determinism <a> <b>`                                                              |
| `096/corpus.py verify`, `corpus.py origins`                                                                                                      | `cargo dev library corpus verify <corpus>`, `library layers` (origins)                               |
| `112/scorer7/origins7.py build`, `125/scorer10/origins10.py build`, `129/scorer12/origins12.py build`                                            | `cargo dev library layers <corpus> --out DIR`                                                        |
| `106/scorer5/chained.py`, `120/scorer9/chained9.py`, `128/scorer11/chained11.py`                                                                 | `cargo dev library layers <corpus> --out DIR`                                                        |
| `166-restore-or-ask/vendor_precision.py`, `aggregate_precision.py`                                                                               | `cargo dev eval vendor-precision check <diag> --truth ...`, `... aggregate <dir>`                    |
| `172-cjs-app-placement/score.py`                                                                                                                 | `cargo dev eval cjs-placement <census.jsonl> <runs-dir> [<out.json>]`                                |
| `test/library-truth-decoder.test.ts`                                                                                                             | `src/library/decoder_test.rs` (+ `carried_verdict` in truth.rs)                                      |
| (new) proof (b) as a self-test                                                                                                                   | `cargo dev library proof <label>... --out SCRATCH`                                                   |

NOT ported (L5 must keep them, or a later lane ports them):
`129/copies2.py`, `132/scorer13/misattr13.py`, `137/scorer14/mapdefect14.py`

- `tokens14.mjs` (and `test/library-truth-mapdefect14.test.ts`,
  `test/library-truth-origins7.test.ts`), `096/corpus.py freeze`,
  `181/scripts/kind_corpus.py` + `prep.sh`, `100/scripts/build.py`,
  `095/selection/scripts/closure.py`, the two cache fillers
  `104/fetch_metadata.py` (npm registry) and `112/scorer7/bundled.py`
  (`npm pack`), `112/scorer7/origins7.py needs|owners|verify|stats`, and the
  vendor-precision RUNNER `166/bundev-precision.sh` + `stub-run.mts` and
  `172/bundev-runs.sh` (they launch the binary against the stub: L3's
  `run stub`). The `library score` runner (096/run.sh, run.py) is L3's; its
  verb entry now says lane L3 and it calls `library::score::rescore`.

## 2. How byte identity was reached

- **Floats.** `round(x, 6)` is `format!("{x:.6}")` read back (correctly
  rounded, ties to even, exactly Python's); `round(x)` is
  `round_ties_even`; `repr` is L1's `json::py::float_repr`. Shares divide
  exact integers (asserted under 2^53).
- **Order.** `json.dumps(sort_keys=True)` hides most order, not all:
  `Counter.most_common` breaks ties by insertion order (the majority label
  of a unit), `Counter + Counter` drops zero counts (a card's
  `statementBytes` omits a zero class that its scope block keeps), and a
  dict built from claims keeps the FIRST position of a span and the LAST
  claim. `library::counter::Counter` reproduces all three.
- **Python's `re`.** `\w`, `\d`, `\s` are Python's `isalnum`/`isdecimal`/
  `isspace` at Unicode 14.0 (a generated range table, `pyclass_table.rs`):
  Rust's `char::is_alphanumeric` counts combining vowel signs Python does
  not (a test pins U+093F). Every pattern the scorer and the ported layer
  builders match is written out by hand with its backtracking semantics
  (`pyre.rs`), each with vectors recorded from Python.
- **JSON input.** serde_json refuses a lone surrogate escape that Python
  keeps; a source map's lone surrogates read as U+FFFD (both are non-word
  characters to every reader). A float passed through to a card is refused
  rather than risk serde's not-always-correctly-rounded parse.
- **gzip.** Python's header (FNAME = basename minus `.gz`, mtime 0, XFL 2,
  OS 255) plus the system zlib at level 9 / memLevel 8 / window 15.
- **os.path / os.walk.** `normpath`, `join`, `dirname`, `relpath` and
  os.walk's sorted, symlink-not-followed order (`pypath.rs`).

## 3. Proof (b): the scorer

Oracle precondition first: score.py run TODAY reproduces the on-disk exp184
cards and summaries byte for byte (9 / 9; it did, 2026-11-07 16:43-17:08).

| Label                | score.py today = recorded | Rust = recorded (cards, summary) |
| -------------------- | ------------------------- | -------------------------------- |
| exp184-cb64-bun-dev  | yes                       | identical, identical (44 s)      |
| exp184-cb64-bun-held | yes                       | identical, identical (25 s)      |
| exp184-cb64-cc       | yes                       | identical, identical (10 s)      |
| exp184-iv-bun-dev    | yes                       | identical, identical (47 s)      |
| exp184-iv-bun-held   | yes                       | identical, identical (27 s)      |
| exp184-iv-cc         | yes                       | identical, identical (12 s)      |
| exp184-spec-bun-dev  | yes                       | identical, identical (56 s)      |
| exp184-spec-bun-held | yes                       | identical, identical (34 s)      |
| exp184-spec-cc       | yes                       | identical, identical (15 s)      |

| Committed label             | score.py today = committed | Rust = committed (cards, summary) |
| --------------------------- | -------------------------- | --------------------------------- |
| bun-dev-exp150-main         | yes                        | identical, identical              |
| bun-dev-exp153-ix4-base     | yes                        | identical, identical              |
| bun-dev-exp153-ix4-ctx-full | yes                        | identical, identical              |
| bun-dev-exp153-ix4-ctx-rf   | yes                        | identical, identical              |
| bun-dev-exp153-ix4-ctx-tb   | yes                        | identical, identical              |
| bun-dev-exp153-ix4-ctx-tbr  | yes                        | identical, identical              |
| bun-held-exp150-main        | yes                        | identical, identical              |
| bun-held-exp153-ix4-base    | yes                        | identical, identical              |
| bun-held-exp153-ix4-ctx-tb  | yes                        | identical, identical              |
| cc-exp150-main              | yes                        | identical, identical              |
| cc-exp153-ix4-base          | yes                        | identical, identical              |
| cc-exp153-ix4-ctx-full      | yes                        | identical, identical              |
| cc-exp153-ix4-ctx-rf        | yes                        | identical, identical              |
| cc-exp153-ix4-ctx-tb        | yes                        | identical, identical              |
| cc-exp153-ix4-ctx-tbr       | yes                        | identical, identical              |

All 24 Rust rows were re-run on the final binary (sha256 `b13bbf09…`, built from `9e96c88d2`); the bun-held cards compared are the on-disk ones beside the reports (a private corpus commits its summary only).

Committed scorer-14 labels: 516. Their reports are on disk for 21; the
other 495 (dev, test, test2, bundlers, apps, pop, remin\*) lost their raw
reports AND their frozen corpus data (`library corpus verify` reports every
file of those ten corpora missing), so no scorer can re-score them. Of the
21, the six exp155 labels ran on `/work/exp155-data/ix/...`, which is gone:
score.py still scores them from its pickle cache, the Rust scorer reads the
index's own JSONL and refuses. The Rust scorer also checks the index dir's
identity (sha of `manifest.json`) against the label's `indexId`, which
score.py never did.

## 4. Proof (b'): the layer builders

`cargo dev library layers <corpus> --out /work/harness-l2b-proof/layers-final/<corpus>`
on the final binary:

| Corpus       | origins | origins7 | origins10 | origins12 | chained | chained9 | chained11 |
| ------------ | ------- | -------- | --------- | --------- | ------- | -------- | --------- |
| bun-dev      | 90/90   | 90/90    | 90/90     | 90/90     | 90/90   | 90/90    | 90/90     |
| bun-held     | 30/30   | 30/30    | 30/30     | 30/30     | 30/30   | 30/30    | 30/30     |
| kind-esbuild | 37/37   | 37/37    | 37/37     | 37/37     | 37/37   | 37/37    | 37/37     |
| kind-webpack | 34/34   | 34/34    | 34/34     | 34/34     | 34/34   | 34/34    | 34/34     |
| kind-rollup  | 34/34   | 34/34    | 34/34     | 34/34     | 34/34   | 34/34    | 34/34     |
| cc           | 0/0     | 0/0      | 0/0       | 0/0       | 0/0     | 0/0      | 0/0       |

Every cell: entries whose re-frozen `.json.gz` sha equals the committed
manifest's, and the re-frozen manifest's canonical identity equals the
committed one (42 / 42 manifests). The chained layers read the apps'
installs on this host and the npm caches (`/work/exp104-data/registry`,
`/work/exp112-data/{registry,bundled}`); a name with no cached npm answer
is refused (the Python fetched it).

## 5. The small instruments

| Instrument                 | Oracle                                                                                                                                 | Result                                                                            |
| -------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| vendor-precision check     | the recorded `.precision.json` of exp176-vp-{cand,main}, exp192-vp-{cand,main}, v2 (80 each); vendor_precision.py today on two of them | 400 / 400 identical                                                               |
| vendor-precision aggregate | exp192-vp-cand `aggregate.json`                                                                                                        | identical                                                                         |
| cjs-placement              | 172 score.py today on cand-names, cand-default, main-names, exp183-main, exp183-new; the recorded score-cand-{names,default}.json      | text 5/5, JSON 5/5 identical                                                      |
| library leaderboard        | leaderboard.py on two committed pairs (goldens)                                                                                        | identical text                                                                    |
| library corpus verify      | corpus.py verify                                                                                                                       | same verdicts (Python crashes on a deleted corpus; Rust lists every missing file) |

## 6. What changed from the design, and why

1. **Data roots live in `data-roots.json`, not in the corpus manifests.**
   A manifest's canonical sha IS the corpus identity every label records;
   adding a key would have refused every label. `--data-root`
   (`libscore.data_root`) overrides it.
2. **The library leaderboard uses THE guard** (`identity::guard`,
   `LibraryLeaderboard`): corpus, manifest, every layer sha (families
   included) and scorer must be equal. leaderboard.py checked the manifest,
   the origin layer and the scorer only. Its refusal text is the guard's.
3. **Scorer 14 only.** The Python kept switches to re-derive scorers 4-13
   (`RULES5/6/9/11/13/14`, `COPIES2`); they are not ported.
4. **The scorer's family judgement stays separate from the matcher's** —
   and they DIFFER, which nothing declared before: libix prefixes families
   (`fam:uri`, `base:uri`), so the npm package `uri` is not in the listed
   family `uri` (fast-uri, uri-js) for the scorer, while the matcher's
   unprefixed spelling (`libindex::families`) joins them (and `aws-sdk`
   with `@aws-sdk/*`). A test pins the difference; unifying them would be a
   new scorer version (Andrew's call).
5. **`library proof`** is new: proof (b) as a committed self-test verb.
6. **`library score` is L3's** (the runner); `library rescore` is the
   scorer it calls.
7. **`eval cjs-placement` takes the census and the runs dir** (as score.py
   did), not a label.

## 7. What I could not prove

- copies2, misattr13 and mapdefect14 layers: not ported, so not re-frozen.
  mapdefect14 needs an acorn-equivalent tokenizer (`tokens14.mjs`, with its
  regex fallback); copies2 and misattr13 read a label's units and the
  index's row tables. Their frozen files are read by the scorer as they are.
- The 495 committed scorer-14 labels whose reports and corpus data are gone,
  and the six exp155 labels whose index is gone.
- The cc216 truth path of vendor-precision (no names-mode cc diagnostics
  file is kept on this host).
- The cache fillers (npm registry fetch, `npm pack`): not ported; the
  caches on this host are complete for every corpus here.

## 8. Gate

GATE

## Proposed CLAUDE.md text (for L5; not applied)

> The library scorer is `cargo dev library rescore <label>` (scorer 14,
> byte-identical to score.py on every label on disk); `cargo dev library
proof <label>... --out DIR` re-checks that. `library layers <corpus> --out
DIR` re-freezes the origin and chained layers and compares every sha.
