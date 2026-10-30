# exp164 — the mint premium: why claimed units mint names the index could answer

STATUS: LANDED (2026-10-30). Branch `exp164-mint-premium` off main
`ff9789c41`. The census is complete and the mechanism is proven at every
layer; two run-side fixes shipped (names-mode only, default-neutral).

## The question

The 2.1.215→2.1.216 hop with names mode read **mints 685 vs default's 81**
(+604, band ±33) — restoration's own cost. The minted declarations
concentrate INSIDE claimed packages: @opentelemetry 633, @grpc 277, semver
274, @aws-sdk 203 minted `lib_*` declarations across the walk. The index
holds names data for these very units (100% unit-row coverage). **Why does
a claimed unit mint at all, and can the premium shrink without hurting
anything else?**

## The census

The shipped-tree junk census: every minted binding located by name, span
and split-ledger file on the exp161-names-hop shipped tree
(`/work/exp164-mint-census/shipped-located-exp161-names-hop.json`) —
**684 junk bindings**, classified against the dump run's eraRestoration
block (Site A's per-statement rows, the freeze's bindings, the converted
claimed regions with raw spans) and the real run's claims:

| junk | bucket        | stmts | top packages                                   |
| ---- | ------------- | ----- | ---------------------------------------------- |
| 351  | H2-capture    | 31    | zod 167, jose 131, @azure 24                   |
| 259  | H4-no-claim   | 73    | jose 153, lodash-es 12, @azure 8               |
| 73   | H3-propagated | 20    | @azure 26, zod-to-json-schema 13, lodash-es 12 |
| 1    | H1-frozen     | 1     | @smithy 1                                      |

Reading the buckets (H1 = coverage, H2 = granularity/capture, H3 = claim
boundary, H4 = no claim — the four hypotheses):

- **The buckets are Site A's shadow, not the mechanism.** The dump's Site
  A rows are targeted by a drifted era conversion (see below), so a junk
  binding's "no site" or "no captured print" row mostly says where Site A
  was NOT, not where the answer is not.
- **The mechanism (proven at every layer):** the claimed CONTAINER
  statements — the unspliced lazy-init wrappers and registrations — are
  frozen by the library layout and never asked by the transfer or the
  waves, so their bindings keep the minifier's names. The capture DOES
  hold answers for them (4,279 captured print hashes; e.g. the zod giant's
  captured answer names 15–16 of its slots) — but they apply at neither
  restoration site:
  - **Site B skipped them by contract** (`in_a_module`): a row outside
    every extracted body was left "to the era".
  - **Site A cannot reach them**: the era conversion's ordinal arithmetic
    (`i − removed_before`) accounts the splice but not the format's
    reshape (only 118/7,628 content-correct), and even with correct
    ordinals the hash bridge is broken — the native formatter is a
    REWRITER (declarator splits, `a,b,c;`→`a;b;c;`, `a&&b;`→`if(a)b;`,
    `c?a:b;`→if/else, `void x;` splits, `!0`→true), so the pipeline print
    hash of an era statement ≠ the captured pre-format hash for exactly
    the code-heavy statements that need restoration. Site A: 0
    applications, 84 captures = twin collisions.
- **The depth gap (F1b), the other half:** `structural_scores_relaxed`
  early-returns when the whole print hits, so the nested functions were
  never consulted — a wrapper's nested locals never got captured even
  when the index holds their per-function rows.

## The fixes (run-side only; no index rebuild)

Both names-gated (`restoration: matching == Names`), so every other mode
is byte-inert by construction:

1. **Site B owns the container statements too**
   (`naming::restore::apply_body_restoration`): the `in_a_module` filter
   is gone — every captured application applies on the bundle, from the
   capture's carried spans+slots (drift-immune; the same ingest
   determinism Site B already relied on). The unpack extends the
   restoration edits into the container's edit list: a body-range edit
   sits under the declaration splice and is dropped by the slice's
   overlap rule, the container's own edits apply in the runtime text, and
   the bodies' slices carry their own as before. The exported surface is
   string-anchored (`Object.defineProperty(module.exports,
"<input name>", { get: () => binding })`) — binding renames leave it
   intact (audited on the shipped tree: 91/93 accessors string==target,
   the 2 mismatches legitimate).
2. **The capture's depth**
   (`libindex::lookup::restoration_answers`): the function prints are
   consulted beside a whole-print hit (the route's greedy selection,
   deduplicated by hash) — the index's per-function rows answer the
   nested locals.

F2 (builder-side `/4 --names`), F3 (propagated claims get lookups) and F4
(fallback naming reuses the index's per-unit answer) are NOT built: the
census's answered share sits in the captured container units, and F3's 73
junk bindings (propagated-only claims) are the smaller, separately-gated
remainder.

## Results

(results sections follow the runs)

## Verification

- red/green: the container application test (the inverted
  `an_application_outside_the_extracted_bodies_is_not_applied`) and the
  capture-depth test each watched fail before the fix;
  `a_body_range_edit_under_a_declaration_splice_is_dropped_from_the_container_slice`
  pins the routing invariant the container extension relies on.
- `cargo test --workspace` green; clippy clean.
- warm neutrality vs main (names off, default AND report mode): byte
  identity.
- gate 13/13 (`npm run check`).
- the hop re-run with the before/after mint table.
