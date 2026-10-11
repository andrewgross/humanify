# exp095 lane "library discovery": graph-closure candidates + leaf-inward fixpoint (lab)

## STATUS (2026-10-11)

Design in `docs/design/library-discovery.md` (§6 has the numbers). This is a lab prototype on Claude Code 2.1.216 only;
nothing in the pipeline reads it. Scored against the negatives lane's `labels.jsonl` (620 random `src/` statements)
and `traps.jsonl`.

## Files

- `proto/`: a throwaway Rust crate, detached from the repo workspace (its own `[workspace]`; not built by
  `npm run check`).
  - `exp095-refgraph` writes the statement-level reference graph: oxc semantic, resolved references between units,
    writes linked both ways, and definitional member mutations, including the `rt`/`Oi` namespace fill. It takes
    1.5 s on the 20 MB bundle and is byte-deterministic.
  - `exp095-declared` runs the pipeline's own `identity::scan_app_declarations` over every unit.
- `graphlib.py`: loads the units, the graph, the labels and the Step B claims; iterative Tarjan; the stratified
  estimator.
- `discover.py`: closure candidates, seeds, the fixpoint (GLUE / DOWN / MODULE), the isolation veto, DOWN-CLASS, the
  UP queue. Writes `results.json`.
- `structure.py`: the SCC structure (the giant app core is 1.2 MB; most app code lies below it).

## Re-run

```
export PATH="$HOME/.cargo/bin:$PATH"; T=<scratch>
B=/Users/andrewgross/Development/claude-code-versions/inputs/claude-code-2.1.216/binary-decompiled/src/entrypoints/index.js
RAW=/work/exp095-library-fingerprints/bundler-variation-and-boundaries/data/units/cc/min/cc-2.1.216/raw.jsonl
(cd proto && CARGO_TARGET_DIR=$T/target cargo build --release --offline)
$T/target/release/exp095-refgraph $B $RAW $T/edges.tsv rt,Oi        # rt = __export, Oi = __reExport in 2.1.216
$T/target/release/exp095-declared $B $RAW > $T/declared.jsonl
python3 discover.py $T/edges.tsv $T/declared.jsonl results.json     # ~35 s
python3 structure.py $T/edges.tsv
```

The helper names are passed by hand here. In the pipeline they come from the shape recogniser
(`emit::load_order::identify_export_registrar`, P12). Seeds "stepB-A" and "stepB-C" are the negatives lane's
`data/stepb-pred-{A,C}.json`.
