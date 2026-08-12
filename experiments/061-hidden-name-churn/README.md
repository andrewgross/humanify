# 061 — name churn the noise buckets cannot see

> **STATUS (2026-08-12, exp061-lever-r1/r2 @ f25d470): EXECUTED — both
> levers shipped, gated cold twice; two of the brief's claims did not
> survive.**
>
> - Success criterion MET, modestly: paired name-only mass on 85→86 =
>   **1,500 / 1,440** on two cold runs (baselines 1,622/1,662; ~4x the
>   repeat spread), anchored cross-file mass flat-to-lower, and
>   `novel`/`realLn` byte-exact on BOTH runs (4,188 / 416,377).
>   Four-pair hidden-churn totals: 3,146 / 3,164 vs 3,448 (−8..9%).
> - DID NOT SURVIVE (1): the hypothesis's lever direction "carry the
>   prior name for a corroborated binding". The safe form is a HINT
>   into the ask, not a carry (exp058's caution held) — and its exact
>   restoration is capped by NAME CONTENTION: 86 of 187 hints landed
>   decorated because the exact name was already claimed (71 by
>   cascade-named bindings; see the duplicate-module case study).
> - DID NOT SURVIVE (2): the implicit hope that this mass was mostly
>   recoverable in-pair. The decomposition caps in-file levers at ~37%
>   of occurrences: ~36% is cross-file member amplification (fixable
>   only at ask time), ~26% is emit-time import-alias churn (instance
>   ordinals / placement), and 40% of LLM churn arrives with a BARE
>   trail (no evidence to carry).
> - The larger yield is the follow-up set recorded below: forwarding-
>   stub collapse + instance-ordinal stability (1,600+ reference lines
>   on one stub family alone), hint-collision counter, caller-set
>   identity for real duplicates.

> **This is a BRIEF — a hypothesis, including its cautions.** Whoever finishes
> it stamps a STATUS block naming which of its claims did not survive.
>
> Read `055-residual-recount/RESULTS.md` first: this experiment exists because
> its Task 0/0b measurements (2026-08-11) found ≈6,138 lines of defensible
> name churn INSIDE the REAL column — nearly the size of the entire
> classified-noise table (7,598) — and the decision rule fixed in the 055
> brief says that mass is now the target.

## What is known before any lever is designed

All numbers from `noise-band-r1`/`r2` trees (commit `76c012b`, cold,
two-run stable). Three distinct populations:

1. **Paired name-only lines: 3,448** (`real-ledger.ts`). Lines inside
   hash-flipped statements that differ ONLY in identifier tokens.
   Mechanism split (85→86, the dominant hop, measured 2026-08-11):
   - **91% whole-word re-rolls** (741 pairs: `initStubModule →
stubModuleFactory`, `documentTitle → agentName`)
   - 6% stem-contained tweaks (`errorCaught → error`)
   - 3% mint-ordinal drift (`initializeApp152 → initializeApp10`)
   - **Spread across ~941 distinct identifier pairs, avg 1.3 uses each;
     the top ten cover <10%.** There is no hot-binding shortlist — the
     lever must be structural.
2. **Cross-file masked twins: 4,830** (`one-sided-ledger.ts`), of which
   **2,690 string-anchored** (same require path, drifted local alias —
   near-certain identity; relocation+rename compounded) and 2,140 bare
   (upper bound only).
3. Unsized remainder: one-sided statements that changed shape AND names
   (no predicate exists), and string-keyed names (export-key drift).

## The hypothesis

The whole-word re-roll mass is the close-match/LLM tier re-deciding names
for bindings whose ROLE did not change, inside statements whose hash
flipped because the surrounding code genuinely changed. exp052 measured
the re-roll rate directly: two cold legs disagree on 33.4% of
LLM-decided bindings by a different word. The carry machinery
(diff-reconcile, snap-to-prior, bundle carry) reconciles same-hash
statements; a hash-flipped statement's bindings lose that protection even
when the binding itself is unchanged.

**Lever direction:** carry or reconcile the prior name for a binding
whose identity is corroborated (same declaration shape, same callees,
same role) even when its enclosing statement's hash flipped. Where the
carry machinery already owns this question, widen the owner — do not add
a second path (responsibility.md).

## Cautions recorded before measuring

- exp058(B) REFUTED widening the binding-cascade carry — the widened
  carry was the identity function. Re-read `project_exp058_binding_placement`
  before assuming a widening lands anything.
- exp044 alias reservation cost +3,742 lines through second-order
  effects with an honest per-scope ceiling (rule 5/6). Any carry lever
  must be sized with a mechanism-derived ceiling BEFORE the run and must
  LOG every carry it applies (rule 11 — an empty trail proves innocence).
- The eval's noiseLn CANNOT see this mass (that is the point). The
  instrument for the lever is `real-ledger.ts`/`one-sided-ledger.ts`
  re-run on the lever's own trees, plus the ordinary hold columns
  (novel/realLn band 0 — three-repeat fact as of 83545cb).
- A name-only line can still be real change (a call rerouted to a
  same-shaped helper). The 91% figure is a predicate result, not a
  hand-verified rate; sample twenty before shipping any claim.

## Task 0 — locate the deciding tier — EXECUTED 2026-08-11

`tier-provenance.ts` joins the churned fresh-side identifiers (631
unique, 1,262 occurrences on 85→86) against the run's strategy trails by
settled name. Result, weighted by occurrences:

| tier            | share | reading                                                                                            |
| --------------- | ----: | -------------------------------------------------------------------------------------------------- |
| **llm**         | 42.0% | the target: cold re-rolls on re-asked bindings                                                     |
| exact-match     | 13.9% | mostly join ambiguity (exact-match APPLIES the prior name; a same-named different binding matched) |
| binding-cascade |  7.2% | carry produced a different word — worth a look                                                     |
| module-vote     |  3.1% |                                                                                                    |
| close-match     |  2.5% | smaller than hypothesized                                                                          |
| (others)        |   <4% |                                                                                                    |
| unmatched       | 27.3% | name-level join misses (decoration between trail and emission, member exprs)                       |

Two hypothesis revisions: the close-match tier is NOT the owner (2.5%,
not the hypothesized driver) — the mass is bindings that reached the
**LLM tier itself**, i.e. the cascade abstained entirely and the binding
was re-asked cold. The lever is therefore about WHY corroborable
bindings fall through to the LLM (or about prior-biasing the LLM ask),
not about the close tier's re-rename policy. Second: the join is
name-level and 27% unmatched; before designing, re-join at loc level
(trail loc → pre-split loc mapping) if the lever needs per-binding
precision.

## Success criterion (fixed now)

The three-repeat bands say any `novel`/`realLn` movement is real: the
lever must hold BOTH exactly. Victory = paired name-only mass (85→86)
drops materially below 1,622 with the anchored cross-file mass not
rising, measured by the 055 instruments on cold trees, twice.

## Task 0b — loc-level provenance join — EXECUTED 2026-08-12

`loc-provenance.ts` re-joins at statement scope using three verified facts:
trail locs are valid line numbers in the saved `humanified.js` (renames do
not reflow lines — 100.0% of 5,000 sampled locs still hold their settled
name), the split-ledger's `order[]` is parallel to the wrapper body, and a
churned line's file plus the trails bucketed into that file's statements
give a per-occurrence decider. Two-run stable (r1/r2):

| decider (terminalBy) | share of all churn | reading                                                        |
| -------------------- | -----------------: | -------------------------------------------------------------- |
| **llm**              |  43.7% (both runs) | 61–62% of joined; the confirmed owner                          |
| exact-match          |         10.2/9.6%  | matched a DIFFERENT prior binding — matching error, not naming |
| binding-cascade      |          8.8/7.8%  | carry produced a different word                                |
| module-vote + others |               ~9%  |                                                                |
| unmatched            |        28.7/29.9%  | see shape split below                                          |

Shape split relocates the unmatched: ~26% of ALL churn is nsVar
require-alias identifiers (emit-time names derived from the import
target's path — second-order churn from placement hops / file naming, no
trail exists), ~36% is member-property positions (`alias.member` — the
declaring binding's rename amplified into every importer; joins globally
to the declaring trail), ~37% is plain in-file binding churn.

**The churn REPRODUCES across cold repeats** — the same identifiers churn
the same way in r1 and r2 (`generateSummary → generateResponse` both
runs). This is prompt-context drift at temperature ~0, not sampling
noise: the fresh legs agree with each other while disagreeing with the
prior. exp052's 33.4% cold-vs-cold disagreement measured a different
axis (two legs of the SAME version).

## Fall-through diagnosis (llm-terminal churn, clean joins, ~510/run)

- 75% came through the **module-binding batch ask**; 24% per-function asks.
- 40% reached the LLM with a **bare trail** (cascade never engaged).
- 32% had a pre-LLM vote proposing the prior name **verbatim or by stem**
  (nearly all `exact-match:vote`) — and the ask never saw it: votes only
  flow through the 2-vote floor (`MIN_MODULE_BINDING_VOTES=2`, ties
  block) or the single-vote pin (exactly one exact vote + role gates);
  vote maps here held 2–12 distinct names from slot testimony, so both
  paths abstained and the evidence was dropped.
- The `suggestedName` prompt channel ("Prior version name: X, strongly
  prefer") existed but was fed ONLY by close-match set elimination.

## Levers shipped (this branch)

**(a) `mixedHunkTier` in diff-reconcile**, enabled in the post-split
pass. Balanced hunks keep their clean name-only pairs alongside dirty
lines; unbalanced hunks pair by unique identifier-blanked skeleton
(ambiguous shapes never vote); a declaration admitted from a mixed hunk
reconciles only when EVERY occurrence line of its binding is itself a
clean untainted pair — the getTempDirPath negative generalized (dirty
call site = interface moved = name is information). Offline A/B (exp054
`ceiling.ts --mixed --skip-import-decls`, licensed because the pass is
deterministic and downstream of every prompt):

|                       |     r1 |     r2 |
| --------------------- | -----: | -----: |
| net diff lines        |   −166 |   −162 |
| lines created         |      0 |      0 |
| 055 name-only ledger  | 1,622→1,514 | 1,662→1,552 |
| one-sided mass        |    −18 |      0 |

**(b) `rankVoteSuggestion`** (single-vote-pin.ts) bridges the dropped
vote evidence into the module batch's `suggestedName` channel when
neither the vote floor nor the pin fired: unique top by (exact, total),
ties abstain, below-floor names excluded from candidacy. A hint, not a
rename; every hint is trail-logged (`vote-suggest`). Offline ceiling
(strict, assumes the LLM honors every hint): 43/40 fully-healed pairs =
**86/80 ledger lines per hop** — the corroborated population is 161/169
occurrences but the prior name is rarely the unique top of a diluted
vote map. The larger value is off-ledger: the hint fixes the name at ASK
time, so the export key and every importer's `alias.member` line heal
too — mass lever (a) structurally cannot reach (property positions
taint; export keys are strings at emit).

## Pre-registered expectation for the cold runs

Mechanism-derived, written BEFORE the two cold scored runs: paired
name-only mass on 85→86 should land around **1,450±100** (lever (a)'s
−110 measured offline, lever (b)'s ≤~90 direct plus unpriced member-line
amplification, minus LLM non-compliance). The success criterion's
"materially below 1,622" is expected to hold on the levers' combined
effect; `novel`/`realLn` must stay byte-equal to `main-2026-08-12`
(three-repeat band is ZERO). A miss on either side is a finding, not a
failure to report.

One-sided ledger on the patched trees: string-anchored (1,792/1,848) and
bare (1,214/1,130) masses are byte-identical to unpatched on both
repeats — lever (a) moved nothing into the cross-file column.

## Cold run 1 (exp061-lever-r1) — 85→86 early read

- NAME-ONLY inside REAL: **1,500** (baseline repeats 1,622 / 1,662;
  offline lever-(a)-only prediction 1,514/1,552; pre-registered window
  1,450±100 — hit).
- vote-suggest fired 187 times; 74 landed EXACTLY, 86 landed as a
  DECORATION of the hint (`initializeValidationHints` →
  `...HintsVal`), 7 stem-adjacent, 20 overridden. **LLM compliance is
  ~89% — the binding constraint is name-space contention, not the
  model:** the exact prior name is already claimed at ask time (likely
  the exact-match join-ambiguity class applying same-spelled names to
  different bindings), so validation collides and the ladder decorates.
  A decorated landing still churns this hop's ledger but is strippable
  and stable forward — the lever's compounding value is cross-hop, which
  a single-pair ledger cannot price. Who exactly holds the contended
  names is the open question for a follow-up.

## Case study: hint collisions are an upstream-anomaly detector

Of run 1's 86 decorated hint landings, 71 contested names are held by
bindings the binding-cascade named. Traced one family end-to-end
(`createGetRoleCredentialsCommand` + three sibling AWS command
factories): the prior bundle holds ONE copy; the fresh bundle holds TWO
byte-identical copies (bundler duplicated the module). The cascade gave
the name to copy 1 — defensible. Copy 2 is new code, but its CALLERS are
not: they exact-matched and truthfully voted the single prior name.
The hint relayed the vote, collided with copy 1, and copy 2 landed
decorated — and every re-pointed caller line now diffs
`createGetRoleCredentialsCommand → createGetRoleCredentialsCommandVal`
(name-only churn charged to real change; the actual change was "module
got duplicated").

A hint collision therefore proves an anomaly with THREE distinct causes
needing different fixes: (1) true twin mis-assignment — the matcher's
ambiguity pool resolves by matched-callee → matched-caller →
scope-parent → SCOPE-ORDINAL, and the ordinal rung can cross-assign
deep duplicate subtrees where all local evidence is twin-identical (the
`onStart` vote sitting on i36 alongside the correct name shows vote
cross-wiring exists); (2) new bundle duplicates — nobody used a wrong
name, two heirs contest one estate, and the churn is every caller line
of whichever copy loses; (3) corrupted votes — the collision correctly
blocked a bad hint.

**Follow-up levers, in value order (not executed here):**
- Surface a per-run `hint-collision` counter with holder classification
  (trail already records everything needed) — a free cross-assignment
  error detector, per the observation that a blocked reuse means a name
  went somewhere wrong (or a duplicate appeared).
- Duplicate-family naming: name new byte-identical copies as a stable
  derived family instead of collision-ladder accidents, and keep
  re-pointed callers on the copy they called before where placement
  allows. CAUTION: positional NAME assignment is a documented disaster
  (exp035/036, +50,606 noiseLn) — any scheme must key off stable
  evidence, not order.
- Vote-path caller/callee corroboration (the cross-wired-vote class).

## Follow-up (sized 2026-08-12): collapse forwarding stubs, stabilize instance ordinals

`lib_eb5345cb` — the ordinal-churn family whose `_2/_3` flip touched
1,600+ reference lines — turned out to be a ONE-LINE forwarding stub
re-exporting the single real react instance
(`exports.f = __commonJS((a,b)=>{b.exports=lib_014f5905.f()})`). The
bundler synthesizes one such shim per ESM↔CJS boundary; they are
byte-identical, stateless by construction, and every importer receives
the identical export object through any of them. Strict census on the
2.1.86 tree: **35 of 1,592 vendor files are pure forwarding stubs**
(one statement, <600 bytes).

Lever, in value order:
1. **Collapse strict stubs**: one emitted file per content hash, all
   importers rewired to it — the exports-identity argument makes this
   behavior-preserving; the guard is the SHAPE (single re-export
   statement), never the hash alone. One-time diff on the first release
   that ships it; the ordinal class then vanishes for stubs permanently.
2. **Widen to provably-stateless tiny modules** (`is-plain-object` ×37,
   `tiny-uuid` ×33 copies): needs a mechanical no-module-state check
   (no module-level mutable bindings, no load-time effects) before
   reuse — tractable at these sizes.
3. **Real logic duplicates** (the AWS command-factory family) must KEEP
   instances — separate module state is possible — and get stable
   identity from their CALLER SETS instead of census ordinals.
