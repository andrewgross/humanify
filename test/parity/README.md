# Parity fixtures

Committed TS-side artifact dumps (the 07 §2 catalog as implemented) cut at
the oracle commit from the e2e fixture corpus, one per fixture — the
`rust:parity` check stage's corpus (scripts/rust-parity.ts compares a
`ts/` side against a `rust/` side when one exists; today only `ts/`).

Cut 2026-09-19 at oracle-0294b28 from the fixture pairs' to-version runs
(prior = the from-version's humanified output). The five fixtures whose
full-pipeline run exited 0: mitt, nanoid, preact, r1b-synthetic,
disambiguation. zustand's dump is from a run whose rename-invariant
FAILED (a pre-existing export-const bug class, recorded in the WP0.4
hand-back) — its dump is still a valid decision record for the hash and
matching sections, which is what phases 1-2 compare.

The cache-key vectors (R4) live beside this: cache-key-vectors.jsonl +
its generator, and babel-counts.mjs (the WP1.2 counts table's Babel side).
