# Review 2026-08-09 — rename-safety correctness, duplicated paths, architecture

Scope per request: (1) correctness of every cross-version name-reuse path,
(2) duplicated code paths that answer one question two ways, (3) how to
structure the repo so cross-version naming strategies can be added and
toggled without losing the pluggable pipeline — plus an audit of whether
the measurement stack can be trusted to size any of it, because the owner's
stated confidence problem is as much about the instruments as the levers.

Method: five parallel reviewers over partitioned slices (~47k lines:
apply-safety, matching cascade, post-naming passes, processor/architecture,
measurement stack), every load-bearing claim re-verified against the code
by hand before action. Findings that did not survive verification are not
listed. Fixes landed on `refactor/correctness-and-arch`, red/green TDD,
`npm run check` green per commit; fingerprint snapshots unchanged on all of
them.

## 1. Correctness bugs FIXED (commits `e6fddf8`, `354a739`, `8ad6982`)

Ranked by blast radius. Each was verified by a failing test first.

1. **Interchangeable-pool injectivity hole** (`fingerprint-index.ts`).
   Certified pools can OVERLAP — `evidenceKey` omits exactly the evidence
   (twoHop, call-graph) that narrows pools differently — and assignment
   tracked claims per-pool against a one-shot snapshot. Two pools could
   claim the same fresh function, _after_ `demoteNonInjectiveMatches` had
   already run, so both priors' names shipped onto one function. Now a
   candidate claimed by an earlier pool voids every later pool containing
   it (its reciprocity certificate no longer holds → abstain). The sibling
   tier `ordinalPairBucket` always did this correctly — same question, two
   answers, the exact responsibility.md failure shape.

2. **Uncorroborated close matches shipped the module-scope var name**
   (`prior-version.ts`). The corroboration gate emptied `nameTransfers`
   ("a shape coincidence must not present a wrong name as continuity") but
   `collectFunctionVarNameTransfers` iterated every close-match context
   entry unconditionally — so the pair whose _parameter_ names were gated
   still shipped the most visible name of all. `CloseMatchInfo` now carries
   `corroborated`; the var-name transfer reads it. Two test fixtures
   (empty/identical arrows) were riding on the leak and now earn their
   transfers through corroboration — worth knowing because the same
   population (byte-identical tiny functions with churned counts) will now
   fall to the LLM instead of inheriting arbitrarily-paired prior names.

3. **Statement-twin tier skipped the below-floor guard**
   (`prior-transfer.ts`). The tier that applies FIRST and settles
   module-level bindings was the only tier without the minted-leftover
   refusal every sibling enforces. Reachable exactly where the cascade
   abstains and twins decide alone (fn-expression-init heads,
   unequal-count clone buckets). A below-floor prior name would settle and
   poison every future hop. Guarded now; fn-internal locals stay exempt.

4. **Family-permute could ship `__familyPermuteSwapN$` in the artifact**
   (`family-permute-step.ts`, found independently by two reviewers). A
   chain fill blocked by an outside binding rolled back to a name a
   chain-mate had already claimed; the rollback's failure was ignored. The
   temp shipped in the FINAL output (this pass runs on shipping code and
   feeds the next hop's prior), invisible to the name-blind structural
   invariant and absent from the move trail — an empty trail was supposed
   to prove innocence (rule 11) and here could not. Moves whose target
   cannot land are dropped to a fixpoint before staging, buckets apply
   independently, and any residual fill failure reverts its whole bucket.

5. **Raw `waveScheduling` reached the sweeps** (`plugin.ts`, two
   reviewers). Two sites passed `options.waveScheduling` unresolved as
   `deterministicApply`, so any non-CLI caller got a deterministic
   processor and a completion-order-nondeterministic coverage sweep — the
   exact two-place-default recurrence `default-args.ts` documents, at two
   NEW sites after the processor one was fixed. Both resolve now, and a
   source-guard test (kill-switches style) fails on any future raw read.
   This was also the strongest in-slice candidate for the rare divergence
   on any leg that ran without CLI-resolved options.

6. **Shorthand properties nuked whole passes** (`post-split-reconcile.ts`,
   `bundle-carry.ts`, `diff-reconcile.ts`). `{ count }` holds two
   identifier nodes at ONE loc; a rename moves only the value, and both
   text-rewrite passes substituted the new name at the loc — rewriting the
   PROPERTY KEY. Their reparse guards caught the semantic change and threw
   away the work wholesale: post-split discarded the whole file's renames;
   bundle-carry aborted the ENTIRE carry (`rewrite-unsound`, carried: 0) —
   one shorthand reference anywhere cut that hop's lineage inheritance,
   a candidate mechanism for "the carry sometimes does nothing". One owner
   (`renameSubstitutionText`, babel-utils.ts, responsibility.md row added)
   now expands the occurrence to `key: newName` for both passes. Upstream,
   diff-reconcile's taint now distinguishes an object-literal shorthand
   VALUE (mere reference — tolerate the failing key twin) from
   destructuring shorthand (the coupled token IS the declaration and the
   key decides WHICH property is read — keeps its deliberate conservative
   taint, existing test unchanged).

7. **Diagnostics hygiene** (`8ad6982`): the binding cascade's shingle tier
   reads `index.functions`, which a binding index never sets — silently
   dead on every consultation, `shingleSimilarityResolved: 0` reading as
   "consulted, found nothing" (the dead-singleton-guard silence, again).
   Now counted (`shingleUnconsultable`), printed next to
   singleton-UNGUARDED, tested to fire. Plus: the shingle floor
   single-sourced (`SHINGLE_SIMILARITY_FLOOR` — it lived as two 0.5
   literals in two files), and `HUMANIFY_SHINGLE_PROBE` read at call time
   per the kill-switch contract.

**Eval verdict (ran 2026-08-09, cold, `REBASE_PRIOR=1`, all four pairs
exit 0 / boot OK / `cache +0`): PASS.** Against `session-2026-08-05`, the
two draw-invariant hold columns are BYTE-IDENTICAL — `novel` 4,188 = 4,188
and `realLines` 416,377 = 416,377 — so the fixes dropped no real change.
Every other delta sits inside its noise band (`namingNoiseLines` −1,057 is
the favourable direction but under the ±2,800 floor, so it is NOT claimed;
`mintedLeftovers` 76 vs 81; vendor/layout wobble within recorded bands).
`novelNames` +19 is the expected direction of fix #2: bindings that used
to inherit uncorroborated close-match names now get fresh LLM names. The
2.1.216 self-hop diverged by 96 lines — same class as the reference's own
122 (cold-run LLM re-roll, exp047), and as of this arc that verdict is
printed in the summary banner instead of sitting unread on disk.

## 2. Confirmed, NOT yet fixed — ranked backlog

Verified real, deferred deliberately. Do not re-derive; fix or explicitly
close each.

1. ~~Wave-barrier rejections corrupt merged report outcomes~~ — **NOT
   REPRODUCED (2026-08-09).** The static read (`processor.ts:2389-2430`
   writing unsuffixed keys post-merge) predicted the phase-0 entry gets
   clobbered; probing the canonical collision fixture (catch `K`
   shadowing var `K`, both suggested the same name, barrier rejection +
   retry) shows the merge/settle timing keeps both entries correct: plain
   `K` holds the phase-0 result, `K#2` the retried shadowed result, both
   matching the emitted code. `processor-report.test.ts` now asserts the
   CONTENT of both entries, so a timing change cannot silently introduce
   the predicted corruption. A lesson in the file's own discipline: a
   mechanism read off the code is a hypothesis until a probe fires.
2. ~~nameOrdinal excludes co-renamed siblings~~ — **UNREACHABLE, pinned
   (2026-08-10).** The corruption requires diff-reconcile to rename BOTH
   same-named sibling declarations; probing shows its clean-declaration
   proof refuses such a group entirely (renames: []). The locator's
   correctness therefore DEPENDS on that refusal, and a tripwire test now
   pins it — if the tier ever learns to emit such pairs, the test names
   the ordinal work that must land with it.
3. ~~Retry cycle-break has no rollback~~ — **FIXED (2026-08-10).**
   Stranded temps are restored best-name-first: the original name, then
   the WANTED name decorated through the owner ladder (descriptive beats
   a temp); a still-stranded temp records a `stranded-temp` trail entry
   instead of failing silently. TDD with a red-proofed swap-cycle fixture
   whose landing fails on a wanted-name-only shadows-child.
4. ~~Evidence-to-binding identity re-check absent on four apply paths~~ —
   **FIXED (2026-08-09).** `attemptValidatedRename` now takes an optional
   `expectedBinding` and rejects `stale-binding` when the name was re-keyed
   (TDD in validated-rename.test.ts). `ModuleBindingRename` carries the
   phase-0 evidence binding; binding-cascade, fn-name-vote and
   closure-capture pass theirs, and module-vote checks the holder's
   declaration identifier against the node's own (era-stable identity).
   The hand-rolled pre-checks on the other four paths can migrate to the
   owner's parameter at leisure — the owner is now authoritative.
5. ~~Close-match tier has no tie detection~~ — **FIXED (2026-08-10),
   eval-validated.** `assignGreedy` abstains when a pair ties exactly with
   another still-available pair sharing either endpoint (mutual, both
   scan directions); disjoint equal-score pairs still match.
   **Eval `precision-fixes-2026-08-10` (cold, REBASE_PRIOR=1, 4/4
   exit 0 / boot OK / cache +0): PASS** — hold columns exactly equal for
   the third consecutive gate (novel 4,188; realLn 416,377), mints 74
   (was 76), reloc 590 (was 639), layout alias halved (144 vs 288,
   sub-band, not claimed). This validated the tie abstention, the
   evidence-identity guards, and the retry restoration together, and the
   run itself exercised the new dispatcher, the hard preflight gate, the
   fixed %det arithmetic (sums to 100.00 on every pair now) and the
   self-hop banner end to end.
6. ~~memberKey absence semantics diverge~~ — **DECLARED DELIBERATE
   (2026-08-10).** The two answers serve different situations: with
   RIVALS in the pool, a candidate that cannot carry the key loses to
   ones that do (and an emptied pool parks the prior as ambiguous — a
   missed match, never a wrong one); in the singleton path there is no
   rival, so refusing on absence would kill the majority tier for zero
   precision gain. Documented at `filterByMemberKey`; both directions
   fail toward abstention.
7. ~~Demotion/contradiction re-widen pools~~ — **FIXED (2026-08-10).**
   Two changes: `evidenceKey` now includes `twoHopShapes` (every feature
   the cascade itself distinguishes on — its omission let pools narrowed
   by different two-hop evidence certify as "identical evidence" and
   overlap, the injectivity hole's enabler), and `MatchResult` carries
   `demotedPriors`, which both position-based tail tiers refuse: a
   contested prior is propagation's to re-resolve with positive
   evidence, never a pair-by-position guess (TDD'd — a broken identity
   resolver forcing a double claim, ordinal must resolve 0). The
   calleeHash-contradiction re-park and propagation's stale empty-pool
   entry remain protected by the evidence-key uniformity gate.
   Behaviour-affecting in the precision direction — fold into the next
   eval batch.
8. **Decoration ladder produces names its own stripper cannot strip**
   (`validation.ts:227` makes `_name`/`name_`/`local_name`/`inner_name`;
   `DECORATION_SUFFIX` strips none of them, so they never snap back to the
   prior name next hop — self-inflicted permanent churn; and
   `decoration-retry` only retries trailing-underscore forms). Two passes
   disagree on "what is a decoration". This is a measurable noise lever,
   and the strip/restore cycle between decoration-retry and diff-reconcile
   is a live suspect for the "decoration-shaped name" divergence artifact.
9. Smaller, real: pin injectivity ignores `closureVotes`
   (`prior-transfer.ts:1493`); family-permute `byName` map loses one of
   two same-named members across wrapper+Program scopes;
   `bySessionPosition` parses sessionIds (`types.ts` forbids it; every
   binding id parses to (0,0) so binding "source ordinal" is really
   insertion order); `evidenceKey` omits `twoHopShapes`;
   `retryBatchWindowMs` is a documented plugin option that does not
   exist on the plugin; LLM call counters double-count
   (`processor.ts:1471,1512,1771`); wave `winners` keyed by name across
   nodes feeds wrong `alreadyRenamed` retry pairs.

Statement-twin F13 (conflicted pairs bypass `ownerAllowsTransfer`'s
pending-state half) was reviewed and left: the window is cascade-claimed
module bindings only, and the override is the documented crossed-head
repair. A comment at the gate would close the ambiguity.

## 3. Duplicated paths — status

Fixed this review: rename-occurrence-to-text (one owner now), shingle
floor (one constant), pool claim-tracking (matches the ordinal tier's
discipline). Confirmed still-open, worth unifying in roughly this order —
each is two answers to one question with nothing declaring the difference:

| question answered twice                  | sites                                                                                                                                   | note                                                            |
| ---------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------- |
| per-function orchestration               | `processFunction` vs `runWaveFunctionPhases` (processor.ts)                                                                             | see §4 — the two schedulers are the root                        |
| deadlock/readiness tiers                 | `computeWaveMembers` vs `breakInitialDeadlockUnified`+2                                                                                 | same 3-tier policy, data-driven vs hand ladder                  |
| "did the LLM re-decorate the prior name" | `snapSuggestionToPrior` (fn path) vs inline stem compare (module path, processor.ts:1266)                                               |                                                                 |
| names visible here + globals             | `collectModuleUsedNames` vs `getUsedIdentifiers` (context-builder)                                                                      | both carry the same review-C1 comment                           |
| declaration text for a binding           | `getDeclarationText` (plugin) vs `getBindingDeclCode` (context-builder)                                                                 | caps differ                                                     |
| soft used-name pre-check                 | FOUR different sets: windowed (:1684), merged (:3090), `liveUsedNames` (wave-scheduler:199), one-shot snapshot (:2999, never refreshed) | hard owner shared; the soft layers are where drift shows first  |
| statement span index                     | `bundleDeclarations` (bundle-carry) vs `declarationIndex` (post-split)                                                                  | near-verbatim incl. binary search                               |
| write-target definition                  | `getConstantViolationLHS` (validated-rename) vs `violationWriteTargetPaths` (babel-utils)                                               | agree today; only a comment says they must                      |
| attempt trails                           | `IdentifierAttemptState.trail` vs `strategyTrail`                                                                                       | two per-identifier histories, different keys, different reports |
| neighbor-set identity key                | prior-version:1362 / binding-role:182 / statement-twin:295                                                                              | statement-twin's is a declared mirror; prior-version's is not   |
| canonical fn-for-binding                 | `handleIdentifierCallee` vs `findFnForBinding` (function-graph)                                                                         | accept different function-node types                            |

Pre-existing open items from `code-review-2026-07-06-duplication.md`
(A3-A8, B1-B2, C1-C11) remain valid; B1 (three rename-application paths)
is subsumed by the scheduler question below.

## 4. Architecture — what it is, and the three moves that matter

**What exists.** Three real adapter registries (unpack, library-detection,
split — same shape: array, select by name/`supports()`, fallback last).
One tier registry with counters+trail derived from data
(`PLACEMENT_TIERS`). One strategy registry for mechanical transfer
(`TRANSFER_PIPELINE`, phase 1 of the naming-pipeline proposal — `{name,
description, run(ctx)}` over a shared `TransferContext`, doc-drift-tested).
Everything else in stages 8–9 is hand-wired: the matching cascade is a
fixed ladder inside `matchFunctions`, the LLM stage is one entangled loop,
and the five post-output passes are `maybeX` functions sequenced by hand in
plugin.ts with "last pass that produced code wins" precedence.

**Move 1 — delete the free-running scheduler.** The deepest correctness
finding is not any single bug: it is that the LLM stage exists TWICE —
wave-barrier path and `--no-wave-scheduling` free-running path — with
hand-maintained parity. Confirmed divergences: report corruption on one
path only (§2.1), retry collision-views reading different sets, prompt
context reading different lifecycle states, mid-flight AST mutation on the
legacy path (the exact thing wave mode exists to forbid), a callback set
whose `applyRename` must never be called with nothing enforcing it — and
the `resolveRemaining` leak already happened once (comment at :2256). The
no-backwards-compat preference is on record; the legacy loop's only
remaining role is a debug flag, and it quietly holds a feature the wave
path dropped (RetryBatcher) while lacking ones it gained (barrier
re-validation, deterministic settle, `fixupRenamedCount`). Deleting it
collapses B1, D1, D2, F3-F5 and makes the collect/barrier-apply split the
ONLY apply path. If a scheduler escape hatch must survive, keep the flag
but make it select a _degenerate wave_ (waves of size 1), not a second
implementation.

**Move 2 — finish the registry the transfer pipeline started.**

- **POST_PASSES** (naming-pipeline phases 3.1–3.5): the five `maybeX`
  passes are already uniform in shape — `{name, eligible(opts,
outputValid), run(code|ast) → {code?, ast?}}` — with the runner owning
  precedence, spans, and stats. Entangled bits to declare as fields, not
  bury: `releaseReconAstBeforeSweep` (memory), family-permute's Babel-cache
  clear (teardown), the ledger's need for per-stage (input, ast).
- **Matching cascade as data**: `resolveMatch`'s ladder is nearly uniform
  (filter → 0=contradiction / 1=match / else next). Declaring it as an
  ordered array of `{name, filter}` gives per-tier counters for free —
  `resolutionStats` already has the fields; today they are wired by hand,
  which is how a tier (shingle-on-bindings) stayed silently dead. The
  cascade ORDER is load-bearing and tested; a registry makes the order a
  reviewable literal instead of control flow.
- **Per-strategy toggles fall out**: a registry filter
  (`HUMANIFY_SKIP_STRATEGY=close-match,family-permute` or a
  `--strategies` allowlist) is a ~20-line runner feature once passes are
  data. That is the ablation instrument §5 needs — today ablating one
  tier means editing code, so nobody does it, so tier value is unmeasured.

**Move 3 — every strategy earns per-item evidence or doesn't run.** The
project already learned this three times (singletonUnguarded, the empty
trail in rule 11, the placement trail): a tier with no trail cannot be
attributed, exonerated, or sized. Make the runner (not each pass) record
the strategy-trail entry and the per-stage counter, so a NEW strategy gets
observability by construction. The registry is what makes this a one-place
change.

**Not proposed** (agreeing with pipeline-stages.md): runtime-measured
strategy selection. The noise floor argument stands.

### Which naming tiers are actually earning their keep

Evidence-quality grading of the big tiers, from the experiment record —
this is the honest answer to "quality of our approaches":

- **Proven at scale, keep**: exact slot tables + statement twins +
  binding cascade (the bulk of transfers; twin repairs are
  self-hop-tested); post-split reconcile (−5,026 ln, draw-pinned 4/4);
  vendor content-keyed reuse (6,393→1,563 cold); reorder/export-registrar
  unpin (−60% src reorder); anchor cascade (reloc −91% cumulative with
  pre-registered ceilings).
- **Real but with a now-fixed hole**: close match (the var-name leak
  shipped uncorroborated names as continuity for its whole life; its
  remaining tie-blindness is §2.5). Its _net_ value has never been
  ablated cold — worth one registry-filter run once toggles exist.
- **Unmeasured or resting on cache-pinned gates**: five shipped levers
  (038, 041, 042, 043, 046) were gated warm (rule 10 born later); their
  mechanism-ceiling halves stand, their "nothing else moved" halves are
  vacuous. The below-floor guard shipped with an accepted +2,600 noiseLn
  reading inside the band and an unresolved design fork (its raw
  `isBunToken` refusal re-rolls good decorated names every hop —
  §2.8 compounds this).
- **Known-cost, keep with eyes open**: LLM re-roll noise is structural —
  the LLM decides 3.64% of bindings and two cold legs disagree on a third
  of them by wording; every mechanical tier above it exists to shrink that
  surface. Decoration/collision handling is the one place mechanical
  churn is SELF-inflicted (§2.8) and is fixable.

## 5. Measurement — can the numbers be trusted?

Short answer: the discipline (11 rules, manifests, neutrality, KPI
registry with direction-as-data) is genuinely strong, and still: the
headline determinism number is arithmetically wrong, the lead noise KPI is
a lower bound with an unexecuted sizing task, only two columns are known
draw-invariant, and part of what seeds the next hop is unscored. Verified
highlights (full agent audit preserved in the session ledger):

1. **`%det + %llm = 101.31%` in every committed scorecard.**
   `closeMatch` is counted inside `f.llm` AND added again by
   `pctReachingLLM`; `coldLLM` is mislabeled (includes close matches).
   One-line fix (`coldLLM = f.llm − f.closeMatch`) — it is the headline
   "98% deterministic" figure.
2. **The current valid reference carries an unread failure.**
   `session-2026-08-05/2.1.216-self-hop.json: {identical: false,
diffLines: 122}` — boot/self-hop verdicts are written by run.sh and
   read by NOTHING downstream. Expected on a cold run per exp047, but then
   the invariant is decorative and should say so in `summary.json` instead
   of passing by omission.
3. **`layoutNoise` is a lower bound, and the sizing task never ran.**
   Name-only lines inside hash-flipped statements are charged to `real`
   (91% of what exp054 removed sat in the `real` column). exp055 Task 0 —
   the measurement that would size the misclassification — has no RESULTS
   file. Until it runs, "noise is nearly done" readings are
   classifier-visibility claims (rule 8), not residual claims.
4. **Only `novel`/`realLn` are demonstrated draw-invariant** (byte-equal
   across cold legs). No other KPI has a published two-cold-run agreement
   band; `vendorReal` has fired 4× outside its recorded band. The single
   highest-leverage measurement change: one 2-3× repeat-run session to
   publish a per-KPI band, and make the leaderboard print `~0 (±band)`
   for deltas inside it instead of a confident sign.
5. **Unscored surfaces that seed the future**: `index.js`/`run.cjs`,
   `.humanify/_bundle.js` + `__bun-runtime.js`, `prior-match-map.json`
   (its silent absence disables two placement tiers — already burned
   exp058 once), split-ledger drift, and the bundle-vs-tree divergence
   created by post-split reconcile (the two halves of the carry disagree
   by exactly that pass's renames; nothing measures the gap). Cheap
   per-pair counts close all of these.
6. **Instrument duplication**: three statement extractors with three
   failure modes (silent program-body fallback / silent `[]` on parse
   failure / the throwing owner in `experiments/lib/trees.ts` the other
   two predate); changed-line counting re-implemented in shell twice;
   cache-entry counting twice; `pairs.json` three ways. The silent `[]`
   on parse failure in `diff-composition.ts` converts a broken file into
   "real removed" lines inside the lead KPI — make it fatal.
7. **Rule-9 debt at the entrance**: `experiments/README.md` stops at 047,
   says "eight rules", and still states two refuted ceilings;
   exp045's own STATUS block carries the refuted reorder ceiling with no
   pointer to 049/050. Readers enter through these.

## 6. Suggested order of work

1. Land the eval verdict on the current fixes (running).
2. §5.1 determinism arithmetic + §5.2 surface self-hop/boot in summary —
   small, restore trust in the two numbers everyone quotes.
3. §2.1 wave outcome keys (feeds coverage KPIs), then §2.2–2.4.
4. Move 1 (delete free-running scheduler) — largest single de-risking.
5. Move 2 registries + toggles; then one ablation sweep to grade tiers
   (§4's unmeasured column) with §5.4's bands published first.
6. §2.8 decoration unification — the one self-inflicted churn lever.
7. exp055 Task 0 + unscored-surface counts (§5.3/§5.5).

Everything in §1 is committed; §2–§5 items are verified against code as of
`8ad6982` — re-verify line numbers before editing, files will have moved.
