# 062 — collapse duplicate vendor instances

> **STATUS (2026-08-13, Task 0 executed @ exp062-duplicate-instances):
> the COLLAPSE lever is REFUTED — two of the brief's claims did not
> survive measurement, and the census redirects the effort.**
>
> - DID NOT SURVIVE (1): "35 strict forwarding stubs are duplicates."
>   They are stub-SHAPED but forward to DIFFERENT targets: the scope-
>   aware census (census.ts — masks only module-local bindings, keeps
>   free identifiers/properties/require-target basenames) finds exactly
>   **1 strict-stub duplicate family (2 files)** on 2.1.86. The v1
>   count masked every identifier and over-merged (`is-null` ≡
>   `is-undefined`).
> - DID NOT SURVIVE (2): "`lib_eb5345cb` is 6+ instances of one
>   library." The three eb5345cb files forward to THREE DIFFERENT
>   modules (react, another stub, a logger). `eb5345cb` is the hash of
>   the stub SHAPE — the pipeline's vendor content hash is rename-blind,
>   so every one-line forwarder collides — and the `-N`/`_N` ordinals
>   are doing the real identity work, assigned by CENSUS ORDER.
>   **Collapsing same-hash stubs would rewire importers across
>   unrelated modules** (react → logger); boot gates would fail. The
>   collapse is unsafe by construction, not merely low-yield.
> - What IS real: **382 src lines on 85→86 differ ONLY in a lib\_<hash>
>   ordinal** (26 on 215→216) — census-order renumbering churn, ~23% of
>   the noisy pair's hidden name-only mass. Duplicate CODE families
>   exist (47–93 families, 125–218 files, tiny is-\*/lodash/AWS
>   helpers) but vendor body churn is already small (82/330 lines/pair).
>
> **Redirect:** the lever is ORDINAL CARRY, not collapse — disambiguate
> same-shape vendor hashes by forward-target identity, and inherit the
> `-N` assignment from the prior release (mint new ordinals only for
> genuinely new instances), in the vendorNamer/priorVendorNames owner.
> Sized at ≤382 ln on the noisy pair. No pipeline code was changed in
> this experiment; nothing to gate. Task 2 (stateless code-family
> collapse) remains open but is bounded by the small vendor-churn
> numbers above.

> **This is a BRIEF — a hypothesis, including its cautions.** Whoever
> finishes it stamps a STATUS block naming which claims did not survive.
>
> Read `061-hidden-name-churn/README.md` first: ~26% of the hidden
> name-only churn is emit-time import-alias/instance-ordinal churn, and
> the `lib_eb5345cb` case proved the mechanism — a ONE-LINE forwarding
> stub re-exporting the single real react, present as 6+ byte-identical
> instances whose census ordinals (`_2`,`_3`,…) renumber when any
> instance appears or disappears, churning 1,600+ reference lines.

## Hypothesis

The bundle's duplicate module instances are mostly (a) bundler-made
ESM↔CJS forwarding stubs and (b) tiny stateless nested-node_modules
copies. Neither needs per-instance identity in OUR tree: emitting one
file per content hash and rewiring importers is behavior-preserving for
(a) by exports-identity, and for (b) once a mechanical no-module-state
check passes. The instance-ordinal churn class then vanishes for
collapsed families. Outputs are not permanent (2026-08-13): the
one-time rewire diff is NOT a cost — judge steady state only.

## Tasks

0. **Census + attribution (measure first).** Content-hash families in
   vendor/: instances per family, stub-shaped vs function-shaped vs
   stateful, and reference lines riding on each family's ordinals across
   the four pairs. Strict-stub count on 2.1.86 was 35 of 1,592 — verify,
   then size the stateless widening (`is-plain-object` ×37, `tiny-uuid`
   ×33 class).
1. **Collapse strict forwarding stubs.** Guard is the SHAPE (single
   re-export statement), never the hash alone. All importers rewire to
   the one emitted file; the survivor keeps the family name with NO
   ordinal.
2. **Widen to provably-stateless modules.** Mechanical check: no
   module-level mutable bindings, no load-time effects beyond the
   export. Anything failing the check keeps its instances.

## Cautions pinned before measuring

- **Boot gates are the hard safety net** — rewired requires must still
  run; a collapse that breaks `--version` is dead on arrival.
- Real logic duplicates (the AWS command-factory family) are OUT OF
  SCOPE here — they may carry state and belong to exp063's caller-set
  identity.
- The survivor file needs a stable home across releases: if the
  "first" instance disappears next release the name must not renumber
  (that would recreate the churn class one level up).
- vendorReal must not move — a collapse that hides genuine dependency
  change is the exact failure the vendor columns exist to catch.

## Success criterion (fixed now)

Cold scored run: require-alias / instance-ordinal churn (loc-provenance
shape split + `lib_*_N` diff census) drops by the collapsed families'
measured share; boot gates OK ×4; `novel`/`realLn` byte-exact;
`vendorReal` within band. Ledgers on cold trees, twice.
