# Family-rotation / head-flip repair ceiling — NO-GO (2026-07-22, post-pin)

Measured on the pin-rebased trees (fresh vs regenerated prior), all 4 pairs,
by `ceiling-family-rotation.ts`: for every noise statement, can substituting
the statement's OWN declared names (positionally paired with its unique
hash-twin, or a reciprocal-unique token-overlap partner inside a family
bucket) reproduce the prior text byte-for-byte — and is that substitution
collision-safe (prior name dead on the fresh side, fresh name novel)?

| pair    | noise ln | unique flip SAFE | family pairable flip | unique OTHER | family other |
| ------- | -------- | ---------------- | -------------------- | ------------ | ------------ |
| 85→86   | 37,143   | **0**            | 146 ln               | 29,788 ln    | 7,203 ln     |
| 118→119 | 6,763    | **0**            | 65 ln                | 5,455 ln     | 1,241 ln     |
| 197→198 | 13,013   | **0**            | 87 ln                | 10,134 ln    | 2,787 ln     |
| 215→216 | 7,605    | **0**            | 19 ln                | 6,070 ln     | 1,484 ln     |

Verdict:

- The isolated head-flip class is EXTINCT post fn-head pin + statement-twin
  (risky remainder: 2-3 tiny statements/pair). The levers shipped 2026-07-22
  harvested it fully.
- Family-bucket pairable mass is ~300 ln total — below build threshold.
- The residual (~51k ln) is "unique-twin other": COUPLED RENAME WEBS.
  A noise statement's diff tokens mix its own declared names with
  references to OTHER flipped roots (changed-leaf chains, lazy-init
  rotation) — no single-statement substitution explains it, so any fix
  must solve the web jointly (fixpoint inheritance across statements).
  The reconcile pass does fixpoint rounds but is corpus-gated off on
  shuffle pairs (85→86: 24.7% aligned); an in-pipeline web solver is a
  major design, not a lever.
- Family "other" (~12.7k ln) is same-hash tiny statements without enough
  descriptive tokens to pair — LLM-floor tail.

Next candidate framing (needs its own ceiling): iterated echo-web
inheritance — pin provable roots, re-diff, repeat until fixpoint,
order-independent. Do not build without measuring convergence on 86.

## Addendum: echo-web fixpoint NO-GO → residual shape = twin-local transfer GO

`ceiling-echo-web.ts` (iterated cross-statement inheritance): 1 rename
total across 4 pairs — the residual has no cross-statement evidence
structure. `diagnose-residual-shape.ts` explains why: EVERY unique-twin
noise statement is pure rename shape (misaligned: 0 st on all pairs),
differing only at non-property identifier slots INSIDE the statement:

| pair    | aligned-locals     | aligned-props | family buckets |
| ------- | ------------------ | ------------- | -------------- |
| 85→86   | 490 st / 29,375 ln | 2 / 419       | 1,228 / 7,349  |
| 118→119 | 44 st / 5,454 ln   | 0             | 445 / 1,306    |
| 197→198 | 136 st / 10,052 ln | 5 / 87        | 773 / 2,874    |
| 215→216 | 52 st / 6,030 ln   | 1 / 72        | 361 / 1,503    |

= 722 st / 50,911 ln (79% of residual noiseLn). Slot kinds: cold-fn
internal locals (incl. J↔M swaps, 30 slots in one 1,183-ln statement),
below-floor minted bindings that missed inheritance (\_\_m←languageCodeMap),
free-identifier mint drift (initEnvironmentVal's probe vars — every pair).

NEXT BUILD (twin-slot descent): extend statement-twin to pair ALL
differing identifier slots of a gated unique twin (descend into nested
fn locals), transfer through validated rename (swap machinery handles
J↔M), freeze owner fns from the LLM. Positional slots inside a
hash-equal statement are exact-grade testimony; no cross-statement
identity risk for internal locals.

## Addendum 2: the mint-poisoning mechanism (proven from the 216 log)

Trace for `__m` / `languageCodeMap` (results/pin-rebased/2.1.216.log):

1. `module-binding: matched vRm→__m` — the cascade matches fresh `vRm`
   to a prior binding whose name is a MINTED LEFTOVER (`__m`).
2. Fresh's minifier coincidentally named a DIFFERENT binding `__m`; its
   prior counterpart is `languageCodeMap`, witnessed by TWO exact votes
   (`exact-match: skipping __m→languageCodeMap` ×2).
3. Fresh-`__m` settles carrying its own minted name (same-name match) →
   not pending → the languageCodeMap votes never tally.
4. `vRm→__m` rejects (`target-in-scope`) at apply AND retry — the token
   is held by the binding from (3). Both bindings end wrong; the mint
   survives another hop (census stays ~flat at 475).

Lever candidate: BELOW-FLOOR PRIOR NAMES ARE NOT NAMES — a match whose
prior name fails the naming floor keeps its identity (context,
eligibility) but must not settle-and-keep or transfer the minted token;
the binding stays nameable (votes/LLM), and "never rename TO a minted
name" gets enforced at the transfer sites (vRm→\_\_m should have been
refused as a downgrade, not attempted).

Text-proxy bounds (crude — string-literal words contaminate the carried
count; binding-level count needs an in-pipeline stat): noise statements
touching carried mints 15/16/5/32 st ≈ 3.1k/4.3k/1.6k/4.3k ln per pair;
pure mint-token diffs 5/5/6/2 st ≈ 3.8k ln total (includes unfixable
free-identifier drift).

## Addendum 3: locality split — the mass is OUTER-reference echoes

aligned-locals decomposes (fresh-token declared-in-statement scan):

| pair    | internal-only   | outer-only         | mixed    |
| ------- | --------------- | ------------------ | -------- |
| 85→86   | 4 st / 1,004 ln | 476 st / 28,068 ln | 10 / 303 |
| 118→119 | 4 st / 1,061 ln | 40 st / 4,393 ln   | 0        |
| 197→198 | 3 st / 1,003 ln | 131 st / 9,032 ln  | 2 / 17   |
| 215→216 | 6 st / 1,325 ln | 46 st / 4,705 ln   | 0        |

~46k of 50.9k ln = unchanged statements ECHOING a modest set of flipped
ROOT bindings declared elsewhere. Internal locals (J↔M swap
megafunctions) are a ~1k ln/pair tail. Fix roots, not slots.

## Standing next steps (ranked, updated)

1. HASH-TWIN CONSUMER PASS: the consumer tier's evidence (reciprocal
   co-flip roots witnessed by unchanged consumers) does not need line
   alignment — hash-twin pairing supplies the witnesses reorder-free.
   Port the consumer gates onto hash-paired twins (attribute-roots
   ceiling-B machinery) as a post-output pass or reconcile pre-pass, so
   shuffle pairs (86: 27 reciprocal pairs / 5.8k direct ln + echo
   knock-on toward the 28k outer-only mass) become reachable. Measure
   the echo knock-on by simulating root renames text-level first.
2. Below-floor prior-name lever (mint poisoning, Addendum 2): add the
   in-pipeline counter, then guard settle/transfer sites.
3. Internal-local swaps (~1k ln/pair): swap-retry machinery
   investigation, low priority.
4. Family buckets (~13k ln, tiny statements) + free-identifier drift:
   tail.

## Addendum 4: hash-twin consumer pass NO-GO; mint-guard population sized

`simulate-root-inherit.ts` (reciprocal changed-leaf pairs on hash-paired
twins, consumer gates, text-applied, iterated): 18 root pairs / 371 ln
on 85→86, ZERO on the quiet pairs. The earlier ceiling-B 5.8k "echoed
ln" double-counted statements that also carry other flipped tokens —
coupled webs do not clean unless most of their roots resolve at once.
Production build NOT justified; standing step 1 closed as measured.

Step 2 population, measured straight from the strategy-trail JSON
(no pipeline change needed — the trail's first dividend): 475 applied
transfers on 216 inherit a minted-looking newName; by strategy:
exact-match 411 (mostly legitimate one-letter FN LOCALS — i/k loop
vars, must NOT be guarded), binding-cascade 36 + module-vote 2 +
retry/twin/close tail = the MODULE-LEVEL poison class (~45/pair),
matching the \_\_m trace and the frozen mint census (149).

Floor-guard design (next build): scope to module-level bindings only —
(a) a matched prior name failing the naming floor never settles-and-
keeps (same-name matches stay pending so votes/LLM can name them:
the **m -> languageCodeMap vote kill), (b) transfer sites refuse
below-floor newNames as downgrades (vRm -> **m). Keep fn-local slot
pairs untouched. Expected effect: census ratchets down per hop instead
of frozen; collisions like vRm/\_\_m dissolve; small first-hop healing
noise where mints get real names.

## Addendum 5: identity-grade second-pass recovery — NO-GO (measured)

User-proposed lever: after easy matches settle, revisit ambiguous
buckets with IDENTITY keys (matched-caller / matched-callee sets
translated through the match map) instead of blurred shapes/hash sets.
Instrumented via HUMANIFY_AMBIGUITY_PROBE (src/prior-version/
ambiguity-probe.ts) + ceiling-identity-recovery.ts on 215→216:

TOTAL ambiguous prior fns: 1,420
callee-identity: 3 uniquely recoverable (only 60 have a matched callee — the residue is leaves)
caller-identity: 7 (465 have matched callers, but callers call MULTIPLE
bucket members — caller sets are identical across the pool)
combined: 6 · union 10 (0.7%)
REMAINING: 1,410

Verdict: the ambiguous residue is not starving for identity evidence —
its evidence neighborhoods are isomorphic at every level we can compute
(hash, shapes, callee hash sets, callee identity, caller identity,
two-hop). The cascade's earlier tiers already extracted what identity
gives.

Honest remaining option for this class (unbuilt): deterministic
tie-breaking — pair provably-interchangeable members by bucket ordinal
under an equal-count guard. "Wrongness" is unmeasurable by construction
(all evidence ties), and names would stabilize across quiet hops; the
cost is ordinal shuffle whenever bucket membership churns (the
getEventQueryString case). Prize ≈ the family share of visible noise
(~hundreds of lines/pair). Park unless that class rises in priority.
